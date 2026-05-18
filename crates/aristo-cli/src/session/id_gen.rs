//! Mint fresh session ids.
//!
//! Format: `YYYYMMDDTHHMMSSZ-<6 base32 chars>`. Two properties matter:
//!
//! 1. **Time-orderable.** Lexicographic sort = chronological sort.
//!    `ls` and `aristo session list` order naturally without an
//!    extra index. The leading UTC stamp also makes the audit trail
//!    skimmable at a glance — no decoding needed.
//! 2. **Collision-resistant.** Six base32 chars on top of a
//!    second-resolution timestamp gives ~30 bits of entropy in any
//!    second-aligned bucket. Sessions are user-driven (≤1/second
//!    realistically), so this is overkill — that's the point.
//!
//! A real ULID would be more compact (26 chars vs our 22) but
//! requires either a new crate or hand-rolling the Crockford-base32
//! timestamp encoding. The readable form is friendlier for the
//! few times a user grep's session ids out of `.aristo/sessions/`.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::session::types::SessionId;

const RANDOM_SUFFIX_LEN: usize = 6;
const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// Mint a fresh session id. Pure function modulo system time + OS RNG.
#[aristo::intent(
    "Session ids start with a sortable UTC timestamp (YYYYMMDDTHHMMSSZ) \
     so `ls .aristo/sessions/active/` and `aristo session list` order \
     chronologically without an index. A refactor that put random bytes \
     first would break `aristo session list`'s expected newest-last \
     ordering and would force a per-session sort on every read.",
    verify = "neural",
    id = "session_id_timestamp_prefix_is_load_bearing_for_ordering"
)]
pub fn mint_session_id() -> SessionId {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is post-1970");
    let stamp = format_utc_compact(now.as_secs());

    let mut bytes = [0u8; RANDOM_SUFFIX_LEN];
    getrandom::getrandom(&mut bytes)
        .expect("OS RNG failed; aborting rather than emitting a low-entropy id");
    let mut suffix = String::with_capacity(RANDOM_SUFFIX_LEN);
    for b in bytes {
        suffix.push(ALPHABET[(b as usize) % ALPHABET.len()] as char);
    }

    SessionId::from_string(format!("{stamp}-{suffix}"))
}

/// Format seconds-since-epoch as `YYYYMMDDTHHMMSSZ`. Pure; no
/// allocations beyond the result string. We avoid the `time` crate's
/// formatting path here to keep the function dependency-free for the
/// id-gen module's tests.
pub(crate) fn format_utc_compact(secs: u64) -> String {
    let (y, mo, d, h, m, s) = utc_components(secs);
    format!("{y:04}{mo:02}{d:02}T{h:02}{m:02}{s:02}Z")
}

/// Format seconds-since-epoch as RFC3339 (`YYYY-MM-DDTHH:MM:SSZ`).
/// Used for `started_at` / `closed_at` / rejection-log `ts` / backlog
/// `deferred_at`. Second-resolution; sessions don't fire often enough
/// to need finer precision.
pub(crate) fn format_rfc3339(secs: u64) -> String {
    let (y, mo, d, h, m, s) = utc_components(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn utc_components(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let secs_of_day = (secs % 86_400) as u32;
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;
    let (y, mo, d) = civil_from_days(days);
    (y, mo, d, h, m, s)
}

/// Days-since-epoch → (year, month, day) via Howard Hinnant's
/// civil-from-days algorithm. Public domain reference algorithm
/// chosen for its zero-allocation purity and easy testability;
/// see http://howardhinnant.github.io/date_algorithms.html.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468; // shift epoch to 0000-03-01
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_produces_expected_shape() {
        let id = mint_session_id();
        let s = id.as_str();
        // "YYYYMMDDTHHMMSSZ-XXXXXX" = 16 + 1 + 6 = 23.
        assert_eq!(s.len(), 23, "unexpected id length for {s}");
        assert!(s.chars().nth(8).unwrap() == 'T', "missing T separator: {s}");
        assert!(s.contains('-'));
    }

    #[test]
    fn ids_minted_in_sequence_sort_chronologically() {
        // Two ids minted back-to-back must lex-compare in mint order.
        // We can't easily force timestamps apart in unit-test time, so
        // we directly call the formatter with explicit secs to assert
        // the property the prefix is designed to guarantee.
        let earlier = format_utc_compact(1_700_000_000); // 2023-11-14
        let later = format_utc_compact(1_700_000_001);
        assert!(earlier < later, "lex order should match time order");
        let across_day = format_utc_compact(1_700_086_400); // +1 day
        assert!(later < across_day);
    }

    #[test]
    fn format_utc_compact_known_values() {
        // 2026-01-01 00:00:00 UTC: easy reference value (start of year,
        // midnight, no time-of-day to verify).
        assert_eq!(format_utc_compact(1_767_225_600), "20260101T000000Z");
        // 2026-05-22 11:00:00 UTC: arbitrary mid-year value with a
        // non-trivial time-of-day component and a mid-month day.
        assert_eq!(format_utc_compact(1_779_447_600), "20260522T110000Z");
        // 2024-02-29 12:00:00 UTC: leap-day check (date algorithm
        // must handle the era-shift correctly across Feb 29).
        assert_eq!(format_utc_compact(1_709_208_000), "20240229T120000Z");
    }

    #[test]
    fn format_rfc3339_uses_dashes_and_colons() {
        // Same instant as above's first case, RFC3339 form.
        assert_eq!(format_rfc3339(1_767_225_600), "2026-01-01T00:00:00Z");
    }

    #[test]
    fn ids_minted_concurrently_are_distinct() {
        // Two mints in the same second must differ thanks to the
        // random suffix. We can't observe wall time precisely enough
        // to guarantee they hit the same second, but if the rng is
        // doing its job, repeated mints differ regardless.
        let a = mint_session_id();
        let b = mint_session_id();
        assert_ne!(a.as_str(), b.as_str());
    }
}
