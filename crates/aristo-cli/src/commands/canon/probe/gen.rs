//! Probe-crate generation for the S2 presence probe — PURE: a unioned
//! [`InstrumentationBundle`] in, `Cargo.toml` + `src/lib.rs` strings
//! out. No filesystem, no cargo — the generated text is snapshot-
//! tested against the cross-repo golden fixture.
//!
//! The generated crate mirrors the de-risk spike
//! (`aretta-books/.planning/instrument-handoff/spike/s2-presence-probe/`):
//! one function per accessor record whose body binds ONLY the typed
//! root receiver and type-checks one accessor call. The functions are
//! never executed — `cargo check` is the entire proof. Bodies use
//! `presence.harness_probe` VERBATIM when it is a self-contained
//! single-receiver statement; otherwise a call is synthesized from
//! `expected_symbol`/`expected_signature`. Synthesized bodies do not
//! annotate the return type (naming a gated wrapper type the probe
//! cannot import would turn a present accessor into a false negative).
//!
//! ## Provenance in the manifest
//!
//! The dependency line pins the bundle's `base_ref` against the SUT's
//! upstream URL for provenance, and `[patch]` redirects resolution to
//! the user's LOCAL SUT checkout — the same shape the spike proved
//! reuses a warm shared build graph. The wire bundle does not carry
//! the vendor repo URL, so the upstream URL is a flavor constant.
//!
//! ## Known generation limits (surfaced, never silent)
//!
//! - A generic container spelled without its parameters in the lock
//!   (`MvStore` for `MvStore<Clock>`) cannot be typed as a receiver;
//!   the compile will fail with a non-`E0599` error and the record is
//!   reported INDETERMINATE rather than guessed.
//! - Records with no `landing.target.container` (Class B statement
//!   seams) have no callable symbol and are emitted as `degraded` —
//!   the runner reports them INDETERMINATE with the reason.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use aristo_core::canon::{InstrumentationBundle, InstrumentationRecord};

use super::classify::emitted_method;

/// Name of the generated probe package (also what cargo error output
/// names, e.g. `could not compile \`aristo-s2-presence-probe\``).
pub(crate) const PROBE_PACKAGE_NAME: &str = "aristo-s2-presence-probe";

/// Upstream git URL for the turso flavor's SUT. Used only as the
/// provenance-documenting dependency source that `[patch]` redirects
/// to the local checkout; cargo never fetches it when the patch
/// satisfies every use. Flavor-specific because the wire bundle does
/// not (yet) carry the lock vendor block's `repo` URL.
const TURSO_UPSTREAM_GIT: &str = "https://github.com/tursodatabase/turso.git";

/// Generated probe crate: two files plus the records the generator
/// could not express a sound probe statement for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProbeCrate {
    pub cargo_toml: String,
    pub lib_rs: String,
    /// `(accessor_id, why)` for records with no compilable probe —
    /// the runner reports these INDETERMINATE instead of letting them
    /// silently classify as PASS.
    pub degraded: Vec<(String, String)>,
    /// Receiver-type imports the GENERATOR derived (type base →
    /// deep-path `use` line). When one of these hits a private module
    /// (`E0603`/`E0432` — some SUT types are only re-exported at the
    /// crate root, e.g. turso's `Wal`), the runner retries once with
    /// that type in `root_overrides`. Overridden and lock-authored
    /// (`required_use`) imports are deliberately absent.
    pub receiver_imports: BTreeMap<String, String>,
}

/// Generate the probe crate for one unioned (single-`payload_ref`)
/// bundle. `sut_patch_path` is the absolute path of the SUT package
/// directory the `[patch]` section redirects to (e.g.
/// `<sut-checkout>/core`). `root_overrides` lists receiver type bases
/// to import from the crate root instead of the derived module path
/// (the self-healing retry — see [`ProbeCrate::receiver_imports`]).
pub(crate) fn generate_probe_crate(
    bundle: &InstrumentationBundle,
    sut_patch_path: &str,
    root_overrides: &BTreeSet<String>,
) -> ProbeCrate {
    let crate_path = bundle.compile_check.package.replace('-', "_");
    let sut_subpath = bundle
        .provenance
        .sut_binding
        .get(&bundle.compile_check.package)
        .cloned()
        .unwrap_or_default();

    let mut imports: BTreeSet<String> = BTreeSet::new();
    let mut receiver_imports: BTreeMap<String, String> = BTreeMap::new();
    let mut fns = String::new();
    let mut degraded: Vec<(String, String)> = Vec::new();

    for record in &bundle.records {
        let probe = record_probe(&crate_path, &sut_subpath, root_overrides, record);
        imports.extend(probe.imports.iter().cloned());
        if let Some((base, line)) = &probe.receiver_import {
            receiver_imports.insert(base.clone(), line.clone());
        }
        if let Some(why) = &probe.degraded {
            degraded.push((record.accessor_id.clone(), why.clone()));
            let _ = writeln!(
                fns,
                "// DEGRADED: accessor `{}` — {}; reported as INDETERMINATE.\n",
                record.accessor_id, why
            );
            continue;
        }
        fns.push_str(&probe.rendered);
        fns.push('\n');
    }

    ProbeCrate {
        cargo_toml: render_cargo_toml(bundle, sut_patch_path),
        lib_rs: render_lib_rs(bundle, &imports, fns.trim_end()),
        degraded,
        receiver_imports,
    }
}

