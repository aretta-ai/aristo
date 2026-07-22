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
/// path — same precedence as [`super::resolve::resolve`] so that
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
/// a single entry keyed `(prod, no-repo)` into the multi-repo store —
/// callers that know the server/repo persist a full record via
/// [`save_full_with`] instead.
pub fn save_with(
    token: &Token,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> io::Result<()> {
    upsert_entry_with(
        CredentialEntry::bare(token.clone(), super::server::ServerUrl::Prod, None),
        xdg_config_home,
        home_override,
    )
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

/// On-disk credentials. Extended in commit 4 of the auth-extraction
/// plan: in addition to the raw `token`, we now persist the server
/// URL the token was minted against, the GitHub user identity, and
/// the repo scope. All four new fields are optional so old bare-
/// token files still parse cleanly.
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
    /// `owner/repo` the token is scoped to server-side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    pub repo: Option<String>,
}

/// Persist a full credentials record (token + server + user + repo).
/// Upserts the entry keyed `(server, repo)` into the multi-repo store,
/// migrating any older single-slot file and preserving other entries.
/// Reads env vars for path resolution; see [`save_full_with`] for the
/// explicit-overrides variant used by tests.
pub fn save_full(creds: &CredentialsRecord) -> io::Result<()> {
    upsert_entry(creds.into())
}

/// Persist a full credentials record with explicit path overrides.
pub fn save_full_with(
    creds: &CredentialsRecord,
    xdg_config_home: Option<&str>,
    home_override: Option<&Path>,
) -> io::Result<()> {
    upsert_entry_with(creds.into(), xdg_config_home, home_override)
}

// ─── v2 multi-repo store ─────────────────────────────────────────────────────
//
// The single-slot `[aretta]` file (v1, above) holds one token. The v2
// store is a keyed, versioned list: entries keyed by `(server, repo)`,
// so a user can be logged in to several repos (and several servers) at
// once. v1 files still read transparently and migrate to v2 on the next
// write.

/// Current on-disk format version. v1 was the single-slot `[aretta]`
/// table; v2 is a keyed, multi-repo `[[entries]]` list.
const STORE_VERSION: u32 = 2;

/// Header comment prepended to the v2 store file. Documents the
/// downgrade caveat for an older CLI that only understands v1.
const STORE_HEADER: &str = "\
# Aristo credentials store (v2, multi-repo).
#
# Managed by `aristo auth`; entries are keyed by (server, repo). Tokens
# are secrets — on Unix this file is created 0600 (owner-only).
#
# DOWNGRADE CAVEAT: aristo < 0.6 understands only the older single-slot
# format and will not read these entries (it treats this file as
# unauthenticated, or errors as malformed). After upgrading run
# `aristo auth login` again, or `aristo auth logout --all` to reset.
";

/// One credential entry, keyed by `(server, repo)`.
#[derive(Debug, Clone)]
pub struct CredentialEntry {
    /// Aretta server this token was minted against.
    pub server: super::server::ServerUrl,
    /// `owner/repo` the token is scoped to, or `None` when the scope is
    /// unknown (e.g. a `--token` paste with no `--repo`).
    pub repo: Option<String>,
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
    /// A minimal entry for a raw token whose `(server, repo)` scope is
    /// otherwise unknown. Stamps `minted_at` now.
    pub fn bare(token: Token, server: super::server::ServerUrl, repo: Option<String>) -> Self {
        CredentialEntry {
            server,
            repo,
            token,
            minted_at: now_iso8601(),
            user_login: None,
            user_id: None,
        }
    }

    /// True iff this entry's key is exactly `(server, repo)`.
    fn keyed_as(&self, server: &super::server::ServerUrl, repo: Option<&str>) -> bool {
        &self.server == server && self.repo.as_deref() == repo
    }
}

impl From<&CredentialsRecord> for CredentialEntry {
    fn from(r: &CredentialsRecord) -> Self {
        CredentialEntry {
            server: r.server.clone(),
            repo: r.repo.clone(),
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
    /// Entries in file order. Keyed by `(server, repo)` via [`upsert`].
    ///
    /// [`upsert`]: CredentialStore::upsert
    pub entries: Vec<CredentialEntry>,
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

    /// The sole entry iff there is exactly one — the single-repo grace
    /// a resolver falls back to when it has no repo hint (or the hint
    /// doesn't match).
    pub fn sole(&self) -> Option<&CredentialEntry> {
        match &self.entries[..] {
            [only] => Some(only),
            _ => None,
        }
    }

    /// The entry scoped to `repo`. When several share a repo (different
    /// servers), the most-recently-minted wins.
    pub fn find_by_repo(&self, repo: &str) -> Option<&CredentialEntry> {
        self.entries
            .iter()
            .filter(|e| e.repo.as_deref() == Some(repo))
            .max_by(|a, b| a.minted_at.cmp(&b.minted_at))
    }

    /// Insert `entry`, or replace the existing one with the same
    /// `(server, repo)` key.
    pub fn upsert(&mut self, entry: CredentialEntry) {
        if let Some(slot) = self
            .entries
            .iter_mut()
            .find(|e| e.keyed_as(&entry.server, entry.repo.as_deref()))
        {
            *slot = entry;
        } else {
            self.entries.push(entry);
        }
    }

    /// Remove every entry scoped to `repo`. Returns the count removed.
    pub fn remove_by_repo(&mut self, repo: &str) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.repo.as_deref() != Some(repo));
        before - self.entries.len()
    }
}

// ─── v2 on-disk schema ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct StoreFileV2 {
    /// Format version — its presence distinguishes v2 from a v1
    /// `[aretta]` file (which has no `version` key).
    version: u32,
    #[serde(default)]
    entries: Vec<EntryToml>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntryToml {
    server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
/// current v2 format, migrates a v1 `[aretta]` file transparently on
/// read (the migration is persisted on the next write), and accepts a
/// legacy bare-token file. A missing file yields an empty store.
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

/// Pure: parse the credentials file text into a store. Tries v2, then a
/// v1 `[aretta]` table, then a legacy bare token.
fn parse_store(raw: &str, path: &Path) -> Result<CredentialStore, AuthError> {
    // v2 — recognized by the `version` key.
    if let Ok(v2) = toml::from_str::<StoreFileV2>(raw) {
        if v2.version != STORE_VERSION {
            return Err(AuthError::Malformed(format!(
                "credentials file at {} is format version {} — upgrade aristo to read it",
                path.display(),
                v2.version
            )));
        }
        let entries = v2
            .entries
            .into_iter()
            .map(|t| entry_from_toml(t, path))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(CredentialStore { entries });
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
            repo: v1.aretta.repo.clone(),
            token: Token::new(token),
            minted_at: v1.aretta.issued_at.clone(),
            user_login: v1.aretta.user_login.clone(),
            user_id: v1.aretta.user_id,
        };
        return Ok(CredentialStore {
            entries: vec![entry],
        });
    }
    // Legacy bare token (a single non-TOML line) — same back-compat the
    // pre-v2 resolver honored.
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
        repo: t.repo,
        token: Token::new(token),
        minted_at: t.minted_at,
        user_login: t.user_login,
        user_id: t.user_id,
    })
}

