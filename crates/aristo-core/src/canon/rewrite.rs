//! Source-rewrite primitive for the canon accept path.
//!
//! Given a Rust source string and the location of an `#[aristo::intent(...)]`
//! attribute (or `#[intent(...)]` bare-imported form), compute the byte
//! range of the attribute's argument-list parentheses and a replacement
//! string that materializes a canon binding per cli-sessions.md Flow 4:
//!
//! Before:
//! ```text
//! #[aristo::intent("each cell should be written exactly once per page edit")]
//! ```
//!
//! After (aristos: tier):
//! ```text
//! #[aristo::intent(id = "aristos:cell_written_exactly_once_per_page_edit", text = "edit_page writes each cell exactly once", verify = "neural")]
//! ```
//!
//! `cargo fmt` reformats the multi-arg attribute to the multi-line shape
//! shown in the mockup; this module emits a single-line canonical form
//! and lets rustfmt do the prettification.
//!
//! ## What is preserved
//!
//! - The attribute's path (`aristo::intent` vs bare `intent`) is left
//!   untouched — only the argument list inside `(...)` is rewritten.
//! - An existing `verify = ...` argument is preserved verbatim. If the
//!   user wrote `verify = "test"` or `verify = false`, that survives.
//!   If `verify` is absent, no `verify` arg is added — the macro's
//!   default (neural) kicks in at expansion time.
//! - An existing `parent = "..."` (single or array) is preserved.
//!
//! ## What is replaced
//!
//! - The positional first arg (the prose text) is consumed and re-emitted
//!   as a named `text = "<canonical_text>"` arg.
//! - Any existing `id = "..."` is replaced by the canon-prefixed form
//!   (`aristos:<canon_id>` or `kanon:<canon_id>` depending on tier).
//!
//! ## Scope
//!
//! Phase 1 supports `intent` annotations only. `assume` annotations are
//! not in the canon match set per the design archive (canon entries
//! describe verifiable claims; assumes are documentary). A request for
//! an assume annotation returns [`RewriteError::AnnotationKindNotSupported`].

use proc_macro2::LineColumn;
use syn::spanned::Spanned;
use syn::visit::Visit;

use super::types::PrefixTier;

/// Inputs to a canon-accept source rewrite.
#[derive(Debug, Clone)]
pub struct AcceptRewriteRequest {
    /// 1-indexed line of the **attribute's first token** (the `#`). This
    /// matches `IntentEntry::line` (the per-annotation line that `aristo
    /// stamp` records during extraction). If the user authored a
    /// multi-line attribute, this is the line carrying the `#`, NOT
    /// the line of the attribute's closing `)]` or the line of the
    /// item it's attached to.
    pub item_line: usize,
    /// Bare canon id (no prefix), e.g.
    /// `"cell_written_exactly_once_per_page_edit"`. The prefix is
    /// applied internally based on `prefix_tier`.
    pub canon_id: String,
    /// Canonical text from the matched canon entry. Replaces the user's
    /// original prose as the annotation's `text = "..."` arg.
    pub canonical_text: String,
    /// Which prefix to apply: `Aristos` → `aristos:<canon_id>`,
    /// `Kanon` → `kanon:<canon_id>`.
    pub prefix_tier: PrefixTier,
}

/// The output of a successful rewrite: the byte range to replace
/// (INCLUDING the outer `(...)` parens) and the replacement text
/// (also including outer parens).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeRewrite {
    /// Inclusive byte start of the `(`.
    pub byte_start: usize,
    /// Exclusive byte end of the `)` (so `source[byte_start..byte_end]`
    /// covers the full `(...)` token group).
    pub byte_end: usize,
    /// The replacement bytes (with the outer parens), e.g.
    /// `(id = "aristos:foo", text = "bar", verify = "neural")`.
    pub replacement: String,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RewriteError {
    #[error("source did not parse as Rust: {0}")]
    Parse(String),
    #[error(
        "no `#[aristo::intent(...)]` or `#[intent(...)]` attribute found on \
         the item at line {line}"
    )]
    NoIntentAttribute { line: usize },
    #[error(
        "annotation at line {line} is an `assume`, not an `intent`. Canon \
         matches only apply to intents per the §13 design archive."
    )]
    AnnotationKindNotSupported { line: usize },
    #[error(
        "attribute at line {line} uses the unit-meta form `#[intent]` \
         with no arguments; canon accept needs at least the existing \
         positional text arg to rewrite."
    )]
    UnitMetaForm { line: usize },
    #[error("internal: failed to convert syn span to byte offset")]
    SpanResolution,
    #[error(
        "attribute argument list at line {line} did not start with a \
         positional text string literal; rewrite expects the standard \
         `intent(\"text\", ...)` shape."
    )]
    NoPositionalText { line: usize },
}