fn render_cargo_toml(bundle: &InstrumentationBundle, sut_patch_path: &str) -> String {
    let package = &bundle.compile_check.package;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# ARISTO S2 PRESENCE PROBE — generated by `aristo canon probe`; DO NOT EDIT.\n\
         #\n\
         # Compiling this crate against the local SUT checkout proves the\n\
         # instrumentation accessors required by the accepted canon matches\n\
         # are present. The dependency line pins the bundle's base_ref for\n\
         # provenance; [patch] redirects resolution to the local SUT checkout\n\
         # so presence is checked against what will actually build.\n\
         #\n\
         # bundle:      {}\n\
         # base_ref:    {}\n\
         # payload_ref: {} (authored_at)",
        bundle.bundle_id, bundle.provenance.base_ref, bundle.provenance.payload_ref
    );
    let _ = writeln!(
        s,
        "\n[package]\n\
         name = \"{PROBE_PACKAGE_NAME}\"\n\
         version = \"0.0.0\"\n\
         edition = \"2021\"\n\
         publish = false\n\
         \n\
         # The probe lives under the SUT's target/ directory; without an\n\
         # empty [workspace] cargo would attach it to the SUT's own\n\
         # workspace (which does not list it as a member) and refuse to\n\
         # build.\n\
         [workspace]\n\
         \n\
         [lib]\n\
         path = \"src/lib.rs\""
    );
    let _ = writeln!(
        s,
        "\n[dependencies]\n{package} = {{ git = \"{TURSO_UPSTREAM_GIT}\", rev = \"{}\" }}",
        bundle.provenance.base_ref
    );
    let _ = writeln!(
        s,
        "\n[features]\ndefault = []\n{}",
        feature_lines(package, &bundle.compile_check.features).trim_end()
    );
    let _ = writeln!(
        s,
        "\n[patch.\"{TURSO_UPSTREAM_GIT}\"]\n{package} = {{ path = \"{}\" }}",
        toml_escape(sut_patch_path)
    );
    s
}

/// Turn the bundle's `compile_check.features` spec (e.g.
/// `"aristo-instr,turso_core/aristo-instr"`) into `[features]` table
/// lines: every non-`dep/feature` entry becomes a probe-local feature
/// forwarding to all the `dep/feature` entries.
fn feature_lines(package: &str, features: &str) -> String {
    let specs: Vec<&str> = features
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let forwards: Vec<String> = specs
        .iter()
        .filter(|s| s.contains('/'))
        .map(|s| format!("\"{s}\""))
        .collect();
    let locals: Vec<&&str> = specs.iter().filter(|s| !s.contains('/')).collect();
    let mut out = String::new();
    if locals.is_empty() {
        // Degenerate spec (only dep/feature entries): still declare a
        // conventional local alias so `--features` has a stable knob.
        let _ = writeln!(out, "aristo-instr = [\"{package}/aristo-instr\"]");
    } else {
        for l in locals {
            let _ = writeln!(out, "{l} = [{}]", forwards.join(", "));
        }
    }
    out
}

fn render_lib_rs(bundle: &InstrumentationBundle, imports: &BTreeSet<String>, fns: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "//! ARISTO S2 PRESENCE PROBE — generated by `aristo canon probe`; DO NOT EDIT.\n\
         //!\n\
         //! One function per accessor record from the unioned instrumentation\n\
         //! bundles of this repo's accepted canon matches. Each body binds only\n\
         //! the typed root receiver and type-checks one accessor call; the\n\
         //! functions are never executed — `cargo check` is the entire proof.\n\
         //! A missing accessor fails loud (`no method named ...`) and the probe\n\
         //! report maps the error back to the record's conformance `catch`.\n\
         //!\n\
         //! bundle: {}\n\
         //! payload_ref: {}\n\
         //! base_ref: {}\n\
         \n\
         #![allow(unused_imports, unused_variables, dead_code)]",
        bundle.bundle_id, bundle.provenance.payload_ref, bundle.provenance.base_ref
    );
    if !imports.is_empty() {
        s.push('\n');
        for import in imports {
            let _ = writeln!(s, "{import}");
        }
    }
    if !fns.is_empty() {
        s.push('\n');
        s.push_str(fns);
        s.push('\n');
    }
    s
}

