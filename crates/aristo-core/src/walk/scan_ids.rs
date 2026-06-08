//! Find every `id = "..."` and `parent = "..."` (and `parent = [..., "x", ...]`
//! element) string-literal occurrence inside Aristo annotations.
//!
//! This is the slice-32 `aristo rename` foundation. The rename command needs
//! byte-range spans for every id reference in source so it can rewrite
//! exactly those bytes — preserving surrounding formatting and comments —
//! instead of round-tripping the whole file through `syn::visit_mut`'s
//! re-serializer (which destroys formatting).
//!
//! Scope: only the four attribute paths the walker recognizes
//! (`#[aristo::intent(...)]`, `#[intent(...)]`, `#[aristo::assume(...)]`,
//! `#[assume(...)]`) and the two stmt-form macros (`aristo::intent_stmt!`
//! / `intent_stmt!` + `assume_stmt!` variants). The stmt-form parser path
//! uses the same `AnnotationArgs::parse` that the extractor uses, so the
//! same id/parent slots are found in both forms.
//!
//! The byte-offset translation from syn's `(line, column)` `LineColumn` to
//! a flat byte index is computed once per file via a line-start prefix
//! table. Columns are 0-indexed UTF-8 char counts (per proc_macro2 docs);
//! the table walks chars per line to convert to byte offsets.

use syn::visit::Visit;

use crate::index::AnnotationKind;

/// Which annotation argument slot a found span belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdOccurrenceKind {
    /// `id = "..."` argument on an `intent` / `assume` annotation.
    Id,
    /// `parent = "..."` singular form.
    ParentSingle,
    /// One element inside `parent = ["a", "b", ...]`. The other
    /// elements of the same array each produce their own occurrence
    /// with this same kind.
    ParentArrayElement,
}

/// One occurrence of an annotation id in source, suitable for byte-range
/// rewriting. `byte_start..byte_end` covers the string-literal CONTENTS
/// only — the surrounding double quotes are NOT in the range, so callers
/// can splice in a new id verbatim without worrying about re-quoting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdOccurrence {
    pub kind: IdOccurrenceKind,
    /// The id value as parsed from source (no surrounding quotes,
    /// escape sequences interpreted by `syn::LitStr::value`).
    pub value: String,
    /// 1-indexed line number of the opening quote, for reporting.
    pub line: usize,
    /// Inclusive start byte offset of the string contents (just after the
    /// opening `"`).
    pub byte_start: usize,
    /// Exclusive end byte offset of the string contents (just before the
    /// closing `"`).
    pub byte_end: usize,
    /// Which annotation host this occurrence belongs to (intent vs assume).
    /// Diagnostic only; the rewriter doesn't branch on it.
    pub host_kind: AnnotationKind,
}

/// Scan `source` for every id-bearing string-literal occurrence inside
/// Aristo annotations. Returns occurrences in source order. Parse failures
/// at the outer file level surface as `Err`; individual malformed
/// annotation argument lists are silently skipped (consistent with the
/// extractor's "the proc-macro emits the user-facing diagnostic" policy).
#[aristo::intent(
    "scan_id_occurrences returns byte-range spans that splice exactly the id \
     value when used as `source[byte_start..byte_end]`. The rename \
     command rewrites source by byte substitution at these spans rather than \
     by syn::visit_mut re-serialization — re-serialization destroys \
     whitespace + comments and produces user-visible churn. Spans MUST \
     exclude surrounding quotes so the new id can be spliced in verbatim.",
    verify = "test",
    id = "scan_id_occurrences_byte_spans_round_trip_exactly"
)]
pub fn scan_id_occurrences(source: &str) -> Result<Vec<IdOccurrence>, syn::Error> {
    let file: syn::File = syn::parse_str(source)?;
    let line_offsets = compute_line_byte_offsets(source);
    let mut visitor = Scanner {
        source,
        line_offsets: &line_offsets,
        found: Vec::new(),
    };
    visitor.visit_file(&file);
    visitor.found.sort_by_key(|o| o.byte_start);
    Ok(visitor.found)
}

struct Scanner<'a> {
    source: &'a str,
    line_offsets: &'a [usize],
    found: Vec<IdOccurrence>,
}

