//! `aristo badge` — generate an SVG verification badge for README / docs.
//!
//! Reads `.aristo/index.toml` + walks the workspace source for the
//! coverage formula's denominator, computes the D7 visible score and
//! D8 tier (with the D4 Areté gate), and emits a shields.io-compatible
//! SVG. Three style variants (`flat`, `flat-square`, `for-the-badge`)
//! × three metric variants (`tier`, `count`, `rate`).
//!
//! Slice 31.5 makes `tier` the default headline metric. `count` and
//! `rate` are kept accessible via `--metric` so existing README badges
//! that pinned `aristos-count` or `verification-rate` semantics during
//! slice 31 can preserve them by switching the flag explicitly.
//!
//! Offline-only. The `--strict` flag (which would cross-check against
//! `aretta.dev/registry/<org>/<repo>`) is server-side and remains
//! deferred to Phase 2 — not stubbed, not declared.
//!
//! See `../aretta-sdk/docs/mockups/08-commercial-cluster/visibility-artifacts.md`
//! for the user-facing v1 surface and
//! `docs/decisions/badge-tier-scheme.md` for the locked tier formula
//! + palette.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use aristo_core::badge::{compute_tier, Tier, TierComputation};
use aristo_core::index::{IdNamespace, IndexEntry, IndexFile, Status};
use aristo_core::walk::{count_fns_per_module_with, WalkOptions};

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

/// Which metric lands in the SVG value half. Progress lines always
/// report all three so the diagnostic surface is uniform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Metric {
    /// The locked D7→D8 tier (default). Value-half color picks up the
    /// per-tier palette from D11.
    Tier,
    /// Total annotation count — slice 31's original surface.
    Count,
    /// Verification rate percentage — `verified-clean / verifiable-intents`.
    Rate,
}

impl Metric {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "tier" => Ok(Self::Tier),
            "count" => Ok(Self::Count),
            "rate" => Ok(Self::Rate),
            other => Err(format!(
                "unknown --metric `{other}`; expected `count`, `rate`, or `tier`"
            )),
        }
    }
}

pub(crate) fn run(out: Option<PathBuf>, style: Style, metric: Metric) -> CliResult<()> {
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;

    let counters = Counters::from(&index);
    // Walk the workspace source for the per-module fn surface that
    // the coverage score needs as its denominator. The badge command
    // is read-only against the index, so we don't propagate walk
    // options from any config write path — `WalkOptions::none()` is
    // the conservative default and matches what slice 31 shipped.
    let fn_counts =
        count_fns_per_module_with(&ws.root, &WalkOptions::none()).map_err(|e| CliError::Other {
            message: format!("failed to walk source for badge coverage: {e}"),
            exit_code: 1,
        })?;
    let default_method = ws.load_config().verify.default_method;
    let computation = compute_tier(&index, &fn_counts, default_method);

    let svg = render_svg(&counters, &computation, style, metric);

    match out {
        Some(path) => write_to_file(&ws.root, &path, &svg, &counters, &computation, style),
        None => write_to_stdout(&svg),
    }
}

