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
    matches!(status, Status::Verified | Status::Tested | Status::Neural)
}

// ─── SVG rendering ────────────────────────────────────────────────────

/// Accessible-label prefix. The redesigned badge is a single tier pill
/// with NO visible "aristo" wordmark (the glyph + tier carry it), but
/// the `<title>` / `aria-label` keep the project name so the badge is
/// still identifiable to screen readers and link previews.
const LABEL: &str = "aristo";

/// Font stack. Fira Sans Condensed is the brand face; it is a web font
/// that GitHub's sanitized SVG won't fetch, so the stack falls back to
/// the condensed/regular system sans GitHub actually renders. Pill
/// widths are sized against the WIDER fallback metrics (see
/// [`text_width`]) so the committed README badge never clips when the
/// brand font is unavailable.
const FONT_STACK: &str = "'Fira Sans Condensed','DejaVu Sans Condensed',Verdana,Geneva,sans-serif";

/// Locked simpleicons-style bridge-as-Ω logo (D11). 24×24 viewBox,
/// fill="currentColor" so the badge's color group propagates.
const LOGO_PATHS: &str = concat!(
    r#"<path d="M5 4 Q12 12 19 4 L19 5.5 Q12 13.5 5 5.5 Z"/>"#,
    r#"<path d="M2 21 L3 4 L7 4 L8 21 Z"/>"#,
    r#"<path d="M16 21 L17 4 L21 4 L22 21 Z"/>"#,
    r#"<path d="M1 21 L23 21 L23 22.5 L1 22.5 Z"/>"#,
);

/// Geometry + type treatment for a single-segment tier pill. The three
/// `--style` flavors are just three of these (see [`Style::geom`]).
struct BadgeGeom {
    height: u32,
    /// Corner radius. `0` for the square flavor.
    rx: u32,
    /// Rendered side of the embedded logo box (the 24×24 viewBox scales
    /// to this). The glyph is vertically centered in `height`.
    glyph_px: u32,
    font_size: u32,
    font_weight: u32,
    /// UPPERCASE the tier label (the `for-the-badge` flavor only).
    uppercase: bool,
    /// Letter-spacing in px; `0.0` for the compact flavors.
    letter_spacing: f32,
    /// Left margin before the glyph.
    pad_left: u32,
    /// Gap between glyph and text.
    gap: u32,
    /// Right margin after the text.
    pad_right: u32,
}

impl Style {
    fn geom(self) -> BadgeGeom {
        match self {
            // 20px compact pill: rounded, mixed-case, regular weight.
            Style::Flat => BadgeGeom {
                height: 20,
                rx: 4,
                glyph_px: 14,
                font_size: 11,
                font_weight: 600,
                uppercase: false,
                letter_spacing: 0.0,
                pad_left: 6,
                gap: 5,
                pad_right: 8,
            },
            // Identical to flat but square corners.
            Style::FlatSquare => BadgeGeom {
                rx: 0,
                ..Style::Flat.geom()
            },
            // Variant C: 28px, bold UPPERCASE, airy letter-spacing.
            Style::ForTheBadge => BadgeGeom {
                height: 28,
                rx: 4,
                glyph_px: 16,
                font_size: 13,
                font_weight: 700,
                uppercase: true,
                letter_spacing: 0.8,
                pad_left: 9,
                gap: 7,
                pad_right: 11,
            },
        }
    }
}

fn render_svg(
    counters: &Counters,
    computation: &TierComputation,
    style: Style,
    metric: Metric,
) -> String {
    let value = headline_value(counters, computation, metric);
    let fill = value_color(computation.tier, metric);
    render_pill(&value, fill, style.geom())
}