impl Scanner<'_> {
    fn record_lit(&mut self, lit: &syn::LitStr, kind: IdOccurrenceKind, host_kind: AnnotationKind) {
        let span = lit.span();
        let start = span.start();
        let end = span.end();
        // syn returns line=0 when span info is unavailable (proc-macro2
        // built without span-locations). The rewriter has nothing to do
        // with such a literal — skip it. Production builds always have
        // span-locations on stable rustc.
        if start.line == 0 || end.line == 0 {
            return;
        }
        let outer_start = match line_col_to_byte(self.source, self.line_offsets, start) {
            Some(b) => b,
            None => return,
        };
        let outer_end = match line_col_to_byte(self.source, self.line_offsets, end) {
            Some(b) => b,
            None => return,
        };
        // The outer span includes the quotes; strip the leading `"` and
        // trailing `"`. Raw strings (r"...", r#"..."#) and byte strings
        // are not part of the Aristo argument grammar — annotation args
        // are always plain `"..."` literals — so a one-byte trim on each
        // side is exact.
        if outer_end <= outer_start + 2 {
            return;
        }
        let bytes = self.source.as_bytes();
        if bytes[outer_start] != b'"' || bytes[outer_end - 1] != b'"' {
            // Defensive: literal didn't start/end with a plain quote.
            // Skip rather than corrupt source on rewrite.
            return;
        }
        self.found.push(IdOccurrence {
            kind,
            value: lit.value(),
            line: start.line,
            byte_start: outer_start + 1,
            byte_end: outer_end - 1,
            host_kind,
        });
    }

    fn record_annotation_args(&mut self, args: AnnotationArgs, host_kind: AnnotationKind) {
        if let Some(lit) = args.id_lit {
            self.record_lit(&lit, IdOccurrenceKind::Id, host_kind);
        }
        match args.parent {
            Some(ParentLits::Single(lit)) => {
                self.record_lit(&lit, IdOccurrenceKind::ParentSingle, host_kind);
            }
            Some(ParentLits::Multiple(lits)) => {
                for lit in lits {
                    self.record_lit(&lit, IdOccurrenceKind::ParentArrayElement, host_kind);
                }
            }
            None => {}
        }
    }

    fn process_attrs(&mut self, attrs: &[syn::Attribute]) {
        for attr in attrs {
            let Some(host_kind) = match_aristo_attr(attr) else {
                continue;
            };
            let Ok(args) = attr.parse_args::<AnnotationArgs>() else {
                continue;
            };
            self.record_annotation_args(args, host_kind);
        }
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.process_attrs(&node.attrs);
        syn::visit::visit_block(self, &node.block);
    }

    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        if let Some(host_kind) = match_aristo_stmt_macro(&node.mac) {
            if let Ok(args) = node.mac.parse_body::<AnnotationArgs>() {
                self.record_annotation_args(args, host_kind);
            }
        }
        syn::visit::visit_stmt_macro(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.process_attrs(&node.attrs);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.process_attrs(&node.attrs);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.process_attrs(&node.attrs);
        for item in &node.items {
            if let syn::TraitItem::Fn(method) = item {
                self.process_attrs(&method.attrs);
            }
        }
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        self.process_attrs(&node.attrs);
        for item in &node.items {
            if let syn::ImplItem::Fn(method) = item {
                self.process_attrs(&method.attrs);
                syn::visit::visit_block(self, &method.block);
            }
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.process_attrs(&node.attrs);
        if let Some((_, items)) = &node.content {
            for item in items {
                self.visit_item(item);
            }
        }
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.process_attrs(&node.attrs);
    }
}

// ─── attr / macro matching (mirrors walk::extract) ────────────────────────

fn match_aristo_attr(attr: &syn::Attribute) -> Option<AnnotationKind> {
    let segs: Vec<String> = attr
        .path()
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    match segs.as_slice() {
        [name] => match name.as_str() {
            "intent" => Some(AnnotationKind::Intent),
            "assume" => Some(AnnotationKind::Assume),
            _ => None,
        },
        [outer, name] if outer == "aristo" => match name.as_str() {
            "intent" => Some(AnnotationKind::Intent),
            "assume" => Some(AnnotationKind::Assume),
            _ => None,
        },
        _ => None,
    }
}

fn match_aristo_stmt_macro(mac: &syn::Macro) -> Option<AnnotationKind> {
    let segs: Vec<String> = mac
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let name = segs.last()?;
    let qualified_ok = segs.len() == 1 || (segs.len() == 2 && segs[0] == "aristo");
    if !qualified_ok {
        return None;
    }
    match name.as_str() {
        "intent_stmt" => Some(AnnotationKind::Intent),
        "assume_stmt" => Some(AnnotationKind::Assume),
        _ => None,
    }
}

// ─── argument parser: keeps the LitStr nodes themselves ───────────────────

struct AnnotationArgs {
    id_lit: Option<syn::LitStr>,
    parent: Option<ParentLits>,
}

enum ParentLits {
    Single(syn::LitStr),
    Multiple(Vec<syn::LitStr>),
}

