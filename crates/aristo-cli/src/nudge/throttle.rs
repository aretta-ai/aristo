//! Anti-nag throttle for the human-facing nudge surface (Phase 18 #9, S0d).
//!
//! A fired signal still has to clear the throttle before it actually
//! surfaces, so the engine doesn't repeat the same nudge every turn. Two
//! ways to clear it (D8 / SPINE-PLAN "throttle"):
//!
//! 1. **Cooldown elapsed** — at least `cooldown_secs(level)` since this
//!    signal last surfaced. Higher aggressiveness = shorter cooldown; `off`
//!    never surfaces (it is already silenced upstream, but the cooldown is
//!    `None` here too as a belt-and-braces guard).
//! 2. **Material increase** — even inside the cooldown, a signal whose metric
//!    has grown by at least `step(level, base)` since it last surfaced
//!    re-arms (a genuinely worse backlog shouldn't wait out the timer).
//!
//! Cadence is wall-clock here (the SPINE-PLAN's "edit-window" notion is
//! approximated by minutes — simpler and robust against missing SessionStart
//! events; the per-signal record carries the last-fired epoch + last metric).
//! The Agent-audience surface (authoring debt) is NOT throttled here — it is
//! same-turn only and cheap; this module governs the consolidated human nudge.

use aristo_core::config::Aggressiveness;

use super::state::ThrottleRecord;

/// Seconds a human signal must wait between surfacings, by aggressiveness.
/// `None` = never surface (`off`).
pub fn cooldown_secs(level: Aggressiveness) -> Option<u64> {
    match level {
        Aggressiveness::Off => None,
        Aggressiveness::Low => Some(30 * 60),
        Aggressiveness::Medium => Some(10 * 60),
        Aggressiveness::High => Some(3 * 60),
    }
}

/// The metric increase that re-arms a signal inside its cooldown. Scales with
/// the signal's base (so a count-signal needs ~a few more, a fraction-signal a
/// bit more fraction) and shrinks at higher aggressiveness (re-arms sooner).
/// Floored at 1.0 so a count signal always needs at least one more.
pub fn rearm_step(level: Aggressiveness, base: f64) -> f64 {
    let k = match level {
        Aggressiveness::Off => return f64::INFINITY, // never re-arms
        Aggressiveness::Low => 1.0,
        Aggressiveness::Medium => 0.66,
        Aggressiveness::High => 0.33,
    };
    (base * k).max(1.0)
}

/// Whether a fired signal may surface now: cooldown elapsed OR a material
/// increase since it last surfaced. A signal that never surfaced always may.
pub fn may_surface(
    record: Option<&ThrottleRecord>,
    now_epoch: u64,
    level: Aggressiveness,
    current_metric: f64,
    base: f64,
) -> bool {
    let Some(cooldown) = cooldown_secs(level) else {
        return false; // off → never
    };
    let Some(record) = record else {
        return true; // never surfaced → surface
    };
    let elapsed = now_epoch.saturating_sub(record.last_fired_epoch);
    if elapsed >= cooldown {
        return true;
    }
    // Inside the cooldown: re-arm only on a material increase.
    current_metric >= record.last_surfaced_metric + rearm_step(level, base)
}

/// The record to store after a signal surfaces.
pub fn record_after_surface(now_epoch: u64, current_metric: f64) -> ThrottleRecord {
    ThrottleRecord {
        last_fired_epoch: now_epoch,
        last_surfaced_metric: current_metric,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: u64 = 3600;

    #[test]
    fn off_never_surfaces() {
        assert!(!may_surface(None, HOUR, Aggressiveness::Off, 100.0, 3.0));
    }

    #[test]
    fn never_surfaced_always_may() {
        assert!(may_surface(None, 0, Aggressiveness::Low, 1.0, 3.0));
    }

    #[test]
    fn waits_out_the_cooldown() {
        let rec = ThrottleRecord {
            last_fired_epoch: HOUR,
            last_surfaced_metric: 3.0,
        };
        // 1 minute later under medium (10m cooldown), same metric → blocked.
        assert!(!may_surface(
            Some(&rec),
            HOUR + 60,
            Aggressiveness::Medium,
            3.0,
            3.0
        ));
        // 11 minutes later → cooldown elapsed → may.
        assert!(may_surface(
            Some(&rec),
            HOUR + 11 * 60,
            Aggressiveness::Medium,
            3.0,
            3.0
        ));
    }

    #[test]
    fn material_increase_rearms_inside_cooldown() {
        let rec = ThrottleRecord {
            last_fired_epoch: HOUR,
            last_surfaced_metric: 3.0,
        };
        // Still inside cooldown (1 min later). base 3.0, medium k=0.66 →
        // step = max(1.98, 1.0) = 1.98. 3.0 + 1.98 = 4.98.
        assert!(
            !may_surface(Some(&rec), HOUR + 60, Aggressiveness::Medium, 4.0, 3.0),
            "metric 4.0 is below the re-arm threshold 4.98 → still throttled"
        );
        assert!(
            may_surface(Some(&rec), HOUR + 60, Aggressiveness::Medium, 5.0, 3.0),
            "metric 5.0 clears the 4.98 re-arm threshold"
        );
    }

    #[test]
    fn higher_aggressiveness_rearms_on_smaller_increase() {
        let rec = ThrottleRecord {
            last_fired_epoch: HOUR,
            last_surfaced_metric: 3.0,
        };
        // base 3.0: high k=0.33 → step max(0.99,1.0)=1.0 → threshold 4.0;
        // low k=1.0 → step 3.0 → threshold 6.0. metric 4.5 inside cooldown:
        assert!(may_surface(
            Some(&rec),
            HOUR + 60,
            Aggressiveness::High,
            4.5,
            3.0
        ));
        assert!(!may_surface(
            Some(&rec),
            HOUR + 60,
            Aggressiveness::Low,
            4.5,
            3.0
        ));
    }

    #[test]
    fn cooldown_shrinks_with_aggressiveness() {
        assert_eq!(cooldown_secs(Aggressiveness::Off), None);
        let low = cooldown_secs(Aggressiveness::Low).unwrap();
        let med = cooldown_secs(Aggressiveness::Medium).unwrap();
        let high = cooldown_secs(Aggressiveness::High).unwrap();
        assert!(low > med && med > high, "{low} > {med} > {high}");
    }
}
