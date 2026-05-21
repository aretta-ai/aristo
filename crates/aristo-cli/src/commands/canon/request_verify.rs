//! `aristo canon request-verify <canon_id> [--notes <text>]` — record
//! a demand signal for verification backing on a canon entry.
//!
//! Per canon-strategy.md §CS11, this is idempotent on
//! `(canon_id, repo_full_name, user_id)` server-side — repeat calls
//! with a fresh `--notes` replace the previous note, but the
//! "first submission" timestamp is preserved.
//!
//! The user runs this when they've bound an annotation at the
//! `kanon:` tier and want to push Aretta to invest in a verification
//! mechanism for that canon entry. The server tallies demand signals
//! to inform the verifier-investment roadmap.

use aristo_core::canon::{
    CanonClient, CanonError, HttpCanonClient, MockCanonClient, RequestVerifyBody,
};

use crate::{CliError, CliResult};

pub(crate) fn run(canon_id: &str, notes: Option<String>) -> CliResult<()> {
    let client: Box<dyn CanonClient> = if let Some(mock) = MockCanonClient::from_env() {
        Box::new(mock)
    } else {
        match aristo_core::auth::resolve_full() {
            Ok(creds) => {
                let base_url = std::env::var("ARETTA_API_URL")
                    .unwrap_or_else(|_| creds.server.as_str().to_string());
                Box::new(HttpCanonClient::new(base_url, &creds.token))
            }
            Err(_) => {
                return Err(CliError::Other {
                    message: "canon API requires authentication.\n  \
                              Run `aristo auth login` to sign in.\n  \
                              `aristo canon request-verify` is a paid-tier feature; \
                              see canon-strategy.md §CS1."
                        .into(),
                    exit_code: 1,
                });
            }
        }
    };

    let body = RequestVerifyBody {
        canon_id: canon_id.to_string(),
        notes,
    };
    let response = client.request_verify(&body).map_err(canon_error_to_cli)?;

    match response.status.as_str() {
        "submitted" => {
            println!(
                "ok: verification demand signal recorded for `{}`.",
                response.canon_id
            );
        }
        "updated" => {
            let prev = response.previously_submitted_at.as_deref().unwrap_or("?");
            println!(
                "ok: verification demand signal updated for `{}` \
                 (first submitted {prev}).",
                response.canon_id
            );
        }
        other => {
            // Server might add new status values; keep the body
            // future-compatible.
            println!(
                "ok: request-verify returned status `{other}` for `{}`.",
                response.canon_id
            );
        }
    }
    if let Some(current_backing) = &response.current_backing {
        println!("   note: this entry already has backing: {current_backing}");
    } else {
        println!(
            "   note: this canon entry has no verification backing yet for your \
             scope. Demand signals like yours drive Aretta's verifier-investment \
             roadmap."
        );
    }
    Ok(())
}

fn canon_error_to_cli(e: CanonError) -> CliError {
    CliError::Other {
        message: format!("canon API error: {e}"),
        exit_code: 1,
    }
}
