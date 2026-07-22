//! `aristo auth {login, status, logout}` — credential lifecycle.
//!
//! Wires `aristo_core::canon::auth` into the CLI dispatcher. The
//! handlers are intentionally thin: token-resolution and
//! persistence live in the library so other binaries (eventually a
//! `aretta-admin` clone or scripted tooling) can call them
//! directly without going through the CLI.
//!
//! ## Login flow (paste-flow, deliberately simple)
//!
//! The first slice of `aristo auth login` is a **paste flow**:
//!
//! 1. Print a one-line prompt telling the user where to get a token.
//! 2. Read a token from stdin (`--stdin` consumes all; default reads
//!    one line; `--token=<T>` bypasses both for tests / scripting).
//! 3. Persist via `canon::auth::save`.
//!
//! Server-side validation of the token (e.g., `GET /auth/whoami`) is
//! intentionally deferred — the first canon API call (`aristo stamp`,
//! `aristo critique`, `aristo canon show`) surfaces a typed
//! [`AuthError::Invalid`] if the token is bad. Adding a validation
//! roundtrip here would couple `aristo auth login` to network state,
//! breaking the offline-friendly invariant.
//!
//! A device-code OAuth flow is a future enhancement (open browser →
//! poll for token); not needed for v0.1.

use std::io::Read;

use aristo_core::auth::{
    self, derive_repo_full_name, login_server, AuthError, LoginServerSource, ServerUrl, Token,
};

use crate::{AuthAction, CliError, CliResult};

/// Dispatcher for `aristo auth` subcommands.
pub(crate) fn run(action: AuthAction) -> CliResult<()> {
    match action {
        AuthAction::Login {
            stdin,
            token,
            server,
            repo,
        } => login(stdin, token, server, repo),
        AuthAction::Status => status(),
        AuthAction::Token => token(),
        AuthAction::Logout => logout(),
    }
}

// ─── login ─────────────────────────────────────────────────────────────────

fn login(
    read_stdin: bool,
    token_flag: Option<String>,
    server_flag: Option<String>,
    repo_flag: Option<String>,
) -> CliResult<()> {
    // Bypass modes — caller supplied a raw token directly. The token's
    // server-side scope is already set, so `--server` / `ARETTA_API_URL`
    // don't participate here.
    if read_stdin || token_flag.is_some() {
        return login_with_raw_token(read_stdin, token_flag);
    }

    // OAuth flow: resolve the server the token is minted against.
    // Precedence: --server flag > ARETTA_API_URL env > prod default.
    // Honoring the env keeps the auth plane aligned with the data plane,
    // which already treats ARETTA_API_URL as its highest-precedence
    // override (see `crate::data_plane`).
    let env_override = std::env::var("ARETTA_API_URL").ok();
    let (server, source) = login_server(server_flag.as_deref(), env_override.as_deref());

    login_via_oauth(&server, source, repo_flag)
}

fn login_via_oauth(
    server: &ServerUrl,
    source: LoginServerSource,
    repo_flag: Option<String>,
) -> CliResult<()> {
    let repo_full_name = resolve_repo_full_name(repo_flag)?;

    // 1. Fetch the GitHub OAuth URL from the proxy.
    let init = auth::oauth_start(server).map_err(auth_error_to_cli)?;

    // 2. Show the URL + try to open the browser. Name where the server
    //    came from when it wasn't the default, so a stale ARETTA_API_URL
    //    export is visible before the user authorizes.
    eprintln!();
    match source.provenance(&repo_full_name) {
        Some(prov) => eprintln!("Authenticating against {server} ({prov})"),
        None => eprintln!("Authenticating against {server}"),
    }
    eprintln!("Scoping token to repo: {repo_full_name}");
    eprintln!();
    eprintln!("Open this URL to authorize with GitHub:");
    eprintln!();
    eprintln!("    {}", init.authorize_url);
    eprintln!();
    let _ = try_open_browser(&init.authorize_url);
    eprintln!("After authorizing, the page will display a code. Paste it here:");

    // 3. Read the code from stdin (one line).
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(CliError::Io)?;
    let code = line.trim();
    if code.is_empty() {
        return Err(CliError::Other {
            message: "no OAuth code provided. Re-run `aristo auth login` and paste the code from the callback page.".into(),
            exit_code: 2,
        });
    }

    // 4. Exchange the code for an arta_* token.
    let resp = auth::oauth_exchange(server, code, &repo_full_name, Some("aristo-cli"))
        .map_err(auth_error_to_cli)?;

    // 5. Persist the full credentials record (token + server + user + repo).
    let token = Token::new(&resp.arta_token);
    let creds = aristo_core::auth::CredentialsRecord {
        token,
        server: server.clone(),
        user_login: Some(resp.user.login.clone()),
        user_id: Some(resp.user.id),
        repo: Some(resp.repo_full_name.clone()),
    };
    aristo_core::auth::save_full(&creds).map_err(CliError::Io)?;

    let path = auth::credentials_path().map_err(auth_error_to_cli)?;
    println!(
        "ok: authenticated as {} for {}",
        resp.user.login, resp.repo_full_name
    );
    println!("    token saved to {}", path.display());
    println!("    `aristo auth status` to verify; `aristo auth logout` to remove.");
    Ok(())
}