impl From<syn::Error> for RewriteError {
    fn from(e: syn::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

/// Compute the byte-range rewrite for a canon accept against an
/// `#[aristo::intent(...)]` attribute. Pure: no I/O, no source mutation
/// (the caller splices `source[result.byte_start..result.byte_end] =
/// result.replacement.bytes()`).
pub fn compute_rewrite(
    source: &str,
    request: &AcceptRewriteRequest,
) -> Result<AttributeRewrite, RewriteError> {
    let file: syn::File = syn::parse_str(source)?;
    let line_offsets = compute_line_byte_offsets(source);

    let mut finder = AttrFinder {
        target_line: request.item_line,
        found: None,
    };
    finder.visit_file(&file);

    let attr_match = finder.found.ok_or(RewriteError::NoIntentAttribute {
        line: request.item_line,
    })?;
    if matches!(attr_match.kind, AttrKind::Assume) {
        return Err(RewriteError::AnnotationKindNotSupported {
            line: request.item_line,
        });
    }

    let meta_list = match &attr_match.attr.meta {
        syn::Meta::List(ml) => ml,
        syn::Meta::Path(_) => {
            return Err(RewriteError::UnitMetaForm {
                line: request.item_line,
            });
        }
        syn::Meta::NameValue(_) => {
            return Err(RewriteError::UnitMetaForm {
                line: request.item_line,
            });
        }
    };

    // Parse the inner tokens to extract preserved (verify, parent) args.
    let parsed =
        meta_list
            .parse_args::<ExistingArgs>()
            .map_err(|_| RewriteError::NoPositionalText {
                line: request.item_line,
            })?;

    // Build the replacement string.
    let prefixed_id = match request.prefix_tier {
        PrefixTier::Aristos => format!("aristos:{}", request.canon_id),
        PrefixTier::Kanon => format!("kanon:{}", request.canon_id),
    };
    let mut args: Vec<String> = Vec::with_capacity(4);
    args.push(format!("id = {}", escape_string(&prefixed_id)));
    args.push(format!("text = {}", escape_string(&request.canonical_text)));
    if let Some(verify) = &parsed.verify_tokens {
        args.push(format!("verify = {verify}"));
    }
    if let Some(parent) = &parsed.parent_tokens {
        args.push(format!("parent = {parent}"));
    }
    let replacement = format!("({})", args.join(", "));

    // Compute byte range of the outer `(...)`. We splice including
    // both parens. The MetaList delimiter token gives us the span of
    // the parens; we read its open/close.
    let delim_span = match &meta_list.delimiter {
        syn::MacroDelimiter::Paren(p) => p.span,
        syn::MacroDelimiter::Brace(_) | syn::MacroDelimiter::Bracket(_) => {
            // Attribute paths use parens by convention; the brace/bracket
            // forms are syntactically legal but the macro doesn't accept
            // them.
            return Err(RewriteError::NoIntentAttribute {
                line: request.item_line,
            });
        }
    };
    let open_lc = delim_span.open().start();
    let close_lc = delim_span.close().end();
    let byte_start =
        line_col_to_byte(source, &line_offsets, open_lc).ok_or(RewriteError::SpanResolution)?;
    let byte_end =
        line_col_to_byte(source, &line_offsets, close_lc).ok_or(RewriteError::SpanResolution)?;

    Ok(AttributeRewrite {
        byte_start,
        byte_end,
        replacement,
    })
}

// ─── annotation-attribute discovery ───────────────────────────────────────

#[derive(Debug)]
struct AttrMatch<'ast> {
    attr: &'ast syn::Attribute,
    kind: AttrKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttrKind {
    Intent,
    Assume,
}

fn classify_aristo_attr(attr: &syn::Attribute) -> Option<AttrKind> {
    let segs: Vec<String> = attr
        .path()
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    match segs.as_slice() {
        [name] => match name.as_str() {
            "intent" => Some(AttrKind::Intent),
            "assume" => Some(AttrKind::Assume),
            _ => None,
        },
        [outer, name] if outer == "aristo" => match name.as_str() {
            "intent" => Some(AttrKind::Intent),
            "assume" => Some(AttrKind::Assume),
            _ => None,
        },
        _ => None,
    }
}

struct AttrFinder<'ast> {
    target_line: usize,
    found: Option<AttrMatch<'ast>>,
}

impl<'ast> Visit<'ast> for AttrFinder<'ast> {
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if self.found.is_some() {
            return;
        }
        let Some(kind) = classify_aristo_attr(attr) else {
            return;
        };
        // The `#` token is the first byte of the attribute span — same
        // line that `aristo stamp` records via `attr.span().start().line`
        // (per `walk::extract`).
        if attr.span().start().line == self.target_line {
            self.found = Some(AttrMatch { attr, kind });
        }
    }
}

