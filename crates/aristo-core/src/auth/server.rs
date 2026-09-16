//! Aretta server URL — `code.aretta.ai` (prod) / `Custom(<url>)`
//! (self-hosted, on-prem, or a per-org conductor).
//!
//! Lives here, in [`crate::auth`], rather than in `canon::http_client`,
//! because the server URL is a **credential property**: an `arta_*`
//! token issued by one deployment is not valid against another.
//! Whatever persists the token also persists the server it came from.
//!
//! ## Parsing user input
//!
//! The CLI's `--server <spec>` flag (and `ARETTA_API_URL`) accept a URL:
//!
//! - a string that starts with `http://` or `https://` →
//!   [`ServerUrl::Custom`] (trailing `/` stripped)
//! - any other string → [`ServerUrl::Custom`] with `https://` prefix
//!   added (so users can type `localhost:8443`).
//!
//! A server is named by its URL, one way. The empty string maps to
//! [`ServerUrl::Prod`] so a credentials entry without a `server` field
//! still reads.

/// The Aretta proxy this credential is for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ServerUrl {
    /// `https://code.aretta.ai` — production.
    #[default]
    Prod,
    /// Self-hosted or on-prem deployment. The string includes the
    /// scheme (`http://` or `https://`).
    Custom(String),
}

impl ServerUrl {
    /// Production base URL.
    pub const PROD: &'static str = "https://code.aretta.ai";

    /// Base URL as a `&str` suitable for `format!` / `Url::parse`.
    /// Returns the full scheme + host (no trailing slash).
    pub fn as_str(&self) -> &str {
        match self {
            Self::Prod => Self::PROD,
            Self::Custom(s) => s,
        }
    }

    /// Parse a user-supplied spec (from the `--server` CLI flag,
    /// `ARETTA_API_URL`, or a persisted credentials-file `server` field).
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        match trimmed {
            // Legacy credentials entries with no `server` field.
            "" => Self::Prod,
            // Already a full URL — pass through.
            other if other.starts_with("http://") || other.starts_with("https://") => {
                Self::Custom(other.trim_end_matches('/').to_string())
            }
            other => Self::Custom(format!("https://{}", other.trim_end_matches('/'))),
        }
    }
}

impl std::fmt::Display for ServerUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolve the base URL for **data-plane** requests — verify-session
/// dispatch and canon match. Two tiers, highest first:
///
/// 1. `env_override` — the `ARETTA_API_URL` env var (CI / test /
///    staging redirect). A blank/whitespace value is treated as unset
///    (matching [`login_server`]); a present value is normalized via
///    [`ServerUrl::parse`], the one way a server spec is read.
/// 2. `server` — the credential's own server: the host the `arta_*`
///    token was minted against, or `ARETTA_API_URL` itself for an env
///    token.
///
/// Kept pure — env is passed in, not read here — so it is
/// unit-testable under the workspace's `unsafe_code` ban on
/// `std::env::set_var`.
#[aristo::intent(
    "Data-plane base-URL precedence is exactly ARETTA_API_URL (env) > \
     the credential's server, and no other tier. A blank/whitespace env \
     override is treated as unset (matching the login resolver, \
     login_server) so it falls through to the credential's server \
     instead of routing to an empty base; a present env override is \
     normalized via ServerUrl::parse, the same reading every server \
     spec gets. Adding a tier, dropping the blank-as-unset guard, or \
     reading the env override differently from the login server would \
     silently misroute verify and canon-match requests to the wrong \
     Aretta deployment.",
    verify = "neural",
    id = "data_plane_base_precedence"
)]
pub fn data_plane_base(env_override: Option<&str>, server: &ServerUrl) -> String {
    match env_override.map(str::trim).filter(|s| !s.is_empty()) {
        Some(v) => ServerUrl::parse(v).as_str().to_string(),
        None => server.as_str().to_string(),
    }
}

/// Where the login (auth-plane) server URL came from. Carried alongside
/// the [`ServerUrl`] so the caller can name the provenance on the
/// "Authenticating against …" line — making a stale `ARETTA_API_URL`
/// export visible at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginServerSource {
    /// The user passed `--server` explicitly.
    Flag,
    /// Resolved from the `ARETTA_API_URL` environment variable.
    Env,
}

impl LoginServerSource {
    /// Short provenance suffix for the "Authenticating against …" line.
    pub fn provenance(self) -> &'static str {
        match self {
            Self::Flag => "from --server",
            Self::Env => "from ARETTA_API_URL",
        }
    }
}

