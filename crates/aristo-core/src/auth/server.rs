//! Aretta server URL — `code.aretta.ai` (prod) / `dev.aretta.ai`
//! (dev) / `Custom(<url>)` (self-hosted, on-prem).
//!
//! Lives here, in [`crate::auth`], rather than in `canon::http_client`,
//! because the server URL is a **credential property**: an `arta_*`
//! token issued by `dev.aretta.ai` is not valid against
//! `code.aretta.ai`. Whatever persists the token also persists the
//! server it came from.
//!
//! ## Parsing user input
//!
//! The CLI's `--server <spec>` flag accepts:
//!
//! - `prod` / `production` → [`ServerUrl::Prod`]
//! - `dev` / `development` / `staging` → [`ServerUrl::Dev`]
//! - any other string that starts with `http://` or `https://` →
//!   [`ServerUrl::Custom`]
//! - any other string → [`ServerUrl::Custom`] with `https://` prefix
//!   added (so users can type `localhost:8443`).

/// The Aretta proxy this credential is for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ServerUrl {
    /// `https://code.aretta.ai` — production.
    #[default]
    Prod,
    /// `https://dev.aretta.ai` — dev / staging.
    Dev,
    /// Self-hosted or on-prem deployment. The string includes the
    /// scheme (`http://` or `https://`).
    Custom(String),
}

impl ServerUrl {
    /// Production base URL.
    pub const PROD: &'static str = "https://code.aretta.ai";
    /// Dev / staging base URL.
    pub const DEV: &'static str = "https://dev.aretta.ai";

    /// Base URL as a `&str` suitable for `format!` / `Url::parse`.
    /// Returns the full scheme + host (no trailing slash).
    pub fn as_str(&self) -> &str {
        match self {
            Self::Prod => Self::PROD,
            Self::Dev => Self::DEV,
            Self::Custom(s) => s,
        }
    }

    /// Parse a user-supplied spec (from the `--server` CLI flag or
    /// a persisted credentials-file `server` field).
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        match trimmed {
            "prod" | "production" => Self::Prod,
            "dev" | "development" | "staging" => Self::Dev,
            "" => Self::Prod,
            // Already a full URL — pass through.
            other if other.starts_with("http://") || other.starts_with("https://") => {
                Self::Custom(other.trim_end_matches('/').to_string())
            }
            other => Self::Custom(format!("https://{}", other.trim_end_matches('/'))),
        }
    }

    /// True iff this is one of the well-known Aretta servers.
    pub fn is_well_known(&self) -> bool {
        matches!(self, Self::Prod | Self::Dev)
    }
}

impl std::fmt::Display for ServerUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolve the base URL for **data-plane** requests — verify-session
/// dispatch and canon match. Precedence, highest first:
///
/// 1. `env_override` — the `ARETTA_API_URL` env var (CI / test /
///    staging redirect). Returned verbatim, preserving the prior
///    `env::var(...).unwrap_or_else(...)` behavior at the call sites.
/// 2. `instance` — the project's `[instance] url` from `aristo.toml`,
///    normalized through [`ServerUrl::parse`] (a bare host gets
///    `https://`, a trailing `/` is stripped). Blank/whitespace is
///    ignored.
/// 3. `server` — the signed-in account's server; its default already
///    resolves to `https://code.aretta.ai`, so it is also the final
///    fallback.
///
/// This is the **data plane**, distinct from the auth/control plane:
/// the `arta_*` token is minted against `server`, but verified-data
/// requests are addressed here. Env and config take precedence so a
/// repo can pin its data plane to a per-repo conductor
/// (`https://<slug>.aretta.ai`) without re-authenticating.
///
/// Kept pure — env is passed in, not read here — so it is
/// unit-testable under the workspace's `unsafe_code` ban on
/// `std::env::set_var`.
#[aristo::intent(
    "Data-plane base-URL precedence is exactly ARETTA_API_URL (env) > \
     aristo.toml [instance] url > the account server, in that order and \
     no other. The env override is returned verbatim (preserving the \
     prior override behavior and CI/test redirects); the [instance] url \
     is normalized via ServerUrl::parse; server.as_str() is the final \
     fallback (its default is code.aretta.ai). Reordering these tiers, \
     or normalizing or dropping the verbatim env override, would \
     silently misroute verify and canon-match requests to the wrong \
     Aretta deployment.",
    verify = "neural",
    id = "data_plane_base_precedence"
)]
pub fn data_plane_base(
    env_override: Option<&str>,
    instance: Option<&str>,
    server: &ServerUrl,
) -> String {
    if let Some(v) = env_override {
        return v.to_string();
    }
    if let Some(inst) = instance.map(str::trim).filter(|s| !s.is_empty()) {
        return ServerUrl::parse(inst).as_str().to_string();
    }
    server.as_str().to_string()
}

