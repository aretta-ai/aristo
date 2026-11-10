//! The conductor routes the CLI calls, generated from the spec's route registry.
//!
//! [`paths`] is GENERATED from `CampanileSkyForge/aretta-registry` tag **v0.2.240**
//! (commit `fb19fba80dfe30ed571eaa6f221d094dad927ac0`, spec root `data/`), vendored at
//! `spec/v0.2.240/` in this repo: the registry rows with `callers: cli`. Never edit it
//! by hand: change the registry, re-vendor it, and regenerate with
//! `scripts/check-conductor-paths.sh --write`. CI runs the same script without
//! `--write` and fails on any difference.
//!
//! A path is the conductor's axum template (`/:repo/api/client/v1/verify/sessions/:id`);
//! [`Route::fill`] puts the values in.

pub mod paths;

pub use paths::Route;

impl Route {
    /// The path with every `:name` segment replaced by its value from
    /// `params`. Values go in as given: a caller encodes them first.
    ///
    /// # Panics
    ///
    /// When the template names a parameter `params` does not hold — a
    /// programming error a unit test catches, never a runtime input.
    pub fn fill(&self, params: &[(&str, &str)]) -> String {
        self.path
            .split('/')
            .map(|segment| match segment.strip_prefix(':') {
                Some(name) => params
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| *v)
                    .unwrap_or_else(|| panic!("route {} needs :{name}", self.id)),
                None => segment,
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

#[cfg(test)]
mod tests {
    use super::paths::*;
    use sha2::{Digest, Sha256};
    use std::path::Path;

    #[test]
    fn fill_replaces_each_parameter_segment() {
        assert_eq!(
            VERIFY_SESSIONS_CANCEL.fill(&[("repo", "widgets"), ("id", "01HN")]),
            "/widgets/api/client/v1/verify/sessions/01HN/cancel"
        );
        assert_eq!(IDENTITY_REPOS_LIST.fill(&[]), "/api/client/v1/org/repos");
    }

    #[test]
    #[should_panic(expected = "route verify.sessions.get needs :id")]
    fn fill_refuses_a_missing_parameter() {
        VERIFY_SESSIONS_GET.fill(&[("repo", "widgets")]);
    }

    /// The (method, path) set the CLI calls: eight of the routes of
    /// `callers: cli` at v0.2.240. A registry change that adds, drops or
    /// moves one shows up here as well as in the drift check.
    #[test]
    fn the_cli_calls_the_eight_generated_routes() {
        let called: Vec<(&str, &str)> = [
            PICTURE_CANON_MATCH,
            PICTURE_CATALOGUE_GET,
            VERIFY_SESSIONS_START,
            VERIFY_SESSIONS_GET,
            VERIFY_SESSIONS_CANCEL,
            IDENTITY_SESSION_LOGIN,
            IDENTITY_CLI_TOKEN_MINT,
            IDENTITY_REPOS_LIST,
        ]
        .iter()
        .map(|r| (r.method, r.path))
        .collect();
        assert_eq!(
            called,
            [
                ("POST", "/:repo/api/client/v1/canon/match"),
                ("GET", "/:repo/api/client/v1/catalogue"),
                ("POST", "/:repo/api/client/v1/verify/sessions"),
                ("GET", "/:repo/api/client/v1/verify/sessions/:id"),
                ("POST", "/:repo/api/client/v1/verify/sessions/:id/cancel"),
                ("GET", "/auth/login"),
                ("POST", "/auth/cli-token"),
                ("GET", "/api/client/v1/org/repos"),
            ]
        );
    }

    /// The vendored registry version: `spec/<SPEC>/` and `spec/<SPEC>.sha256`.
    const SPEC: &str = "v0.2.240";

    /// `spec/<SPEC>/VERSION` names the vendored directory's version.
    #[test]
    fn the_vendored_version_is_the_directorys() {
        let spec = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec");
        let version = std::fs::read_to_string(spec.join(SPEC).join("VERSION")).unwrap();
        assert_eq!(format!("v{}", version.trim()), SPEC);
    }

    /// `spec/v0.2.240/` is the tag's files byte for byte: each one's
    /// sha256 equals its line in `spec/v0.2.240.sha256`.
    #[test]
    fn the_vendored_spec_matches_its_sha256_list() {
        let spec = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec");
        let list = std::fs::read_to_string(spec.join(format!("{SPEC}.sha256"))).unwrap();
        let mut n = 0;
        for line in list.lines() {
            let (want, file) = line.split_once("  ").unwrap();
            let bytes = std::fs::read(spec.join(SPEC).join(file)).unwrap();
            let got: String = Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(got, want, "spec/{SPEC}/{file} differs from its lock");
            n += 1;
        }
        assert_eq!(
            n, 8,
            "the lock lists the registry, VERSION and the generator's six files"
        );
    }
}