// ─── Per-record probe rendering ─────────────────────────────────────────────

struct RecordProbe {
    imports: Vec<String>,
    rendered: String,
    degraded: Option<String>,
    /// `(type base, use line)` when the generator derived the
    /// receiver import itself (candidate for the root-import retry).
    receiver_import: Option<(String, String)>,
}

/// The typed root receiver derived from `landing.target.container`.
struct Receiver {
    /// Parameter type text, e.g. `LogicalLog`, `dyn Wal`,
    /// `MvStore<MvccClock>`.
    ty_text: String,
    /// Path-expression form for associated-item bindings, e.g.
    /// `MvStore::<MvccClock>` (turbofish) or `LogicalLog`.
    path_expr: String,
    /// Type name without generics, used for the `use` line.
    base: String,
    is_trait: bool,
}

fn record_probe(
    crate_path: &str,
    sut_subpath: &str,
    root_overrides: &BTreeSet<String>,
    record: &InstrumentationRecord,
) -> RecordProbe {
    let method = emitted_method(&record.presence.expected_symbol, &record.accessor_id);
    let receiver = receiver_for(&record.landing.target);

    let mut imports: Vec<String> = record
        .landing
        .required_use
        .iter()
        .map(|line| rewrite_required_use(crate_path, line))
        .collect();
    let mut receiver_import: Option<(String, String)> = None;
    if let Some(recv) = &receiver {
        if root_overrides.contains(&recv.base) {
            // Self-healing retry: the deep module path hit a privacy
            // wall; the type must be reachable via the crate-root
            // re-export instead (rustc's own E0603 suggestion).
            imports.push(format!("use {crate_path}::{};", recv.base));
        } else {
            let file = record.landing.target.get("file").and_then(|v| v.as_str());
            if let Some(import) = derive_receiver_import(crate_path, sut_subpath, file, &recv.base)
            {
                receiver_import = Some((recv.base.clone(), import.clone()));
                imports.push(import);
            }
        }
    }

    // Idents the verbatim line's LHS type annotation may legally name:
    // prelude-ish types plus anything an emitted `use` line brings in.
    let mut known_types: BTreeSet<String> = PRELUDE_TYPES.iter().map(|s| s.to_string()).collect();
    for import in &imports {
        known_types.extend(idents(import).map(str::to_string));
    }

    let verbatim = record
        .presence
        .harness_probe
        .as_deref()
        .and_then(|line| verbatim_probe_stmt(line, &method, &known_types));

    let (param, stmt, note) = match verbatim {
        Some((recv_ident, stmt)) => {
            let param = match (&recv_ident, &receiver) {
                (Some(ident), Some(recv)) => Some((ident.clone(), format!("&{}", recv.ty_text))),
                (Some(_), None) => {
                    return degraded_probe(
                        record,
                        imports,
                        "harness probe needs a receiver but the record carries no \
                         landing.target.container to type it",
                    );
                }
                (None, _) => None, // Container::method(...) constructor form
            };
            (param, stmt, None)
        }
        None => match synthesized_stmt(record, &method, receiver.as_ref()) {
            Ok((param, stmt)) => {
                let note = record.presence.harness_probe.as_deref().map(|_| {
                    "// note: harness probe line was not self-contained; call synthesized \
                     from expected_signature."
                        .to_string()
                });
                (param, stmt, note)
            }
            Err(why) => return degraded_probe(record, imports, &why),
        },
    };

    let mut rendered = String::new();
    let _ = writeln!(
        rendered,
        "/// accessor `{}` — expects `{}`\n/// signature: `{}`\n/// catch: {}",
        record.accessor_id,
        record.presence.expected_symbol,
        record.presence.expected_signature,
        single_line(&record.catch)
    );
    if let Some(note) = note {
        let _ = writeln!(rendered, "{note}");
    }
    let fn_name = format!("probe_{}", sanitize_ident(&record.accessor_id));
    match &param {
        Some((ident, ty)) => {
            let _ = writeln!(rendered, "pub fn {fn_name}({ident}: {ty}) {{");
        }
        None => {
            let _ = writeln!(rendered, "pub fn {fn_name}() {{");
        }
    }
    let _ = writeln!(rendered, "    {stmt}\n}}");

    RecordProbe {
        imports,
        rendered,
        degraded: None,
        receiver_import,
    }
}

