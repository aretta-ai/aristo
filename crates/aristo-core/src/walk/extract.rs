//! Parse a Rust source string and extract every Aristo annotation.
//!
//! Recognized invocation paths (the resolution rules `aristo stamp` will
//! follow at the use site):
//!
//! | Form              | Recognized syntaxes                                          |
//! |-------------------|--------------------------------------------------------------|
//! | Intent attribute  | `#[aristo::intent(...)]`, `#[intent(...)]` (after `use aristo::intent;`) |
//! | Assume attribute  | `#[aristo::assume(...)]`, `#[assume(...)]`                   |
//! | Intent statement  | `aristo::intent_stmt!(...)`, `intent_stmt!(...)`             |
//! | Assume statement  | `aristo::assume_stmt!(...)`, `assume_stmt!(...)`             |
//!
//! The walker recognizes both the fully-qualified `aristo::*` form and the
//! bare-imported form — matching what authoring agents actually write per
//! the cheat sheet.
//!
//! Each annotation produces an [`ExtractedAnnotation`] carrying the parsed
//! arguments, hashes, and source location. Validation of values (verify
//! enum, id charset, etc.) happens downstream in `aristo stamp` — this
//! layer is "what's there", not "is it valid".

use std::fmt;

use syn::spanned::Spanned;
use syn::visit::Visit;

use crate::hash::{body_hash, text_hash};
use crate::index::{AnnotationKind, CoveredRegion, Sha256};

/// Surface form of an annotation. The two have identical argument shapes
/// but live at different syntactic positions, which affects how `aristo
/// stamp` computes the covered-region body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationForm {
    /// `#[aristo::intent(...)]` on an item (fn / struct / impl / trait / mod / type).
    Attribute,
    /// `aristo::intent_stmt!(...)` inside a function body, attached to a
    /// statement, block, or loop that follows it.
    Statement,
}

/// Raw `parent = ...` value as it appears in source. String validation,
/// resolution against existing ids, and cycle-membership checks happen
/// downstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParentRaw {
    Single(String),
    Multiple(Vec<String>),
}

/// One annotation discovered in source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedAnnotation {
    pub kind: AnnotationKind,
    pub form: AnnotationForm,
    pub text: String,
    /// Raw `verify` expression token text (e.g. `"test"`, `true`, `false`).
    /// Absent when the user omitted the argument. Validation happens in
    /// `aristo stamp`.
    pub verify: Option<String>,
    pub parent: Option<ParentRaw>,
    /// User-written id, if present. `aristo stamp` assigns one when absent.
    pub id: Option<String>,
    /// Human-readable site label, e.g. `"fn balance_non_root"`,
    /// `"struct CellArray"`, `"impl Holder"`. Used by `aristo show`.
    pub site: String,
    /// 1-indexed line number of the annotation's first token.
    pub line: usize,
    /// What region of source the annotation covers. Determined by the
    /// item kind for attribute form; always `Statement` for the macro form.
    pub covered_region: CoveredRegion,
    /// Hash of the annotation text, normalized (whitespace-collapsed).
    pub text_hash: Sha256,
    /// Hash of the covered region's source bytes, verbatim.
    pub body_hash: Sha256,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("source did not parse as Rust: {0}")]
    Parse(#[from] syn::Error),
}

#[aristo::intent(
    "Annotations return in source order — top of file first. Sorting or \
     hashing the result would silently break stable index ordering and \
     the test fixtures that index into it positionally.",
    verify = "test",
    id = "extract_returns_annotations_in_source_order"
)]
pub fn extract_from_source(source: &str) -> Result<Vec<ExtractedAnnotation>, ExtractError> {
    let file: syn::File = syn::parse_str(source)?;
    let mut visitor = Walker::new(source);
    visitor.visit_file(&file);
    Ok(visitor.found)
}