fn login_with_raw_token(read_stdin: bool, token_flag: Option<String>) -> CliResult<()> {
    let token_raw = collect_raw_token(read_stdin, token_flag)?;
    let trimmed = token_raw.trim();
    if trimmed.is_empty() {
        return Err(CliError::Other {
            message: "no token provided.\n\
                     Get an API token at https://code.aretta.ai/dashboard/settings/tokens, then run\n  \
                       `aristo auth login` (OAuth flow, default),\n  \
                       `aristo auth login --stdin` (pipe), or\n  \
                       `aristo auth login --token <TOKEN>` (scripting)."
                .into(),
            exit_code: 2,
        });
    }
    let token = Token::new(trimmed);
    auth::save(&token).map_err(CliError::Io)?;

    let path = auth::credentials_path().map_err(auth_error_to_cli)?;
    println!("ok: authenticated. token saved to {}", path.display());
    println!("   `aristo auth status` to verify; `aristo auth logout` to remove.");
    Ok(())
}

/// Determine where the raw token comes from in bypass modes.
fn collect_raw_token(read_stdin: bool, token_flag: Option<String>) -> CliResult<String> {
    if let Some(t) = token_flag {
        return Ok(t);
    }
    if read_stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(CliError::Io)?;
        return Ok(buf);
    }
    // Should not be reached — caller checks the flags first.
    Err(CliError::Other {
        message: "internal: collect_raw_token called without --stdin or --token".into(),
        exit_code: 1,
    })
}

fn resolve_repo_full_name(repo_flag: Option<String>) -> CliResult<String> {
    if let Some(r) = repo_flag {
        let trimmed = r.trim();
        if trimmed.is_empty() {
            return Err(CliError::Other {
                message: "--repo must be `owner/repo` (got empty string)".into(),
                exit_code: 2,
            });
        }
        if !trimmed.contains('/') {
            return Err(CliError::Other {
                message: format!("--repo `{trimmed}` is not in `owner/repo` form"),
                exit_code: 2,
            });
        }
        return Ok(trimmed.to_string());
    }
    let cwd = std::env::current_dir().map_err(CliError::Io)?;
    derive_repo_full_name(&cwd).map_err(auth_error_to_cli)
}

fn try_open_browser(url: &str) -> std::io::Result<()> {
    // Test mode: e2e tests spawn the real aristo binary and would
    // otherwise launch the developer's browser on every test run.
    // The `ARISTO_NO_BROWSER` env var suppresses the spawn. Set it
    // in tests + any CI that doesn't want browser pop-ups.
    if std::env::var("ARISTO_NO_BROWSER").is_ok() {
        return Ok(());
    }
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "start"
    } else {
        "xdg-open"
    };
    std::process::Command::new(cmd)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

fn auth_error_to_cli(e: AuthError) -> CliError {
    CliError::Other {
        message: e.to_string(),
        exit_code: 1,
    }
}

// ─── status ────────────────────────────────────────────────────────────────