fn entry_to_toml(e: &CredentialEntry) -> EntryToml {
    EntryToml {
        server: e.server.as_str().to_string(),
        repo: e.repo.clone(),
        token: e.token.as_str().to_string(),
        minted_at: e.minted_at.clone(),
        user_login: e.user_login.clone(),
        user_id: e.user_id,
    }
}

/// Persist the whole store as v2 (versioned + header comment), reading
/// env vars for the path. Atomic write, `0600` on Unix — same idiom as
/// the v1 [`save_full_with`].
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
    let file = StoreFileV2 {
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

/// Insert-or-replace one entry, migrating any existing v1 file to v2 and
/// preserving every other entry. Reads env vars for the path.
pub fn upsert_entry(entry: CredentialEntry) -> io::Result<()> {
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
) -> io::Result<()> {
    // Load first so we migrate (not clobber) an existing file; a corrupt
    // file surfaces as an error rather than silent data loss.
    let mut store = load_store_with(xdg_config_home, home_override).map_err(io_from_auth_error)?;
    store.upsert(entry);
    save_store_with(&store, xdg_config_home, home_override)
}

impl CredentialEntry {
    /// A bare-token entry with no timestamp — used only for the legacy
    /// bare-token migration read, where no mint time exists.
    fn bare_no_stamp(token: Token) -> Self {
        CredentialEntry {
            server: super::server::ServerUrl::Prod,
            repo: None,
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
    use tempfile::TempDir;

    /// Test environment: a TempDir + a pinned XDG path so tests are
    /// platform-independent (macOS would otherwise resolve to
    /// `~/Library/Application Support` instead of `~/.config`).
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
        let meta = fs::metadata(&env.creds).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "credentials file should be owner-only readable, got {mode:o}"
        );
    }

    #[test]
    fn clear_removes_file() {
        let env = TestEnv::new();
        save_with(&Token::new("tok"), Some(env.xdg_str()), dummy_home()).unwrap();
        assert!(env.creds.exists());
        clear_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert!(!env.creds.exists());
    }

    #[test]
    fn clear_when_file_missing_is_not_an_error() {
        let env = TestEnv::new();
        clear_with(Some(env.xdg_str()), dummy_home()).unwrap();
        // Idempotent — calling twice is fine.
        clear_with(Some(env.xdg_str()), dummy_home()).unwrap();
    }

    #[test]
    fn xdg_config_home_lands_file_under_xdg_path() {
        let env = TestEnv::new();
        save_with(&Token::new("tok"), Some(env.xdg_str()), dummy_home()).unwrap();
        // File lands under XDG path, not under the (fake) home.
        assert!(env.xdg.join("aristo/credentials").exists());
    }

    #[test]
    fn credentials_path_combines_xdg_with_filename() {
        let env = TestEnv::new();
        let p = credentials_path_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(p, env.creds);
    }

    // ─── v2 multi-repo store ─────────────────────────────────────────────────

    use crate::auth::server::ServerUrl;

    fn entry(server: ServerUrl, repo: &str, token: &str, minted_at: &str) -> CredentialEntry {
        CredentialEntry {
            server,
            repo: Some(repo.to_string()),
            token: Token::new(token),
            minted_at: minted_at.to_string(),
            user_login: Some("octocat".into()),
            user_id: Some(1),
        }
    }

    #[test]
    fn store_round_trips_multiple_entries() {
        let env = TestEnv::new();
        let mut store = CredentialStore::default();
        store.upsert(entry(
            ServerUrl::parse("https://turso.aretta.ai"),
            "tursodatabase/turso",
            "arta_turso",
            "2026-07-22T00:00:00Z",
        ));
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/other",
            "arta_other",
            "2026-07-22T01:00:00Z",
        ));
        save_store_with(&store, Some(env.xdg_str()), dummy_home()).unwrap();

        let loaded = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(loaded.len(), 2);
        let t = loaded.find_by_repo("tursodatabase/turso").unwrap();
        assert_eq!(t.token.as_str(), "arta_turso");
        assert_eq!(t.server, ServerUrl::parse("https://turso.aretta.ai"));
        assert_eq!(t.minted_at, "2026-07-22T00:00:00Z");
        assert_eq!(
            loaded.find_by_repo("owner/other").unwrap().token.as_str(),
            "arta_other"
        );
    }

    #[test]
    fn v1_file_reads_as_single_entry() {
        // A v1 [aretta] file migrates on read to one keyed entry.
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
        // v1 stored the full URL; parse round-trips it (as a Custom with
        // the same as_str(), exactly as the pre-v2 resolver did).
        assert_eq!(e.server.as_str(), "https://dev.aretta.ai");
        assert_eq!(e.repo.as_deref(), Some("owner/legacy"));
        assert_eq!(e.minted_at, "2026-05-20T00:00:00Z");
    }

    #[test]
    fn upsert_migrates_v1_and_preserves_the_old_entry() {
        // The headline migration: an existing v1 entry survives, the new
        // entry is added, and the file is rewritten as v2.
        let env = TestEnv::new();
        fs::create_dir_all(env.creds.parent().unwrap()).unwrap();
        fs::write(
            &env.creds,
            "[aretta]\ntoken = \"old-tok\"\nissued_at = \"2026-05-20T00:00:00Z\"\nrepo = \"owner/old\"\n",
        )
        .unwrap();

        upsert_entry_with(
            entry(
                ServerUrl::Prod,
                "owner/new",
                "new-tok",
                "2026-07-22T00:00:00Z",
            ),
            Some(env.xdg_str()),
            dummy_home(),
        )
        .unwrap();

        // Both entries present.
        let store = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.find_by_repo("owner/old").unwrap().token.as_str(),
            "old-tok"
        );
        assert_eq!(
            store.find_by_repo("owner/new").unwrap().token.as_str(),
            "new-tok"
        );
        // File is now v2.
        let raw = fs::read_to_string(&env.creds).unwrap();
        assert!(raw.contains("version = 2"), "expected v2 file; got:\n{raw}");
        assert!(!raw.contains("[aretta]"), "v1 table should be gone:\n{raw}");
    }

    #[test]
    fn upsert_replaces_entry_with_same_key() {
        let mut store = CredentialStore::default();
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/repo",
            "tok1",
            "2026-07-22T00:00:00Z",
        ));
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/repo",
            "tok2",
            "2026-07-22T02:00:00Z",
        ));
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.find_by_repo("owner/repo").unwrap().token.as_str(),
            "tok2"
        );
    }

    #[test]
    fn upsert_keeps_distinct_keys() {
        // Same repo on different servers are distinct keys; so are
        // different repos on the same server.
        let mut store = CredentialStore::default();
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/repo",
            "prod-tok",
            "2026-07-22T00:00:00Z",
        ));
        store.upsert(entry(
            ServerUrl::Dev,
            "owner/repo",
            "dev-tok",
            "2026-07-22T00:00:00Z",
        ));
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/other",
            "other-tok",
            "2026-07-22T00:00:00Z",
        ));
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn find_by_repo_prefers_most_recent_when_repo_is_shared() {
        let mut store = CredentialStore::default();
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/repo",
            "older",
            "2026-07-22T00:00:00Z",
        ));
        store.upsert(entry(
            ServerUrl::Dev,
            "owner/repo",
            "newer",
            "2026-07-22T05:00:00Z",
        ));
        assert_eq!(
            store.find_by_repo("owner/repo").unwrap().token.as_str(),
            "newer"
        );
        assert!(store.find_by_repo("nope/nope").is_none());
    }

    #[test]
    fn sole_only_with_exactly_one_entry() {
        let mut store = CredentialStore::default();
        assert!(store.sole().is_none());
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/a",
            "a",
            "2026-07-22T00:00:00Z",
        ));
        assert_eq!(store.sole().unwrap().token.as_str(), "a");
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/b",
            "b",
            "2026-07-22T00:00:00Z",
        ));
        assert!(store.sole().is_none());
    }

    #[test]
    fn remove_by_repo_removes_and_counts() {
        let mut store = CredentialStore::default();
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/a",
            "a",
            "2026-07-22T00:00:00Z",
        ));
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/b",
            "b",
            "2026-07-22T00:00:00Z",
        ));
        assert_eq!(store.remove_by_repo("owner/a"), 1);
        assert_eq!(store.remove_by_repo("owner/a"), 0);
        assert_eq!(store.len(), 1);
        assert_eq!(store.entries[0].repo.as_deref(), Some("owner/b"));
    }

    #[test]
    fn load_missing_file_is_empty_store() {
        let env = TestEnv::new();
        let store = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert!(store.is_empty());
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
    fn unknown_version_is_a_helpful_error() {
        let env = TestEnv::new();
        fs::create_dir_all(env.creds.parent().unwrap()).unwrap();
        fs::write(&env.creds, "version = 3\n").unwrap();
        match load_store_with(Some(env.xdg_str()), dummy_home()).unwrap_err() {
            AuthError::Malformed(m) => {
                assert!(m.contains("version 3"), "got: {m}");
                assert!(m.contains("upgrade"), "got: {m}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn bare_token_file_reads_as_single_entry() {
        let env = TestEnv::new();
        fs::create_dir_all(env.creds.parent().unwrap()).unwrap();
        fs::write(&env.creds, "arta_bare_legacy\n").unwrap();
        let store = load_store_with(Some(env.xdg_str()), dummy_home()).unwrap();
        assert_eq!(store.len(), 1);
        assert_eq!(store.entries[0].token.as_str(), "arta_bare_legacy");
    }

    #[test]
    fn save_writes_header_with_downgrade_caveat() {
        let env = TestEnv::new();
        let mut store = CredentialStore::default();
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/repo",
            "tok",
            "2026-07-22T00:00:00Z",
        ));
        save_store_with(&store, Some(env.xdg_str()), dummy_home()).unwrap();
        let raw = fs::read_to_string(&env.creds).unwrap();
        assert!(raw.contains("DOWNGRADE CAVEAT"), "header missing:\n{raw}");
        assert!(raw.contains("aristo < 0.6"), "caveat missing:\n{raw}");
    }

    #[test]
    #[cfg(unix)]
    fn save_store_sets_owner_only_perms() {
        use std::os::unix::fs::PermissionsExt;
        let env = TestEnv::new();
        let mut store = CredentialStore::default();
        store.upsert(entry(
            ServerUrl::Prod,
            "owner/repo",
            "tok",
            "2026-07-22T00:00:00Z",
        ));
        save_store_with(&store, Some(env.xdg_str()), dummy_home()).unwrap();
        let mode = fs::metadata(&env.creds).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
    }
}