/// Resolve the base URL for **auth-plane** login — where an `arta_*`
/// token is minted. Precedence, highest first:
///
/// 1. `flag` — an explicit `--server` value the user passed. Threaded
///    as `Option<&str>` so `None` means "unset"; parsed via
///    [`ServerUrl::parse`].
/// 2. `env_override` — the `ARETTA_API_URL` env var. A blank/whitespace
///    value is treated as unset; a present value is parsed via
///    [`ServerUrl::parse`] so full URLs and bare hosts both work, and
///    the minted token's server matches the data plane
///    ([`data_plane_base`]).
///
/// `None` when neither is given. The platform apex cannot mint an
/// org-scoped token, so the caller asks the user for the server
/// instead of guessing one.
///
/// Kept pure — env is passed in, not read here — so it is unit-testable
/// under the workspace's `unsafe_code` ban on `std::env::set_var`.
pub fn login_server(
    flag: Option<&str>,
    env_override: Option<&str>,
) -> Option<(ServerUrl, LoginServerSource)> {
    if let Some(f) = flag {
        return Some((ServerUrl::parse(f), LoginServerSource::Flag));
    }
    let v = env_override.map(str::trim).filter(|s| !s.is_empty())?;
    Some((ServerUrl::parse(v), LoginServerSource::Env))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prod_resolves_to_code_aretta_ai() {
        assert_eq!(ServerUrl::Prod.as_str(), "https://code.aretta.ai");
    }

    #[test]
    fn parse_has_no_alias() {
        // A server is named by URL. Only the empty field maps to the
        // platform default.
        assert_eq!(ServerUrl::parse(""), ServerUrl::Prod);
        assert_eq!(ServerUrl::parse("   "), ServerUrl::Prod);
        assert_eq!(
            ServerUrl::parse("prod"),
            ServerUrl::Custom("https://prod".into())
        );
        // Trims whitespace.
        assert_eq!(
            ServerUrl::parse("  x.example.com  "),
            ServerUrl::Custom("https://x.example.com".into())
        );
    }

    #[test]
    fn former_dev_aliases_are_now_plain_custom_hosts() {
        // Operator-ruled surface removal (0.6.0): dev.aretta.ai is retired,
        // so `dev` / `development` / `staging` are no longer well-known
        // aliases — they fall through to the bare-host path like any other
        // input (`dev` → `https://dev`). No special-case, no error branch.
        assert_eq!(
            ServerUrl::parse("dev"),
            ServerUrl::Custom("https://dev".into())
        );
        assert_eq!(
            ServerUrl::parse("development"),
            ServerUrl::Custom("https://development".into())
        );
        assert_eq!(
            ServerUrl::parse("staging"),
            ServerUrl::Custom("https://staging".into())
        );
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
    fn display_renders_full_url() {
        assert_eq!(format!("{}", ServerUrl::Prod), "https://code.aretta.ai");
        assert_eq!(
            format!("{}", ServerUrl::Custom("https://x.example.com".into())),
            "https://x.example.com"
        );
    }

    #[test]
    fn data_plane_base_env_override_wins_and_is_normalized() {
        // ARETTA_API_URL beats the credential's server and is read the
        // same way as every server spec.
        let s = data_plane_base(Some("ci.example.com/"), &ServerUrl::Prod);
        assert_eq!(s, "https://ci.example.com");
    }

    #[test]
    fn data_plane_base_blank_env_falls_through_to_server() {
        // An empty or whitespace ARETTA_API_URL (an unset CI Variable
        // expands to "") is unset — matching login_server.
        let custom = ServerUrl::Custom("https://staging.example.com".into());
        assert_eq!(
            data_plane_base(Some(""), &custom),
            "https://staging.example.com"
        );
        assert_eq!(
            data_plane_base(Some("   "), &custom),
            "https://staging.example.com"
        );
    }

    #[test]
    fn data_plane_base_falls_back_to_server() {
        assert_eq!(
            data_plane_base(None, &ServerUrl::Prod),
            "https://code.aretta.ai"
        );
    }

    // ─── login_server precedence ────────────────────────────────────────────

    #[test]
    fn login_server_flag_beats_env() {
        // An explicit --server always wins, even when ARETTA_API_URL is set.
        let (server, source) = login_server(
            Some("https://flag.example.com"),
            Some("https://turso.aretta.ai"),
        )
        .unwrap();
        assert_eq!(server, ServerUrl::Custom("https://flag.example.com".into()));
        assert_eq!(source, LoginServerSource::Flag);
    }

    #[test]
    fn login_server_env_when_no_flag() {
        let (server, source) = login_server(None, Some("https://turso.aretta.ai")).unwrap();
        assert_eq!(server, ServerUrl::Custom("https://turso.aretta.ai".into()));
        assert_eq!(source, LoginServerSource::Env);
    }

    #[test]
    fn login_server_env_parsed_via_serverurl_parse() {
        // The env value goes through ServerUrl::parse: bare hosts
        // (→ https://) and trailing slashes normalize.
        assert_eq!(
            login_server(None, Some("turso.aretta.ai/")).unwrap().0,
            ServerUrl::Custom("https://turso.aretta.ai".into())
        );
    }

    #[test]
    fn login_server_blank_env_is_unset() {
        // A blank/whitespace ARETTA_API_URL is treated as unset, not as an
        // empty custom server; nothing else fills in.
        assert_eq!(login_server(None, Some("   ")), None);
    }

    #[test]
    fn login_server_has_no_default() {
        assert_eq!(login_server(None, None), None);
    }

    #[test]
    fn login_server_provenance_is_always_named() {
        assert_eq!(LoginServerSource::Flag.provenance(), "from --server");
        assert_eq!(LoginServerSource::Env.provenance(), "from ARETTA_API_URL");
    }
}
