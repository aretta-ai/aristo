//! GitHub OAuth login flow against the Aretta proxy's auth endpoints.
//!
//! ## Two-step flow
//!
//! 1. **Start.** [`oauth_start`] calls `GET <server>/auth/login` (no
//!    redirect_uri — the proxy uses its own `/auth/callback` page,
//!    which displays the OAuth code for the user to copy + paste).
//!    Returns the GitHub authorization URL.
//!
//! 2. **Exchange.** The user authorizes on GitHub, lands on the
//!    proxy's `/auth/callback`, copies the displayed code, and pastes
//!    it back to the CLI. [`oauth_exchange`] then POSTs the code to
//!    `POST <server>/auth/cli-token` along with the user's
//!    `repo_full_name`. The proxy does OAuth code exchange against
//!    GitHub, JWT mint (for dashboard access), and `arta_*` token
//!    mint scoped to `(user_id, repo_full_name)`. Returns
//!    [`CliTokenResponse`] with the raw `arta_token` (shown ONCE per
//!    the proxy's D-12 / Pitfall-8 — the SDK persists it).
//!
//! See `aretta-code/packages/proxy/src/routes.ts:251` for the
//! `/auth/cli-token` route source, and the addendum at
//! `docs/mockups/13-canon-and-matching/PLAN-auth-extraction-and-oauth.md`
//! §3 for the architecture decisions.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use ureq::http::Response as HttpResponse;

use super::error::AuthError;
use super::server::ServerUrl;

/// Per-request timeout — same value as `canon::http_client`.
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Parsed response from `GET /auth/login` — the GitHub authorization
/// URL the user must visit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OAuthInit {
    /// The full `https://github.com/login/oauth/authorize?...` URL.
    /// Open this in a browser (or print it for the user to copy).
    #[serde(rename = "url")]
    pub authorize_url: String,
}

/// Body sent to `POST /auth/cli-token`.
#[derive(Debug, Clone, Serialize)]
struct CliTokenRequest<'a> {
    code: &'a str,
    #[serde(rename = "repoFullName")]
    repo_full_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
}

/// Parsed response from `POST /auth/cli-token`.
#[derive(Debug, Clone, Deserialize)]
pub struct CliTokenResponse {
    /// Raw `arta_*` token. Shown to the caller **once** per the
    /// proxy's D-12 / Pitfall-8; the SDK persists it via
    /// [`super::store::save`] (or the extended TOML save in
    /// commit 4 of the plan addendum).
    pub arta_token: String,
    /// JWT for any `/dashboard/api/*` calls the SDK might want.
    /// Optional — the SDK doesn't need it for canon API access (the
    /// arta_token is the primary credential). `serde(default)` so a
    /// conductor that drops this SDK-unused field can't break login.
    #[serde(default)]
    pub jwt: String,
    /// GitHub user identity.
    pub user: GitHubUser,
    /// Internal opaque id for the minted token, useful for later
    /// revocation via the dashboard. `serde(default)` — SDK-unused, so a
    /// dropped field must not break login.
    #[serde(default)]
    pub token_id: String,
    /// Lowercased `owner/repo` (the proxy normalizes case).
    pub repo_full_name: String,
    /// Last 4 chars of the token, for masked display in `aristo
    /// auth status`. `serde(default)` — SDK-unused, so a dropped field
    /// must not break login.
    #[serde(default)]
    pub last_4: String,
}

/// GitHub user identity returned by the proxy.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitHubUser {
    /// Numeric GitHub user id (immutable across renames).
    pub id: u64,
    /// GitHub login / username (can change if the user renames; for
    /// display only — pair with `id` for stable identity).
    pub login: String,
}

/// Start the OAuth flow: call `GET <server>/auth/login` and return
/// the GitHub authorization URL.
///
/// **Why we pass `redirect_uri` explicitly:** the proxy's
/// `/auth/login` handler falls back to deriving the redirect URI
/// from its own incoming `c.req.url`. Behind a reverse proxy
/// (Caddy terminates TLS at the edge, talks plain HTTP to the
/// upstream proxy), `c.req.url` has the **internal** `http://`
/// scheme. The OAuth App at GitHub has only the public `https://`
/// callback registered, so the derived redirect URI gets rejected
/// with "redirect_uri is not associated with this application."
/// Passing `redirect_uri=<server>/auth/callback` with the proper
/// `https://` scheme bypasses the fallback and matches GitHub's
/// allowlist. Same fix that `aretta-code/packages/tui/src/hooks/useAuth.ts`
/// applies.
///
/// The caller is responsible for opening the returned URL in a
/// browser (or printing it for the user) and prompting for the code
/// that the proxy's `/auth/callback` page displays.
pub fn oauth_start(server: &ServerUrl) -> Result<OAuthInit, AuthError> {
    let redirect_uri = format!("{}/auth/callback", server.as_str());
    let url = format!(
        "{}/auth/login?redirect_uri={}",
        server.as_str(),
        url_encode(&redirect_uri),
    );
    let agent = build_agent();
    let result = agent.get(&url).call();
    let body = consume_response::<OAuthInit>(result)?;
    Ok(body)
}

