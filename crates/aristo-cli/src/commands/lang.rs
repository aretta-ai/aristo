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
  `aristos:` prefix appears on id after `aristo sync` binds the annotation
  to the Aristo server. NEVER write `aristos:` manually.

## Cargo features (consumer-side)
  aristo_verify | injects mined assertions during `aristo verify --filter ...`
  aristo_check  | compile-time per-annotation validation
  aristo_doc    | rustdoc integration via include_str!

For full reference: https://aristo.ai/docs/lang/rust
";

#[aristo::intent(
    "aristo lang's output is the single source of truth for annotation \
     syntax that authoring skills consult; the cheat sheet must always \
     match the macros aristo-macros currently exports",
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
}

fn detect_for_dir(cwd: &Path) -> CliResult<Lang> {
    let cargo_toml = cwd.join("Cargo.toml");
    if cargo_toml.is_file() {
        return Ok(Lang::Rust {
            manifest: cargo_toml,
        });
    }
    Err(unsupported_error())
}

fn detect_for_file(path: &Path) -> CliResult<Lang> {
    // Phase 1: only `.rs` is supported. Python / Go / TypeScript land with
    // their LanguageSyntax impls in Phase 2+.
    if path.extension().and_then(|s| s.to_str()) == Some("rs") {
        // Anchor the manifest at the file's parent's Cargo.toml-ancestor —
        // but Phase 1 keeps it simple: the manifest path is informational,
        // so use the file's path verbatim.
        return Ok(Lang::Rust {
            manifest: path.to_path_buf(),
        });
    }
    Err(unsupported_error())
}

fn unsupported_error() -> CliError {
    // Custom error so the user sees the supported / planned list verbatim.
    CliError::Other {
        message: "Cannot detect a supported language in this repository.\n       \
                  Aristo supports: Rust.\n       \
                  Planned: Python, Go, TypeScript.\n       \
                  For unsupported languages, see https://aristo.ai/docs/languages"
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
    }
}
