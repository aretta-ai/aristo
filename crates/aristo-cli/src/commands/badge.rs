//! `aristo badge` — generate an SVG verification badge for README / docs.
//!
//! Reads `.aristo/index.toml` + walks the workspace source for the
//! coverage formula's denominator, computes the D7 visible score and
//! D8 tier (with the D4 Areté gate), and renders a shields.io-quality
//! SVG via the `badge-maker` crate — entirely offline, so the output
//! sits flush with the crate / docs / CI shields badges in a README.
//! Three style variants (`flat-square` default, `flat`, `plastic`)
//! × three metric variants (`tier`, `count`, `rate`).
//!
//! Slice 31.5 makes `tier` the default headline metric. `count` and
//! `rate` are kept accessible via `--metric` so existing README badges
//! that pinned `aristos-count` or `verification-rate` semantics during
//! slice 31 can preserve them by switching the flag explicitly.
//!
//! Offline-only. The `--strict` flag (which would cross-check against
//! `aretta.ai/registry/<org>/<repo>`) is server-side and remains
//! deferred to Phase 2 — not stubbed, not declared.
//!
//! See `../aretta-sdk/docs/mockups/08-commercial-cluster/visibility-artifacts.md`
//! for the user-facing v1 surface and
//! `docs/decisions/badge-tier-scheme.md` for the locked tier formula
//! + palette.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use badge_maker::{BadgeBuilder, Logo, Style as BmStyle};

use aristo_core::badge::{compute_tier, Tier, TierComputation};
use aristo_core::index::{IdNamespace, IndexEntry, IndexFile, Status};
use aristo_core::walk::{count_fns_per_module_with, WalkOptions};