struct Walker<'a> {
    source: &'a str,
    found: Vec<ExtractedAnnotation>,
    /// Site label active for any `intent_stmt!` / `assume_stmt!` invocation
    /// discovered during a `visit_block` descent. Set when entering an
    /// item-level fn / impl-method / trait default method; restored on
    /// exit. `None` means we're outside any fn body — stmt-form macros
    /// at module top-level (which would be a user error anyway) are
    /// silently dropped.
    current_site: Option<String>,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            found: Vec::new(),
            current_site: None,
        }
    }

    /// Run `f` with `current_site` temporarily set to `site`. Used when
    /// entering a fn body so nested stmt-macros (even inside match arms,
    /// closures, unsafe blocks, etc.) attribute correctly.
    fn with_site<R>(&mut self, site: String, f: impl FnOnce(&mut Self) -> R) -> R {
        let prev = self.current_site.replace(site);
        let r = f(self);
        self.current_site = prev;
        r
    }

    fn process_attrs<S: Spanned>(
        &mut self,
        attrs: &[syn::Attribute],
        site: &str,
        body_span: S,
        covered_region: CoveredRegion,
    ) {
        for attr in attrs {
            let Some(kind) = match_aristo_attr(attr) else {
                continue;
            };
            let Ok(args) = attr.parse_args::<AnnotationArgs>() else {
                // Malformed args: skip silently here — the proc-macro layer
                // emits a compile_error! for the same input, so the user
                // sees the diagnostic from the right tool. Walker would
                // double-emit if it errored too.
                continue;
            };
            self.found.push(make_annotation(
                kind,
                AnnotationForm::Attribute,
                args,
                site,
                attr.span().start().line,
                covered_region,
                source_slice(self.source, body_span.span()),
            ));
        }
    }

    fn process_stmt_macro(&mut self, stmt: &syn::StmtMacro, enclosing_site: &str) {
        let Some(kind) = match_aristo_stmt_macro(&stmt.mac) else {
            return;
        };
        let Ok(args) = stmt.mac.parse_body::<AnnotationArgs>() else {
            return;
        };
        // For statement form the "covered region" body is just the macro
        // tokens for now. Hashing identical macro invocations attached to
        // different statements WILL collide on body_hash; slice 17
        // (`aristo stamp`) refines this by including the following stmt
        // when computing the canonical body span.
        self.found.push(make_annotation(
            kind,
            AnnotationForm::Statement,
            args,
            enclosing_site,
            stmt.span().start().line,
            CoveredRegion::Statement,
            source_slice(self.source, stmt.span()),
        ));
    }
}

fn make_annotation(
    kind: AnnotationKind,
    form: AnnotationForm,
    args: AnnotationArgs,
    site: &str,
    line: usize,
    covered_region: CoveredRegion,
    body_text: String,
) -> ExtractedAnnotation {
    let text_h = text_hash(&args.text);
    let body_h = body_hash(&body_text);
    ExtractedAnnotation {
        kind,
        form,
        text: args.text,
        verify: args.verify,
        parent: args.parent,
        id: args.id,
        site: site.to_string(),
        line,
        covered_region,
        text_hash: text_h,
        body_hash: body_h,
    }
}

