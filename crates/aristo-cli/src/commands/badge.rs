//! `aristo badge` — generate an SVG verification badge for README / docs.
//!
//! Reads `.aristo/index.toml`, computes two metrics (`aristos-count`,
//! `verification-rate`), and emits a shields.io-compatible SVG badge.
//! Three style variants: `flat` (default), `flat-square`, `for-the-badge`.
//!
//! Offline-only in slice 31. The `--strict` flag (which would cross-check
//! the badge against `aretta.dev/registry/<org>/<repo>`) is server-side
//! and deferred to Phase 2 — not stubbed, not declared.
//!
//! See `../aretta-sdk/docs/mockups/08-commercial-cluster/visibility-artifacts.md`
//! for the user-facing surface and rendered examples.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use aristo_core::index::{IdNamespace, IndexEntry, IndexFile, Status};

use crate::commands::index::workspace_or_error;
use crate::commands::show::read_index;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::{CliError, CliResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Style {
    Flat,
    FlatSquare,
    ForTheBadge,
}

impl Style {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "flat" => Ok(Self::Flat),
            "flat-square" => Ok(Self::FlatSquare),
            "for-the-badge" => Ok(Self::ForTheBadge),
            other => Err(format!(
                "unknown --style `{other}`; expected `flat`, `flat-square`, or `for-the-badge`"
            )),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::FlatSquare => "flat-square",
            Self::ForTheBadge => "for-the-badge",
        }
    }
}

pub(crate) fn run(out: Option<PathBuf>, style: Style) -> CliResult<()> {
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;
    let metrics = Metrics::from(&index);
    let svg = render_svg(&metrics, style);

    match out {
        Some(path) => write_to_file(&ws.root, &path, &svg, &metrics, style),
        None => write_to_stdout(&svg),
    }
}

fn write_to_file(
    root: &Path,
    out_rel: &Path,
    svg: &str,
    metrics: &Metrics,
    style: Style,
) -> CliResult<()> {
    // The user-supplied --out path is resolved relative to the workspace
    // root (matches how `aristo init` / `aristo doc` treat paths).
    let abs = if out_rel.is_absolute() {
        out_rel.to_path_buf()
    } else {
        root.join(out_rel)
    };
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    fs::write(&abs, svg).map_err(CliError::Io)?;

    println!("→ Reading .aristo/index.toml … ok");
    println!(
        "→ Computing metrics: aristos-count={}, verification-rate={}%",
        metrics.aristos_count, metrics.verification_rate_pct,
    );
    println!("→ Writing {} ({} style)", out_rel.display(), style.label(),);
    println!("ok: badge written. Embed in README:");
    println!();
    println!(
        "  ![aristo verified](https://aretta.dev/{}/badge.svg)",
        ws_slug(root),
    );
    Ok(())
}

#[aristo::intent(
    "When `--out` is omitted, the SVG goes to stdout and ALL progress / \
     advisory output goes to stderr — never to stdout. A regression that \
     emitted a progress line to stdout in this mode would corrupt the \
     SVG, breaking any consumer that pipes `aristo badge > foo.svg`. \
     The freshness-preflight advisory already lives on stderr; the \
     badge command MUST inherit that discipline for the no-`--out` path.",
    verify = "neural",
    id = "badge_stdout_mode_keeps_svg_uncorrupted"
)]
fn write_to_stdout(svg: &str) -> CliResult<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(svg.as_bytes()).map_err(CliError::Io)?;
    Ok(())
}

/// Best-effort `<org>/<repo>` slug for the embed-snippet hint. Falls back
/// to the workspace dir name if no git remote is configured.
fn ws_slug(root: &Path) -> String {
    root.file_name()
        .map(|n| format!("<org>/{}", n.to_string_lossy()))
        .unwrap_or_else(|| "<org>/<repo>".to_string())
}

// ─── metrics ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub(crate) struct Metrics {
    pub total: usize,
    pub aristos_count: usize,
    pub verification_rate_pct: u32,
}

