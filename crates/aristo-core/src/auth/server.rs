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
}