impl<'ast> Visit<'ast> for Walker<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let site = format!("fn {}", node.sig.ident);
        // Body span = the fn's `{ ... }` block, EXCLUDING the `#[...]`
        // attribute. Excluding attributes lets text-only edits (re-wording
        // the intent prose) leave body_hash unchanged — drift detection
        // cares about CODE drift, not annotation text drift.
        self.process_attrs(&node.attrs, &site, &node.block, CoveredRegion::Function);
        self.with_site(site, |this| syn::visit::visit_block(this, &node.block));
    }

    #[aristo::intent(
        "stmt-form intents are discovered via syn::Visit's full descent \
         (visit_block + default traversal of every Expr variant), NOT a \
         hand-rolled whitelist of expression kinds. A whitelist silently \
         drops macros nested inside any unenumerated context — match \
         arms, closures, unsafe blocks, async blocks, try blocks, let \
         initializers — and the failure mode is invisible (the intent \
         doesn't appear in `aristo list`, can't be cited as a ground in \
         a proof, and skips the freshness check). The Visit-based \
         descent is open by default; new syn::Expr variants get visited \
         automatically.",
        verify = "test",
        id = "stmt_form_intents_use_open_visit_descent_not_whitelist"
    )]
    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        if let Some(site) = self.current_site.clone() {
            self.process_stmt_macro(node, &site);
        }
        // Continue default traversal so nested macros (e.g., a macro
        // invocation whose body contains another stmt-macro) are also
        // visited.
        syn::visit::visit_stmt_macro(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let site = format!("struct {}", node.ident);
        // Body = the field list. Text-only edits to the annotation don't
        // shift the field span.
        self.process_attrs(&node.attrs, &site, &node.fields, CoveredRegion::Type);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let site = format!("enum {}", node.ident);
        self.process_attrs(
            &node.attrs,
            &site,
            node.brace_token.span.span(),
            CoveredRegion::Type,
        );
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        let site = format!("trait {}", node.ident);
        self.process_attrs(
            &node.attrs,
            &site,
            node.brace_token.span.span(),
            CoveredRegion::Type,
        );
        for item in &node.items {
            if let syn::TraitItem::Fn(method) = item {
                let method_site = format!("trait {}::{}", node.ident, method.sig.ident);
                if let Some(default_block) = &method.default {
                    self.process_attrs(
                        &method.attrs,
                        &method_site,
                        default_block,
                        CoveredRegion::Function,
                    );
                } else {
                    self.process_attrs(
                        &method.attrs,
                        &method_site,
                        &method.sig,
                        CoveredRegion::Function,
                    );
                }
            }
        }
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let site = impl_site(node);
        self.process_attrs(
            &node.attrs,
            &site,
            node.brace_token.span.span(),
            CoveredRegion::ImplMethods,
        );
        for item in &node.items {
            if let syn::ImplItem::Fn(method) = item {
                let method_site = format!("{}::{}", site, method.sig.ident);
                self.process_attrs(
                    &method.attrs,
                    &method_site,
                    &method.block,
                    CoveredRegion::Function,
                );
                self.with_site(method_site, |this| {
                    syn::visit::visit_block(this, &method.block)
                });
            }
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let site = format!("mod {}", node.ident);
        match &node.content {
            Some((brace, _)) => {
                self.process_attrs(
                    &node.attrs,
                    &site,
                    brace.span.span(),
                    CoveredRegion::ModuleInlineBody,
                );
            }
            None => {
                self.process_attrs(
                    &node.attrs,
                    &site,
                    &node.ident,
                    CoveredRegion::ModuleInlineBody,
                );
            }
        }
        if let Some((_, items)) = &node.content {
            for item in items {
                self.visit_item(item);
            }
        }
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        let site = format!("type {}", node.ident);
        self.process_attrs(&node.attrs, &site, node.ty.as_ref(), CoveredRegion::Type);
    }
}

/// Match `#[aristo::intent(...)]` / `#[aristo::assume(...)]` (also bare
/// `intent` / `assume` for the `use aristo::intent;` user style). Returns
/// the kind on a hit, `None` otherwise.
fn match_aristo_attr(attr: &syn::Attribute) -> Option<AnnotationKind> {
    let path_segments: Vec<String> = attr
        .path()
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    match path_segments.as_slice() {
        [name] | [_, name] => match (path_segments.first().map(String::as_str), name.as_str()) {
            (Some("aristo"), "intent") | (None, "intent") => Some(AnnotationKind::Intent),
            (Some("aristo"), "assume") | (None, "assume") => Some(AnnotationKind::Assume),
            _ if path_segments.len() == 1 && name == "intent" => Some(AnnotationKind::Intent),
            _ if path_segments.len() == 1 && name == "assume" => Some(AnnotationKind::Assume),
            _ => None,
        },
        _ => None,
    }
}

/// Match `aristo::intent_stmt!(...)` / `intent_stmt!(...)` and the assume
/// variant. Returns the kind on a hit.
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

fn impl_site(node: &syn::ItemImpl) -> String {
    let self_ty = display_path_or_unknown(&node.self_ty);
    match &node.trait_ {
        Some((_, trait_path, _)) => {
            let trait_name = path_to_string(trait_path);
            format!("impl {trait_name} for {self_ty}")
        }
        None => format!("impl {self_ty}"),
    }
}

