//! Best-effort "a newer `aristo` is available" check against crates.io.
//!
//! This module is deliberately non-fatal: every failure mode — offline,
//! a slow endpoint, a malformed response, a missing `$HOME`, a corrupt
//! cache file — collapses to "emit no notice". It never touches stdout
//! and never returns an error to the caller. The worst case is that the
//! user simply isn't told an update exists.
//!
//! The network is hit at most once per [`CHECK_INTERVAL`]. Between
//! checks the last-seen latest version is read from a small cache file
//! under the per-user config dir, so the steady-state cost is one file
//! read plus a timestamp comparison.
//!
//! ## Layering
//!
//! Gating — is stderr a TTY? is `CI` set? did the user opt out? — is the
//! caller's concern (see the CLI's `update_notify` module). This module
//! only answers "given the clock and the cache, is there a newer
//! version, and what should the cache now hold?". The decision logic is
//! split into pure functions ([`should_fetch`], [`next_cache`],
//! [`notice`], [`is_newer`]) so it is unit-testable without a clock,
//! filesystem, or network.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::auth::config_dir;

/// The crate that owns the `aristo` binary on crates.io. Users run
/// `cargo install aristo-cli` even though the installed command is
/// `aristo`, so the lookup and the upgrade hint both name `aristo-cli`.
pub const CRATE_NAME: &str = "aristo-cli";

/// Minimum spacing between network checks. A check more recent than this
/// reuses the cached `latest_seen` version instead of hitting the
/// network.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Per-request timeout for the crates.io lookup. The notice is never
/// worth stalling the user for long; this matches the canon client's
/// conservative budget.
const FETCH_TIMEOUT: Duration = Duration::from_secs(2);

/// Filename of the throttle/cache file inside the config dir.
const CACHE_FILENAME: &str = "update-check.toml";

/// On-disk throttle state.
///
/// `last_check_unix` records when we last *attempted* a network check
/// (success or failure), so a flaky network can't make us poll on every
/// invocation. `latest_seen` is the most recent stable version
/// crates.io reported; it is carried forward across failed checks so the
/// reminder persists until the user actually updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cache {
    /// Unix seconds of the last network attempt.
    pub last_check_unix: u64,
    /// Last stable version crates.io reported, if any has been seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_seen: Option<String>,
}

/// Returns the one-line upgrade notice to print to stderr, or `None` if
/// there's nothing to say. Performs all I/O (cache read, optional
/// network fetch, cache write); every failure is swallowed.
///
/// `current` is the running binary's version
/// (`env!("CARGO_PKG_VERSION")`).
pub fn check(current: &str) -> Option<String> {
    let path = cache_path()?;
    let now = now_unix()?;
    let cache = read_cache(&path);

    let latest = if should_fetch(now, cache.as_ref()) {
        let fetched = fetch_latest_stable();
        let next = next_cache(now, fetched, cache);
        // Best-effort persist; a write failure just means we re-check
        // sooner next time.
        let _ = write_cache(&path, &next);
        next.latest_seen
    } else {
        cache.and_then(|c| c.latest_seen)
    };

    notice(current, latest.as_deref())
}

/// Has enough time elapsed since the last attempt to check again? No
/// cache (first run) always checks.
pub fn should_fetch(now_unix: u64, cache: Option<&Cache>) -> bool {
    match cache {
        None => true,
        Some(c) => now_unix.saturating_sub(c.last_check_unix) >= CHECK_INTERVAL.as_secs(),
    }
}

/// The cache to persist after an attempt. A successful fetch updates
/// `latest_seen`; a failed one (`fetched == None`) carries the previous
/// value forward so a transient outage doesn't erase a known-newer
/// version.
pub fn next_cache(now_unix: u64, fetched: Option<String>, prev: Option<Cache>) -> Cache {
    let latest_seen = fetched.or_else(|| prev.and_then(|c| c.latest_seen));
    Cache {
        last_check_unix: now_unix,
        latest_seen,
    }
}

/// Render the upgrade notice if `latest` is a valid semver strictly
/// greater than `current`. Unparseable versions yield `None` — better
/// silent than wrong.
pub fn notice(current: &str, latest: Option<&str>) -> Option<String> {
    let latest = latest?;
    if !is_newer(current, latest) {
        return None;
    }
    Some(format!(
        "\nA new release of aristo is available: {current} -> {latest}\n\
         Update with: cargo install {CRATE_NAME} --locked --force"
    ))
}

/// Is `latest` a strictly-greater semver than `current`? Either side
/// failing to parse yields `false`.
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (
        semver::Version::parse(current),
        semver::Version::parse(latest),
    ) {
        (Ok(cur), Ok(new)) => new > cur,
        _ => false,
    }
}

// ─── I/O boundary ──────────────────────────────────────────────────────────
// Thin wrappers around the clock, filesystem, and network. The decision
// logic above is pure and carries the test coverage; these are exercised
// end-to-end by the CLI.

