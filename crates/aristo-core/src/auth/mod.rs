//! Auth — Aretta credential resolution + on-disk persistence + (soon)
//! the GitHub OAuth login flow.
//!
//! ## Scope
//!
//! This module is foundational SDK infrastructure: anything in aristo
//! that calls into the Aretta proxy uses it. **It is intentionally NOT
//! a submodule of any feature** (canon, verify, telemetry, ...) so
//! cross-feature consumers don't have to reach into a sibling
//! feature's namespace.
//!
//! ## Layout
//!
//! - [`token`] — the [`Token`] newtype with `Debug` redaction.
//! - [`store`] — credentials-file atomic I/O. Honors `$XDG_CONFIG_HOME`;
//!   `0600` perms on Unix.
//! - [`mod@resolve`] — env-var → the checkout's stored entry. Returns
//!   [`ResolvedCreds`] or [`AuthError`].
//! - [`error`] — [`AuthError`] variants (`NoToken`, `Invalid`,
//!   `Malformed`).
//!
//! - `server` — [`ServerUrl`] plus the login-server and data-plane
//!   resolvers.
//! - `oauth` — GitHub OAuth code-exchange against the proxy's
//!   `/auth/cli-token` endpoint.
//! - `git` — auto-derive `repo_full_name` from a workspace's
//!   `.git/config`.
//!
//! ## Test pattern
//!
//! Every public function has a `_with` injection variant for testing.
//! The workspace forbids `unsafe_code`, which `std::env::set_var`
//! requires; tests pass explicit env-var + home-dir overrides instead
//! of mutating process state.

pub mod error;
pub mod git;
pub mod oauth;
pub mod resolve;
pub mod server;
pub mod store;
pub mod token;

pub use error::{AuthError, EntrySummary};
pub use git::derive_repo_full_name;
pub use oauth::{oauth_exchange, oauth_start, CliTokenResponse, GitHubUser, OAuthInit};
pub use resolve::{
    checkout_at, cwd_checkout, resolve_full, resolve_full_for_checkout, resolve_full_with,
    ResolvedCreds, ENV_VAR,
};
pub use server::{data_plane_base, login_server, LoginServerSource, ServerUrl};
pub use store::{
    clear, clear_with, clear_with_home, config_dir, credentials_path, load_store, load_store_with,
    save, save_full, save_full_with, save_store, save_store_with, save_with, save_with_home,
    upsert_entry, upsert_entry_with, CredentialEntry, CredentialStore, CredentialsRecord,
    UpsertOutcome, UpsertReport, CREDENTIALS_FILENAME,
};
pub use token::Token;