fn degraded_probe(_record: &InstrumentationRecord, imports: Vec<String>, why: &str) -> RecordProbe {
    RecordProbe {
        imports,
        rendered: String::new(),
        degraded: Some(why.to_string()),
        receiver_import: None,
    }
}

fn receiver_for(target: &serde_json::Value) -> Option<Receiver> {
    let container = target.get("container")?.as_str()?;
    let token = container.split_whitespace().next()?;
    if token.is_empty() {
        return None;
    }
    let is_trait = container.contains("(trait)");
    let base = token.split('<').next().unwrap_or(token).to_string();
    let ty_text = if is_trait {
        format!("dyn {token}")
    } else {
        token.to_string()
    };
    // `MvStore<MvccClock>` → `MvStore::<MvccClock>` in expression
    // position.
    let path_expr = match token.find('<') {
        Some(i) => format!("{}::{}", &token[..i], &token[i..]),
        None => token.to_string(),
    };
    Some(Receiver {
        ty_text,
        path_expr,
        base,
        is_trait,
    })
}

/// Derive the `use` line for the receiver type from the lock row's
/// SUT-relative `target.file` (calibrated on the turso harness's real
/// import paths): strip the `sut_binding` subpath and `.rs`/`/mod`,
/// turn the remaining path into a module path. Single-segment modules
/// (`connection.rs`) and `lib.rs` import from the crate root — those
/// types are re-exported there (`turso_core::Connection`,
/// `turso_core::Database`).
fn derive_receiver_import(
    crate_path: &str,
    sut_subpath: &str,
    file: Option<&str>,
    type_base: &str,
) -> Option<String> {
    let file = file?;
    let rel = if sut_subpath.is_empty() {
        file
    } else {
        file.strip_prefix(&format!("{sut_subpath}/"))
            .unwrap_or(file)
    };
    let rel = rel.strip_suffix(".rs").unwrap_or(rel);
    let rel = rel.strip_suffix("/mod").unwrap_or(rel);
    let segments: Vec<&str> = rel
        .split('/')
        .filter(|s| !s.is_empty() && *s != "lib")
        .collect();
    if segments.len() <= 1 {
        Some(format!("use {crate_path}::{type_base};"))
    } else {
        Some(format!(
            "use {crate_path}::{}::{type_base};",
            segments.join("::")
        ))
    }
}

/// Rewrite a lock-row `required_use` line for the probe's external
/// vantage point: the lock captures the LANDING site's imports
/// (`use crate::…`), which the probe reaches as `use <sut_crate>::…`.
fn rewrite_required_use(crate_path: &str, line: &str) -> String {
    let line = line.trim();
    let mut s = if let Some(rest) = line.strip_prefix("use crate::") {
        format!("use {crate_path}::{rest}")
    } else if line.starts_with("use ") {
        line.to_string()
    } else {
        format!("use {line}")
    };
    if !s.ends_with(';') {
        s.push(';');
    }
    s
}

