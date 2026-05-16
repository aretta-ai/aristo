//! `aristo lint --fix` — apply auto-fixable rules to annotation text and
//! rewrite source files in place.
//!
//! Only mechanical whitespace fixes (`doubled_spaces`,
//! `trailing_whitespace`) — never semantic rewrites. The pre-commit
//! hook's `"fix"` mode wraps this and re-stages modified files; the
//! re-stage is git-side and not this layer's concern.
//!
//! Walks every `.rs` file under the workspace root (built-in ignore set
//! matches `aristo-core::walk::fs`), re-parses each with `syn`, and
//! locates the text literal of every aristo annotation. If applying the
//! auto-fix rules changes the text, the literal's source bytes are
//! replaced and the file is atomically rewritten.
//!
//! Returns `(total_fixes, files_modified)`. Each rule firing on one
//! literal counts as one fix; one annotation with both `doubled_spaces`
//! and `trailing_whitespace` problems = 2 fixes.

use std::path::{Path, PathBuf};

use aristo_core::walk::WalkOptions;
use proc_macro2::{LineColumn, Span};
use syn::visit::Visit;

use crate::{CliError, CliResult, Workspace};

const IGNORED_DIRS: &[&str] = &["target", ".git", ".aristo", "node_modules"];

pub(crate) fn run_fix(ws: &Workspace) -> CliResult<(usize, usize)> {
    let mut total_fixes = 0usize;
    let mut files_modified = 0usize;

    let opts =
        WalkOptions::from_index_config(&ws.load_config().index).map_err(|e| CliError::Other {
            message: format!("aristo.toml [index].exclude: {e}"),
            exit_code: 2,
        })?;

    for path in collect_rs_files(&ws.root, &opts) {
        let source = std::fs::read_to_string(&path).map_err(|e| CliError::Other {
            message: format!("read {}: {e}", path.display()),
            exit_code: 1,
        })?;
        let Some((new_source, n)) = rewrite_source(&source) else {
            continue;
        };
        if n == 0 {
            continue;
        }
        atomic_write(&path, &new_source)?;
        total_fixes += n;
        files_modified += 1;
    }

    Ok((total_fixes, files_modified))
}

fn collect_rs_files(root: &Path, opts: &WalkOptions) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            !(e.file_type().is_dir()
                && e.file_name()
                    .to_str()
                    .map(|n| IGNORED_DIRS.contains(&n))
                    .unwrap_or(false))
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
        .filter(|e| {
            let rel = e.path().strip_prefix(root).unwrap_or(e.path());
            !opts.excludes_path(rel)
        })
        .map(|e| e.into_path())
        .collect()
}

/// Parse `source` and apply auto-fix rules to every aristo annotation's
/// text literal. Returns the rewritten source + total fix count, or
/// `None` if the file didn't parse (skip it; `aristo lint --check`
/// surfaces the parse error elsewhere).
fn rewrite_source(source: &str) -> Option<(String, usize)> {
    let file: syn::File = syn::parse_str(source).ok()?;
    let mut visitor = LiteralFinder { hits: Vec::new() };
    visitor.visit_file(&file);

    let offsets = LineOffsets::new(source);
    let mut edits: Vec<Edit> = Vec::new();
    let mut total_fixes = 0usize;

    for hit in visitor.hits {
        let (fixed, n) = apply_autofix(&hit.value);
        if n == 0 {
            continue;
        }
        let span = hit.span;
        let (start, end) = match (
            offsets.byte_offset(span.start()),
            offsets.byte_offset(span.end()),
        ) {
            (Some(s), Some(e)) if s <= e && e <= source.len() => (s, e),
            _ => continue, // span info missing → skip rather than corrupt
        };
        let replacement = render_str_literal(&fixed);
        edits.push(Edit {
            start,
            end,
            replacement,
        });
        total_fixes += n;
    }

    if edits.is_empty() {
        return Some((source.to_string(), 0));
    }

    // Apply right-to-left so earlier byte ranges stay valid.
    edits.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = source.to_string();
    for edit in edits {
        out.replace_range(edit.start..edit.end, &edit.replacement);
    }
    Some((out, total_fixes))
}

struct Edit {
    start: usize,
    end: usize,
    replacement: String,
}

struct LiteralHit {
    span: Span,
    value: String,
}

struct LiteralFinder {
    hits: Vec<LiteralHit>,
}

impl<'ast> Visit<'ast> for LiteralFinder {
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if is_aristo_annotation_attr(attr) {
            if let Some(lit) = first_string_lit_in_attr(attr) {
                self.hits.push(LiteralHit {
                    span: lit.span(),
                    value: lit.value(),
                });
            }
        }
        syn::visit::visit_attribute(self, attr);
    }

    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        if is_aristo_annotation_stmt_macro(&node.mac) {
            if let Some(lit) = first_string_lit_in_macro(&node.mac) {
                self.hits.push(LiteralHit {
                    span: lit.span(),
                    value: lit.value(),
                });
            }
        }
        syn::visit::visit_stmt_macro(self, node);
    }
}

fn is_aristo_annotation_attr(attr: &syn::Attribute) -> bool {
    let segs: Vec<String> = attr
        .path()
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let last = match segs.last() {
        Some(s) => s.as_str(),
        None => return false,
    };
    if last != "intent" && last != "assume" {
        return false;
    }
    // Bare (`intent`) or aristo-qualified (`aristo::intent`).
    segs.len() == 1 || (segs.len() == 2 && segs[0] == "aristo")
}

fn is_aristo_annotation_stmt_macro(mac: &syn::Macro) -> bool {
    let segs: Vec<String> = mac
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let last = match segs.last() {
        Some(s) => s.as_str(),
        None => return false,
    };
    if last != "intent_stmt" && last != "assume_stmt" {
        return false;
    }
    segs.len() == 1 || (segs.len() == 2 && segs[0] == "aristo")
}

