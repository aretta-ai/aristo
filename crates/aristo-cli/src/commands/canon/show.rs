//! `aristo canon show <canon_id>` — fetch the canon entry detail via
//! `GET /canon/entry/<canon_id>` and render the full per-entry view.
//!
//! This is the surface behind the `[d]etail` choice in
//! cli-sessions.md Flow 3. The user wants the longer pedagogical
//! description, the example shape, and the references. This render
//! is the canon-entry detail dump; the user-facing trust card
//! lands in PR #10 as the canon-aware extension of `aristo show`.
//!
//! ## Access policy (per CS10 / canon-strategy.md)
//!
//! The `/canon/entry/<id>` endpoint is accessible to any paid-tier
//! user; there is no match-history gate. The server enforces the
//! "your scope or a public scope" filter on `related_entries` so the
//! card never leaks the IP of overlay scopes the user doesn't have.

use aristo_core::canon::{CanonClient, CanonError, HttpCanonClient, MockCanonClient};

use crate::{CliError, CliResult};

pub(crate) fn run(canon_id: &str, version: Option<String>) -> CliResult<()> {
    // Same client selection precedence as the stamp/critique runner:
    // ARISTO_CANON_FIXTURE wins for test mode; else auth resolves to
    // HttpCanonClient; else error out (no canon-API call possible).
    let client: Box<dyn CanonClient> = if let Some(mock) = MockCanonClient::from_env() {
        Box::new(mock)
    } else {
        match aristo_core::auth::resolve_full() {
            Ok(creds) => {
                // ARETTA_API_URL still wins as an explicit override
                // (useful for tests); otherwise use the server URL
                // persisted with the token.
                let base_url = std::env::var("ARETTA_API_URL")
                    .unwrap_or_else(|_| creds.server.as_str().to_string());
                Box::new(HttpCanonClient::new(base_url, &creds.token))
            }
            Err(_) => {
                return Err(CliError::Other {
                    message: "canon API requires authentication.\n  \
                              Run `aristo auth login` to sign in.\n  \
                              `aristo canon show` is a paid-tier feature; see \
                              canon-strategy.md §CS1."
                        .into(),
                    exit_code: 1,
                });
            }
        }
    };

    let entry = client
        .get_entry(canon_id, version.as_deref())
        .map_err(canon_error_to_cli)?;

    println!();
    println!("canon entry: {}", entry.id);
    println!("version:     {}", entry.version);
    println!("scope:       {}", entry.scope);
    println!("category:    {}", entry.category);
    println!("property:    {}", entry.property_type);
    println!("applies to:  {}", entry.applies_to.join(", "));
    if let Some(backed_by) = &entry.backed_by {
        println!("backed by:   {backed_by}");
    } else {
        println!("backed by:   — (kanon: tier; no verification backing yet for your scope)");
    }
    println!();
    println!("statement:");
    println!("  {}", entry.canonical_text);
    if let Some(desc) = &entry.description {
        println!();
        println!("description:");
        for line in desc.lines() {
            println!("  {line}");
        }
    }
    if !entry.examples.is_empty() {
        println!();
        println!("example shape (abstract — not your code):");
        for line in entry.examples[0].lines() {
            println!("  {line}");
        }
    }
    let refs = &entry.references;
    if !refs.literature.is_empty() || !refs.related_entries.is_empty() || !refs.external.is_empty()
    {
        println!();
        println!("references:");
        for lit in &refs.literature {
            println!("  • {lit}");
        }
        for url in &refs.external {
            println!("  • {url}");
        }
        for rel in &refs.related_entries {
            let backed = match rel.prefix_tier {
                aristo_core::canon::PrefixTier::Aristos => "backed",
                aristo_core::canon::PrefixTier::Kanon => "unbacked",
            };
            println!(
                "  • {prefix}{id}  ({prefix} {backed})",
                prefix = rel.prefix_tier.as_prefix(),
                id = rel.canon_id,
            );
        }
    }
    println!();
    Ok(())
}

fn canon_error_to_cli(e: CanonError) -> CliError {
    match &e {
        CanonError::NotEnabled => CliError::Other {
            message: "canon API is disabled for this client.\n  \
                      Run `aristo auth login` if you haven't, or check \
                      `[canon] enabled = true` in aristo.toml."
                .into(),
            exit_code: 1,
        },
        CanonError::Auth(_) => CliError::Other {
            message: format!(
                "canon API auth error: {e}.\n  \
                 Try `aristo auth logout` then `aristo auth login` with a fresh token."
            ),
            exit_code: 1,
        },
        CanonError::Network(_) | CanonError::Timeout => CliError::Other {
            message: format!(
                "canon API unreachable: {e}.\n  \
                 Check your network; the cached state in `.aristo/canon-matches.toml` \
                 remains valid for previously-matched annotations."
            ),
            exit_code: 1,
        },
        _ => CliError::Other {
            message: format!("canon API error: {e}"),
            exit_code: 1,
        },
    }
}