/// Where the login (auth-plane) server URL was resolved from. Carried
/// alongside the [`ServerUrl`] so the caller can name the provenance on
/// the "Authenticating against …" line — making a stale `ARETTA_API_URL`
/// export visible at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginServerSource {
    /// The user passed `--server` explicitly.
    Flag,
    /// Resolved from the `ARETTA_API_URL` environment variable.
    Env,
    /// Resolved by zero-config org discovery (queried at the
    /// prod-default platform when neither flag nor env was supplied).
    Discovered,
    /// Neither flag nor env nor discovery — the built-in production
    /// default.
    Default,
}

impl LoginServerSource {
    /// Short provenance suffix for the "Authenticating against …" line,
    /// or `None` for the built-in [`Default`](Self::Default) (where
    /// naming the source adds no signal). `repo_full_name` is
    /// interpolated only for [`Discovered`](Self::Discovered)
    /// (`discovered for <repo>`), where the repo is what was looked up.
    pub fn provenance(self, repo_full_name: &str) -> Option<String> {
        match self {
            Self::Flag => Some("from --server".to_string()),
            Self::Env => Some("from ARETTA_API_URL".to_string()),
            Self::Discovered => Some(format!("discovered for {repo_full_name}")),
            Self::Default => None,
        }
    }
}

/// Resolve the base URL for **auth-plane** login — where an `arta_*`
/// token is minted. Precedence, highest first:
///
/// 1. `flag` — an explicit `--server` value the user passed. Threaded
///    as `Option<&str>` so `None` means "unset" (distinguishing a
///    user-supplied value from clap's default); parsed via
///    [`ServerUrl::parse`].
/// 2. `env_override` — the `ARETTA_API_URL` env var. A blank/whitespace
///    value is treated as unset; a present value is parsed via
///    [`ServerUrl::parse`] so full URLs and bare hosts both work, and
///    the minted token's server matches the data plane
///    ([`data_plane_base`]).
/// 3. The prod default ([`ServerUrl::Prod`] = `code.aretta.ai`).
///
/// This mirrors the data-plane precedence so the auth plane and data
/// plane agree: without honoring `ARETTA_API_URL` here, a user who
/// exported it to target an org conductor would still authenticate
/// against prod and mint a token that org rejects.
///
/// Kept pure — env is passed in, not read here — so it is unit-testable
/// under the workspace's `unsafe_code` ban on `std::env::set_var`.
///
/// (The sibling [`data_plane_base`] carries an `#[aristo::intent]` for
/// this same precedence-invariant class; a matching intent for this
/// function should be authored via the `aristo-authoring` skill in a
/// follow-up rather than hand-written — see CLAUDE.md §10.)
pub fn login_server(
    flag: Option<&str>,
    env_override: Option<&str>,
) -> (ServerUrl, LoginServerSource) {
    if let Some(f) = flag {
        return (ServerUrl::parse(f), LoginServerSource::Flag);
    }
    if let Some(v) = env_override.map(str::trim).filter(|s| !s.is_empty()) {
        return (ServerUrl::parse(v), LoginServerSource::Env);
    }
    (ServerUrl::Prod, LoginServerSource::Default)
}

