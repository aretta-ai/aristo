//! `aristo canon` subcommand family + shared canon-step helpers.
//!
//! `runner` is the API-call + cache-update primitive shared between
//! `aristo stamp` (PR #5) and `aristo critique` (PR #6). The
//! user-facing `aristo canon {show, list, refresh, unbind,
//! request-verify}` subcommands land in PR #8/#9 alongside the
//! trust-card renderer.
//!
//! `accept` lands PR #7's atomic source rewrite + index rebind +
//! cache pending → accepted move for an accepted canon match.

pub(crate) mod accept;
pub(crate) mod list;
pub(crate) mod refresh;
pub(crate) mod reject;
pub(crate) mod runner;
pub(crate) mod show;
