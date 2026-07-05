//! `aristo lang` — emit a syntax cheat sheet for the detected language.
//!
//! Auto-detects by looking for the language's canonical manifest file in
//! the current directory (`Cargo.toml` for Rust). Phase 1 ships Rust only;
//! per K5, adding a new language is implementing the cheat-sheet text +
//! the manifest detector, with no other CLI changes required. The skills
//! that author annotations are instructed to run `aristo lang` first, so
//! they always get an authoritative source of syntax instead of guessing.
//!
//! `--file <path>` lets skills pick a per-file language (for polyglot
//! monorepos). Phase 1 errors on any non-Rust file; the Python / Go /
//! TypeScript cheat sheets land with their respective `LanguageSyntax`
//! impls in Phase 2+.

use std::path::{Path, PathBuf};

use crate::{CliError, CliResult};

const RUST_CHEAT_SHEET: &str = "\
# Aristo annotation syntax — Rust

## Attribute form (preferred for fn / struct / impl / trait / mod / type / field / variant)
  #[aristo::intent(\"text here\", verify = \"test\", id = \"snake_case_id\", parent = \"other_id\")]
  fn the_thing() { ... }

## Function-like form (sub-item: before a statement / loop / block)
  aristo::intent_stmt!(\"text here\", verify = \"test\");
  for item in items { ... }

## Assume (no verify field; states external invariants you rely on)
  #[aristo::assume(\"OS guarantee or library invariant\")]
  fn the_thing() { ... }

## Parent linkage (singular or list)
  parent = \"balance_no_duplicate_cells\"
  parent = [\"a\", \"b\"]

## Verify levels
  false      | documentation only; no check ever runs
  \"neural\"   | AI-reasoned property check
  \"test\"     | mined assertions + existing test suite
  \"full\"     | server formal proof attempt (paid tier)
  true       | resolves to project default in aristo.toml [verify] default_method

## Namespace prefix
  `aristos:` and `kanon:` prefixes are applied by `aristo canon accept`
  when you accept a pending canon match. NEVER write them manually.

## Cargo features (in your `Cargo.toml`)
  aristo_verify | injects mined assertions during `aristo verify --filter ...`
  aristo_check  | compile-time per-annotation validation
  aristo_doc    | rustdoc integration via include_str!
";

const C_CHEAT_SHEET: &str = "\
# Aristo annotation syntax — C

## Directive form (a `//` line comment directly above the item)
  // @aristo intent(\"text here\", verify = \"test\", id = \"snake_case_id\", parent = \"other_id\")
  int the_thing(void) { ... }

Attaches to the function / struct / union / enum on the next line, or — inside
a function body — to the statement on the next line.

## Assume (no verify field; states external invariants you rely on)
  // @aristo assume(\"OS guarantee or library invariant\")
  int the_thing(void) { ... }

## Explicit target (site = \"name\" as the FIRST argument)
  // @aristo intent(site = \"do_read\", \"text here\", verify = \"test\")
  SYSCALL_DEFINE3(read, ...) { ... }
Reaches a target adjacency can't: a macro-defined function, or one held off
from its directive by a doc-comment block. Resolves by name anywhere in the file.

## Parent linkage (singular or list)
  parent = \"balance_no_duplicate_cells\"
  parent = [\"a\", \"b\"]

## Verify levels
  false      | documentation only; no check ever runs
  \"neural\"   | AI-reasoned property check
  \"test\"     | mined assertions + existing test suite
  \"full\"     | server formal proof attempt (paid tier)
  true       | resolves to project default in aristo.toml [verify] default_method

## Namespace prefix
  `aristos:` and `kanon:` prefixes are applied by `aristo canon accept`
  when you accept a pending canon match. NEVER write them manually.

## Notes
  Only `//` line comments are directives (not `/* ... */`). Keep the text on one
  line; whitespace in the text is normalized when hashed.
";

#[aristo::intent(
    "Each language's cheat sheet MUST match what that language's front-end \
     actually recognizes — the Rust cheat sheet the macros `aristo-macros` \
     export, the C cheat sheet the C directive extractor accepts. Adding, \
     renaming, or removing a form requires updating the matching cheat sheet \
     in the same change — agents are instructed to trust this output over \
     their training data.",
    verify = "test",
    id = "lang_cheat_sheet_matches_macros"
)]
pub(crate) fn run(file: Option<PathBuf>) -> CliResult<()> {
    let cwd = std::env::current_dir()?;
    let lang = match file {
        Some(path) => detect_for_file(&path)?,
        None => detect_for_dir(&cwd)?,
    };
    emit_cheat_sheet(lang);
    Ok(())
}

enum Lang {
    Rust { manifest: PathBuf },
    C { detected_via: String },
}

/// C build-system manifests, checked in order for deterministic detection.
const C_MANIFESTS: &[&str] = &[
    "CMakeLists.txt",
    "Makefile",
    "GNUmakefile",
    "meson.build",
    "configure.ac",
    "compile_commands.json",
];

fn detect_for_dir(cwd: &Path) -> CliResult<Lang> {
    let cargo_toml = cwd.join("Cargo.toml");
    if cargo_toml.is_file() {
        return Ok(Lang::Rust {
            manifest: cargo_toml,
        });
    }
    if let Some(signal) = detect_c_dir_signal(cwd) {
        return Ok(Lang::C {
            detected_via: signal,
        });
    }
    Err(unsupported_error())
}

/// Detect a C project by a build-system manifest, or — failing that — any C
/// source/header directly in the directory. Manifests are preferred so the
/// reported signal is deterministic (`read_dir` order is not).
fn detect_c_dir_signal(cwd: &Path) -> Option<String> {
    for name in C_MANIFESTS {
        if cwd.join(name).is_file() {
            return Some((*name).to_string());
        }
    }
    let mut c_files: Vec<String> = std::fs::read_dir(cwd)
        .ok()?
        .flatten()
        .filter(|e| {
            matches!(
                e.path().extension().and_then(|s| s.to_str()),
                Some("c") | Some("h")
            )
        })
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();
    c_files.sort(); // deterministic pick regardless of read_dir order
    c_files.into_iter().next()
}

fn detect_for_file(path: &Path) -> CliResult<Lang> {
    match path.extension().and_then(|s| s.to_str()) {
        // The manifest path is informational, so use the file's path verbatim.
        Some("rs") => Ok(Lang::Rust {
            manifest: path.to_path_buf(),
        }),
        Some("c") | Some("h") => Ok(Lang::C {
            detected_via: path.display().to_string(),
        }),
        _ => Err(unsupported_error()),
    }
}

fn unsupported_error() -> CliError {
    // Custom error so the user sees the supported / planned list verbatim.
    CliError::Other {
        message: "Cannot detect a supported language in this repository.\n       \
                  Aristo supports: Rust, C.\n       \
                  Planned: Python, Go, TypeScript."
            .to_string(),
        exit_code: 2,
    }
}

fn emit_cheat_sheet(lang: Lang) {
    match lang {
        Lang::Rust { manifest } => {
            println!(
                "Detected language: Rust (from Cargo.toml at {})",
                manifest.display()
            );
            println!();
            print!("{RUST_CHEAT_SHEET}");
        }
        Lang::C { detected_via } => {
            println!("Detected language: C (from {detected_via})");
            println!();
            print!("{C_CHEAT_SHEET}");
        }
    }
}