/// Resolve the login (auth-plane) server with zero-config org discovery
/// folded into the precedence. Highest first:
///
/// 1. `flag` — an explicit `--server` value.
/// 2. `env_override` — the `ARETTA_API_URL` env var (blank = unset).
/// 3. **discovery** — `discover(platform)`, run *only* when neither
///    flag nor env is supplied. `Some` redirects login to the
///    discovered `base_url`; `None` (404 / miss / any error) falls
///    through to `platform`.
/// 4. `platform` — the discovery platform itself, which is also the
///    miss fallback (the prod default in production; see below).
///
/// `discover` is invoked at most once, and **never** when an explicit
/// choice (flag or env) is present — an explicit server always wins and
/// skips the network lookup entirely. It receives `platform` so the
/// caller can query `<platform>/.well-known/aretta-org`.
///
/// `platform` is the "prod default platform" in production
/// (`code.aretta.ai`); the caller may relocate it (e.g. a self-hosted
/// deployment, or a test capture server) so discovery *and* its miss
/// fallback move together. The flag/env tiers delegate to
/// [`login_server`] so the two resolvers can't drift.
///
/// Env-var interaction: because a present `ARETTA_API_URL` (the env tier)
/// short-circuits discovery, the discovery `platform` — which
/// `ARETTA_DISCOVERY_URL` relocates (see the CLI's `discovery_platform`) —
/// is ignored whenever `ARETTA_API_URL` is set. The two never both take
/// effect: `ARETTA_API_URL` pins the login server outright,
/// `ARETTA_DISCOVERY_URL` only matters when discovery actually runs.
pub fn login_server_discovering(
    flag: Option<&str>,
    env_override: Option<&str>,
    platform: &ServerUrl,
    discover: impl FnOnce(&ServerUrl) -> Option<super::discovery::DiscoveredOrg>,
) -> (ServerUrl, LoginServerSource) {
    let (server, source) = login_server(flag, env_override);
    // An explicit choice (flag or env) short-circuits discovery: the
    // network lookup runs only when `login_server` fell to the default.
    if source != LoginServerSource::Default {
        return (server, source);
    }
    // Neither flag nor env: query discovery at `platform`, and fall back
    // to `platform` itself (not a hardcoded prod) on a miss so the two
    // move together when the platform is relocated.
    match discover(platform) {
        Some(org) => (
            ServerUrl::parse(&org.base_url),
            LoginServerSource::Discovered,
        ),
        None => (platform.clone(), LoginServerSource::Default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prod_resolves_to_code_aretta_ai() {
        assert_eq!(ServerUrl::Prod.as_str(), "https://code.aretta.ai");
    }

    #[test]
    fn dev_resolves_to_dev_aretta_ai() {
        assert_eq!(ServerUrl::Dev.as_str(), "https://dev.aretta.ai");
    }

    #[test]
    fn parse_prod_aliases() {
        assert_eq!(ServerUrl::parse("prod"), ServerUrl::Prod);
        assert_eq!(ServerUrl::parse("production"), ServerUrl::Prod);
        // Trims whitespace.
        assert_eq!(ServerUrl::parse("  prod  "), ServerUrl::Prod);
    }

    #[test]
    fn parse_dev_aliases() {
        assert_eq!(ServerUrl::parse("dev"), ServerUrl::Dev);
        assert_eq!(ServerUrl::parse("development"), ServerUrl::Dev);
        assert_eq!(ServerUrl::parse("staging"), ServerUrl::Dev);
    }

    #[test]
    fn parse_full_url_passes_through_as_custom() {
        let s = ServerUrl::parse("https://aretta.example.com");
        assert_eq!(s, ServerUrl::Custom("https://aretta.example.com".into()));
        assert_eq!(s.as_str(), "https://aretta.example.com");
    }

    #[test]
    fn parse_http_url_is_accepted_for_self_hosted() {
        // Self-hosted / on-prem deployments may not have TLS terminated
        // at the proxy. We don't force https — that's the caller's
        // security posture to decide.
        let s = ServerUrl::parse("http://aretta.internal");
        assert_eq!(s, ServerUrl::Custom("http://aretta.internal".into()));
    }

    #[test]
    fn parse_strips_trailing_slash_for_clean_format_strings() {
        let s = ServerUrl::parse("https://example.com/");
        assert_eq!(s.as_str(), "https://example.com");
    }

    #[test]
    fn parse_bare_host_defaults_to_https() {
        let s = ServerUrl::parse("aretta.example.com");
        assert_eq!(s, ServerUrl::Custom("https://aretta.example.com".into()));
    }

    #[test]
    fn parse_empty_string_falls_back_to_prod() {
        assert_eq!(ServerUrl::parse(""), ServerUrl::Prod);
        assert_eq!(ServerUrl::parse("   "), ServerUrl::Prod);
    }

    #[test]
    fn default_is_prod() {
        assert_eq!(ServerUrl::default(), ServerUrl::Prod);
    }

    #[test]
    fn is_well_known_matches_prod_and_dev_only() {
        assert!(ServerUrl::Prod.is_well_known());
        assert!(ServerUrl::Dev.is_well_known());
        assert!(!ServerUrl::Custom("https://example.com".into()).is_well_known());
    }

    #[test]
    fn display_renders_full_url() {
        assert_eq!(format!("{}", ServerUrl::Prod), "https://code.aretta.ai");
        assert_eq!(format!("{}", ServerUrl::Dev), "https://dev.aretta.ai");
        assert_eq!(
            format!("{}", ServerUrl::Custom("https://x.example.com".into())),
            "https://x.example.com"
        );
    }

    #[test]
    fn data_plane_base_env_override_wins_verbatim() {
        // ARETTA_API_URL (passed in) beats both instance and server, and
        // is returned verbatim — no normalization — so CI/test redirects
        // behave exactly as before.
        let s = data_plane_base(
            Some("https://ci.example.com"),
            Some("https://turso.aretta.ai"),
            &ServerUrl::Prod,
        );
        assert_eq!(s, "https://ci.example.com");
    }

    #[test]
    fn data_plane_base_instance_beats_server_and_is_normalized() {
        // No env override: the [instance] url wins over the account
        // server, and a bare host + trailing slash is normalized.
        let s = data_plane_base(None, Some("turso.aretta.ai/"), &ServerUrl::Prod);
        assert_eq!(s, "https://turso.aretta.ai");
    }

    #[test]
    fn data_plane_base_blank_instance_is_ignored() {
        let s = data_plane_base(None, Some("   "), &ServerUrl::Dev);
        assert_eq!(s, "https://dev.aretta.ai");
    }

    #[test]
    fn data_plane_base_falls_back_to_server() {
        let s = data_plane_base(None, None, &ServerUrl::Prod);
        assert_eq!(s, "https://code.aretta.ai");
    }

    // ─── login_server precedence ────────────────────────────────────────────

    #[test]
    fn login_server_flag_beats_env() {
        // An explicit --server always wins, even when ARETTA_API_URL is set.
        let (server, source) = login_server(Some("dev"), Some("https://turso.aretta.ai"));
        assert_eq!(server, ServerUrl::Dev);
        assert_eq!(source, LoginServerSource::Flag);
    }

    #[test]
    fn login_server_env_beats_default() {
        // No flag: ARETTA_API_URL wins over the prod default. This is the
        // field bug — the old login path ignored the env and hit prod.
        let (server, source) = login_server(None, Some("https://turso.aretta.ai"));
        assert_eq!(server, ServerUrl::Custom("https://turso.aretta.ai".into()));
        assert_eq!(source, LoginServerSource::Env);
    }

    #[test]
    fn login_server_env_parsed_via_serverurl_parse() {
        // The env value goes through ServerUrl::parse: well-known aliases,
        // bare hosts (→ https://), and trailing slashes all normalize.
        assert_eq!(login_server(None, Some("dev")).0, ServerUrl::Dev);
        assert_eq!(
            login_server(None, Some("turso.aretta.ai/")).0,
            ServerUrl::Custom("https://turso.aretta.ai".into())
        );
    }

    #[test]
    fn login_server_blank_env_is_ignored() {
        // A blank/whitespace ARETTA_API_URL is treated as unset, not as an
        // empty custom server, so it falls through to the prod default.
        let (server, source) = login_server(None, Some("   "));
        assert_eq!(server, ServerUrl::Prod);
        assert_eq!(source, LoginServerSource::Default);
    }

    #[test]
    fn login_server_unset_env_falls_back_to_prod() {
        let (server, source) = login_server(None, None);
        assert_eq!(server, ServerUrl::Prod);
        assert_eq!(source, LoginServerSource::Default);
    }

    #[test]
    fn login_server_provenance_named_only_when_not_default() {
        let repo = "owner/repo";
        assert_eq!(
            LoginServerSource::Flag.provenance(repo).as_deref(),
            Some("from --server")
        );
        assert_eq!(
            LoginServerSource::Env.provenance(repo).as_deref(),
            Some("from ARETTA_API_URL")
        );
        assert_eq!(
            LoginServerSource::Discovered.provenance(repo).as_deref(),
            Some("discovered for owner/repo")
        );
        assert_eq!(LoginServerSource::Default.provenance(repo), None);
    }

    // ─── login_server_discovering (precedence + discovery tier) ──────────────

    use super::super::discovery::DiscoveredOrg;

    fn discovered(base_url: &str) -> DiscoveredOrg {
        DiscoveredOrg {
            org: "acme".into(),
            base_url: base_url.into(),
        }
    }

    #[test]
    fn discovering_flag_skips_the_network_lookup() {
        // An explicit --server wins outright: the discovery closure must
        // never run (it panics if it does).
        let (server, source) = login_server_discovering(
            Some("dev"),
            Some("https://turso.aretta.ai"),
            &ServerUrl::Prod,
            |_| panic!("discovery must not run when --server is given"),
        );
        assert_eq!(server, ServerUrl::Dev);
        assert_eq!(source, LoginServerSource::Flag);
    }

    #[test]
    fn discovering_env_skips_the_network_lookup() {
        // ARETTA_API_URL (no flag) also short-circuits discovery.
        let (server, source) = login_server_discovering(
            None,
            Some("https://turso.aretta.ai"),
            &ServerUrl::Prod,
            |_| panic!("discovery must not run when ARETTA_API_URL is set"),
        );
        assert_eq!(server, ServerUrl::Custom("https://turso.aretta.ai".into()));
        assert_eq!(source, LoginServerSource::Env);
    }

    #[test]
    fn discovering_uses_discovered_base_url_at_the_platform() {
        // Neither flag nor env: discovery runs, is handed the platform,
        // and its base_url wins with the Discovered source.
        let mut queried_platform = None;
        let (server, source) = login_server_discovering(None, None, &ServerUrl::Prod, |platform| {
            queried_platform = Some(platform.as_str().to_string());
            Some(discovered("https://turso.aretta.ai"))
        });
        assert_eq!(server, ServerUrl::Custom("https://turso.aretta.ai".into()));
        assert_eq!(source, LoginServerSource::Discovered);
        assert_eq!(queried_platform.as_deref(), Some(ServerUrl::PROD));
    }

    #[test]
    fn discovering_falls_back_to_the_platform_on_miss() {
        // Discovery ran but the repo isn't mapped (None) → the platform
        // (prod default here).
        let (server, source) = login_server_discovering(None, None, &ServerUrl::Prod, |_| None);
        assert_eq!(server, ServerUrl::Prod);
        assert_eq!(source, LoginServerSource::Default);
    }

    #[test]
    fn discovering_miss_fallback_follows_a_relocated_platform() {
        // When the platform is relocated (self-host / test), a discovery
        // miss falls back to *that* platform, not a hardcoded prod — so
        // discovery and its fallback move together.
        let platform = ServerUrl::Custom("http://127.0.0.1:9".into());
        let (server, source) = login_server_discovering(None, None, &platform, |_| None);
        assert_eq!(server, platform);
        assert_eq!(source, LoginServerSource::Default);
    }

    #[test]
    fn discovering_blank_env_still_runs_discovery() {
        // A blank ARETTA_API_URL is treated as unset, so discovery is
        // still eligible (mirrors login_server's blank-env handling).
        let (server, source) =
            login_server_discovering(None, Some("   "), &ServerUrl::Prod, |_| {
                Some(discovered("https://x.aretta.ai"))
            });
        assert_eq!(server, ServerUrl::Custom("https://x.aretta.ai".into()));
        assert_eq!(source, LoginServerSource::Discovered);
    }
}