/// Render a single tier-colored pill: centered logo + tier text, no
/// "aristo" label segment, no gloss gradient. Text + glyph pick a
/// white or dark ink by the fill's luminance ([`ink_for`]) so every
/// tier in the D11 palette stays legible.
fn render_pill(value: &str, fill: &str, g: BadgeGeom) -> String {
    let text = if g.uppercase {
        value.to_uppercase()
    } else {
        value.to_string()
    };
    let ink = ink_for(fill);

    // Letter-spacing widens the visual run; fold it into the measured
    // width so the right padding stays honest.
    let spacing_w =
        (text.chars().count().saturating_sub(1) as f32 * g.letter_spacing).round() as u32;
    let text_w = text_width(&text, g.font_size) + spacing_w;
    let glyph_x = g.pad_left;
    let text_x = g.pad_left + g.glyph_px + g.gap;
    let total_w = text_x + text_w + g.pad_right;

    let glyph_y = (g.height - g.glyph_px) / 2;
    // Optical baseline: a touch below vertical center reads as centered.
    let text_y = g.height / 2 + g.font_size / 3;
    let spacing_attr = if g.letter_spacing > 0.0 {
        format!(r#" letter-spacing="{:.1}""#, g.letter_spacing)
    } else {
        String::new()
    };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{height}" role="img" aria-label="{LABEL}: {value}">
  <title>{LABEL}: {value}</title>
  <rect width="{total_w}" height="{height}" rx="{rx}" fill="{fill}"/>
  <g transform="translate({glyph_x} {glyph_y})" fill="{ink}">
    <svg width="{glyph_px}" height="{glyph_px}" viewBox="0 0 24 24" fill="currentColor">{logo}</svg>
  </g>
  <text x="{text_x}" y="{text_y}" fill="{ink}" font-family="{FONT_STACK}" font-size="{font_size}" font-weight="{font_weight}"{spacing_attr}>{text}</text>
</svg>
"##,
        height = g.height,
        rx = g.rx,
        glyph_px = g.glyph_px,
        font_size = g.font_size,
        font_weight = g.font_weight,
        logo = LOGO_PATHS,
    )
}

/// White or dark ink for text/glyph laid over `bg_hex`, chosen by
/// perceived luminance (0.299R + 0.587G + 0.114B). Threshold 150
/// reproduces the D11 contrast intent exactly — white on the stone +
/// red tiers and the count/rate green, dark `#2b2824` on the light tan
/// (Apprentice) and gold (Areté) tiers — without hardcoding a per-tier
/// map that would silently break if the palette shifts.
fn ink_for(bg_hex: &str) -> &'static str {
    let (r, gc, b) = parse_hex(bg_hex);
    let luma = 0.299 * r as f32 + 0.587 * gc as f32 + 0.114 * b as f32;
    if luma > 150.0 {
        "#2b2824"
    } else {
        "#fff"
    }
}

/// Parse `#rgb` or `#rrggbb` into 8-bit channels. Unparseable input
/// falls back to mid-grey, which is harmless (it only steers ink
/// choice, and every palette color is a valid literal anyway).
fn parse_hex(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let expand = |s: &str| u8::from_str_radix(s, 16).unwrap_or(128);
    match h.len() {
        3 => {
            let c: Vec<char> = h.chars().collect();
            (
                expand(&format!("{0}{0}", c[0])),
                expand(&format!("{0}{0}", c[1])),
                expand(&format!("{0}{0}", c[2])),
            )
        }
        6 => (expand(&h[0..2]), expand(&h[2..4]), expand(&h[4..6])),
        _ => (128, 128, 128),
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
    "Badge text width is approximated per-character, scaled by font \
     size, and deliberately calibrated to the WIDER fallback sans \
     (DejaVu/Verdana) rather than the narrower brand font (Fira Sans \
     Condensed). GitHub strips the web-font fetch from committed SVGs, \
     so the README badge renders in the fallback; sizing to the brand \
     font's metrics would clip the tier text there. Over-estimating is \
     safe (a little right padding); under-estimating clips. The trycmd \
     scenarios match the SVG with wildcards (only `<svg ...>` ↔ \
     `</svg>` framing is pinned, not pixel dimensions), so this \
     heuristic is the sole guard against clipping.",
    verify = "neural",
    id = "badge_text_width_calibrated_to_fallback_font"
)]
fn text_width(text: &str, font_size: u32) -> u32 {
    // ~0.62em average advance for fallback condensed/regular sans —
    // measured generously so the pill never clips when the brand font
    // is unavailable.
    let per_char = (font_size as f32 * 0.62).ceil() as u32;
    text.chars().count() as u32 * per_char
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
        assert!(svg.contains(r#"rx="4""#), "expected rx=4; got:\n{svg}");
    }

    #[test]
    fn render_svg_for_the_badge_uppercases_tier_value_and_taller_box() {
        let counters = Counters {
            total: 47,
            aristos_count: 20,
            verification_rate_pct: 80,
        };
        // sample_computation() lands on Adept — for-the-badge uppercases
        // the tier VALUE (the "aristo" wordmark is gone in the redesign).
        let computation = sample_computation();
        let svg = render_svg(&counters, &computation, Style::ForTheBadge, Metric::Tier);
        assert!(
            svg.contains("ADEPT"),
            "expected uppercase tier value; got:\n{svg}"
        );
        assert!(
            !svg.contains("ARISTO"),
            "the 'aristo' wordmark segment is removed; got:\n{svg}"
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