use crate::commands::index::workspace_or_error;
use crate::commands::show::load_index;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::{CliError, CliResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Style {
    Flat,
    FlatSquare,
    Plastic,
}

impl Style {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "flat" => Ok(Self::Flat),
            "flat-square" => Ok(Self::FlatSquare),
            "plastic" => Ok(Self::Plastic),
            other => Err(format!(
                "unknown --style `{other}`; expected `flat`, `flat-square`, or `plastic`"
            )),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::FlatSquare => "flat-square",
            Self::Plastic => "plastic",
        }
    }

    fn to_badge_maker(self) -> BmStyle {
        match self {
            Self::Flat => BmStyle::Flat,
            Self::FlatSquare => BmStyle::FlatSquare,
            Self::Plastic => BmStyle::Plastic,
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
    let index = load_index(&ws)?;

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
    println!("  ![aristo verified]({})", out_rel.display());
    let _ = root;
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
    // Delegates to the single shared definition (A2) so the badge rate, the
    // status counts, and the nudge engine can never disagree on "verified".
    status.is_terminal_clean()
}

// ─── SVG rendering ────────────────────────────────────────────────────

/// The badge's label half — the project name, on the dark stone half.
const LABEL: &str = "aristo";

/// Dark stone (D11) label-half background. The `aristo` wordmark and the
/// white bridge glyph sit on it; the tier color fills the message half.
const LABEL_COLOR: &str = "#2b2824";

/// The bridge glyph, embedded as the badge logo: a stroked suspension
/// bridge (catenary arc + deck + three cables), white so it reads on the
/// dark label half. `badge-maker` inlines this SVG as a base64 data-URI,
/// so the rendered badge is fully self-contained — no external logo
/// fetch, which keeps `aristo badge` an offline command. The logo
/// *concept* is still unsettled (bridge vs. keystone vs. shield); this
/// is the mockup placeholder, not a locked logo.
const BRIDGE_GLYPH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 16.5 Q12 5 21 16.5"/><path d="M3 16.5h18"/><path d="M8 11v5.5"/><path d="M12 8v8.5"/><path d="M16 11v5.5"/></svg>"##;

/// Render the badge SVG via `badge-maker` (shields.io-parity output,
/// generated entirely offline): the `aristo` label on the dark stone
/// half with the bridge glyph, then the headline value (tier / count /
/// rate) in the tier color on the message half.
fn render_svg(
    counters: &Counters,
    computation: &TierComputation,
    style: Style,
    metric: Metric,
) -> String {
    let value = headline_value(counters, computation, metric);
    let color = value_color(computation.tier, metric);
    BadgeBuilder::new()
        .label(LABEL)
        .message(&value)
        .label_color_parse(LABEL_COLOR)
        .color_parse(color)
        .style(style.to_badge_maker())
        .logo(Logo::SVGLogo {
            svg: BRIDGE_GLYPH.to_string(),
            color: None,
            width: 14,
            padding: 3,
        })
        .build()
        // The inputs are a fixed label, a fixed dark stone label color,
        // and a value color drawn from the closed D11 palette (or the
        // count/rate green) — all valid, so build never errors here.
        .expect("badge inputs are fixed valid colors + style")
        .svg()
}

fn headline_value(counters: &Counters, computation: &TierComputation, metric: Metric) -> String {
    match metric {
        Metric::Tier => computation.tier.label().to_string(),
        Metric::Count => format!("✓ {}", counters.total),
        Metric::Rate => format!("{}%", counters.verification_rate_pct),
    }
}

/// The message half picks up the tier color ONLY for `--metric=tier`
/// (where the palette signal is load-bearing). `count` and `rate` keep
/// the slice-31 default green so existing README embeds keep identity.
fn value_color(tier: Tier, metric: Metric) -> &'static str {
    match metric {
        Metric::Tier => tier.color_hex(),
        Metric::Count | Metric::Rate => "#4c1",
    }
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
        assert_eq!(Style::parse("plastic"), Ok(Style::Plastic));
    }

    #[test]
    fn style_rejects_unknown_form() {
        // `for-the-badge` is no longer offered (badge-maker renders flat /
        // flat-square / plastic), so it now parses as unknown.
        let err = Style::parse("for-the-badge").unwrap_err();
        assert!(err.contains("unknown --style"), "got: {err}");
        assert!(err.contains("for-the-badge"), "got: {err}");
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

    #[test]
    fn metrics_and_badge_agree_on_verified_clean_count() {
        // A2 anti-drift: the badge's verification_rate_pct and the nudge
        // engine's Metrics derive their "verified" numerator from ONE shared
        // definition (Status::is_terminal_clean). This pins the two surfaces
        // together so a future change to one can't silently diverge.
        use aristo_core::metrics::Metrics;
        let index = make_index(vec![
            (
                "a",
                intent(
                    VerifyLevel::Method(VerifyMethod::Neural),
                    Status::Neural,
                    false,
                ),
            ), // clean
            (
                "b",
                intent(
                    VerifyLevel::Method(VerifyMethod::Test),
                    Status::Stale,
                    false,
                ),
            ), // not clean
            (
                "c",
                intent(
                    VerifyLevel::Method(VerifyMethod::Full),
                    Status::Verified,
                    false,
                ),
            ), // clean
            (
                "d",
                intent(VerifyLevel::Bool(false), Status::Unknown, false),
            ), // doc-only
            ("e", assume()), // assume
        ]);
        let counters = Counters::from(&index);
        let metrics = Metrics::from_index(&index, &BTreeMap::new(), None);

        assert_eq!(metrics.intents, 4, "4 intents (assume excluded)");
        assert_eq!(metrics.verified_clean, 2, "a + c are terminal-clean");
        // The badge's published rate (verified-clean / all-intents) must be
        // reproducible from the same shared count the engine reports.
        let expected_pct =
            ((metrics.verified_clean as f64 / metrics.intents as f64) * 100.0).round() as u32;
        assert_eq!(
            counters.verification_rate_pct, expected_pct,
            "badge rate must derive from the same verified-clean count the engine sees"
        );
    }

    // ─── rendering via badge-maker (shields-parity output) ───────────

    fn demo_counters() -> Counters {
        Counters {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        }
    }

    fn arete_computation() -> TierComputation {
        TierComputation {
            verifiable: 1,
            verification_ratio: 1.0,
            coverage_score: 1.0,
            articulation_floor: 0.05,
            visible_score: 1.0,
            arete_gate_met: true,
            tier: Tier::Arete,
        }
    }

    #[test]
    fn render_svg_default_metric_emits_tier_label_in_message() {
        let svg = render_svg(
            &demo_counters(),
            &sample_computation(),
            Style::FlatSquare,
            Metric::Tier,
        );
        assert!(
            svg.contains(sample_computation().tier.label()),
            "tier label must appear in the tier-metric SVG; got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_count_metric_preserves_slice_31_surface() {
        let svg = render_svg(
            &demo_counters(),
            &sample_computation(),
            Style::FlatSquare,
            Metric::Count,
        );
        assert!(svg.contains("✓ 47"), "expected `✓ 47`; got:\n{svg}");
    }

    #[test]
    fn render_svg_rate_metric_emits_percentage_value() {
        let svg = render_svg(
            &demo_counters(),
            &sample_computation(),
            Style::FlatSquare,
            Metric::Rate,
        );
        assert!(svg.contains("80%"), "expected `80%`; got:\n{svg}");
    }

    #[test]
    fn render_svg_tier_metric_fills_message_with_palette_color() {
        // sample_computation() → Adept → #C0362C, lowercased by badge-maker.
        let computation = sample_computation();
        assert_eq!(computation.tier, Tier::Adept);
        let svg = render_svg(
            &demo_counters(),
            &computation,
            Style::FlatSquare,
            Metric::Tier,
        );
        assert!(
            svg.contains("#c0362c"),
            "Adept message half is International Orange; got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_count_metric_keeps_slice_31_green() {
        let svg = render_svg(
            &demo_counters(),
            &sample_computation(),
            Style::FlatSquare,
            Metric::Count,
        );
        assert!(
            svg.contains("#4c1"),
            "count metric keeps the slice-31 green; got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_has_valid_svg_framing() {
        let svg = render_svg(
            &demo_counters(),
            &sample_computation(),
            Style::FlatSquare,
            Metric::Tier,
        );
        assert!(svg.starts_with("<svg "), "got:\n{svg}");
        assert!(svg.trim_end().ends_with("</svg>"), "got:\n{svg}");
    }

    #[test]
    fn render_svg_labels_aristo_on_the_dark_stone_half() {
        // Standard shields two-segment layout: `aristo` on the dark stone
        // label half (#2b2824), the tier color on the message half — so
        // the badge sits flush with the crate / license shields badges.
        let svg = render_svg(
            &demo_counters(),
            &sample_computation(),
            Style::FlatSquare,
            Metric::Tier,
        );
        assert!(
            svg.contains("aristo"),
            "the `aristo` label is present; got:\n{svg}"
        );
        assert!(
            svg.contains(r##"fill="#2b2824""##),
            "dark stone label half; got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_embeds_bridge_glyph_as_inline_logo() {
        // The bridge ships embedded as a base64 SVG data-URI <image>, so
        // the badge is self-contained — no external logo fetch. Catches a
        // regression that drops the glyph.
        let svg = render_svg(
            &demo_counters(),
            &sample_computation(),
            Style::FlatSquare,
            Metric::Tier,
        );
        assert!(
            svg.contains("<image"),
            "logo <image> element present; got:\n{svg}"
        );
        assert!(
            svg.contains("data:image/svg+xml;base64,"),
            "logo inlined as a data-URI (offline, self-contained); got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_flat_square_uses_crisp_edges() {
        let svg = render_svg(
            &demo_counters(),
            &sample_computation(),
            Style::FlatSquare,
            Metric::Tier,
        );
        assert!(
            svg.contains("crispEdges"),
            "flat-square renders square (crisp) edges; got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_flat_style_rounds_corners() {
        let svg = render_svg(
            &demo_counters(),
            &sample_computation(),
            Style::Flat,
            Metric::Tier,
        );
        assert!(
            svg.contains(r#"rx="3""#),
            "flat style rounds corners (rx=3); got:\n{svg}"
        );
    }

    #[test]
    fn render_svg_arete_tier_uses_gold_color_and_glyph() {
        let svg = render_svg(
            &demo_counters(),
            &arete_computation(),
            Style::FlatSquare,
            Metric::Tier,
        );
        assert!(svg.contains("#d4a017"), "Areté gold color; got:\n{svg}");
        assert!(svg.contains("✦"), "Areté ✦ glyph; got:\n{svg}");
        assert!(svg.contains("Areté"), "Areté label; got:\n{svg}");
    }
}