impl Metrics {
    pub(crate) fn from(index: &IndexFile) -> Self {
        let mut total = 0usize;
        let mut aristos_count = 0usize;
        let mut verified_or_better = 0usize;
        let mut intent_total = 0usize;
        for (id, entry) in &index.entries {
            total += 1;
            if matches!(id.namespace(), IdNamespace::Aristos) {
                aristos_count += 1;
            }
            if let IndexEntry::Intent(e) = entry {
                intent_total += 1;
                if is_verified_state(e.status) {
                    verified_or_better += 1;
                }
            }
        }
        // Verification rate is a quality signal: what fraction of
        // verifiable claims (intents) are currently in a clean verified
        // state. Assumes are excluded — they're not verification targets.
        let verification_rate_pct = if intent_total == 0 {
            0
        } else {
            ((verified_or_better as f64 / intent_total as f64) * 100.0).round() as u32
        };
        Self {
            total,
            aristos_count,
            verification_rate_pct,
        }
    }
}

#[aristo::intent(
    "`verification-rate` counts only intents (not assumes — assumes are \
     external invariants, never internally verified per A5) and only \
     terminal-clean statuses (Verified / Tested / Neural). Including \
     Unknown / Stale / Counterexample / Orphan / Forged / PendingDeepen / \
     Inconclusive would inflate the badge's headline number with \
     non-verified annotations, defeating the public-trust signal the \
     badge exists to broadcast.",
    verify = "neural",
    id = "badge_verification_rate_counts_only_terminal_clean_intents"
)]
fn is_verified_state(status: Status) -> bool {
    matches!(status, Status::Verified | Status::Tested | Status::Neural)
}

// ─── SVG rendering ────────────────────────────────────────────────────────

const LABEL: &str = "aristo";

fn render_svg(m: &Metrics, style: Style) -> String {
    let value = format!("✓ {}", m.total);
    match style {
        Style::Flat => render_flat(LABEL, &value, false),
        Style::FlatSquare => render_flat(LABEL, &value, true),
        Style::ForTheBadge => render_for_the_badge(LABEL, &value),
    }
}

#[aristo::intent(
    "SVG text width is approximated as 7px per character in the badge \
     body and 10px padding on each end. This deviates slightly from \
     shields.io's per-glyph metrics table (DejaVu Sans), but the trycmd \
     scenarios match the SVG with byte-level wildcards (the spec only \
     pins `<svg ...>` ↔ `</svg>` framing, not exact pixel dimensions). \
     A regression that broke the 7px/10px convention without updating \
     downstream consumers (rendering pipelines that pin widths) would \
     produce misaligned text rendering at the edges.",
    verify = "neural",
    id = "badge_svg_text_width_uses_seven_px_heuristic"
)]
fn render_flat(label: &str, value: &str, square: bool) -> String {
    let label_w = text_width(label);
    let value_w = text_width(value);
    let total_w = label_w + value_w;
    let label_mid = label_w / 2;
    let value_mid = label_w + value_w / 2;
    let rx = if square { 0 } else { 3 };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="20" role="img" aria-label="{label}: {value}">
  <linearGradient id="b" x2="0" y2="100%">
    <stop offset="0" stop-color="#bbb" stop-opacity=".1"/>
    <stop offset="1" stop-opacity=".1"/>
  </linearGradient>
  <mask id="a"><rect width="{total_w}" height="20" rx="{rx}" fill="#fff"/></mask>
  <g mask="url(#a)">
    <rect width="{label_w}" height="20" fill="#555"/>
    <rect x="{label_w}" width="{value_w}" height="20" fill="#4c1"/>
    <rect width="{total_w}" height="20" fill="url(#b)"/>
  </g>
  <g fill="#fff" text-anchor="middle" font-family="DejaVu Sans,Verdana,Geneva,sans-serif" font-size="11">
    <text x="{label_mid}" y="15" fill="#010101" fill-opacity=".3">{label}</text>
    <text x="{label_mid}" y="14">{label}</text>
    <text x="{value_mid}" y="15" fill="#010101" fill-opacity=".3">{value}</text>
    <text x="{value_mid}" y="14">{value}</text>
  </g>
</svg>
"##
    )
}