/// Minimal URL-encoder for query-param values. Hand-rolled instead
/// of pulling a dep — only encodes the reserved chars we actually
/// emit (`:` `/`). Shared with [`super::discovery`], which encodes the
/// `repo` query param the same way.
pub(crate) fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Exchange an OAuth `code` (returned by the proxy's `/auth/callback`
/// page) for an `arta_*` token scoped to `(github_user, repo_full_name)`.
///
/// The proxy's `name` parameter defaults to `"aristo-cli"` if `None`.
pub fn oauth_exchange(
    server: &ServerUrl,
    code: &str,
    repo_full_name: &str,
    name: Option<&str>,
) -> Result<CliTokenResponse, AuthError> {
    let url = format!("{}/auth/cli-token", server.as_str());
    let req = CliTokenRequest {
        code,
        repo_full_name,
        name,
    };
    let agent = build_agent();
    let result = agent
        .post(&url)
        .header("Content-Type", "application/json")
        .send_json(&req);
    consume_response::<CliTokenResponse>(result)
}

// ─── transport ─────────────────────────────────────────────────────────────

fn build_agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
        .user_agent(format!("aristo/{}", env!("CARGO_PKG_VERSION")))
        // Same convention as canon::http_client — non-2xx responses
        // come back as Ok(Response) so the status dispatch lives in
        // one place (consume_response).
        .http_status_as_error(false)
        .build();
    config.into()
}

/// Drive a ureq result through a JSON-body decode + status-code
/// dispatch into an [`AuthError`].
fn consume_response<T>(
    result: Result<HttpResponse<ureq::Body>, ureq::Error>,
) -> Result<T, AuthError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let response = match result {
        Ok(r) => r,
        Err(e) => return Err(transport_error_to_auth(e)),
    };
    let status = response.status().as_u16();
    let body_text = read_body_capped(response, 64 * 1024);
    map_response(status, &body_text)
}

/// Pure: turn a (status, body) pair into either a decoded T or an
/// AuthError. Split out for unit testing.
pub(crate) fn map_response<T>(status: u16, body: &str) -> Result<T, AuthError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    match status {
        200..=299 => serde_json::from_str::<T>(body).map_err(|e| {
            AuthError::Malformed(format!("proxy returned 2xx with unparseable body: {e}"))
        }),
        401 | 403 => Err(AuthError::Invalid),
        // 410 Gone: the retired platform default (e.g. code.aretta.ai)
        // returns this once it stops minting CLI tokens. Say so plainly and
        // point at the fix, while still surfacing whatever the server sent.
        410 => Err(AuthError::Malformed(format!(
            "this server no longer mints CLI tokens (HTTP 410 Gone) — your CLI \
             is pointed at the retired platform default. Re-run against your \
             org's server (`--server <url>` or `ARETTA_API_URL=<url>`), or \
             upgrade aristo. Server said: {}",
            extract_error_message(body).unwrap_or_else(|| truncate(body, 200))
        ))),
        400..=499 => {
            // Try to surface the proxy's error field if it's there.
            let msg = extract_error_message(body)
                .unwrap_or_else(|| format!("HTTP {status}: {}", truncate(body, 200)));
            Err(AuthError::Malformed(msg))
        }
        500..=599 => Err(AuthError::Malformed(format!(
            "proxy HTTP {status}: {}",
            extract_error_message(body).unwrap_or_else(|| truncate(body, 200))
        ))),
        other => Err(AuthError::Malformed(format!(
            "unexpected HTTP {other} from proxy"
        ))),
    }
}

fn extract_error_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("error")
        .and_then(|e| e.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            v.get("message")
                .and_then(|e| e.as_str())
                .map(|s| s.to_string())
        })
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

