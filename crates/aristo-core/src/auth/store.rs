//! Credentials-file atomic I/O. The on-disk store for the token.
//!
//! ## File location
//!
//! Honors `$XDG_CONFIG_HOME` when set (Linux convention); else falls
//! back to a per-OS default:
//!
//! - Linux:   `~/.config/aristo/credentials`
//! - macOS:   `~/Library/Application Support/aristo/credentials`
//!
//! On Unix, the file is created with mode `0600` (owner-only).
//!
//! ## Atomic writes
//!
//! `save_with` writes to `<path>.tmp` then `rename`s. Either the new
//! file exists in full, or the old one is untouched — no half-written
//! credentials.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::AuthError;
use super::token::Token;

/// Filename inside the per-user config directory.
pub const CREDENTIALS_FILENAME: &str = "credentials";

/// Persist a token to the credentials file. Reads `$XDG_CONFIG_HOME`
/// and `$HOME` from the process env to determine the destination
/// path — same precedence as [`super::resolve::resolve_full`] so that
/// `aristo auth login` and the next API call agree on which file to
/// touch.
pub fn save(token: &Token) -> io::Result<()> {
    save_with(
        token,
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Persist with explicit home-dir override (no XDG override). Used by
/// tests that pin behavior to a particular `$HOME` without touching
/// `$XDG_CONFIG_HOME`.
pub fn save_with_home(token: &Token, home_override: Option<&Path>) -> io::Result<()> {
    save_with(token, None, home_override)
}

/// Persist a bare token with full env-var + home-dir overrides. Upserts
/// a single entry keyed by the platform server — callers that know the
/// server persist a full record via [`save_full_with`] instead.
pub fn save_with(
    token: &Token,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> io::Result<()> {
    upsert_entry_with(
        CredentialEntry::bare(token.clone(), super::server::ServerUrl::Prod),
        xdg_config_home,
        home_override,
    )
    .map(|_| ())
}

/// Remove the credentials file, if it exists. Idempotent — missing
/// file is not an error.
pub fn clear() -> io::Result<()> {
    clear_with(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Clear with an explicit home-dir override (no XDG override).
pub fn clear_with_home(home_override: Option<&Path>) -> io::Result<()> {
    clear_with(None, home_override)
}

/// Clear with explicit env-var + home-dir overrides.
pub fn clear_with(xdg_config_home: Option<&str>, home_override: Option<&Path>) -> io::Result<()> {
    let path = credentials_path_with(xdg_config_home, home_override).map_err(io_from_auth_error)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Absolute path to the credentials file (does not check existence).
pub fn credentials_path() -> Result<PathBuf, AuthError> {
    credentials_path_with(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

pub(super) fn credentials_path_with(
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> Result<PathBuf, AuthError> {
    let dir = config_dir_with(xdg_config_home, home_override)?;
    Ok(dir.join(CREDENTIALS_FILENAME))
}

/// Aristo's config directory: `$XDG_CONFIG_HOME/aristo` (or
/// `~/.config/aristo`) on Linux; `~/Library/Application Support/aristo`
/// on macOS.
pub fn config_dir() -> Result<PathBuf, AuthError> {
    config_dir_with(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

fn config_dir_with(
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> Result<PathBuf, AuthError> {
    if let Some(xdg) = xdg_config_home {
        let xdg = xdg.trim();
        if !xdg.is_empty() {
            return Ok(PathBuf::from(xdg).join("aristo"));
        }
    }
    let home = home_override.ok_or_else(|| {
        AuthError::Malformed("could not determine $HOME for credentials file".into())
    })?;
    if cfg!(target_os = "macos") {
        Ok(home.join("Library/Application Support/aristo"))
    } else {
        Ok(home.join(".config/aristo"))
    }
}

pub(super) fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn io_from_auth_error(e: AuthError) -> io::Error {
    io::Error::other(e.to_string())
}

#[cfg(unix)]
fn set_unix_owner_only(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)
}

fn now_iso8601() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

// ─── on-disk schema ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct CredentialsFile {
    pub(super) aretta: AretaCredentials,
}

/// The v1 single-slot record: the raw `token`, the server it was minted
/// against, and the GitHub user identity. Read-only today; every
/// field but `token` is optional so the oldest files still parse.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct AretaCredentials {
    pub(super) token: String,
    #[serde(default)]
    #[allow(dead_code)] // Persisted for audit + future expiry checks.
    pub(super) issued_at: String,
    /// Aretta proxy this token was minted against
    /// (e.g. `"https://code.aretta.ai"`). Optional for back-compat —
    /// missing → assume production.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) server: Option<String>,
    /// GitHub login at mint time. Display-only — pair with
    /// `user_id` for stable identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) user_login: Option<String>,
    /// Numeric GitHub user id at mint time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) user_id: Option<u64>,
    /// Present in files written before tokens were org-scoped; ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    pub(super) repo: Option<String>,
}

/// Full credentials record carried by the OAuth login flow into
/// [`save_full_with`]. Mirrors the fields persisted on disk.
#[derive(Debug, Clone)]
pub struct CredentialsRecord {
    pub token: Token,
    pub server: super::server::ServerUrl,
    pub user_login: Option<String>,
    pub user_id: Option<u64>,
}

/// Persist a full credentials record (token + server + user). Upserts
/// the entry for its server (see [`CredentialStore::upsert`]),
/// migrating any older file and preserving other servers' entries. Reads env vars
/// for path resolution; see [`save_full_with`] for the
/// explicit-overrides variant used by tests. Returns what happened and
/// the store as saved, so a caller can tell the user.
pub fn save_full(creds: &CredentialsRecord) -> io::Result<UpsertReport> {
    upsert_entry(creds.into())
}

/// Persist a full credentials record with explicit path overrides.
pub fn save_full_with(
    creds: &CredentialsRecord,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> io::Result<UpsertReport> {
    upsert_entry_with(creds.into(), xdg_config_home, home_override)
}

// ─── the keyed store ─────────────────────────────────────────────────────────
//
// One entry per server: an `arta_*` token is an org grant, minted at the
// org's server, valid for every repo the org admits. Older files (v1
// single slot, v2 keyed by server + repo, bare token) still read; the
// next write persists the current format.

/// Current on-disk format version. v1 was the single-slot `[aretta]`
/// table; v2 keyed entries by server + repo; v3 keys by server.
const STORE_VERSION: u32 = 3;

/// Header comment prepended to the store file. Documents the downgrade
/// caveat for an older CLI.
const STORE_HEADER: &str = "\
# Aristo credentials store (v3, one entry per server).
#
# Managed by `aristo auth`; a login at a server replaces its entry. Tokens
# are secrets — on Unix this file is created 0600 (owner-only).
#
# DOWNGRADE CAVEAT: aristo < 0.8 does not read this format (it errors as
# malformed). After upgrading, sign in again
# (`aristo auth login --server https://<org>.aretta.ai`) or run
# `aristo auth logout --all` to reset.
";

/// One credential entry, keyed by its server (the org's host).
#[derive(Debug, Clone)]
pub struct CredentialEntry {
    /// Aretta server this token was minted against.
    pub server: super::server::ServerUrl,
    /// The `arta_*` token.
    pub token: Token,
    /// RFC-3339 timestamp when this entry was minted / written.
    pub minted_at: String,
    /// GitHub login at mint time (display-only).
    pub user_login: Option<String>,
    /// Numeric GitHub user id at mint time.
    pub user_id: Option<u64>,
}

impl CredentialEntry {
    /// A minimal entry for a raw token with no user identity. Stamps
    /// `minted_at` now.
    pub fn bare(token: Token, server: super::server::ServerUrl) -> Self {
        CredentialEntry {
            server,
            token,
            minted_at: now_iso8601(),
            user_login: None,
            user_id: None,
        }
    }

    /// The printable, token-free view of this entry.
    pub fn summary(&self) -> super::error::EntrySummary {
        super::error::EntrySummary {
            server: self.server.as_str().to_string(),
            user_login: self.user_login.clone(),
        }
    }
}

impl From<&CredentialsRecord> for CredentialEntry {
    fn from(r: &CredentialsRecord) -> Self {
        CredentialEntry {
            server: r.server.clone(),
            token: r.token.clone(),
            minted_at: now_iso8601(),
            user_login: r.user_login.clone(),
            user_id: r.user_id,
        }
    }
}

/// The whole keyed credential store: zero or more entries.
#[derive(Debug, Clone, Default)]
pub struct CredentialStore {
    /// Entries in file order. Keyed via [`upsert`].
    ///
    /// [`upsert`]: CredentialStore::upsert
    pub entries: Vec<CredentialEntry>,
}

/// What [`CredentialStore::upsert`] did with the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    /// No entry for that server; the entry was appended.
    Added,
    /// The server's entry was replaced.
    Replaced,
}

/// What a persisting upsert did, plus the store exactly as saved — so
/// a caller can report the entry count without a second read.
#[derive(Debug, Clone)]
pub struct UpsertReport {
    pub outcome: UpsertOutcome,
    pub store: CredentialStore,
}

impl CredentialStore {
    /// No entries stored.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The entry a resolver picks: the one for `server` when a server
    /// was named, else the single entry when exactly one is on file,
    /// else nothing. An org token is valid from any directory, so with
    /// one server on file there is nothing to choose; with several, the
    /// caller must name one. This is THE selection rule — `resolve_full`
    /// and every "which server will this use?" answer go through it.
    pub fn resolve_for(
        &self,
        server: Option<&super::server::ServerUrl>,
    ) -> Option<&CredentialEntry> {
        match server {
            Some(s) => self.find_by_server(s),
            None => match &self.entries[..] {
                [only] => Some(only),
                _ => None,
            },
        }
    }

    /// The entry minted at `server`.
    pub fn find_by_server(&self, server: &super::server::ServerUrl) -> Option<&CredentialEntry> {
        self.entries.iter().find(|e| &e.server == server)
    }

    /// Insert `entry`, replacing the entry for the same server: a server
    /// holds one org grant per person, so a re-login never accumulates.
    pub fn upsert(&mut self, entry: CredentialEntry) -> UpsertOutcome {
        let before = self.entries.len();
        self.entries.retain(|e| e.server != entry.server);
        let outcome = if self.entries.len() < before {
            UpsertOutcome::Replaced
        } else {
            UpsertOutcome::Added
        };
        self.entries.push(entry);
        outcome
    }

    /// Remove the entry for `server`. Returns whether one was removed.
    pub fn remove_by_server(&mut self, server: &super::server::ServerUrl) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| &e.server != server);
        self.entries.len() < before
    }
}

// ─── v2 on-disk schema ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct StoreFileVersioned {
    /// Format version — its presence distinguishes a keyed file from a
    /// v1 `[aretta]` file (which has no `version` key).
    version: u32,
    #[serde(default)]
    entries: Vec<EntryToml>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntryToml {
    server: String,
    /// Written by v2 files (per-repo tokens); read and ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[allow(dead_code)]
    repo: Option<String>,
    token: String,
    #[serde(default)]
    minted_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_login: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_id: Option<u64>,
}

/// Load the credential store, reading env vars for the path. Reads the
/// current format and the older ones (v2 keyed by server + repo, v1
/// `[aretta]` single slot, bare token) transparently; the next write
/// persists the current format. A missing file yields an empty store.
pub fn load_store() -> Result<CredentialStore, AuthError> {
    load_store_with(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Load the store with explicit path overrides.
pub fn load_store_with(
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> Result<CredentialStore, AuthError> {
    let path = credentials_path_with(xdg_config_home, home_override)?;
    if !path.exists() {
        return Ok(CredentialStore::default());
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| AuthError::Malformed(format!("read {}: {e}", path.display())))?;
    parse_store(&raw, &path)
}

/// Pure: parse the credentials file text into a store. Tries the keyed
/// formats (v3, v2), then a v1 `[aretta]` table, then a bare token. A
/// v2 file may hold several entries for one server (one per repo);
/// the newest wins so the store holds one per server.
fn parse_store(raw: &str, path: &Path) -> Result<CredentialStore, AuthError> {
    if let Ok(keyed) = toml::from_str::<StoreFileVersioned>(raw) {
        if keyed.version > STORE_VERSION || keyed.version < 2 {
            return Err(AuthError::Malformed(format!(
                "credentials file at {} is format version {} — upgrade aristo to read it",
                path.display(),
                keyed.version
            )));
        }
        let mut store = CredentialStore::default();
        let mut entries = keyed
            .entries
            .into_iter()
            .map(|t| entry_from_toml(t, path))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by(|a, b| a.minted_at.cmp(&b.minted_at));
        for e in entries {
            store.upsert(e);
        }
        return Ok(store);
    }
    // v1 — single `[aretta]` table. Migrated to one keyed entry.
    if let Ok(v1) = toml::from_str::<CredentialsFile>(raw) {
        let token = v1.aretta.token.trim();
        if token.is_empty() {
            return Err(AuthError::Malformed(format!(
                "credentials file at {} has an empty token",
                path.display()
            )));
        }
        let entry = CredentialEntry {
            server: v1
                .aretta
                .server
                .as_deref()
                .map(super::server::ServerUrl::parse)
                .unwrap_or_default(),
            token: Token::new(token),
            minted_at: v1.aretta.issued_at.clone(),
            user_login: v1.aretta.user_login.clone(),
            user_id: v1.aretta.user_id,
        };
        return Ok(CredentialStore {
            entries: vec![entry],
        });
    }
    // Bare token (a single non-TOML line).
    let token = raw.trim();
    if !token.is_empty() && !token.contains('=') && !token.contains('[') {
        return Ok(CredentialStore {
            entries: vec![CredentialEntry::bare_no_stamp(Token::new(token))],
        });
    }
    Err(AuthError::Malformed(format!(
        "credentials file at {} is not parseable",
        path.display()
    )))
}

fn entry_from_toml(t: EntryToml, path: &Path) -> Result<CredentialEntry, AuthError> {
    let token = t.token.trim();
    if token.is_empty() {
        return Err(AuthError::Malformed(format!(
            "credentials file at {} has an entry with an empty token",
            path.display()
        )));
    }
    Ok(CredentialEntry {
        server: super::server::ServerUrl::parse(&t.server),
        token: Token::new(token),
        minted_at: t.minted_at,
        user_login: t.user_login,
        user_id: t.user_id,
    })
}

fn entry_to_toml(e: &CredentialEntry) -> EntryToml {
    EntryToml {
        server: e.server.as_str().to_string(),
        repo: None,
        token: e.token.as_str().to_string(),
        minted_at: e.minted_at.clone(),
        user_login: e.user_login.clone(),
        user_id: e.user_id,
    }
}

/// Persist the whole store (versioned + header comment), reading env
/// vars for the path. Atomic write, `0600` on Unix.
pub fn save_store(store: &CredentialStore) -> io::Result<()> {
    save_store_with(
        store,
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Persist the store with explicit path overrides.
pub fn save_store_with(
    store: &CredentialStore,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> io::Result<()> {
    let path = credentials_path_with(xdg_config_home, home_override).map_err(io_from_auth_error)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = StoreFileVersioned {
        version: STORE_VERSION,
        entries: store.entries.iter().map(entry_to_toml).collect(),
    };
    let body = toml::to_string_pretty(&file)
        .map_err(|e| io::Error::other(format!("serialize credentials: {e}")))?;
    let text = format!("{STORE_HEADER}\n{body}");
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text.as_bytes())?;
    #[cfg(unix)]
    set_unix_owner_only(&tmp)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Insert-or-replace one entry, migrating any older file to the current
/// format and preserving every other server's entry. Reads env vars
/// for the path.
pub fn upsert_entry(entry: CredentialEntry) -> io::Result<UpsertReport> {
    upsert_entry_with(
        entry,
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        home_dir().as_deref(),
    )
}

/// Upsert with explicit path overrides.
pub fn upsert_entry_with(
    entry: CredentialEntry,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> io::Result<UpsertReport> {
    // Load first so we migrate (not clobber) an existing file; a corrupt
    // file surfaces as an error rather than silent data loss.
    let mut store = load_store_with(xdg_config_home, home_override).map_err(io_from_auth_error)?;
    let outcome = store.upsert(entry);
    save_store_with(&store, xdg_config_home, home_override)?;
    Ok(UpsertReport { outcome, store })
}

impl CredentialEntry {
    /// A bare-token entry with no timestamp — used only for the legacy
    /// bare-token migration read, where no mint time exists.
    fn bare_no_stamp(token: Token) -> Self {
        CredentialEntry {
            server: super::server::ServerUrl::Prod,
            token,
            minted_at: String::new(),
            user_login: None,
            user_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::server::ServerUrl;
    use tempfile::TempDir;

    struct TestEnv {
        _tmp: TempDir,
        xdg: PathBuf,
        creds: PathBuf,
    }

    impl TestEnv {
        fn new() -> Self {
            let tmp = TempDir::new().unwrap();
            let xdg = tmp.path().join("xdg");
            let creds = xdg.join("aristo/credentials");
            Self {
                _tmp: tmp,
                xdg,
                creds,
            }
        }

        fn xdg_str(&self) -> &str {
            self.xdg.to_str().unwrap()
        }
    }

    fn dummy_home() -> Option<&'static Path> {
        Some(Path::new("/nonexistent-test-home"))
    }

    fn entry(server: &str, token: &str, minted_at: &str) -> CredentialEntry {
        CredentialEntry {
            server: ServerUrl::parse(server),
            token: Token::new(token),
            minted_at: minted_at.to_string(),
            user_login: Some("octocat".into()),
            user_id: Some(1),
        }
    }

    const ACME: &str = "https://acme.aretta.ai";
    const OTHER: &str = "https://other.aretta.ai";

    #[test]
    fn save_creates_parent_directory() {
        let env = TestEnv::new();
        assert!(!env.xdg.join("aristo").exists());
        save_with(&Token::new("tok"), Some(env.xdg_str()), dummy_home()).unwrap();
        assert!(env.creds.exists());
    }

    #[test]
    #[cfg(unix)]
    fn save_sets_owner_only_unix_perms() {
        use std::os::unix::fs::PermissionsExt;
        let env = TestEnv::new();
        save_with(&Token::new("tok"), Some(env.xdg_str()), dummy_home()).unwrap();
        let mode = fs::metadata(&env.creds).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
    }

    #[test]
    fn clear_removes_file_and_is_idempotent() {
        let env = TestEnv::new();
        save_with(&Token::new("tok"), Some(env.xdg_str()), dummy_home()).unwrap();
        clear_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert!(!env.creds.exists());
        clear_with(Some(env.xdg_str()), dummy_home()).unwrap();
    }

    #[test]
    fn credentials_path_combines_xdg_with_filename() {
        let env = TestEnv::new();
        let p = credentials_path_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(p, env.creds);
    }

    // ─── keyed by server ─────────────────────────────────────────────────────

    #[test]
    fn store_round_trips_one_entry_per_server() {
        let env = TestEnv::new();
        let mut store = CredentialStore::default();
        store.upsert(entry(ACME, "arta_acme", "2026-09-15T00:00:00Z"));
        store.upsert(entry(OTHER, "arta_other", "2026-09-15T01:00:00Z"));
        save_store_with(&store, Some(env.xdg_str()), dummy_home()).unwrap();

        let loaded = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(loaded.len(), 2);
        let a = loaded.find_by_server(&ServerUrl::parse(ACME)).unwrap();
        assert_eq!(a.token.as_str(), "arta_acme");
        assert_eq!(a.minted_at, "2026-09-15T00:00:00Z");
        let raw = fs::read_to_string(&env.creds).unwrap();
        assert!(raw.contains("version = 3"), "{raw}");
        assert!(!raw.contains("repo"), "no repo is ever written: {raw}");
    }

    #[test]
    fn upsert_replaces_the_servers_entry_and_reports_it() {
        let mut store = CredentialStore::default();
        assert_eq!(
            store.upsert(entry(ACME, "t1", "2026-09-15T00:00:00Z")),
            UpsertOutcome::Added
        );
        assert_eq!(
            store.upsert(entry(ACME, "t2", "2026-09-15T01:00:00Z")),
            UpsertOutcome::Replaced
        );
        assert_eq!(
            store.upsert(entry(OTHER, "t3", "2026-09-15T02:00:00Z")),
            UpsertOutcome::Added
        );
        assert_eq!(store.len(), 2);
        assert_eq!(
            store
                .find_by_server(&ServerUrl::parse(ACME))
                .unwrap()
                .token
                .as_str(),
            "t2"
        );
    }

    #[test]
    fn resolve_for_named_server_or_the_single_entry() {
        let mut store = CredentialStore::default();
        assert!(store.resolve_for(None).is_none());
        store.upsert(entry(ACME, "a", "2026-09-15T00:00:00Z"));
        // One entry: used from anywhere, named or not.
        assert_eq!(store.resolve_for(None).unwrap().token.as_str(), "a");
        assert_eq!(
            store
                .resolve_for(Some(&ServerUrl::parse(ACME)))
                .unwrap()
                .token
                .as_str(),
            "a"
        );
        assert!(store.resolve_for(Some(&ServerUrl::parse(OTHER))).is_none());
        store.upsert(entry(OTHER, "b", "2026-09-15T01:00:00Z"));
        // Several: only a named server resolves.
        assert!(store.resolve_for(None).is_none());
        assert_eq!(
            store
                .resolve_for(Some(&ServerUrl::parse(OTHER)))
                .unwrap()
                .token
                .as_str(),
            "b"
        );
    }

    #[test]
    fn remove_by_server_removes_and_reports() {
        let mut store = CredentialStore::default();
        store.upsert(entry(ACME, "a", "2026-09-15T00:00:00Z"));
        store.upsert(entry(OTHER, "b", "2026-09-15T00:00:00Z"));
        assert!(store.remove_by_server(&ServerUrl::parse(ACME)));
        assert!(!store.remove_by_server(&ServerUrl::parse(ACME)));
        assert_eq!(store.len(), 1);
        assert_eq!(store.entries[0].server, ServerUrl::parse(OTHER));
    }

    #[test]
    fn upsert_entry_with_reports_outcome_and_the_saved_store() {
        let env = TestEnv::new();
        let first = upsert_entry_with(
            entry(ACME, "t1", "2026-09-15T00:00:00Z"),
            Some(env.xdg_str()),
            dummy_home(),
        )
        .unwrap();
        assert_eq!(first.outcome, UpsertOutcome::Added);
        assert_eq!(first.store.len(), 1);
        let second = upsert_entry_with(
            entry(ACME, "t2", "2026-09-15T01:00:00Z"),
            Some(env.xdg_str()),
            dummy_home(),
        )
        .unwrap();
        assert_eq!(second.outcome, UpsertOutcome::Replaced);
        let on_disk = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(on_disk.len(), 1);
        assert_eq!(on_disk.entries[0].token.as_str(), "t2");
    }

    // ─── older files still read ──────────────────────────────────────────────

    #[test]
    fn v2_file_collapses_to_one_entry_per_server_newest_first() {
        // A v2 file (per-repo tokens) may hold several entries for one
        // server; the newest is kept, the repo field is ignored.
        let env = TestEnv::new();
        fs::create_dir_all(env.creds.parent().unwrap()).unwrap();
        fs::write(
            &env.creds,
            r#"version = 2

[[entries]]
server = "https://acme.aretta.ai"
repo = "acme/a"
token = "older"
minted_at = "2026-09-14T00:00:00Z"

[[entries]]
server = "https://acme.aretta.ai"
repo = "acme/b"
token = "newer"
minted_at = "2026-09-15T00:00:00Z"

[[entries]]
server = "https://other.aretta.ai"
repo = "other/x"
token = "other"
minted_at = "2026-09-13T00:00:00Z"
"#,
        )
        .unwrap();
        let store = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(store.len(), 2);
        assert_eq!(
            store
                .find_by_server(&ServerUrl::parse(ACME))
                .unwrap()
                .token
                .as_str(),
            "newer"
        );
        assert_eq!(
            store
                .find_by_server(&ServerUrl::parse(OTHER))
                .unwrap()
                .token
                .as_str(),
            "other"
        );
    }

    #[test]
    fn v1_file_reads_as_single_entry() {
        let env = TestEnv::new();
        fs::create_dir_all(env.creds.parent().unwrap()).unwrap();
        fs::write(
            &env.creds,
            r#"
[aretta]
token = "v1-token"
issued_at = "2026-05-20T00:00:00Z"
server = "https://dev.aretta.ai"
repo = "owner/legacy"
"#,
        )
        .unwrap();
        let store = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(store.len(), 1);
        let e = &store.entries[0];
        assert_eq!(e.token.as_str(), "v1-token");
        assert_eq!(e.server.as_str(), "https://dev.aretta.ai");
        assert_eq!(e.minted_at, "2026-05-20T00:00:00Z");
    }

    #[test]
    fn upsert_migrates_an_older_file_and_preserves_other_servers() {
        let env = TestEnv::new();
        fs::create_dir_all(env.creds.parent().unwrap()).unwrap();
        fs::write(
            &env.creds,
            "[aretta]\ntoken = \"old-tok\"\nissued_at = \"2026-05-20T00:00:00Z\"\nserver = \"https://other.aretta.ai\"\n",
        )
        .unwrap();
        upsert_entry_with(
            entry(ACME, "new-tok", "2026-09-15T00:00:00Z"),
            Some(env.xdg_str()),
            dummy_home(),
        )
        .unwrap();
        let store = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(store.len(), 2);
        assert_eq!(
            store
                .find_by_server(&ServerUrl::parse(OTHER))
                .unwrap()
                .token
                .as_str(),
            "old-tok"
        );
        let raw = fs::read_to_string(&env.creds).unwrap();
        assert!(raw.contains("version = 3"), "expected v3 file; got:\n{raw}");
        assert!(!raw.contains("[aretta]"), "v1 table should be gone:\n{raw}");
    }

    #[test]
    fn bare_token_file_reads_as_single_entry() {
        let env = TestEnv::new();
        fs::create_dir_all(env.creds.parent().unwrap()).unwrap();
        fs::write(&env.creds, "arta_bare\n").unwrap();
        let store = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(store.len(), 1);
        assert_eq!(store.entries[0].token.as_str(), "arta_bare");
    }

    #[test]
    fn load_missing_file_is_empty_store() {
        let env = TestEnv::new();
        assert!(load_store_with(Some(env.xdg_str()), dummy_home())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn malformed_file_is_error() {
        let env = TestEnv::new();
        fs::create_dir_all(env.creds.parent().unwrap()).unwrap();
        fs::write(&env.creds, "this is not TOML at all = = =").unwrap();
        let err = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap_err();
        assert!(matches!(err, AuthError::Malformed(_)));
    }

    #[test]
    fn newer_version_is_a_helpful_error() {
        let env = TestEnv::new();
        fs::create_dir_all(env.creds.parent().unwrap()).unwrap();
        fs::write(&env.creds, "version = 4\n").unwrap();
        match load_store_with(Some(env.xdg_str()), dummy_home()).unwrap_err() {
            AuthError::Malformed(m) => {
                assert!(m.contains("version 4") && m.contains("upgrade"), "got: {m}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn save_writes_header_with_downgrade_caveat_and_owner_only_perms() {
        let env = TestEnv::new();
        let mut store = CredentialStore::default();
        store.upsert(entry(ACME, "tok", "2026-09-15T00:00:00Z"));
        save_store_with(&store, Some(env.xdg_str()), dummy_home()).unwrap();
        let raw = fs::read_to_string(&env.creds).unwrap();
        assert!(
            raw.contains("DOWNGRADE CAVEAT") && raw.contains("aristo < 0.8"),
            "{raw}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&env.creds).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
        }
    }
}