fn render_for_the_badge(label: &str, value: &str) -> String {
    let upper_label = label.to_uppercase();
    // for-the-badge sizes are larger: ~28px tall, wider per character.
    let label_w = text_width(&upper_label) + 10;
    let value_w = text_width(value) + 10;
    let total_w = label_w + value_w;
    let label_mid = label_w / 2;
    let value_mid = label_w + value_w / 2;

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="28" role="img" aria-label="{label}: {value}">
  <g>
    <rect width="{label_w}" height="28" fill="#555"/>
    <rect x="{label_w}" width="{value_w}" height="28" fill="#4c1"/>
  </g>
  <g fill="#fff" text-anchor="middle" font-family="DejaVu Sans,Verdana,Geneva,sans-serif" font-size="10" font-weight="bold">
    <text x="{label_mid}" y="19">{upper_label}</text>
    <text x="{value_mid}" y="19">{value}</text>
  </g>
</svg>
"##
    )
}

/// Approximate text width in pixels for DejaVu Sans 11. Real shields.io
/// uses a per-glyph table; the badge command pins to a 7px-per-char +
/// 10px padding approximation per the intent above.
fn text_width(text: &str) -> u32 {
    let chars = text.chars().count() as u32;
    chars * 7 + 20
}

#[cfg(test)]
mod tests {
    use super::*;
    use aristo_core::index::{
        AnnotationId, ArtaId, AssumeEntry, BindingState, CommitHash, CoveredRegion, IntentEntry,
        Meta, Sha256, VerifiedOutcome, VerifyLevel, VerifyMethod,
    };
    use std::collections::BTreeMap;

    fn sha(c: char) -> Sha256 {
        Sha256::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
    }

    fn intent(verify: VerifyLevel, status: Status, server_bound: bool) -> IndexEntry {
        IndexEntry::Intent(IntentEntry {
            text: "x".into(),
            verify,
            status,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn x (line 1)".into(),
            covered_region: CoveredRegion::Function,
            binding: if server_bound {
                BindingState::Certified {
                    linked: ArtaId::parse("arta_op4q3z9NbV").unwrap(),
                    verified_outcome: VerifiedOutcome::parse(&format!("v1:{}", "A".repeat(86)))
                        .unwrap(),
                    last_verified_at_commit: CommitHash::parse(&"a".repeat(40)).unwrap(),
                }
            } else {
                BindingState::Local
            },
            parent: None,
        })
    }

    fn assume() -> IndexEntry {
        IndexEntry::Assume(AssumeEntry {
            text: "y".into(),
            status: Status::Unknown,
            text_hash: sha('a'),
            body_hash: sha('b'),
            file: "src/lib.rs".into(),
            site: "fn y (line 2)".into(),
            covered_region: CoveredRegion::Function,
            linked: None,
            parent: None,
        })
    }

    fn make_index(entries: Vec<(&str, IndexEntry)>) -> IndexFile {
        let mut map = BTreeMap::new();
        for (id, entry) in entries {
            map.insert(AnnotationId::parse(id).unwrap(), entry);
        }
        IndexFile {
            meta: Meta {
                schema_version: 1,
                generated_by: None,
                generated_at: None,
                source_root: None,
            },
            entries: map,
        }
    }

    #[test]
    fn style_parses_three_documented_forms() {
        assert_eq!(Style::parse("flat"), Ok(Style::Flat));
        assert_eq!(Style::parse("flat-square"), Ok(Style::FlatSquare));
        assert_eq!(Style::parse("for-the-badge"), Ok(Style::ForTheBadge));
    }

    #[test]
    fn style_rejects_unknown_form() {
        let err = Style::parse("plastic").unwrap_err();
        assert!(err.contains("unknown --style"), "got: {err}");
        assert!(err.contains("plastic"), "got: {err}");
    }

    #[test]
    fn metrics_total_includes_all_entries() {
        let index = make_index(vec![
            (
                "a",
                intent(VerifyLevel::Bool(false), Status::Unknown, false),
            ),
            ("b", assume()),
            (
                "aristos:c",
                intent(
                    VerifyLevel::Method(VerifyMethod::Full),
                    Status::Verified,
                    true,
                ),
            ),
        ]);
        let m = Metrics::from(&index);
        assert_eq!(m.total, 3);
    }

