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
}