fn first_string_lit_in_attr(attr: &syn::Attribute) -> Option<syn::LitStr> {
    let args = attr.parse_args::<FirstLit>().ok()?;
    Some(args.0)
}

fn first_string_lit_in_macro(mac: &syn::Macro) -> Option<syn::LitStr> {
    let args = mac.parse_body::<FirstLit>().ok()?;
    Some(args.0)
}

struct FirstLit(syn::LitStr);

impl syn::parse::Parse for FirstLit {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let lit: syn::LitStr = input.parse()?;
        Ok(FirstLit(lit))
    }
}

/// Apply the two whitespace auto-fix rules. Returns `(fixed_text,
/// fix_count)`. Each rule that actually changed the text counts as one
/// fix.
fn apply_autofix(text: &str) -> (String, usize) {
    let mut work = text.to_string();
    let mut count = 0usize;

    let after_trail = strip_trailing_whitespace(&work);
    if after_trail != work {
        work = after_trail;
        count += 1;
    }

    let after_doubled = collapse_doubled_spaces(&work);
    if after_doubled != work {
        work = after_doubled;
        count += 1;
    }

    (work, count)
}

fn strip_trailing_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut first = true;
    for line in s.split('\n') {
        if !first {
            out.push('\n');
        }
        first = false;
        out.push_str(line.trim_end_matches([' ', '\t']));
    }
    out
}

fn collapse_doubled_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == ' ' {
            if !prev_space {
                out.push(ch);
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

/// Render `value` as a Rust string literal — same format `proc_macro2`'s
/// `Literal::string` produces, but built locally so we don't depend on
/// `Literal::span_byte_range` (nightly-only).
fn render_str_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Byte offset of every line start (1-indexed lines).
struct LineOffsets<'a> {
    source: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> LineOffsets<'a> {
    fn new(source: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            source,
            line_starts,
        }
    }

    fn byte_offset(&self, lc: LineColumn) -> Option<usize> {
        if lc.line == 0 {
            return None; // proc-macro2 span info missing
        }
        let line_idx = lc.line - 1;
        let line_start = *self.line_starts.get(line_idx)?;
        let line_end = self
            .line_starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(self.source.len());
        let line_slice = self.source.get(line_start..line_end)?;
        // proc-macro2 `column` is char-count from line start.
        let mut bytes = 0usize;
        for (i, ch) in line_slice.chars().enumerate() {
            if i == lc.column {
                return Some(line_start + bytes);
            }
            bytes += ch.len_utf8();
        }
        // Past end of last char → end of line.
        if lc.column >= line_slice.chars().count() {
            return Some(line_start + bytes);
        }
        None
    }
}

/// Atomic via write-temp + rename. Same dir as target so `rename` is a
/// single inode op (cross-fs renames would copy + delete, which isn't atomic).
fn atomic_write(path: &Path, contents: &str) -> CliResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let tmp = parent.join(format!(
        ".{}.aristo-fix.{pid}.{nanos}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("source")
    ));
    std::fs::write(&tmp, contents).map_err(|e| CliError::Other {
        message: format!("write {}: {e}", tmp.display()),
        exit_code: 1,
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        CliError::Other {
            message: format!("rename to {}: {e}", path.display()),
            exit_code: 1,
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_trailing_whitespace_handles_multi_line() {
        let s = "alpha   \nbeta\ngamma \t ";
        assert_eq!(strip_trailing_whitespace(s), "alpha\nbeta\ngamma");
    }

    #[test]
    fn collapse_doubled_spaces_leaves_single_spaces_alone() {
        assert_eq!(collapse_doubled_spaces("one two three"), "one two three");
        assert_eq!(collapse_doubled_spaces("one  two   three"), "one two three");
    }

    #[test]
    fn apply_autofix_counts_each_rule_once() {
        let (fixed, n) = apply_autofix("with  doubled and \ntrailing");
        assert_eq!(fixed, "with doubled and\ntrailing");
        assert_eq!(n, 2);
    }

    #[test]
    fn apply_autofix_clean_text_is_zero_fixes() {
        let (fixed, n) = apply_autofix("nothing to fix here");
        assert_eq!(fixed, "nothing to fix here");
        assert_eq!(n, 0);
    }

    #[test]
    fn render_str_literal_escapes_control_chars() {
        assert_eq!(render_str_literal("a\nb"), r#""a\nb""#);
        assert_eq!(
            render_str_literal(r#"with "quotes""#),
            r#""with \"quotes\"""#
        );
        assert_eq!(render_str_literal("back\\slash"), r#""back\\slash""#);
    }

    #[test]
    fn rewrite_source_replaces_text_literal_in_attribute_form() {
        let src = "#[aristo::intent(\"hello  world \")]\nfn x() {}\n";
        let (out, n) = rewrite_source(src).unwrap();
        assert_eq!(n, 2);
        assert!(out.contains("\"hello world\""), "got: {out}");
    }

    #[test]
    fn rewrite_source_no_changes_when_text_is_clean() {
        let src = "#[aristo::intent(\"already clean\")]\nfn x() {}\n";
        let (out, n) = rewrite_source(src).unwrap();
        assert_eq!(n, 0);
        assert_eq!(out, src);
    }

    #[test]
    fn rewrite_source_handles_statement_form() {
        let src = "fn f() { aristo::intent_stmt!(\"trail \"); }\n";
        let (out, n) = rewrite_source(src).unwrap();
        assert_eq!(n, 1);
        assert!(out.contains("\"trail\""), "got: {out}");
    }
}