fn cache_path() -> Option<PathBuf> {
    config_dir().ok().map(|d| d.join(CACHE_FILENAME))
}

fn now_unix() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

fn read_cache(path: &Path) -> Option<Cache> {
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str(&text).ok()
}

fn write_cache(path: &Path, cache: &Cache) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(cache)
        .map_err(|e| std::io::Error::other(format!("serialize update cache: {e}")))?;
    // Atomic write: <path>.tmp then rename, mirroring the credentials
    // store so a crash can't leave a half-written cache.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, path)
}

/// Hit crates.io for the latest stable version of [`CRATE_NAME`]. Any
/// failure — transport, non-success status, decode — yields `None`.
fn fetch_latest_stable() -> Option<String> {
    let url = format!("https://crates.io/api/v1/crates/{CRATE_NAME}");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(FETCH_TIMEOUT))
        // crates.io's crawler policy asks for an identifying UA with a
        // contact/repo. Default error-on-status is left on, so any
        // non-2xx surfaces as Err and is swallowed below.
        .user_agent(format!(
            "aristo-cli/{} (+https://github.com/aretta-ai/aristo)",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .into();

    let mut resp = agent.get(&url).call().ok()?;
    if resp.status().as_u16() != 200 {
        return None;
    }
    let body = resp.body_mut().read_to_string().ok()?;
    let parsed: CratesIoResponse = serde_json::from_str(&body).ok()?;
    parsed.krate.max_stable_version
}

/// Minimal projection of the crates.io `GET /api/v1/crates/{name}`
/// response — we only need the latest stable version.
#[derive(Deserialize)]
struct CratesIoResponse {
    #[serde(rename = "crate")]
    krate: CrateInfo,
}

#[derive(Deserialize)]
struct CrateInfo {
    /// Highest non-prerelease, non-yanked version. `null` if every
    /// published version is a prerelease.
    max_stable_version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_with_no_cache_triggers_fetch() {
        assert!(should_fetch(1_000_000, None));
    }

    #[test]
    fn fetch_skipped_within_interval() {
        let cache = Cache {
            last_check_unix: 1_000_000,
            latest_seen: Some("0.2.0".into()),
        };
        // One second later — well within 24h.
        assert!(!should_fetch(1_000_001, Some(&cache)));
    }

    #[test]
    fn fetch_resumes_exactly_at_interval_boundary() {
        let cache = Cache {
            last_check_unix: 1_000_000,
            latest_seen: None,
        };
        let boundary = 1_000_000 + CHECK_INTERVAL.as_secs();
        assert!(should_fetch(boundary, Some(&cache)));
        assert!(!should_fetch(boundary - 1, Some(&cache)));
    }

    #[test]
    fn next_cache_records_attempt_and_new_version() {
        let c = next_cache(42, Some("0.3.0".into()), None);
        assert_eq!(c.last_check_unix, 42);
        assert_eq!(c.latest_seen.as_deref(), Some("0.3.0"));
    }

    #[test]
    fn next_cache_carries_previous_version_when_fetch_fails() {
        let prev = Cache {
            last_check_unix: 1,
            latest_seen: Some("0.3.0".into()),
        };
        let c = next_cache(99, None, Some(prev));
        assert_eq!(c.last_check_unix, 99);
        // A failed fetch must not erase a known-newer version.
        assert_eq!(c.latest_seen.as_deref(), Some("0.3.0"));
    }

    #[test]
    fn notice_emitted_only_when_strictly_newer() {
        let n = notice("0.2.0", Some("0.3.0")).expect("newer -> notice");
        assert!(n.contains("0.2.0 -> 0.3.0"));
        assert!(n.contains("cargo install aristo-cli --locked --force"));
    }

    #[test]
    fn no_notice_when_equal_older_or_absent() {
        assert!(notice("0.2.0", Some("0.2.0")).is_none());
        assert!(notice("0.2.0", Some("0.1.9")).is_none());
        assert!(notice("0.2.0", None).is_none());
    }

    #[test]
    fn no_notice_on_unparseable_versions() {
        assert!(notice("not-semver", Some("0.3.0")).is_none());
        assert!(notice("0.2.0", Some("garbage")).is_none());
    }

    #[test]
    fn is_newer_across_patch_minor_major() {
        assert!(is_newer("0.2.0", "0.2.1"));
        assert!(is_newer("0.2.0", "0.3.0"));
        assert!(is_newer("0.9.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
    }

    #[test]
    fn cache_round_trips_through_toml() {
        let c = Cache {
            last_check_unix: 1_717_000_000,
            latest_seen: Some("0.4.2".into()),
        };
        let back: Cache = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn cache_without_latest_seen_round_trips() {
        let c = Cache {
            last_check_unix: 5,
            latest_seen: None,
        };
        let back: Cache = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(c, back);
    }
}