fn display_path_or_unknown(ty: &syn::Type) -> String {
    if let syn::Type::Path(tp) = ty {
        path_to_string(&tp.path)
    } else {
        "<type>".to_string()
    }
}

fn path_to_string(p: &syn::Path) -> String {
    p.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn source_slice(source: &str, span: proc_macro2::Span) -> String {
    let start = span.start();
    let end = span.end();
    if start.line == 0 || end.line == 0 {
        // Span info missing (rare; usually means proc-macro2 was built
        // without span-locations). Fall back to whole source.
        return source.to_string();
    }
    let mut out = String::new();
    for (idx, line) in source.lines().enumerate() {
        let line_no = idx + 1;
        if line_no < start.line || line_no > end.line {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let lo = if line_no == start.line {
            start.column
        } else {
            0
        };
        let hi = if line_no == end.line {
            end.column.min(chars.len())
        } else {
            chars.len()
        };
        if lo <= hi && lo <= chars.len() {
            for ch in &chars[lo..hi] {
                out.push(*ch);
            }
        }
        if line_no != end.line {
            out.push('\n');
        }
    }
    out
}

// ─── Argument parsing (mirrors aristo-macros::IntentArgs) ─────────────────

struct AnnotationArgs {
    text: String,
    verify: Option<String>,
    parent: Option<ParentRaw>,
    id: Option<String>,
}

impl syn::parse::Parse for AnnotationArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut text = String::new();
        let mut verify: Option<String> = None;
        let mut parent: Option<ParentRaw> = None;
        let mut id: Option<String> = None;

        if input.is_empty() {
            return Ok(Self {
                text,
                verify,
                parent,
                id,
            });
        }

        // First arg: positional text string literal.
        let lit: syn::LitStr = input.parse()?;
        text = lit.value();

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
                    verify = Some(token_text(&expr));
                }
                "parent" => {
                    parent = Some(parse_parent(input)?);
                }
                "id" => {
                    let lit: syn::LitStr = input.parse()?;
                    id = Some(lit.value());
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown annotation argument `{other}`"),
                    ));
                }
            }
        }

        Ok(Self {
            text,
            verify,
            parent,
            id,
        })
    }
}

fn parse_parent(input: syn::parse::ParseStream) -> syn::Result<ParentRaw> {
    if input.peek(syn::token::Bracket) {
        let content;
        syn::bracketed!(content in input);
        let mut entries = Vec::new();
        while !content.is_empty() {
            let lit: syn::LitStr = content.parse()?;
            entries.push(lit.value());
            if content.peek(syn::Token![,]) {
                content.parse::<syn::Token![,]>()?;
            }
        }
        Ok(ParentRaw::Multiple(entries))
    } else {
        let lit: syn::LitStr = input.parse()?;
        Ok(ParentRaw::Single(lit.value()))
    }
}

fn token_text<T: quote_compat::ToTokens>(t: &T) -> String {
    let mut s = String::new();
    quote_compat::write_tokens(t, &mut s);
    s
}

// Small inline `quote!`-free token rendering — avoids pulling `quote` as a
// dep just for displaying one expression.
mod quote_compat {
    pub trait ToTokens {
        fn to_token_stream(&self) -> proc_macro2::TokenStream;
    }
    impl<T: syn::__private::quote::ToTokens> ToTokens for T {
        fn to_token_stream(&self) -> proc_macro2::TokenStream {
            syn::__private::quote::ToTokens::to_token_stream(self)
        }
    }
    pub fn write_tokens<T: ToTokens>(t: &T, out: &mut String) {
        use std::fmt::Write;
        let _ = write!(out, "{}", t.to_token_stream());
    }
}

