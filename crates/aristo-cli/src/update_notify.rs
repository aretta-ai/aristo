//! Interactive "a newer aristo is available" notice.
//!
//! Wraps [`aristo_core::update_check`] with the gating a CLI needs: the
//! check runs only when stderr is a terminal, `CI` is unset, and the
//! user hasn't opted out via `ARISTO_NO_UPDATE_NOTIFIER`. The notice
//! prints to stderr *after* the command's real output, so
//! machine-readable stdout is never touched and pipelines stay
//! deterministic.

use std::io::IsTerminal;

/// Env var that disables the update notice entirely (set to any value).
const OPT_OUT_ENV: &str = "ARISTO_NO_UPDATE_NOTIFIER";

/// Print an upgrade notice to stderr if one is due. Best-effort and
/// silent on every failure — never affects the command's exit code.
pub fn maybe_notify() {
    if !enabled() {
        return;
    }
    if let Some(msg) = aristo_core::update_check::check(env!("CARGO_PKG_VERSION")) {
        eprintln!("{msg}");
    }
}

/// Read the real environment and stderr, then defer to [`enabled_with`].
fn enabled() -> bool {
    enabled_with(
        std::env::var_os(OPT_OUT_ENV).is_some(),
        std::env::var_os("CI").is_some(),
        std::io::stderr().is_terminal(),
    )
}

/// Pure gate decision: notify only on an interactive terminal, with no
/// opt-out and no `CI` marker. Keeping this separate lets the matrix be
/// tested without mutating process env (the workspace forbids
/// `unsafe_code`, which `set_var` requires).
fn enabled_with(opt_out: bool, ci: bool, stderr_is_terminal: bool) -> bool {
    !opt_out && !ci && stderr_is_terminal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifies_on_interactive_terminal_without_optout_or_ci() {
        assert!(enabled_with(false, false, true));
    }

    #[test]
    fn suppressed_when_stderr_not_a_terminal() {
        assert!(!enabled_with(false, false, false));
    }

    #[test]
    fn suppressed_in_ci_even_on_a_terminal() {
        assert!(!enabled_with(false, true, true));
    }

    #[test]
    fn suppressed_when_opted_out() {
        assert!(!enabled_with(true, false, true));
    }
}
