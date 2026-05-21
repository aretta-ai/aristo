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

use aristo_core::canon::auth::{self, Token};
use aristo_core::canon::AuthError;

use crate::{AuthAction, CliError, CliResult};

/// Dispatcher for `aristo auth` subcommands.
pub(crate) fn run(action: AuthAction) -> CliResult<()> {
    match action {
        AuthAction::Login { stdin, token } => login(stdin, token),
        AuthAction::Status => status(),
        AuthAction::Logout => logout(),
    }
}

// ─── login ─────────────────────────────────────────────────────────────────

fn login(read_stdin: bool, token_flag: Option<String>) -> CliResult<()> {
    let token_raw = collect_token(read_stdin, token_flag)?;
    let trimmed = token_raw.trim();
    if trimmed.is_empty() {
        return Err(CliError::Other {
            message: "no token provided.\n\
                     Get an API token at https://code.aretta.ai/dashboard/settings/tokens, then run\n  \
                       `aristo auth login` (paste interactively),\n  \
                       `aristo auth login --stdin` (pipe), or\n  \
                       `aristo auth login --token <TOKEN>` (scripting)."
                .into(),
            exit_code: 2,
        });
    }
    let token = Token::new(trimmed);
    auth::save(&token).map_err(CliError::Io)?;

    // Resolve the path back from canon::auth so the success message
    // points at the actual location used (honors XDG_CONFIG_HOME).
    let path = match auth::credentials_path() {
        Ok(p) => p,
        Err(e) => {
            return Err(CliError::Other {
                message: format!("token saved, but couldn't resolve credentials path: {e}"),
                exit_code: 1,
            })
        }
    };
    println!("ok: authenticated. token saved to {}", path.display());
    println!("   `aristo auth status` to verify; `aristo auth logout` to remove.");
    Ok(())
}

/// Determine where the token comes from and read it. Three sources:
///
/// - `--token=<T>` → use `T` verbatim (no prompt, no stdin).
/// - `--stdin` → consume all of stdin (typical CI pattern:
///   `echo "$T" | aristo auth login --stdin`).
/// - neither → print a prompt and read one line from stdin.
fn collect_token(read_stdin: bool, token_flag: Option<String>) -> CliResult<String> {
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
    // Interactive prompt. Use stderr for the prompt so stdout stays
    // pipe-clean (some users will pipe stdout to grep or similar).
    eprintln!(
        "To authenticate, get an API token at https://code.aretta.ai/dashboard/settings/tokens"
    );
    eprintln!("Paste the token below and press Enter:");
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(CliError::Io)?;
    Ok(line)
}

// ─── status ────────────────────────────────────────────────────────────────

fn status() -> CliResult<()> {
    match auth::resolve() {
        Ok(_token) => {
            // Don't print the token. Just identify its source so the
            // user can confirm what's wired up.
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