impl fmt::Display for AnnotationForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnnotationForm::Attribute => f.write_str("attribute"),
            AnnotationForm::Statement => f.write_str("statement"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(s: &str) -> Vec<ExtractedAnnotation> {
        extract_from_source(s).expect("test source must parse as Rust")
    }

    // ─── attribute form on items ─────────────────────────────────────────

    #[test]
    fn extracts_intent_attribute_on_free_fn() {
        let src = r#"
            #[aristo::intent("the function returns the input plus one", verify = "test")]
            fn add_one(x: i32) -> i32 { x + 1 }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].kind, AnnotationKind::Intent);
        assert_eq!(ann[0].form, AnnotationForm::Attribute);
        assert_eq!(ann[0].text, "the function returns the input plus one");
        assert_eq!(ann[0].verify.as_deref(), Some("\"test\""));
        assert_eq!(ann[0].id, None);
        assert_eq!(ann[0].site, "fn add_one");
        assert_eq!(ann[0].covered_region, CoveredRegion::Function);
    }

    #[test]
    fn extracts_assume_attribute_on_free_fn() {
        let src = r#"
            #[aristo::assume("OS guarantees mmap pages are zero-initialized")]
            fn requires_zero_pages() -> u8 { 0 }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].kind, AnnotationKind::Assume);
        assert_eq!(ann[0].verify, None);
    }

    #[test]
    fn extracts_attribute_on_struct() {
        let src = r#"
            #[aristo::intent("a tiny holder", verify = true, id = "tiny_holder")]
            struct Holder { value: i32 }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].site, "struct Holder");
        assert_eq!(ann[0].covered_region, CoveredRegion::Type);
        assert_eq!(ann[0].verify.as_deref(), Some("true"));
        assert_eq!(ann[0].id.as_deref(), Some("tiny_holder"));
    }

    #[test]
    fn extracts_attribute_on_inherent_impl_and_its_methods() {
        let src = r#"
            #[aristo::intent("inherent impl on Holder")]
            impl Holder {
                #[aristo::intent("constructor preserves value")]
                fn new(value: i32) -> Self { Self { value } }
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 2);
        assert_eq!(ann[0].site, "impl Holder");
        assert_eq!(ann[0].covered_region, CoveredRegion::ImplMethods);
        assert_eq!(ann[1].site, "impl Holder::new");
        assert_eq!(ann[1].covered_region, CoveredRegion::Function);
    }

    #[test]
    fn extracts_attribute_on_trait_impl() {
        let src = r#"
            #[aristo::intent("Holder is sendable across threads")]
            impl Send for Holder {}
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].site, "impl Send for Holder");
    }

    #[test]
    fn extracts_attribute_on_trait_and_method_decls() {
        let src = r#"
            #[aristo::intent("trait describing doublable things")]
            trait Doublable {
                #[aristo::intent("doubles by integer multiplication")]
                fn doubled(&self) -> i32;
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 2);
        assert_eq!(ann[0].site, "trait Doublable");
        assert_eq!(ann[1].site, "trait Doublable::doubled");
    }

    #[test]
    fn extracts_attribute_on_inline_module_and_recurses_into_it() {
        let src = r#"
            #[aristo::intent("math helpers module")]
            mod math {
                #[aristo::intent("squares its input")]
                fn sq(x: i32) -> i32 { x * x }
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 2);
        assert_eq!(ann[0].site, "mod math");
        assert_eq!(ann[0].covered_region, CoveredRegion::ModuleInlineBody);
        assert_eq!(ann[1].site, "fn sq");
    }

    #[test]
    fn extracts_attribute_on_type_alias() {
        let src = r#"
            #[aristo::intent("alias for clarity")]
            type SmallInt = i32;
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].site, "type SmallInt");
    }

    // ─── bare-import variants ────────────────────────────────────────────

    #[test]
    fn extracts_bare_intent_after_use_aristo_intent() {
        let src = r#"
            use aristo::intent;
            #[intent("after use")]
            fn x() {}
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].kind, AnnotationKind::Intent);
        assert_eq!(ann[0].text, "after use");
    }

    #[test]
    fn extracts_bare_assume_after_use_aristo_assume() {
        let src = r#"
            use aristo::assume;
            #[assume("after use")]
            fn x() {}
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].kind, AnnotationKind::Assume);
    }

    // ─── statement form ──────────────────────────────────────────────────

    #[test]
    fn extracts_intent_stmt_inside_fn_body() {
        let src = r#"
            fn balance() {
                aristo::intent_stmt!("the loop accumulates a sum", verify = "test");
                let mut total = 0;
                for x in &[1, 2, 3] { total += x; }
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].form, AnnotationForm::Statement);
        assert_eq!(ann[0].kind, AnnotationKind::Intent);
        assert_eq!(ann[0].covered_region, CoveredRegion::Statement);
        assert_eq!(ann[0].site, "fn balance", "site is the enclosing fn");
    }

    #[test]
    fn extracts_assume_stmt_inside_fn_body() {
        let src = r#"
            fn reads(buf: &[u8]) -> u8 {
                aristo::assume_stmt!("caller has the read lock");
                buf.first().copied().unwrap_or(0)
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].form, AnnotationForm::Statement);
        assert_eq!(ann[0].kind, AnnotationKind::Assume);
    }

    #[test]
    fn extracts_intent_stmt_inside_for_loop() {
        let src = r#"
            fn loops() {
                for x in 0..10 {
                    aristo::intent_stmt!("each iteration is independent");
                    let _ = x;
                }
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].form, AnnotationForm::Statement);
    }

    #[test]
    fn extracts_intent_stmt_inside_method_body() {
        let src = r#"
            impl Holder {
                fn check(&self) -> bool {
                    aristo::intent_stmt!("returns true iff value is even");
                    self.value % 2 == 0
                }
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].site, "impl Holder::check");
    }

    #[test]
    fn extracts_bare_intent_stmt() {
        let src = r#"
            use aristo::intent_stmt;
            fn x() {
                intent_stmt!("the answer is 42");
                let _ = 42;
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].kind, AnnotationKind::Intent);
    }

    // ─── regression: stmt-form descent into all expression kinds ─────────
    // The previous extractor manually whitelisted Block/ForLoop/While/Loop/If.
    // Anything else (Match, Closure, Unsafe, Async, TryBlock, let-initializers
    // containing blocks) silently dropped nested stmt-form intents. The fix
    // uses syn::Visit's open descent. These tests lock the bug closed.

    #[test]
    fn extracts_intent_stmt_inside_match_arm() {
        // The original bug: aristo::intent_stmt! inside a `match` arm body
        // was dropped because Stmt::Expr(Expr::Match, _) wasn't in the
        // expression whitelist.
        let src = r#"
            fn dispatch(x: u8) -> u8 {
                match x {
                    0 => {
                        aristo::intent_stmt!("zero is the identity", verify = "test");
                        0
                    }
                    n => n + 1,
                }
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1, "match-arm intent_stmt must be indexed");
        assert_eq!(ann[0].kind, AnnotationKind::Intent);
        assert_eq!(ann[0].site, "fn dispatch");
    }

    #[test]
    fn extracts_intent_stmt_inside_closure() {
        let src = r#"
            fn build() -> impl Fn() {
                || {
                    aristo::intent_stmt!("the closure runs on every tick");
                }
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1, "closure-body intent_stmt must be indexed");
        assert_eq!(ann[0].site, "fn build");
    }

    #[test]
    fn extracts_intent_stmt_inside_unsafe_block() {
        let src = r#"
            fn raw() {
                unsafe {
                    aristo::intent_stmt!("the raw pointer is non-null at this point");
                }
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1, "unsafe-block intent_stmt must be indexed");
        assert_eq!(ann[0].site, "fn raw");
    }

    #[test]
    fn extracts_intent_stmt_inside_nested_match_in_let_else() {
        // Exercise multi-level nesting: let-else whose else-branch
        // contains a match whose arm body contains the intent.
        let src = r#"
            fn nested(x: Option<u8>) -> u8 {
                let Some(v) = x else {
                    match 0 {
                        n => {
                            aristo::intent_stmt!("the fallback path returns zero");
                            return n;
                        }
                    }
                };
                v
            }
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 1, "deeply nested intent_stmt must be indexed");
        assert_eq!(ann[0].site, "fn nested");
    }

    // ─── argument parsing edge cases ─────────────────────────────────────

    #[test]
    fn parses_parent_singular_and_list() {
        let src = r#"
            #[aristo::intent("singular", parent = "ancestor")]
            fn a() {}
            #[aristo::intent("list", parent = ["one", "two", "three"])]
            fn b() {}
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 2);
        assert_eq!(
            ann[0].parent,
            Some(ParentRaw::Single("ancestor".to_string()))
        );
        assert_eq!(
            ann[1].parent,
            Some(ParentRaw::Multiple(vec![
                "one".into(),
                "two".into(),
                "three".into()
            ]))
        );
    }

    #[test]
    fn captures_verify_value_token_text() {
        let src = r#"
            #[aristo::intent("a", verify = false)]
            fn fa() {}
            #[aristo::intent("b", verify = true)]
            fn fb() {}
            #[aristo::intent("c", verify = "neural")]
            fn fc() {}
        "#;
        let ann = extract(src);
        assert_eq!(ann[0].verify.as_deref(), Some("false"));
        assert_eq!(ann[1].verify.as_deref(), Some("true"));
        assert_eq!(ann[2].verify.as_deref(), Some("\"neural\""));
    }

    // ─── other walker behavior ───────────────────────────────────────────

    #[test]
    fn empty_source_returns_empty() {
        assert!(extract("").is_empty());
    }

    #[test]
    fn source_without_aristo_attributes_returns_empty() {
        let src = r#"
            fn boring() -> i32 { 42 }
            #[derive(Debug)]
            struct Plain;
        "#;
        assert!(extract(src).is_empty());
    }

    #[test]
    fn malformed_aristo_attr_silently_skipped() {
        // The proc-macro layer emits compile_error! for bad args; the
        // walker doesn't double-report.
        let src = r#"
            #[aristo::intent(no_text_arg = "x")]
            fn bad() {}
        "#;
        assert!(extract(src).is_empty());
    }

    #[test]
    fn unrelated_macro_and_attribute_ignored() {
        let src = r#"
            #[derive(Debug, Clone)]
            #[serde(rename = "foo")]
            struct X;
            fn main() {
                println!("hello");
                vec![1, 2, 3];
            }
        "#;
        assert!(extract(src).is_empty());
    }

    #[test]
    fn returns_annotations_in_source_order() {
        let src = r#"
            #[aristo::intent("first")]
            fn a() {}
            #[aristo::intent("second")]
            fn b() {}
            #[aristo::intent("third")]
            fn c() {}
        "#;
        let ann = extract(src);
        assert_eq!(ann.len(), 3);
        assert_eq!(ann[0].text, "first");
        assert_eq!(ann[1].text, "second");
        assert_eq!(ann[2].text, "third");
        // Lines must be strictly increasing too.
        assert!(ann[0].line < ann[1].line);
        assert!(ann[1].line < ann[2].line);
    }

    #[test]
    fn malformed_source_returns_parse_error() {
        let err = extract_from_source("fn unbalanced(").unwrap_err();
        assert!(matches!(err, ExtractError::Parse(_)));
    }

    #[test]
    fn text_hash_is_populated() {
        let ann = extract(r#"#[aristo::intent("hello")] fn x() {}"#);
        let expected = text_hash("hello");
        assert_eq!(ann[0].text_hash, expected);
    }

    #[test]
    fn body_hash_changes_with_body_change() {
        let a = extract(r#"#[aristo::intent("x")] fn f() { 1 }"#);
        let b = extract(r#"#[aristo::intent("x")] fn f() { 2 }"#);
        assert_eq!(a[0].text_hash, b[0].text_hash, "text unchanged");
        assert_ne!(
            a[0].body_hash, b[0].body_hash,
            "body changed → hash changes"
        );
    }

    #[test]
    fn body_hash_unchanged_when_only_text_changes() {
        // Text-only edit must NOT flip body_hash — lets `aristo stamp`'s
        // drift detection distinguish "re-word the intent" (review-cache
        // invalidate) from "edit the code" (re-verify needed). This test
        // is the slice-17 contract that depends on the slice-14B body-span
        // computation excluding the attribute itself.
        let a = extract(r#"#[aristo::intent("v1")] fn f() -> i32 { 42 }"#);
        let b = extract(r#"#[aristo::intent("v2")] fn f() -> i32 { 42 }"#);
        assert_ne!(a[0].text_hash, b[0].text_hash, "text changed");
        assert_eq!(
            a[0].body_hash, b[0].body_hash,
            "body unchanged → hash unchanged"
        );
    }
}