// ─── argument extraction (preserve verify/parent verbatim) ────────────────

/// Parser that consumes the positional first arg (the text), then
/// captures verify and parent as raw token strings so they round-trip
/// exactly into the rewrite. Everything else (unknown keys) is dropped
/// silently — same tolerance as the extractor.
struct ExistingArgs {
    verify_tokens: Option<String>,
    parent_tokens: Option<String>,
}

impl syn::parse::Parse for ExistingArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut verify_tokens = None;
        let mut parent_tokens = None;

        if input.is_empty() {
            return Err(input.error("expected positional text arg"));
        }

        // Drain the positional text string literal (or error if absent).
        let _: syn::LitStr = input.parse()?;

        while input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: syn::Ident = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            match key.to_string().as_str() {
                "verify" => {
                    let expr: syn::Expr = input.parse()?;
                    verify_tokens = Some(expr_to_token_string(&expr));
                }
                "parent" => {
                    // Could be `"x"` or `[..]`. Either way, capture as
                    // tokens.
                    if input.peek(syn::token::Bracket) {
                        let content;
                        syn::bracketed!(content in input);
                        let mut lits = Vec::new();
                        while !content.is_empty() {
                            let lit: syn::LitStr = content.parse()?;
                            lits.push(format!("{:?}", lit.value()));
                            if content.peek(syn::Token![,]) {
                                content.parse::<syn::Token![,]>()?;
                            }
                        }
                        parent_tokens = Some(format!("[{}]", lits.join(", ")));
                    } else {
                        let lit: syn::LitStr = input.parse()?;
                        parent_tokens = Some(format!("{:?}", lit.value()));
                    }
                }
                "id" => {
                    // Existing id discarded — canon accept replaces it.
                    let _: syn::LitStr = input.parse()?;
                }
                _ => {
                    // Unknown key: drain a generic expression to keep
                    // the parser robust.
                    let _: syn::Expr = input.parse()?;
                }
            }
        }
        Ok(Self {
            verify_tokens,
            parent_tokens,
        })
    }
}

fn expr_to_token_string(expr: &syn::Expr) -> String {
    // proc_macro2 doesn't ship its own ToTokens impl for syn::Expr;
    // we already depend on `syn` which re-exports `quote`. Use the
    // re-export so we don't add a new top-level dep.
    use syn::__private::ToTokens;
    let mut out = proc_macro2::TokenStream::new();
    expr.to_tokens(&mut out);
    out.to_string()
}

fn escape_string(s: &str) -> String {
    // Rust string literal escapes via {:?}. Adequate for prose text
    // (handles quotes, backslashes, control chars).
    format!("{s:?}")
}

// ─── line/column → byte-offset conversion (mirrors scan_ids.rs) ──────────

fn compute_line_byte_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0, 0]; // sentinel + line 1
    for (idx, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            offsets.push(idx + 1);
        }
    }
    offsets
}

fn line_col_to_byte(source: &str, line_offsets: &[usize], lc: LineColumn) -> Option<usize> {
    if lc.line == 0 || lc.line >= line_offsets.len() {
        return None;
    }
    let line_start = line_offsets[lc.line];
    let rest = &source[line_start..];
    let mut byte_in_line = 0;
    let mut chars = rest.char_indices();
    for _ in 0..lc.column {
        match chars.next() {
            Some((_, ch)) => {
                if ch == '\n' {
                    return None;
                }
                byte_in_line += ch.len_utf8();
            }
            None => return None,
        }
    }
    Some(line_start + byte_in_line)
}

#[cfg(test)]
#[allow(clippy::vec_init_then_push)]
mod tests {
    use super::*;