fn status() -> CliResult<()> {
    match aristo_core::auth::resolve_full() {
        Ok(creds) => {
            // Don't print the token. Just identify its source + the
            // associated server / user / repo so the user can confirm
            // what's wired up.
            let from_env = std::env::var(auth::ENV_VAR)
                .ok()
                .is_some_and(|v| !v.trim().is_empty());
            if from_env {
                println!(
                    "ok: authenticated via {} environment variable.",
                    auth::ENV_VAR
                );
                println!("    (env var takes precedence over the on-disk credentials file.)");
            } else {
                let path = auth::credentials_path().map_err(|e| CliError::Other {
                    message: format!("couldn't resolve credentials path: {e}"),
                    exit_code: 1,
                })?;
                println!("ok: authenticated via {}", path.display());
            }
            println!("    server: {}", creds.server);
            if let Some(login) = &creds.user_login {
                println!("    user:   {login}");
            }
            if let Some(repo) = &creds.repo {
                println!("    repo:   {repo}");
            }
            Ok(())
        }
        Err(AuthError::NoToken) => {
            println!("not authenticated.");
            println!(
                "    Run `aristo auth login` to log in, or set the {} env var for CI.",
                auth::ENV_VAR
            );
            // `aristo auth status` shouldn't exit non-zero just
            // because the user isn't logged in — that would break
            // CI gating patterns that check for canon availability
            // optionally. Use a typed condition (parse stdout) for
            // CI gating.
            Ok(())
        }
        Err(AuthError::Invalid) => {
            // Phase 1 `resolve()` never returns Invalid (server
            // validation is deferred to the first canon API call),
            // but handle it defensively so future-proofing is clean.
            Err(CliError::Other {
                message: "stored token was rejected by the server. \
                          Run `aristo auth login` to refresh."
                    .into(),
                exit_code: 1,
            })
        }
        Err(AuthError::Malformed(msg)) => Err(CliError::Other {
            message: format!(
                "credentials file is malformed: {msg}\n  \
                 Run `aristo auth logout` then `aristo auth login` to re-create it."
            ),
            exit_code: 1,
        }),
    }
}

// ─── token ─────────────────────────────────────────────────────────────────

/// Print the resolved token to stdout — and NOTHING else — so it pipes
/// cleanly into a clipboard tool (`aristo auth token | pbcopy`) or a CI
/// secret. Unlike `status`, this deliberately prints the secret value, so
/// it's only ever written to stdout on explicit request.
fn token() -> CliResult<()> {
    match aristo_core::auth::resolve_full() {
        Ok(creds) => {
            println!("{}", creds.token.as_str());
            Ok(())
        }
        Err(AuthError::NoToken) => Err(CliError::Other {
            message: format!(
                "not authenticated — no token found.\n  \
                 Run `aristo auth login` to mint one, or set the {} env var.",
                auth::ENV_VAR
            ),
            exit_code: 1,
        }),
        Err(AuthError::Malformed(msg)) => Err(CliError::Other {
            message: format!(
                "credentials file is malformed: {msg}\n  \
                 Run `aristo auth logout` then `aristo auth login` to re-create it."
            ),
            exit_code: 1,
        }),
        Err(AuthError::Invalid) => Err(CliError::Other {
            message: "stored token was rejected by the server. \
                      Run `aristo auth login` to refresh."
                .into(),
            exit_code: 1,
        }),
    }
}

// ─── logout ────────────────────────────────────────────────────────────────

fn logout() -> CliResult<()> {
    // Resolve the path first so we can include it in the success
    // message even if the file didn't exist (idempotent).
    let path = auth::credentials_path().map_err(|e| CliError::Other {
        message: format!("couldn't resolve credentials path: {e}"),
        exit_code: 1,
    })?;
    let existed = path.exists();
    auth::clear().map_err(CliError::Io)?;
    if existed {
        println!(
            "ok: logged out. credentials cleared from {}",
            path.display()
        );
    } else {
        println!("ok: not logged in (no credentials to clear).");
    }
    if std::env::var(auth::ENV_VAR).is_ok() {
        println!(
            "    note: {} is set in the environment; canon calls will still use it.",
            auth::ENV_VAR
        );
    }
    Ok(())
}