fn write_to_file(
    root: &Path,
    out_rel: &Path,
    svg: &str,
    counters: &Counters,
    computation: &TierComputation,
    style: Style,
) -> CliResult<()> {
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
        "→ Computing metrics: aristos-count={}, verification-rate={}%, score={:.2}, tier={}",
        counters.aristos_count,
        counters.verification_rate_pct,
        computation.visible_score,
        computation.tier.label(),
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

// ─── counters (the simple metrics the progress line surfaces) ─────────

#[derive(Debug, Clone, Copy)]
pub(crate) struct Counters {
    pub total: usize,
    pub aristos_count: usize,
    pub verification_rate_pct: u32,
}

impl Counters {
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

// ─── SVG rendering ────────────────────────────────────────────────────

const LABEL: &str = "aristo";

/// Locked simpleicons-style bridge-as-Ω logo (D11). 24×24 viewBox,
/// fill="currentColor" so the badge's color group propagates.
const LOGO_PATHS: &str = concat!(
    r#"<path d="M5 4 Q12 12 19 4 L19 5.5 Q12 13.5 5 5.5 Z"/>"#,
    r#"<path d="M2 21 L3 4 L7 4 L8 21 Z"/>"#,
    r#"<path d="M16 21 L17 4 L21 4 L22 21 Z"/>"#,
    r#"<path d="M1 21 L23 21 L23 22.5 L1 22.5 Z"/>"#,
);

fn render_svg(
    counters: &Counters,
    computation: &TierComputation,
    style: Style,
    metric: Metric,
) -> String {
    let value = headline_value(counters, computation, metric);
    let value_color = value_color(computation.tier, metric);
    match style {
        Style::Flat => render_flat(LABEL, &value, value_color, false),
        Style::FlatSquare => render_flat(LABEL, &value, value_color, true),
        Style::ForTheBadge => render_for_the_badge(LABEL, &value, value_color),
    }
}

fn headline_value(counters: &Counters, computation: &TierComputation, metric: Metric) -> String {
    match metric {
        Metric::Tier => computation.tier.label().to_string(),
        Metric::Count => format!("✓ {}", counters.total),
        Metric::Rate => format!("{}%", counters.verification_rate_pct),
    }
}

/// The value half of the badge picks up the tier color ONLY when
/// `--metric=tier` is in play — that's where the palette signal is
/// load-bearing. `count` and `rate` get the slice-31 default green
/// so existing README embeds keep their visual identity.
fn value_color(tier: Tier, metric: Metric) -> &'static str {
    match metric {
        Metric::Tier => tier.color_hex(),
        Metric::Count | Metric::Rate => "#4c1",
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
fn render_flat(label: &str, value: &str, value_color: &str, square: bool) -> String {
    // The label half embeds the 14×14 bridge-as-Ω logo before the
    // wordmark (D11). Logo width + small gap = 18px reserved.
    let logo_w = 18u32;
    let label_text_w = text_width(label);
    let label_w = label_text_w + logo_w;
    let value_w = text_width(value);
    let total_w = label_w + value_w;
    let label_text_mid = logo_w + label_text_w / 2;
    let value_mid = label_w + value_w / 2;
    let rx = if square { 0 } else { 3 };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="20" role="img" aria-label="{label}: {value}">
  <title>{label}: {value}</title>
  <linearGradient id="b" x2="0" y2="100%">
    <stop offset="0" stop-color="#bbb" stop-opacity=".1"/>
    <stop offset="1" stop-opacity=".1"/>
  </linearGradient>
  <mask id="a"><rect width="{total_w}" height="20" rx="{rx}" fill="#fff"/></mask>
  <g mask="url(#a)">
    <rect width="{label_w}" height="20" fill="#555"/>
    <rect x="{label_w}" width="{value_w}" height="20" fill="{value_color}"/>
    <rect width="{total_w}" height="20" fill="url(#b)"/>
  </g>
  <g transform="translate(3 3)" fill="#fff">
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">{logo}</svg>
  </g>
  <g fill="#fff" text-anchor="middle" font-family="DejaVu Sans,Verdana,Geneva,sans-serif" font-size="11">
    <text x="{label_text_mid}" y="15" fill="#010101" fill-opacity=".3">{label}</text>
    <text x="{label_text_mid}" y="14">{label}</text>
    <text x="{value_mid}" y="15" fill="#010101" fill-opacity=".3">{value}</text>
    <text x="{value_mid}" y="14">{value}</text>
  </g>
</svg>
"##,
        logo = LOGO_PATHS,
    )
}

fn render_for_the_badge(label: &str, value: &str, value_color: &str) -> String {
    let upper_label = label.to_uppercase();
    // 16×16 logo for the taller style.
    let logo_w = 22u32;
    let label_text_w = text_width(&upper_label) + 10;
    let label_w = label_text_w + logo_w;
    let value_w = text_width(value) + 10;
    let total_w = label_w + value_w;
    let label_text_mid = logo_w + label_text_w / 2;
    let value_mid = label_w + value_w / 2;

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="28" role="img" aria-label="{label}: {value}">
  <title>{label}: {value}</title>
  <g>
    <rect width="{label_w}" height="28" fill="#555"/>
    <rect x="{label_w}" width="{value_w}" height="28" fill="{value_color}"/>
  </g>
  <g transform="translate(4 6)" fill="#fff">
    <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">{logo}</svg>
  </g>
  <g fill="#fff" text-anchor="middle" font-family="DejaVu Sans,Verdana,Geneva,sans-serif" font-size="10" font-weight="bold">
    <text x="{label_text_mid}" y="19">{upper_label}</text>
    <text x="{value_mid}" y="19">{value}</text>
  </g>
</svg>
"##,
        logo = LOGO_PATHS,
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
            last_critiqued_at_text_hash: None,
            last_critique_finding_count: None,
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

    fn sample_computation() -> TierComputation {
        let index = make_index(vec![(
            "a",
            intent(
                VerifyLevel::Method(VerifyMethod::Neural),
                Status::Neural,
                false,
            ),
        )]);
        let fn_counts: BTreeMap<std::path::PathBuf, u32> =
            [(std::path::PathBuf::from("src/lib.rs"), 1u32)]
                .into_iter()
                .collect();
        compute_tier(&index, &fn_counts, None)
    }

    // ─── parse surface ───────────────────────────────────────────────

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
    fn metric_parses_three_documented_forms() {
        assert_eq!(Metric::parse("tier"), Ok(Metric::Tier));
        assert_eq!(Metric::parse("count"), Ok(Metric::Count));
        assert_eq!(Metric::parse("rate"), Ok(Metric::Rate));
    }

    #[test]
    fn metric_rejects_unknown_form() {
        let err = Metric::parse("quality").unwrap_err();
        assert!(err.contains("unknown --metric"), "got: {err}");
        assert!(err.contains("quality"), "got: {err}");
        assert!(
            err.contains("count") && err.contains("rate") && err.contains("tier"),
            "diagnostic should list all three valid values; got: {err}"
        );
    }

    // ─── counters parity with slice 31 ────────────────────────────────

    #[test]
    fn counters_total_includes_all_entries() {
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
        let c = Counters::from(&index);
        assert_eq!(c.total, 3);
    }

    #[test]
    fn counters_aristos_count_filters_by_namespace() {
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
        let c = Counters::from(&index);
        assert_eq!(c.aristos_count, 2);
    }

    #[test]
    fn counters_verification_rate_excludes_assumes() {
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
        let c = Counters::from(&index);
        assert_eq!(c.verification_rate_pct, 50);
    }

    // ─── rendering — metric routing + SVG framing ────────────────────

    #[test]
    fn render_svg_default_metric_emits_tier_label_in_value() {
        let counters = Counters {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let computation = sample_computation();
        let svg = render_svg(&counters, &computation, Style::Flat, Metric::Tier);
        assert!(
            svg.contains(computation.tier.label()),
            "tier label must appear in tier-metric SVG; got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_count_metric_preserves_slice_31_surface() {
        let counters = Counters {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let computation = sample_computation();
        let svg = render_svg(&counters, &computation, Style::Flat, Metric::Count);
        assert!(svg.contains("✓ 47"), "expected `✓ 47`; got:\n{svg}");
    }

    #[test]
    fn render_svg_rate_metric_emits_percentage_value() {
        let counters = Counters {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let computation = sample_computation();
        let svg = render_svg(&counters, &computation, Style::Flat, Metric::Rate);
        assert!(svg.contains("80%"), "expected `80%`; got:\n{svg}");
    }

    #[test]
    fn render_svg_value_color_tier_uses_palette() {
        // Adept (computed from 1 neural-verified intent in a 1-fn module
        // → ratio 0.6 × coverage 1.0 = 0.6 → Adept).
        let counters = Counters {
            total: 1,
            aristos_count: 0,
            verification_rate_pct: 100,
        };
        let computation = sample_computation();
        assert_eq!(computation.tier, Tier::Adept);
        let svg = render_svg(&counters, &computation, Style::Flat, Metric::Tier);
        assert!(
            svg.contains("#C0362C"),
            "Adept tier should color with International Orange; got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_value_color_count_keeps_slice_31_green() {
        let counters = Counters {
            total: 1,
            aristos_count: 0,
            verification_rate_pct: 100,
        };
        let computation = sample_computation();
        let svg = render_svg(&counters, &computation, Style::Flat, Metric::Count);
        assert!(
            svg.contains("#4c1"),
            "count metric should keep slice-31 green; got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_flat_has_svg_framing() {
        let counters = Counters {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let computation = sample_computation();
        let svg = render_svg(&counters, &computation, Style::Flat, Metric::Tier);
        assert!(svg.starts_with("<svg "), "got:\n{svg}");
        assert!(svg.trim_end().ends_with("</svg>"), "got:\n{svg}");
    }

    #[test]
    fn render_svg_flat_square_uses_no_corner_radius() {
        let counters = Counters {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let computation = sample_computation();
        let svg = render_svg(&counters, &computation, Style::FlatSquare, Metric::Tier);
        assert!(svg.contains(r#"rx="0""#), "expected rx=0; got:\n{svg}");
    }

    #[test]
    fn render_svg_flat_uses_corner_radius() {
        let counters = Counters {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let computation = sample_computation();
        let svg = render_svg(&counters, &computation, Style::Flat, Metric::Tier);
        assert!(svg.contains(r#"rx="3""#), "expected rx=3; got:\n{svg}");
    }

    #[test]
    fn render_svg_for_the_badge_uses_uppercase_label_and_taller_box() {
        let counters = Counters {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        let computation = sample_computation();
        let svg = render_svg(&counters, &computation, Style::ForTheBadge, Metric::Tier);
        assert!(
            svg.contains("ARISTO"),
            "expected uppercase label; got:\n{svg}"
        );
        assert!(svg.contains(r#"height="28""#), "expected h=28; got:\n{svg}");
    }

    #[test]
    fn render_svg_embeds_locked_bridge_logo() {
        let counters = Counters {
            total: 0,
            aristos_count: 0,
            verification_rate_pct: 0,
        };
        let computation = sample_computation();
        // The locked bridge-as-Ω logo's first path is the catenary that
        // dips between the towers — D11. If a future edit accidentally
        // ships a different logo, this assertion catches it.
        let svg = render_svg(&counters, &computation, Style::Flat, Metric::Tier);
        assert!(
            svg.contains(r#"<path d="M5 4 Q12 12 19 4 L19 5.5 Q12 13.5 5 5.5 Z"/>"#),
            "expected locked catenary path in SVG; got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_arete_tier_uses_gold_color_and_glyph() {
        let counters = Counters {
            total: 0,
            aristos_count: 0,
            verification_rate_pct: 0,
        };
        let computation = TierComputation {
            verifiable: 1,
            verification_ratio: 1.0,
            coverage_score: 1.0,
            articulation_floor: 0.05,
            visible_score: 1.0,
            arete_gate_met: true,
            tier: Tier::Arete,
        };
        let svg = render_svg(&counters, &computation, Style::Flat, Metric::Tier);
        assert!(svg.contains("#d4a017"), "Areté gold color; got:\n{svg}");
        assert!(svg.contains("✦"), "Areté ✦ glyph; got:\n{svg}");
        assert!(svg.contains("Areté"), "Areté label; got:\n{svg}");
    }
}