/// Can the harness-extracted probe line be used VERBATIM? It must be
/// a single self-contained `let` statement calling the emitted method
/// with no free variables beyond ONE root receiver:
/// `let _r: Option<u8> = log.inspect_header_version();`. Returns the
/// receiver ident (`None` for `Container::method(...)` constructor
/// form) and the normalized statement.
fn verbatim_probe_stmt(
    line: &str,
    method: &str,
    known_types: &BTreeSet<String>,
) -> Option<(Option<String>, String)> {
    if line.contains('\n') {
        return None;
    }
    let trimmed = line.trim();
    if !trimmed.starts_with("let ") || trimmed.contains('?') || trimmed.contains(".await") {
        return None;
    }
    let mut stmt = trimmed.to_string();
    if !stmt.ends_with(';') {
        stmt.push(';');
    }

    let eq = stmt.find('=')?;
    // LHS type annotation may only name types the probe can resolve
    // (prelude + this record's imports) — a harness-local type would
    // turn a PRESENT accessor into a false negative.
    let lhs = &stmt[..eq];
    if let Some(colon) = lhs.find(':') {
        let annot = &lhs[colon + 1..];
        let unknown = idents(annot).any(|id| {
            id.chars().next().is_some_and(char::is_uppercase) && !known_types.contains(id)
        });
        if unknown {
            return None;
        }
    }
    let rhs = &stmt[eq + 1..];

    let dot_pat = format!(".{method}(");
    let path_pat = format!("::{method}(");
    let (recv, open_idx) = if let Some(pos) = rhs.find(&dot_pat) {
        let head = &rhs[..pos];
        let ident_start = head
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
            .map_or(0, |i| i + 1);
        let ident = &head[ident_start..];
        if ident.is_empty()
            || ident.chars().next().is_some_and(|c| c.is_ascii_digit())
            || RUST_KEYWORDS.contains(&ident)
        {
            return None;
        }
        // Root receiver only: reject chained receivers like
        // `state.log.inspect_x()` or `wal().inspect_x()`.
        let prev = head[..ident_start].chars().next_back();
        if !matches!(prev, None | Some(' ' | '\t' | '(' | '{' | ',' | '&' | '=')) {
            return None;
        }
        (Some(ident.to_string()), pos + dot_pat.len() - 1)
    } else if let Some(pos) = rhs.find(&path_pat) {
        (None, pos + path_pat.len() - 1)
    } else {
        return None;
    };

    let close_idx = matching_paren(rhs, open_idx)?;
    // Arguments must be self-contained: a lowercase identifier is a
    // free variable the probe fn does not bind.
    let args = &rhs[open_idx + 1..close_idx];
    let free = idents(args).any(|id| {
        id.chars().next().is_some_and(char::is_lowercase) && id != "true" && id != "false"
    });
    if free {
        return None;
    }
    // Nothing may chain after the call (`.map(...)`, `{`, …).
    let tail = rhs[close_idx + 1..].trim();
    if !(tail.is_empty() || tail == ";") {
        return None;
    }
    Some((recv, stmt))
}

/// Synthesize the probe statement from `expected_symbol` +
/// `expected_signature` when no verbatim harness line is usable.
/// Deliberately does NOT annotate the return type (see module docs).
fn synthesized_stmt(
    record: &InstrumentationRecord,
    method: &str,
    receiver: Option<&Receiver>,
) -> Result<(Option<(String, String)>, String), String> {
    let sig = &record.presence.expected_signature;
    let params_inner = sig
        .find('(')
        .and_then(|open| matching_paren(sig, open).map(|close| sig[open + 1..close].trim()));
    let self_only = matches!(
        params_inner,
        Some("&self" | "&mut self" | "self" | "mut self")
    );

    match receiver {
        Some(recv) if self_only => Ok((
            Some(("recv".to_string(), format!("&{}", recv.ty_text))),
            format!("let _r = recv.{method}();"),
        )),
        Some(recv) if !recv.is_trait => {
            // Associated fn / non-self signature: bind the item path.
            // Proves presence without conjuring argument values.
            Ok((None, format!("let _f = {}::{method};", recv.path_expr)))
        }
        Some(_) => Err(format!(
            "`{method}` is an associated fn on a trait container — no \
             receiver-free probe statement is expressible; verify by hand"
        )),
        None => Err(
            "record carries no landing.target.container to type a receiver \
             (Class B statement seam?)"
                .to_string(),
        ),
    }
}

fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let mut depth = 0usize;
    for (i, b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn idents(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| !t.is_empty() && !t.chars().next().unwrap().is_ascii_digit())
}