    fn aristos_req(canon_id: &str, canonical: &str, item_line: usize) -> AcceptRewriteRequest {
        AcceptRewriteRequest {
            item_line,
            canon_id: canon_id.into(),
            canonical_text: canonical.into(),
            prefix_tier: PrefixTier::Aristos,
        }
    }

    fn kanon_req(canon_id: &str, canonical: &str, item_line: usize) -> AcceptRewriteRequest {
        AcceptRewriteRequest {
            item_line,
            canon_id: canon_id.into(),
            canonical_text: canonical.into(),
            prefix_tier: PrefixTier::Kanon,
        }
    }

    fn apply_rewrite(source: &str, rw: &AttributeRewrite) -> String {
        let mut bytes = source.as_bytes().to_vec();
        bytes.splice(
            rw.byte_start..rw.byte_end,
            rw.replacement.as_bytes().iter().copied(),
        );
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn aristos_tier_rewrites_positional_text_into_named_args() {
        let src = "#[aristo::intent(\"each cell should be written exactly once per page edit\")]\nfn edit_page() {}\n";
        let req = aristos_req(
            "cell_written_exactly_once_per_page_edit",
            "edit_page writes each cell exactly once",
            1,
        );
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        assert!(
            post.contains(r#"id = "aristos:cell_written_exactly_once_per_page_edit""#),
            "post: {post}"
        );
        assert!(
            post.contains(r#"text = "edit_page writes each cell exactly once""#),
            "post: {post}"
        );
        // Original prose is gone — replaced.
        assert!(!post.contains("should be written exactly"), "post: {post}");
        // Re-parse the rewritten source as a sanity check that we
        // produced syntactically valid Rust.
        let _: syn::File =
            syn::parse_str(&post).unwrap_or_else(|e| panic!("rewritten source must parse: {e}"));
    }

    #[test]
    fn kanon_tier_uses_kanon_prefix() {
        let src = "#[aristo::intent(\"total can't be negative\")]\nfn compute_total() {}\n";
        let req = kanon_req(
            "checkout_total_non_negative",
            "checkout total is non-negative",
            1,
        );
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        assert!(
            post.contains(r#"id = "kanon:checkout_total_non_negative""#),
            "post: {post}"
        );
        assert!(!post.contains("aristos:"), "post: {post}");
    }

    #[test]
    fn existing_verify_arg_is_preserved() {
        let src = "#[aristo::intent(\"x\", verify = \"test\")]\nfn f() {}\n";
        let req = aristos_req("foo", "y", 1);
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        assert!(post.contains(r#"verify = "test""#), "post: {post}");
        // Re-parses cleanly.
        let _: syn::File = syn::parse_str(&post).expect("must parse");
    }

    #[test]
    fn existing_verify_bool_arg_is_preserved_verbatim() {
        // `verify = true` / `verify = false` are valid expressions; the
        // rewrite must round-trip them as bare keywords, not as strings.
        let src = "#[aristo::intent(\"x\", verify = false)]\nfn f() {}\n";
        let req = aristos_req("foo", "y", 1);
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        assert!(
            post.contains("verify = false") || post.contains("verify=false"),
            "post: {post}"
        );
    }

    #[test]
    fn existing_id_arg_is_replaced_by_canon_id() {
        let src = "#[aristo::intent(\"x\", id = \"my_local_invariant\")]\nfn f() {}\n";
        let req = aristos_req("foo_bar", "y", 1);
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        assert!(post.contains(r#"id = "aristos:foo_bar""#), "post: {post}");
        assert!(!post.contains("my_local_invariant"), "post: {post}");
    }

    #[test]
    fn existing_parent_single_arg_is_preserved() {
        let src = "#[aristo::intent(\"x\", parent = \"my_ancestor\")]\nfn f() {}\n";
        let req = aristos_req("foo", "y", 1);
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        assert!(post.contains(r#"parent = "my_ancestor""#), "post: {post}");
    }

    #[test]
    fn existing_parent_array_is_preserved_with_all_elements() {
        let src = "#[aristo::intent(\"x\", parent = [\"a\", \"b\", \"c\"])]\nfn f() {}\n";
        let req = aristos_req("foo", "y", 1);
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        assert!(post.contains(r#"parent = ["a", "b", "c"]"#), "post: {post}");
    }

    #[test]
    fn multi_line_intent_attribute_is_rewritten_at_correct_paren_range() {
        let src = "\
#[aristo::intent(
    \"each cell is written exactly once\",
    verify = \"test\",
    id = \"edit_page_cell_write_invariant\",
)]
fn edit_page() {}
";
        // The `#` token sits on line 1; line 1 is the attr line.
        let req = aristos_req(
            "cell_written_exactly_once_per_page_edit",
            "edit_page writes each cell exactly once",
            1,
        );
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        assert!(
            post.contains(r#"id = "aristos:cell_written_exactly_once_per_page_edit""#),
            "post: {post}"
        );
        assert!(
            post.contains(r#"text = "edit_page writes each cell exactly once""#),
            "post: {post}"
        );
        assert!(post.contains(r#"verify = "test""#), "post: {post}");
        // Re-parses.
        let _: syn::File = syn::parse_str(&post).expect("must parse");
    }

    #[test]
    fn bare_imported_intent_attribute_form_is_recognized() {
        // Line 1: `use aristo::intent;`; Line 2: `#[intent("hi")]`.
        let src = "use aristo::intent;\n#[intent(\"hi\")]\nfn f() {}\n";
        let req = aristos_req("foo", "hello", 2);
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        assert!(post.contains(r#"id = "aristos:foo""#), "post: {post}");
    }

    #[test]
    fn assume_annotation_is_explicitly_rejected() {
        let src = "#[aristo::assume(\"x\")]\nfn f() {}\n";
        let req = aristos_req("foo", "y", 1);
        let err = compute_rewrite(src, &req).expect_err("must reject assume");
        assert!(
            matches!(err, RewriteError::AnnotationKindNotSupported { line: 1 }),
            "got: {err:?}"
        );
    }

    #[test]
    fn no_intent_attribute_at_line_returns_error() {
        let src = "fn alone() {}\n";
        let req = aristos_req("foo", "y", 1);
        let err = compute_rewrite(src, &req).expect_err("no attr");
        assert!(
            matches!(err, RewriteError::NoIntentAttribute { line: 1 }),
            "got: {err:?}"
        );
    }

    #[test]
    fn unparseable_source_returns_parse_error() {
        let src = "fn unbalanced(";
        let req = aristos_req("foo", "y", 1);
        let err = compute_rewrite(src, &req).expect_err("must err");
        assert!(matches!(err, RewriteError::Parse(_)), "got: {err:?}");
    }

    #[test]
    fn impl_method_annotation_is_found_via_attribute_line() {
        let src = "\
impl Holder {
    #[aristo::intent(\"x\")]
    fn ctor(v: i32) -> Self { Self { v } }
}
";
        // Line 2 of the source carries the `#[aristo::intent(...)]`
        // attribute. The default `Visit` traversal descends into impl
        // methods and emits `visit_attribute` for each method-level
        // attribute, so the line-match works the same way as a
        // top-level fn.
        let req = aristos_req("foo", "y", 2);
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        assert!(post.contains(r#"id = "aristos:foo""#), "post: {post}");
    }

    #[test]
    fn canonical_text_with_quotes_is_escaped_safely() {
        // The canon entry could theoretically carry a quote in its
        // canonical_text. Verify the rewrite escapes correctly.
        let src = "#[aristo::intent(\"plain\")]\nfn f() {}\n";
        let req = aristos_req("foo", r#"says "hello" loudly"#, 1);
        let rw = compute_rewrite(src, &req).expect("rewrite");
        let post = apply_rewrite(src, &rw);
        // The escaped form must re-parse.
        let _: syn::File =
            syn::parse_str(&post).unwrap_or_else(|e| panic!("rewritten must parse: {e}"));
    }

    #[test]
    fn byte_range_covers_outer_parentheses_inclusive() {
        let src = "#[aristo::intent(\"x\")]\nfn f() {}\n";
        let req = aristos_req("foo", "y", 1);
        let rw = compute_rewrite(src, &req).expect("rewrite");
        // The replacement starts and ends with `(` / `)`.
        assert!(rw.replacement.starts_with('('), "got: {}", rw.replacement);
        assert!(rw.replacement.ends_with(')'), "got: {}", rw.replacement);
        // The byte range in source covers the same parens.
        assert_eq!(&src.as_bytes()[rw.byte_start..rw.byte_start + 1], b"(");
        assert_eq!(&src.as_bytes()[rw.byte_end - 1..rw.byte_end], b")");
    }
}
