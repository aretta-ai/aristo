//! Zero-config org discovery.
//!
//! When a user runs `aristo auth login` without an explicit `--server`
//! flag or `ARETTA_API_URL` env var, the CLI asks the prod-default
//! platform where the repo's org lives:
//!
//! ```text
//! GET <platform>/.well-known/aretta-org?repo=<owner/repo>
//!   → 200 { "org": "...", "base_url": "https://<slug>.aretta.ai" }
//!   → 404 (repo is not mapped to a hosted org)
//! ```
//!
//! A 200 with a usable `base_url` redirects the whole login flow to that
//! org's conductor; anything else (404, timeout, transport error,
//! unparseable body) yields `None` and login falls back to its
//! pre-discovery behavior **exactly**. Discovery must never break login:
//! it is a best-effort convenience, so every failure mode degrades
//! silently to the default.

use std::time::Duration;

use serde::Deserialize;

use super::server::ServerUrl;

/// Short per-request timeout. Discovery is a login-latency convenience,
/// not a hard dependency — we don't make the user wait on a slow or
/// unreachable platform. A miss just falls back to the prod default.
const DISCOVERY_TIMEOUT_SECS: u64 = 3;

/// A successful zero-config org discovery result: the repo maps to a
/// hosted org served at `base_url`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DiscoveredOrg {
    /// Human-readable org slug (informational; e.g. `"tursodatabase"`).
    pub org: String,
    /// Base URL the org's conductor is served at
    /// (e.g. `"https://turso.aretta.ai"`). This becomes the login
    /// server when discovery wins the precedence.
    pub base_url: String,
}

/// Query `<platform>/.well-known/aretta-org?repo=<repo>` for the repo's
/// hosted org. Returns `Some` only on a 200 carrying a usable
/// `base_url`; every other outcome — 404, any non-2xx, a timeout, a
/// transport error, or an unparseable/empty body — returns `None` so
/// the caller falls back to its pre-discovery server unchanged.
///
/// `platform` is the server queried *before* discovery resolves — the
/// prod default in the only path that reaches here (discovery runs only
/// when neither `--server` nor `ARETTA_API_URL` is supplied).
pub fn discover_org(platform: &ServerUrl, repo_full_name: &str) -> Option<DiscoveredOrg> {
    let url = format!(
        "{}/.well-known/aretta-org?repo={}",
        platform.as_str(),
        super::oauth::url_encode(repo_full_name),
    );
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(DISCOVERY_TIMEOUT_SECS)))
        .user_agent(format!("aristo/{}", env!("CARGO_PKG_VERSION")))
        // Non-2xx comes back as Ok(Response) so the (status, body)
        // dispatch lives in one pure place.
        .http_status_as_error(false)
        .build();
    let agent: ureq::Agent = config.into();
    let response = agent.get(&url).call().ok()?;
    let status = response.status().as_u16();
    let body = read_body_capped(response, 16 * 1024);
    map_discovery_response(status, &body)
}

/// Pure: turn a `(status, body)` pair into an optional discovery
/// result. Split out from the transport so the fallback contract is
/// unit-testable without a network.
///
/// `Some` iff the status is 2xx, the body decodes as `DiscoveredOrg`,
/// and `base_url` is non-empty. Everything else is `None`.
pub(crate) fn map_discovery_response(status: u16, body: &str) -> Option<DiscoveredOrg> {
    if !(200..=299).contains(&status) {
        return None;
    }
    let org: DiscoveredOrg = serde_json::from_str(body).ok()?;
    if org.base_url.trim().is_empty() {
        return None;
    }
    Some(org)
}

fn read_body_capped(response: ureq::http::Response<ureq::Body>, cap: usize) -> String {
    use std::io::Read;
    let mut reader = response.into_body().into_reader();
    let mut buf = Vec::with_capacity(4 * 1024);
    let mut tmp = [0u8; 4 * 1024];
    while buf.len() < cap {
        match reader.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                let take = (cap - buf.len()).min(n);
                buf.extend_from_slice(&tmp[..take]);
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_200_with_valid_body_yields_discovered_org() {
        let body = r#"{"org":"tursodatabase","base_url":"https://turso.aretta.ai"}"#;
        let got = map_discovery_response(200, body).expect("should discover");
        assert_eq!(got.org, "tursodatabase");
        assert_eq!(got.base_url, "https://turso.aretta.ai");
    }

    #[test]
    fn map_404_yields_none() {
        // Repo is not mapped to a hosted org — fall back to prod.
        assert!(map_discovery_response(404, r#"{"error":"not found"}"#).is_none());
    }

    #[test]
    fn map_non_2xx_yields_none() {
        assert!(map_discovery_response(500, "boom").is_none());
        assert!(map_discovery_response(301, "").is_none());
    }

    #[test]
    fn map_2xx_unparseable_body_yields_none() {
        // A 200 with garbage must degrade gracefully, not error.
        assert!(map_discovery_response(200, "not json").is_none());
    }

    #[test]
    fn map_2xx_empty_base_url_yields_none() {
        // A structurally-valid body with no usable base_url is a miss.
        let body = r#"{"org":"x","base_url":"   "}"#;
        assert!(map_discovery_response(200, body).is_none());
    }

    #[test]
    fn discover_org_against_closed_port_returns_none() {
        // Graceful: a refused connection (nothing listening) must fall
        // back to None, never panic or error. Bind then drop to get a
        // definitely-closed port.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let platform = ServerUrl::Custom(format!("http://{addr}"));
        assert!(discover_org(&platform, "owner/repo").is_none());
    }
}