fn sanitize_ident(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn single_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

const PRELUDE_TYPES: [&str; 17] = [
    "Option", "Some", "None", "Vec", "String", "Result", "Ok", "Err", "Box", "Arc", "Rc", "Cow",
    "HashMap", "HashSet", "BTreeMap", "BTreeSet", "PathBuf",
];

const RUST_KEYWORDS: [&str; 20] = [
    "as", "box", "break", "const", "continue", "crate", "dyn", "else", "fn", "for", "if", "impl",
    "in", "let", "loop", "match", "mod", "move", "mut", "self",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-identical copy of the cross-repo golden fixture
    /// (`aretta-books/.planning/instrument-handoff/slice23/golden-bundle.json`,
    /// sha256 4bfc582e…; the canonical mirror also lives in
    /// `aristo-core/src/canon/fixtures/`).
    const GOLDEN: &str = include_str!("fixtures/golden-bundle.json");

    fn golden_bundle() -> aristo_core::canon::InstrumentationBundle {
        serde_json::from_str(GOLDEN).expect("golden fixture decodes")
    }

    #[test]
    fn golden_bundle_lib_rs_snapshot() {
        // Full-string snapshot: the generated probe body is part of
        // the S2 contract (verbatim harness lines, typed root
        // receivers, no gated type named). Update deliberately.
        let krate = generate_probe_crate(&golden_bundle(), "/sut/core", &BTreeSet::new());
        assert!(krate.degraded.is_empty(), "got: {:?}", krate.degraded);
        let expected = r#"//! ARISTO S2 PRESENCE PROBE — generated by `aristo canon probe`; DO NOT EDIT.
//!
//! One function per accessor record from the unioned instrumentation
//! bundles of this repo's accepted canon matches. Each body binds only
//! the typed root receiver and type-checks one accessor call; the
//! functions are never executed — `cargo check` is the entire proof.
//! A missing accessor fails loud (`no method named ...`) and the probe
//! report maps the error back to the record's conformance `catch`.
//!
//! bundle: turso:7b6cbae:ae85f8792372
//! payload_ref: 7b6cbaec04e86c0d9ac47819c77444af5054c50a
//! base_ref: ad351877c5cf38c1fafc7f08703bfe521b8f4437

#![allow(unused_imports, unused_variables, dead_code)]

use turso_core::mvcc::persistent_storage::logical_log::LogicalLog;
use turso_core::storage::wal::Wal;

/// accessor `inspect_header_version` — expects `LogicalLog::inspect_header_version`
/// signature: `fn inspect_header_version(&self) -> Option<u8>`
/// catch: Logical-log DOI catch: under a LogicalLogWriteHeader fault the log must end durable-or-intact (never a torn intermediate). Bug tag C-1.
pub fn probe_inspect_header_version(log: &LogicalLog) {
    let _r: Option<u8> = log.inspect_header_version();
}

/// accessor `installed_snapshot` — expects `Wal::installed_snapshot`
/// signature: `fn installed_snapshot(&self) -> WalInstalledSnapshot`
/// catch: WAL install coherence: max_frame and transaction_count must be installed from one coherent shared sample (WR-03).
pub fn probe_installed_snapshot(wal: &dyn Wal) {
    let snap = wal.installed_snapshot();
}
"#;
        assert_eq!(krate.lib_rs, expected, "got:\n{}", krate.lib_rs);
    }

    #[test]
    fn golden_bundle_cargo_toml_pins_base_ref_and_patches_to_sut_path() {
        let krate = generate_probe_crate(&golden_bundle(), "/sut/core", &BTreeSet::new());
        let toml = &krate.cargo_toml;
        assert!(
            toml.contains("name = \"aristo-s2-presence-probe\""),
            "got:\n{toml}"
        );
        assert!(
            toml.contains("\n[workspace]\n"),
            "standalone workspace: {toml}"
        );
        assert!(
            toml.contains(
                "turso_core = { git = \"https://github.com/tursodatabase/turso.git\", \
                 rev = \"ad351877c5cf38c1fafc7f08703bfe521b8f4437\" }"
            ),
            "dep line pins base_ref; got:\n{toml}"
        );
        assert!(
            toml.contains("[patch.\"https://github.com/tursodatabase/turso.git\"]"),
            "got:\n{toml}"
        );
        assert!(
            toml.contains("turso_core = { path = \"/sut/core\" }"),
            "patch redirects to the local SUT checkout; got:\n{toml}"
        );
        assert!(
            toml.contains("aristo-instr = [\"turso_core/aristo-instr\"]"),
            "feature alias forwards to the SUT feature; got:\n{toml}"
        );
        assert!(toml.contains("default = []"), "got:\n{toml}");
        // It must parse as TOML.
        let parsed: Result<toml::Value, _> = toml::from_str(toml);
        assert!(
            parsed.is_ok(),
            "generated manifest must be valid TOML: {parsed:?}"
        );
    }

    #[test]
    fn verbatim_line_with_unknown_lhs_type_falls_back_to_synthesis() {
        // The harness line annotates a harness-local type the probe
        // cannot import — using it verbatim would fail compile even
        // when the accessor exists (false negative).
        let mut bundle = golden_bundle();
        bundle.records.truncate(1);
        bundle.records[0].presence.harness_probe =
            Some("let hv: HeaderView = log.inspect_header_version();".into());
        let krate = generate_probe_crate(&bundle, "/sut/core", &BTreeSet::new());
        assert!(krate.degraded.is_empty(), "got: {:?}", krate.degraded);
        assert!(
            krate
                .lib_rs
                .contains("let _r = recv.inspect_header_version();"),
            "synthesized call, no return-type annotation; got:\n{}",
            krate.lib_rs
        );
        assert!(
            krate.lib_rs.contains("recv: &LogicalLog"),
            "typed root receiver; got:\n{}",
            krate.lib_rs
        );
        assert!(
            krate
                .lib_rs
                .contains("harness probe line was not self-contained"),
            "fallback is noted in the generated source; got:\n{}",
            krate.lib_rs
        );
    }

    #[test]
    fn verbatim_line_with_free_argument_variables_falls_back() {
        let mut bundle = golden_bundle();
        bundle.records.truncate(1);
        bundle.records[0].presence.harness_probe =
            Some("let _r = log.inspect_header_version(page_id);".into());
        let krate = generate_probe_crate(&bundle, "/sut/core", &BTreeSet::new());
        assert!(
            krate
                .lib_rs
                .contains("let _r = recv.inspect_header_version();"),
            "got:\n{}",
            krate.lib_rs
        );
    }

    #[test]
    fn verbatim_line_with_chained_receiver_falls_back() {
        let mut bundle = golden_bundle();
        bundle.records.truncate(1);
        bundle.records[0].presence.harness_probe =
            Some("let _r = state.log.inspect_header_version();".into());
        let krate = generate_probe_crate(&bundle, "/sut/core", &BTreeSet::new());
        assert!(
            krate
                .lib_rs
                .contains("let _r = recv.inspect_header_version();"),
            "chained receiver must not be treated as a root binding; got:\n{}",
            krate.lib_rs
        );
    }

    #[test]
    fn verbatim_line_with_trailing_chain_falls_back() {
        // The real c1 harness line (c1_header_upgrade_failure.rs:139)
        // chains `.map(...)` into a harness-local struct literal — the
        // conductor may extract exactly this shape.
        let mut bundle = golden_bundle();
        bundle.records.truncate(1);
        bundle.records[0].presence.harness_probe = Some(
            "let impl_header = log.inspect_header_version().map(|version| HeaderView {".into(),
        );
        let krate = generate_probe_crate(&bundle, "/sut/core", &BTreeSet::new());
        assert!(
            krate
                .lib_rs
                .contains("let _r = recv.inspect_header_version();"),
            "got:\n{}",
            krate.lib_rs
        );
    }

    #[test]
    fn missing_harness_probe_synthesizes_from_signature() {
        let mut bundle = golden_bundle();
        bundle.records.truncate(1);
        bundle.records[0].presence.harness_probe = None;
        let krate = generate_probe_crate(&bundle, "/sut/core", &BTreeSet::new());
        assert!(
            krate
                .lib_rs
                .contains("pub fn probe_inspect_header_version(recv: &LogicalLog) {"),
            "got:\n{}",
            krate.lib_rs
        );
        assert!(
            krate
                .lib_rs
                .contains("let _r = recv.inspect_header_version();"),
            "got:\n{}",
            krate.lib_rs
        );
        assert!(
            !krate.lib_rs.contains("not self-contained"),
            "no fallback note when there was no harness line at all; got:\n{}",
            krate.lib_rs
        );
    }

    #[test]
    fn non_self_signature_binds_the_item_path() {
        // Constructor-style accessor (`fn new_for_test(tx: TxID) -> Self`):
        // no receiver, no conjured argument values — bind the fn item.
        let mut bundle = golden_bundle();
        bundle.records.truncate(1);
        let r = &mut bundle.records[0];
        r.accessor_id = "new_for_test".into();
        r.presence.expected_symbol = "LogRecord::new_for_test".into();
        r.presence.expected_signature = "fn new_for_test(tx_timestamp: TxID) -> Self".into();
        r.presence.harness_probe = None;
        r.landing.target = serde_json::json!({
            "crate": "turso_core",
            "file": "core/mvcc/database/mod.rs",
            "container": "LogRecord"
        });
        let krate = generate_probe_crate(&bundle, "/sut/core", &BTreeSet::new());
        assert!(krate.degraded.is_empty(), "got: {:?}", krate.degraded);
        assert!(
            krate.lib_rs.contains("pub fn probe_new_for_test() {"),
            "got:\n{}",
            krate.lib_rs
        );
        assert!(
            krate.lib_rs.contains("let _f = LogRecord::new_for_test;"),
            "got:\n{}",
            krate.lib_rs
        );
        assert!(
            krate
                .lib_rs
                .contains("use turso_core::mvcc::database::LogRecord;"),
            "`/mod.rs` maps to the containing module path; got:\n{}",
            krate.lib_rs
        );
    }

    #[test]
    fn record_without_container_is_degraded_not_silently_passing() {
        let mut bundle = golden_bundle();
        bundle.records.truncate(1);
        let r = &mut bundle.records[0];
        r.accessor_id = "yield_point_seam".into();
        r.presence.harness_probe = None;
        r.landing.target = serde_json::json!({ "fn": "write_header", "seam": "pre-pwrite" });
        let krate = generate_probe_crate(&bundle, "/sut/core", &BTreeSet::new());
        assert_eq!(krate.degraded.len(), 1, "got: {:?}", krate.degraded);
        assert_eq!(krate.degraded[0].0, "yield_point_seam");
        assert!(
            krate
                .lib_rs
                .contains("DEGRADED: accessor `yield_point_seam`"),
            "degraded records stay visible in the generated source; got:\n{}",
            krate.lib_rs
        );
    }

    #[test]
    fn root_override_imports_the_type_from_the_crate_root() {
        // The self-healing retry: `storage::wal` is private in the
        // real fork (Wal is a crate-root re-export), so the runner
        // regenerates with Wal in root_overrides.
        let bundle = golden_bundle();
        let baseline = generate_probe_crate(&bundle, "/sut/core", &BTreeSet::new());
        assert!(
            baseline
                .lib_rs
                .contains("use turso_core::storage::wal::Wal;"),
            "got:\n{}",
            baseline.lib_rs
        );
        assert_eq!(
            baseline.receiver_imports.get("Wal").map(String::as_str),
            Some("use turso_core::storage::wal::Wal;")
        );
        assert!(baseline.receiver_imports.contains_key("LogicalLog"));

        let overrides: BTreeSet<String> = ["Wal".to_string()].into();
        let healed = generate_probe_crate(&bundle, "/sut/core", &overrides);
        assert!(
            healed.lib_rs.contains("use turso_core::Wal;"),
            "got:\n{}",
            healed.lib_rs
        );
        assert!(
            !healed.lib_rs.contains("use turso_core::storage::wal::Wal;"),
            "got:\n{}",
            healed.lib_rs
        );
        // Overridden types leave receiver_imports so a second retry
        // cannot loop on the same name.
        assert!(!healed.receiver_imports.contains_key("Wal"));
        assert!(healed.receiver_imports.contains_key("LogicalLog"));
        // The probe bodies are untouched by the flip.
        assert!(
            healed
                .lib_rs
                .contains("let snap = wal.installed_snapshot();"),
            "got:\n{}",
            healed.lib_rs
        );
    }

    #[test]
    fn required_use_lines_are_rewritten_to_the_sut_crate() {
        // Real lock rows carry landing-site-relative imports
        // (`use crate::…`) — the probe must reach the same items as
        // `use turso_core::…` (the PROPOSAL's differential-wrapper
        // import mandate).
        assert_eq!(
            rewrite_required_use(
                "turso_core",
                "use crate::mvcc::database::differential::TxnSnapshot;"
            ),
            "use turso_core::mvcc::database::differential::TxnSnapshot;"
        );
        assert_eq!(
            rewrite_required_use("turso_core", "use turso_core::Connection;"),
            "use turso_core::Connection;"
        );
        assert_eq!(
            rewrite_required_use("turso_core", "turso_core::mvcc::MvStore"),
            "use turso_core::mvcc::MvStore;"
        );
    }

    #[test]
    fn receiver_import_calibration() {
        // Calibrated against the real turso paths the books harness
        // imports (see verification/db/flavors/turso): deep modules
        // import at their module path; single-segment files and
        // lib.rs import the crate-root re-export.
        let case = |file: &str, base: &str| {
            derive_receiver_import("turso_core", "core", Some(file), base).unwrap()
        };
        assert_eq!(
            case("core/mvcc/persistent_storage/logical_log.rs", "LogicalLog"),
            "use turso_core::mvcc::persistent_storage::logical_log::LogicalLog;"
        );
        assert_eq!(
            case("core/storage/wal.rs", "Wal"),
            "use turso_core::storage::wal::Wal;"
        );
        assert_eq!(
            case("core/connection.rs", "Connection"),
            "use turso_core::Connection;"
        );
        assert_eq!(case("core/lib.rs", "Database"), "use turso_core::Database;");
        assert_eq!(
            case("core/mvcc/database/mod.rs", "MvStore"),
            "use turso_core::mvcc::database::MvStore;"
        );
    }

    #[test]
    fn generic_container_token_gets_turbofish_path_expr() {
        let recv = receiver_for(&serde_json::json!({ "container": "MvStore<MvccClock>" })).unwrap();
        assert_eq!(recv.ty_text, "MvStore<MvccClock>");
        assert_eq!(recv.path_expr, "MvStore::<MvccClock>");
        assert_eq!(recv.base, "MvStore");
        assert!(!recv.is_trait);
    }

    #[test]
    fn trait_container_becomes_dyn_receiver() {
        let recv =
            receiver_for(&serde_json::json!({ "container": "Wal (trait) / impl Wal for WalFile" }))
                .unwrap();
        assert_eq!(recv.ty_text, "dyn Wal");
        assert!(recv.is_trait);
    }
}