fn read_body_capped(response: HttpResponse<ureq::Body>, cap: usize) -> String {
    use std::io::Read;
    let mut reader = response.into_body().into_reader();
    let mut buf = Vec::with_capacity(8 * 1024);
    let mut tmp = [0u8; 8 * 1024];
    while buf.len() < cap {
        match reader.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                let to_take = (cap - buf.len()).min(n);
                buf.extend_from_slice(&tmp[..to_take]);
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn transport_error_to_auth(e: ureq::Error) -> AuthError {
    let s = e.to_string();
    if s.contains("timed out") || s.contains("timeout") {
        AuthError::Malformed(format!("proxy request timed out: {s}"))
    } else {
        AuthError::Malformed(format!("proxy transport error: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure response-mapping tests — no network. The end-to-end
    // HTTP coverage uses a TcpListener mock (see
    // `crates/aristo-cli/tests/auth_oauth_command.rs`).

    #[test]
    fn map_response_2xx_decodes_oauth_init() {
        let body =
            r#"{"url":"https://github.com/login/oauth/authorize?client_id=x&redirect_uri=y"}"#;
        let r: OAuthInit = map_response(200, body).expect("decode");
        assert_eq!(
            r.authorize_url,
            "https://github.com/login/oauth/authorize?client_id=x&redirect_uri=y"
        );
    }

    #[test]
    fn map_response_2xx_decodes_cli_token_response() {
        let body = r#"{
            "arta_token": "arta_xyz",
            "jwt": "jwt-blob",
            "user": { "id": 42, "login": "octocat" },
            "token_id": "tk_001",
            "repo_full_name": "owner/repo",
            "last_4": "wxyz"
        }"#;
        let r: CliTokenResponse = map_response(200, body).expect("decode");
        assert_eq!(r.arta_token, "arta_xyz");
        assert_eq!(r.user.login, "octocat");
        assert_eq!(r.user.id, 42);
        assert_eq!(r.last_4, "wxyz");
    }

    #[test]
    fn map_response_cli_token_tolerates_missing_sdk_unused_fields() {
        // A conductor that drops the SDK-unused fields (jwt / token_id /
        // last_4) must not break login — only arta_token / user /
        // repo_full_name are required.
        let body = r#"{
            "arta_token": "arta_min",
            "user": { "id": 7, "login": "min" },
            "repo_full_name": "owner/repo"
        }"#;
        let r: CliTokenResponse = map_response(200, body).expect("decode without optional fields");
        assert_eq!(r.arta_token, "arta_min");
        assert_eq!(r.repo_full_name, "owner/repo");
        assert_eq!(r.jwt, "");
        assert_eq!(r.token_id, "");
        assert_eq!(r.last_4, "");
    }

    #[test]
    fn map_response_401_maps_to_invalid() {
        let r: Result<OAuthInit, _> = map_response(401, r#"{"error":"bad token"}"#);
        assert_eq!(r.unwrap_err(), AuthError::Invalid);
    }

    #[test]
    fn map_response_403_maps_to_invalid() {
        let r: Result<OAuthInit, _> = map_response(403, r#"{"error":"forbidden"}"#);
        assert_eq!(r.unwrap_err(), AuthError::Invalid);
    }

    #[test]
    fn map_response_410_gone_gives_retired_platform_message() {
        let r: Result<CliTokenResponse, _> = map_response(
            410,
            r#"{"error":"CLI token minting disabled on this host"}"#,
        );
        match r.unwrap_err() {
            AuthError::Malformed(m) => {
                assert!(m.contains("no longer mints CLI tokens"), "got: {m}");
                assert!(m.contains("410"), "got: {m}");
                assert!(m.contains("retired platform default"), "got: {m}");
                // The re-run guidance and the server's own message both survive.
                assert!(m.contains("ARETTA_API_URL"), "got: {m}");
                assert!(
                    m.contains("CLI token minting disabled on this host"),
                    "got: {m}"
                );
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn map_response_400_extracts_error_field() {
        let r: Result<CliTokenResponse, _> = map_response(400, r#"{"error":"Missing code"}"#);
        match r.unwrap_err() {
            AuthError::Malformed(m) => assert!(m.contains("Missing code"), "got: {m}"),
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn map_response_400_falls_back_to_message_field() {
        let r: Result<CliTokenResponse, _> = map_response(400, r#"{"message":"bad shape"}"#);
        match r.unwrap_err() {
            AuthError::Malformed(m) => assert!(m.contains("bad shape"), "got: {m}"),
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn map_response_400_with_non_json_body_uses_truncated_body() {
        let r: Result<CliTokenResponse, _> = map_response(400, "plain text error");
        match r.unwrap_err() {
            AuthError::Malformed(m) => {
                assert!(m.contains("400"), "got: {m}");
                assert!(m.contains("plain text error"), "got: {m}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn map_response_500_maps_to_malformed_with_proxy_label() {
        let r: Result<CliTokenResponse, _> = map_response(500, r#"{"error":"oauth failed"}"#);
        match r.unwrap_err() {
            AuthError::Malformed(m) => {
                assert!(m.contains("500"), "got: {m}");
                assert!(m.contains("oauth failed"), "got: {m}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn map_response_2xx_garbage_body_surfaces_malformed() {
        let r: Result<OAuthInit, _> = map_response(200, "not json");
        match r.unwrap_err() {
            AuthError::Malformed(m) => assert!(m.contains("unparseable"), "got: {m}"),
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn truncate_short_string_passes_through() {
        assert_eq!(truncate("short", 100), "short");
    }

    #[test]
    fn truncate_long_string_clips_with_ellipsis() {
        let s = "a".repeat(500);
        let t = truncate(&s, 10);
        assert_eq!(t.chars().count(), 11); // 10 + ellipsis
        assert!(t.ends_with('…'));
    }
}