impl syn::parse::Parse for AnnotationArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut id_lit = None;
        let mut parent = None;

        if input.is_empty() {
            return Ok(Self { id_lit, parent });
        }

        // First positional arg is the text string literal; consume and drop.
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
                    // verify takes any expression (bool literal or "test" /
                    // "neural" string). Drain it as an Expr without
                    // capturing.
                    let _: syn::Expr = input.parse()?;
                }
                "parent" => {
                    if input.peek(syn::token::Bracket) {
                        let content;
                        syn::bracketed!(content in input);
                        let mut lits = Vec::new();
                        while !content.is_empty() {
                            let lit: syn::LitStr = content.parse()?;
                            lits.push(lit);
                            if content.peek(syn::Token![,]) {
                                content.parse::<syn::Token![,]>()?;
                            }
                        }
                        parent = Some(ParentLits::Multiple(lits));
                    } else {
                        let lit: syn::LitStr = input.parse()?;
                        parent = Some(ParentLits::Single(lit));
                    }
                }
                "id" => {
                    let lit: syn::LitStr = input.parse()?;
                    id_lit = Some(lit);
                }
                _ => {
                    // Unknown key: try to drain an expression so parsing
                    // doesn't abort the whole annotation. Mirrors the
                    // extractor's tolerance.
                    let _: syn::Expr = input.parse()?;
                }
            }
        }
        Ok(Self { id_lit, parent })
    }
}

// ─── line/column → byte-offset conversion ─────────────────────────────────

/// Pre-compute the byte offset of the start of each line (1-indexed: index
/// 0 is unused, index N is the byte offset of the first byte of line N).
fn compute_line_byte_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0, 0]; // sentinel + line 1
    for (idx, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            offsets.push(idx + 1);
        }
    }
    offsets
}