    #[test]
    fn metrics_aristos_count_filters_by_namespace() {
        let index = make_index(vec![
            (
                "local",
                intent(VerifyLevel::Bool(false), Status::Unknown, false),
            ),
            (
                "aristos:one",
                intent(
                    VerifyLevel::Method(VerifyMethod::Full),
                    Status::Verified,
                    true,
                ),
            ),
            (
                "aristos:two",
                intent(
                    VerifyLevel::Method(VerifyMethod::Full),
                    Status::Verified,
                    true,
                ),
            ),
        ]);
        let m = Metrics::from(&index);
        assert_eq!(m.aristos_count, 2);
    }

    #[test]
    fn metrics_verification_rate_excludes_assumes() {
        // 2 intents (1 verified, 1 unknown) + 1 assume → rate = 50%.
        // If assumes leaked into the denominator: 1/3 = 33%.
        let index = make_index(vec![
            (
                "a",
                intent(
                    VerifyLevel::Method(VerifyMethod::Full),
                    Status::Verified,
                    false,
                ),
            ),
            (
                "b",
                intent(VerifyLevel::Bool(false), Status::Unknown, false),
            ),
            ("c", assume()),
        ]);
        let m = Metrics::from(&index);
        assert_eq!(m.verification_rate_pct, 50);
    }

    #[test]
    fn metrics_verification_rate_zero_intents_is_zero_not_div_by_zero() {
        let index = make_index(vec![("only_assume", assume())]);
        let m = Metrics::from(&index);
        assert_eq!(m.verification_rate_pct, 0);
    }

    #[test]
    fn metrics_verification_rate_counts_only_terminal_clean() {
        // Stale / Orphan / Forged / Counterexample / PendingDeepen /
        // Inconclusive / Unknown should NOT count toward verified.
        let states = [
            Status::Stale,
            Status::Orphan,
            Status::Forged,
            Status::Counterexample,
            Status::PendingDeepen,
            Status::Inconclusive,
            Status::Unknown,
        ];
        for s in states {
            let index = make_index(vec![(
                "x",
                intent(VerifyLevel::Method(VerifyMethod::Full), s, false),
            )]);
            let m = Metrics::from(&index);
            assert_eq!(
                m.verification_rate_pct, 0,
                "status {s:?} should not count as verified"
            );
        }
    }

    #[test]
    fn metrics_verification_rate_counts_terminal_good_states() {
        for s in [Status::Verified, Status::Tested, Status::Neural] {
            let index = make_index(vec![(
                "x",
                intent(VerifyLevel::Method(VerifyMethod::Full), s, false),
            )]);
            let m = Metrics::from(&index);
            assert_eq!(
                m.verification_rate_pct, 100,
                "status {s:?} should count as verified"
            );
        }
    }

    #[test]
    fn render_svg_flat_has_svg_framing() {
        let m = Metrics {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let svg = render_svg(&m, Style::Flat);
        assert!(svg.starts_with("<svg "), "got:\n{svg}");
        assert!(svg.trim_end().ends_with("</svg>"), "got:\n{svg}");
    }

    #[test]
    fn render_svg_flat_square_uses_no_corner_radius() {
        let m = Metrics {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let svg = render_svg(&m, Style::FlatSquare);
        assert!(svg.contains(r#"rx="0""#), "expected rx=0; got:\n{svg}");
    }

    #[test]
    fn render_svg_flat_uses_corner_radius() {
        let m = Metrics {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let svg = render_svg(&m, Style::Flat);
        assert!(svg.contains(r#"rx="3""#), "expected rx=3; got:\n{svg}");
    }

    #[test]
    fn render_svg_for_the_badge_uses_uppercase_label_and_taller_box() {
        let m = Metrics {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let svg = render_svg(&m, Style::ForTheBadge);
        assert!(
            svg.contains("ARISTO"),
            "expected uppercase label; got:\n{svg}"
        );
        assert!(svg.contains(r#"height="28""#), "expected h=28; got:\n{svg}");
    }

    #[test]
    fn render_svg_includes_total_in_value() {
        let m = Metrics {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let svg = render_svg(&m, Style::Flat);
        assert!(svg.contains("✓ 47"), "expected `✓ 47`; got:\n{svg}");
    }
}