/// Convert a syn `LineColumn` (1-indexed line, 0-indexed column in UTF-8
/// chars) to a byte offset into `source`. Returns `None` if the location
/// is out of bounds.
fn line_col_to_byte(
    source: &str,
    line_offsets: &[usize],
    lc: proc_macro2::LineColumn,
) -> Option<usize> {
    if lc.line == 0 || lc.line >= line_offsets.len() {
        return None;
    }
    let line_start = line_offsets[lc.line];
    // proc_macro2 columns are 0-indexed chars (not bytes). Walk the chars
    // on this line to find the byte offset.
    let rest = &source[line_start..];
    let mut chars = rest.char_indices();
    let mut byte_in_line = 0;
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
mod tests {
    use super::*;

    fn scan(s: &str) -> Vec<IdOccurrence> {
        scan_id_occurrences(s).expect("test source must parse as Rust")
    }

    #[test]
    fn finds_id_argument_on_attribute_form() {
        let src = r#"#[aristo::intent("the claim", verify = "test", id = "my_intent")] fn f() {}"#;
        let occs = scan(src);
        assert_eq!(occs.len(), 1);
        assert_eq!(occs[0].kind, IdOccurrenceKind::Id);
        assert_eq!(occs[0].value, "my_intent");
        // Round-trip: substring at the recorded span equals the value.
        assert_eq!(&src[occs[0].byte_start..occs[0].byte_end], "my_intent");
    }

    #[test]
    fn finds_parent_single_form() {
        let src = r#"#[aristo::intent("c", parent = "anc", id = "child")] fn f() {}"#;
        let occs = scan(src);
        assert_eq!(occs.len(), 2);
        // Order: parent appears first in source (left of id).
        assert_eq!(occs[0].kind, IdOccurrenceKind::ParentSingle);
        assert_eq!(occs[0].value, "anc");
        assert_eq!(occs[1].kind, IdOccurrenceKind::Id);
        assert_eq!(occs[1].value, "child");
    }

    #[test]
    fn finds_each_parent_array_element_separately() {
        let src = r#"#[aristo::intent("c", parent = ["a", "b", "c_par"], id = "kid")] fn f() {}"#;
        let occs = scan(src);
        assert_eq!(occs.len(), 4); // a, b, c_par, kid
        assert_eq!(occs[0].kind, IdOccurrenceKind::ParentArrayElement);
        assert_eq!(occs[0].value, "a");
        assert_eq!(&src[occs[0].byte_start..occs[0].byte_end], "a");
        assert_eq!(occs[1].kind, IdOccurrenceKind::ParentArrayElement);
        assert_eq!(occs[1].value, "b");
        assert_eq!(occs[2].kind, IdOccurrenceKind::ParentArrayElement);
        assert_eq!(occs[2].value, "c_par");
        assert_eq!(occs[3].kind, IdOccurrenceKind::Id);
        assert_eq!(occs[3].value, "kid");
    }

    #[test]
    fn occurrences_are_returned_in_source_order() {
        let src = r#"
            #[aristo::intent("first", id = "alpha")] fn a() {}
            #[aristo::intent("second", id = "beta")] fn b() {}
            #[aristo::intent("third", id = "gamma")] fn c() {}
        "#;
        let occs = scan(src);
        let ids: Vec<&str> = occs.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn handles_bare_intent_after_use_aristo_intent() {
        let src = r#"
            use aristo::intent;
            #[intent("hi", id = "bare_form")] fn f() {}
        "#;
        let occs = scan(src);
        assert_eq!(occs.len(), 1);
        assert_eq!(occs[0].value, "bare_form");
    }

    #[test]
    fn handles_assume_attribute() {
        let src = r#"#[aristo::assume("an OS guarantee", id = "the_assume")] fn f() {}"#;
        let occs = scan(src);
        assert_eq!(occs.len(), 1);
        assert_eq!(occs[0].kind, IdOccurrenceKind::Id);
        assert_eq!(occs[0].host_kind, AnnotationKind::Assume);
    }

    #[test]
    fn handles_intent_stmt_macro_inside_fn() {
        let src = r#"
            fn f() {
                aristo::intent_stmt!("statement claim", id = "stmt_intent");
                let _ = 0;
            }
        "#;
        let occs = scan(src);
        assert_eq!(occs.len(), 1);
        assert_eq!(occs[0].value, "stmt_intent");
        assert_eq!(occs[0].host_kind, AnnotationKind::Intent);
    }

    #[test]
    fn handles_assume_stmt_macro() {
        let src = r#"
            fn f() {
                aristo::assume_stmt!("caller holds the lock", id = "lock_held");
            }
        "#;
        let occs = scan(src);
        assert_eq!(occs.len(), 1);
        assert_eq!(occs[0].host_kind, AnnotationKind::Assume);
    }

    #[test]
    fn descends_into_impl_method_attributes_and_stmt_macros() {
        let src = r#"
            impl Holder {
                #[aristo::intent("ctor preserves value", id = "ctor_invariant")]
                fn new(v: i32) -> Self { Self { v } }
                fn check(&self) -> bool {
                    aristo::intent_stmt!("stmt inside method", id = "method_stmt");
                    self.v % 2 == 0
                }
            }
        "#;
        let occs = scan(src);
        let ids: Vec<&str> = occs.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(ids, vec!["ctor_invariant", "method_stmt"]);
    }

    #[test]
    fn ignores_unrelated_attributes() {
        let src = r#"
            #[derive(Debug)] #[serde(rename = "foo")]
            struct X;
            fn main() { let _id = "not_an_aristo_id"; }
        "#;
        assert!(scan(src).is_empty());
    }

    #[test]
    fn malformed_annotation_args_skip_silently() {
        // Missing the positional text string — args.parse fails.
        let src = r#"#[aristo::intent(id = "lonely")] fn f() {}"#;
        assert!(
            scan(src).is_empty(),
            "malformed annotation has no occurrences"
        );
    }

    #[test]
    fn outer_parse_error_surfaces_as_err() {
        assert!(scan_id_occurrences("fn unbalanced(").is_err());
    }

    #[test]
    fn aristos_namespace_id_value_is_captured_verbatim() {
        // The rename command needs to see `aristos:` ids so it can reject
        // them (per slice 32 scope trim). The scanner just records what's
        // there.
        let src = r#"#[aristo::intent("c", parent = "aristos:anc", id = "aristos:foo")] fn f() {}"#;
        let occs = scan(src);
        assert_eq!(occs.len(), 2);
        assert_eq!(occs[0].value, "aristos:anc");
        assert_eq!(occs[1].value, "aristos:foo");
    }

    #[test]
    fn byte_spans_round_trip_for_each_occurrence() {
        // Substituting between byte_start and byte_end MUST yield exactly
        // the value the user wrote — this is the rewriter's contract.
        let src = r#"
            #[aristo::intent("c", parent = ["one", "two"], id = "three")]
            fn f() {}
        "#;
        let occs = scan(src);
        for occ in &occs {
            assert_eq!(
                &src[occ.byte_start..occ.byte_end],
                occ.value,
                "byte span must equal the value for {:?}",
                occ.kind
            );
        }
    }

    #[test]
    fn byte_spans_account_for_multibyte_chars_earlier_in_file() {
        // Multibyte chars before the annotation must not shift its
        // byte offset — proc_macro2 columns are CHAR-based.
        let src = "// 🦀 leading multibyte rust crab\n#[aristo::intent(\"c\", id = \"after_crab\")] fn f() {}";
        let occs = scan(src);
        assert_eq!(occs.len(), 1);
        assert_eq!(&src[occs[0].byte_start..occs[0].byte_end], "after_crab");
    }
}
