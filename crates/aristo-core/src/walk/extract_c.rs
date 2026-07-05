//! Parse a C source string and extract every Aristo comment-directive annotation.
//!
//! C has no attribute syntax, so annotations ride in **line-comment directives**
//! placed on the line immediately above the function they describe:
//!
//! ```c
//! // @aristo intent("recover replays every intact record, newest wins", verify = "test", id = "recover_newest_wins")
//! int db_open(const char *dir) { /* ... */ }
//! ```
//!
//! The directive keyword (`intent` / `assume`) picks the kind. The parenthesized
//! argument list uses the **exact same grammar** as the Rust `#[aristo::intent(...)]`
//! macro — positional text string, then `verify = ...`, `parent = ...`, `id = ...` —
//! parsed by the shared [`AnnotationArgs`] grammar so the two languages can never
//! drift apart. "Same shape, different skin."
//!
//! Scope (narrow by design, expanding per slice):
//!
//! - **Function, type, and statement directives.** A directive attaches to the
//!   function or the `struct` / `union` / `enum` definition (tagged or
//!   `typedef`) on the next line; or, inside a function body, to the statement
//!   on the next line (`CoveredRegion::Statement` — the C analog of Rust's
//!   `intent_stmt!`), at any nesting depth.
//! - **Attachment is by adjacency, or by explicit target.** A directive with no
//!   `site` attaches to the item on the line directly below it (a blank-line gap
//!   detaches it). A directive whose first argument is `site = "name"` instead
//!   attaches to the function or type named `name` anywhere in the file — the
//!   escape hatch for macro-defined or doc-comment-separated targets adjacency
//!   can't reach.
//! - **Block comments `/* ... */` are not directives** — only `//` line comments,
//!   because a block comment can float anywhere (mid-expression, mid-argument).
//! - **The covered region is the item's brace-delimited body** (`{ ... }`) — a
//!   function's block, or a type's field / enumerator list — hashed verbatim,
//!   like a Rust `fn` block or struct body, so a code edit flips `body_hash`
//!   while a prose-only edit to the directive does not.

use std::collections::HashMap;

use tree_sitter::{Node, Parser};

use crate::index::{AnnotationKind, CoveredRegion};
use crate::walk::extract::{
    make_annotation, AnnotationArgs, AnnotationForm, ExtractError, ExtractedAnnotation,
};

/// A recognized, fully-parsed `// @aristo <kind>(<args>)` directive. `site` is
/// the optional C-only target selector (Option B: peeled off before the shared
/// Rust grammar sees the args); `args` is the shared grammar's output.
struct CDirective {
    kind: AnnotationKind,
    site: Option<String>,
    args: AnnotationArgs,
}

#[aristo::intent(
    "C annotations return in source order — top of file first — mirroring the \
     Rust extractor's contract. The parse tree is walked top-to-bottom and \
     results are pushed in encounter order; collecting through any unordered \
     structure, or sorting, would break the stable index ordering the \
     downstream fixtures index into positionally.",
    verify = "test",
    id = "extract_c_returns_annotations_in_source_order"
)]
pub fn extract_from_c_source(source: &str) -> Result<Vec<ExtractedAnnotation>, ExtractError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .map_err(|e| ExtractError::CParse(format!("tree-sitter-c language load failed: {e}")))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ExtractError::CParse("tree-sitter returned no parse tree".to_string()))?;
    let root = tree.root_node();
    let src_bytes = source.as_bytes();

    // Pass 1: index every top-level item — by bare name (for `site = "..."`,
    // which may target an item defined anywhere in the file) and by start row
    // (for adjacency).
    let mut items: Vec<CItem> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut item_at_row: HashMap<usize, usize> = HashMap::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some(item) = c_item(&child, source) {
            let idx = items.len();
            // First definition wins on a duplicate name (C forbids two anyway;
            // this just keeps `site` resolution deterministic).
            by_name.entry(item.name.clone()).or_insert(idx);
            item_at_row.insert(child.start_position().row, idx);
            items.push(item);
        }
    }

    // Pass 2: gather each contiguous run of directive comments and resolve it.
    // Comments are tree-sitter "extra" nodes interleaved among the top-level
    // declarations. A directive with `site = "..."` attaches to the named item;
    // otherwise it attaches to the item on the line directly below its run.
    // Runs resolve in source order, so annotations come out in source order.
    let mut found = Vec::new();
    let mut run: Vec<CPendingDirective> = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "comment" {
            let text = child.utf8_text(src_bytes).unwrap_or("");
            match parse_directive(text) {
                Some(dir) => {
                    // A directive not on the line immediately below the
                    // previous one starts a fresh run.
                    if let Some(last) = run.last() {
                        if child.start_position().row != last.end_row + 1 {
                            resolve_run(&run, &items, &by_name, &item_at_row, source, &mut found);
                            run.clear();
                        }
                    }
                    run.push(CPendingDirective {
                        dir,
                        start_row: child.start_position().row,
                        end_row: child.end_position().row,
                    });
                }
                // A non-directive comment breaks the contiguous run.
                None => {
                    resolve_run(&run, &items, &by_name, &item_at_row, source, &mut found);
                    run.clear();
                }
            }
        } else {
            // Any non-comment node ends the run; its adjacent item (if any) was
            // recorded in pass 1, so `resolve_run` finds it by row.
            resolve_run(&run, &items, &by_name, &item_at_row, source, &mut found);
            run.clear();
            // Statement-form directives live inside function bodies; descend.
            if child.kind() == "function_definition" {
                if let Some(name) = c_function_name(&child, source) {
                    let site = format!("fn {name}");
                    if let Some(body) = child.child_by_field_name("body") {
                        walk_stmt_directives(&body, &site, &items, &by_name, source, &mut found);
                    }
                }
            }
        }
    }
    resolve_run(&run, &items, &by_name, &item_at_row, source, &mut found);

    Ok(found)
}

/// Recognize `// @aristo intent(<args>)` / `// @aristo assume(<args>)` and
/// fully parse the argument list. Returns `None` for a non-directive comment
/// or malformed args — the compile/lint layer reports malformed args; the
/// extractor does not double-report. Only `//` line comments are directives.
fn parse_directive(comment_text: &str) -> Option<CDirective> {
    let body = comment_text.strip_prefix("//")?.trim_start();
    let rest = body.strip_prefix("@aristo")?.trim_start();

    // The kind keyword must be followed by `(` (after optional whitespace),
    // so `intentional(...)` does not masquerade as an `intent` directive.
    let (kind, rest) = if let Some(r) = rest.strip_prefix("intent") {
        (AnnotationKind::Intent, r)
    } else if let Some(r) = rest.strip_prefix("assume") {
        (AnnotationKind::Assume, r)
    } else {
        return None;
    };

    let rest = rest.trim();
    let inner = rest.strip_prefix('(')?.strip_suffix(')')?;
    let parsed: CArgs = syn::parse_str(inner).ok()?;
    Some(CDirective {
        kind,
        site: parsed.site,
        args: parsed.args,
    })
}

/// C directive arguments: an optional C-only `site = "..."` target selector
/// peeled off here so it never reaches the shared Rust grammar (decision
/// "Option B"), followed by the shared [`AnnotationArgs`].
#[aristo::intent(
    "`site` is a C-only target selector and is peeled off here, never entering \
     the shared AnnotationArgs grammar (design decision Option B). Adding site \
     to AnnotationArgs to `simplify` this would pollute the Rust contract with \
     a field Rust has no use for — Rust attaches structurally and never needs \
     an explicit target. site must be the first argument so this peel stays a \
     single leading-token check instead of a full re-parse of the arg list.",
    verify = "neural",
    id = "c_site_selector_stays_out_of_shared_grammar"
)]
struct CArgs {
    site: Option<String>,
    args: AnnotationArgs,
}

impl syn::parse::Parse for CArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut site = None;
        // A leading `site = "..."` is the only ident-first form: the shared
        // grammar always starts with the positional text string literal, so an
        // identifier in first position can only be a C-side key.
        if input.peek(syn::Ident) && input.peek2(syn::Token![=]) {
            let ident: syn::Ident = input.fork().parse()?;
            if ident == "site" {
                input.parse::<syn::Ident>()?;
                input.parse::<syn::Token![=]>()?;
                let lit: syn::LitStr = input.parse()?;
                site = Some(lit.value());
                input.parse::<syn::Token![,]>()?;
            }
        }
        let args: AnnotationArgs = input.parse()?;
        Ok(CArgs { site, args })
    }
}

/// A pending directive within a contiguous run, carrying its comment's row
/// span (for adjacency and the 1-indexed line the annotation records).
struct CPendingDirective {
    dir: CDirective,
    start_row: usize,
    end_row: usize,
}

/// Resolve one contiguous run of directives to their target items and push the
/// resulting annotations (in run order = source order). A directive with
/// `site` resolves by name; otherwise by adjacency to the item directly below
/// the run. A directive with no resolvable target (unknown `site`, or no item
/// below) is dropped.
fn resolve_run(
    run: &[CPendingDirective],
    items: &[CItem],
    by_name: &HashMap<String, usize>,
    item_at_row: &HashMap<usize, usize>,
    source: &str,
    found: &mut Vec<ExtractedAnnotation>,
) {
    let Some(last) = run.last() else {
        return;
    };
    let adjacent = item_at_row.get(&(last.end_row + 1)).copied();
    for pending in run {
        let target = match &pending.dir.site {
            Some(name) => by_name.get(name).copied(),
            None => adjacent,
        };
        if let Some(idx) = target {
            if let Some(ann) =
                build_c_annotation(&pending.dir, &items[idx], pending.start_row + 1, source)
            {
                found.push(ann);
            }
        }
    }
}

/// Build one [`ExtractedAnnotation`] from a directive attached to a C item.
/// Reuses the (already-parsed) shared argument grammar and annotation builder
/// verbatim, so hashing and field semantics are identical across languages.
fn build_c_annotation(
    dir: &CDirective,
    item: &CItem,
    line: usize,
    source: &str,
) -> Option<ExtractedAnnotation> {
    // Body = the item's brace-delimited region (a function's `{ ... }` block,
    // or a type's `{ ... }` field / enumerator list), excluding the signature
    // and the directive — so a prose-only edit to the directive leaves
    // body_hash unchanged, and only a code edit to the body flips it.
    let body_text = source
        .get(item.body.start_byte()..item.body.end_byte())?
        .to_string();
    Some(make_annotation(
        dir.kind,
        AnnotationForm::Attribute,
        dir.args.clone(),
        &item.site,
        line,
        item.region,
        body_text,
    ))
}

/// Walk a function body recursively for statement-form directives — the C
/// analog of Rust's `intent_stmt!`. A directive attaches to the statement on
/// the line directly below it, with the enclosing function as its `site` and
/// `CoveredRegion::Statement`. A directive carrying `site` still resolves to
/// the named top-level item, exactly as at file scope.
fn walk_stmt_directives(
    block: &Node,
    enclosing_site: &str,
    items: &[CItem],
    by_name: &HashMap<String, usize>,
    source: &str,
    found: &mut Vec<ExtractedAnnotation>,
) {
    let mut run: Vec<CPendingDirective> = Vec::new();
    let mut cursor = block.walk();
    // `named_children` skips the `{ } ;` punctuation tokens, so every
    // non-comment child here is a real statement / declaration.
    for child in block.named_children(&mut cursor) {
        if child.kind() == "comment" {
            match parse_directive(child.utf8_text(source.as_bytes()).unwrap_or("")) {
                Some(dir) => {
                    if let Some(last) = run.last() {
                        if child.start_position().row != last.end_row + 1 {
                            resolve_stmt_run(
                                &run,
                                None,
                                enclosing_site,
                                items,
                                by_name,
                                source,
                                found,
                            );
                            run.clear();
                        }
                    }
                    run.push(CPendingDirective {
                        dir,
                        start_row: child.start_position().row,
                        end_row: child.end_position().row,
                    });
                }
                None => {
                    resolve_stmt_run(&run, None, enclosing_site, items, by_name, source, found);
                    run.clear();
                }
            }
        } else {
            resolve_stmt_run(
                &run,
                Some(child),
                enclosing_site,
                items,
                by_name,
                source,
                found,
            );
            run.clear();
            // Descend so directives nested in loop / if / block bodies attach too.
            walk_stmt_directives(&child, enclosing_site, items, by_name, source, found);
        }
    }
    resolve_stmt_run(&run, None, enclosing_site, items, by_name, source, found);
}

/// Resolve a run of in-body directives: `site` ones to their named top-level
/// item, the rest by adjacency to `stmt` (the statement directly below the
/// run, when it is on the next line).
fn resolve_stmt_run(
    run: &[CPendingDirective],
    stmt: Option<Node>,
    enclosing_site: &str,
    items: &[CItem],
    by_name: &HashMap<String, usize>,
    source: &str,
    found: &mut Vec<ExtractedAnnotation>,
) {
    let Some(last) = run.last() else {
        return;
    };
    let adjacent = stmt.filter(|s| s.start_position().row == last.end_row + 1);
    for pending in run {
        let line = pending.start_row + 1;
        match &pending.dir.site {
            Some(name) => {
                if let Some(&idx) = by_name.get(name) {
                    if let Some(ann) = build_c_annotation(&pending.dir, &items[idx], line, source) {
                        found.push(ann);
                    }
                }
            }
            None => {
                if let Some(stmt) = adjacent {
                    if let Some(ann) =
                        build_c_stmt_annotation(&pending.dir, &stmt, enclosing_site, line, source)
                    {
                        found.push(ann);
                    }
                }
            }
        }
    }
}

/// Build a statement-form annotation: covered region is the whole statement's
/// bytes, the form is `Statement`, and the site is the enclosing function.
fn build_c_stmt_annotation(
    dir: &CDirective,
    stmt: &Node,
    enclosing_site: &str,
    line: usize,
    source: &str,
) -> Option<ExtractedAnnotation> {
    let body_text = source.get(stmt.start_byte()..stmt.end_byte())?.to_string();
    Some(make_annotation(
        dir.kind,
        AnnotationForm::Statement,
        dir.args.clone(),
        enclosing_site,
        line,
        CoveredRegion::Statement,
        body_text,
    ))
}

/// A C item an annotation can attach to: the bare `name` (for `site = "..."`
/// resolution), the display `site` label, the node whose bytes feed
/// `body_hash`, and the covered-region kind.
struct CItem<'t> {
    name: String,
    site: String,
    body: Node<'t>,
    region: CoveredRegion,
}

/// Resolve the top-level C item a directive attaches to. C-1 covered
/// functions; C-2 adds tagged and typedef `struct` / `union` / `enum`
/// definitions (covered region = the `{ ... }` field / enumerator list).
fn c_item<'t>(node: &Node<'t>, source: &str) -> Option<CItem<'t>> {
    match node.kind() {
        "function_definition" => {
            let name = c_function_name(node, source)?;
            let body = node.child_by_field_name("body")?;
            Some(CItem {
                site: format!("fn {name}"),
                name,
                body,
                region: CoveredRegion::Function,
            })
        }
        // `struct Name { ... };` / `union` / `enum` — a bare tagged definition
        // is a top-level specifier (not wrapped in a `declaration`).
        "struct_specifier" | "union_specifier" | "enum_specifier" => {
            c_tagged_type_item(node, source)
        }
        // `typedef struct { ... } Name;` — the site name is the new typedef
        // name; the body is the (usually anonymous) specifier's list.
        "type_definition" => c_typedef_type_item(node, source),
        _ => None,
    }
}

/// A bare top-level `struct Name { ... };` (also union / enum): name and body
/// come from the specifier itself.
fn c_tagged_type_item<'t>(spec: &Node<'t>, source: &str) -> Option<CItem<'t>> {
    let keyword = type_keyword(spec)?;
    let name = spec
        .child_by_field_name("name")?
        .utf8_text(source.as_bytes())
        .ok()?
        .to_string();
    // A specifier with no `{ ... }` body is a *reference* (`struct Foo x;`),
    // not a definition — there is nothing to cover, so it does not attach.
    let body = spec.child_by_field_name("body")?;
    Some(CItem {
        site: format!("{keyword} {name}"),
        name,
        body,
        region: CoveredRegion::Type,
    })
}

/// `typedef struct { ... } Name;` — the site name is the typedef name; the
/// body is the specifier's list.
fn c_typedef_type_item<'t>(td: &Node<'t>, source: &str) -> Option<CItem<'t>> {
    let spec = td.child_by_field_name("type")?;
    let keyword = type_keyword(&spec)?;
    let body = spec.child_by_field_name("body")?;
    let name = td
        .child_by_field_name("declarator")?
        .utf8_text(source.as_bytes())
        .ok()?
        .to_string();
    Some(CItem {
        site: format!("{keyword} {name}"),
        name,
        body,
        region: CoveredRegion::Type,
    })
}

fn type_keyword(spec: &Node) -> Option<&'static str> {
    match spec.kind() {
        "struct_specifier" => Some("struct"),
        "union_specifier" => Some("union"),
        "enum_specifier" => Some("enum"),
        _ => None,
    }
}

/// Resolve a `function_definition`'s name by descending its declarator chain
/// (`pointer_declarator` / `parenthesized_declarator` wrappers for
/// pointer-returning functions) down to the `function_declarator`'s identifier.
fn c_function_name(func: &Node, source: &str) -> Option<String> {
    let mut cur = func.child_by_field_name("declarator")?;
    for _ in 0..16 {
        if cur.kind() == "function_declarator" {
            let name_node = cur.child_by_field_name("declarator")?;
            return declarator_identifier(&name_node, source);
        }
        cur = cur.child_by_field_name("declarator")?;
    }
    None
}

/// Descend a declarator to its innermost `identifier` (past any
/// `parenthesized_declarator` / pointer wrappers).
fn declarator_identifier(start: &Node, source: &str) -> Option<String> {
    let mut cur = *start;
    for _ in 0..16 {
        if cur.kind() == "identifier" {
            return cur.utf8_text(source.as_bytes()).ok().map(|s| s.to_string());
        }
        cur = cur.child_by_field_name("declarator")?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::text_hash;
    use crate::walk::extract::ParentRaw;

    fn extract(s: &str) -> Vec<ExtractedAnnotation> {
        extract_from_c_source(s).expect("test source must parse as C")
    }

    #[test]
    fn extracts_intent_directive_above_function() {
        let src = "\
// @aristo intent(\"adds one to the input\", verify = \"test\", id = \"add_one\")
int add_one(int x) { return x + 1; }
";
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].kind, AnnotationKind::Intent);
        assert_eq!(ann[0].form, AnnotationForm::Attribute);
        assert_eq!(ann[0].text, "adds one to the input");
        assert_eq!(ann[0].verify.as_deref(), Some("\"test\""));
        assert_eq!(ann[0].id.as_deref(), Some("add_one"));
        assert_eq!(ann[0].site, "fn add_one");
        assert_eq!(ann[0].covered_region, CoveredRegion::Function);
        assert_eq!(ann[0].line, 1);
    }

    #[test]
    fn extracts_assume_directive_has_no_verify() {
        let src = "\
// @aristo assume(\"the OS zero-fills freshly mmapped pages\")
int reads_zeroed(void) { return 0; }
";
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].kind, AnnotationKind::Assume);
        assert_eq!(ann[0].verify, None);
    }

    #[test]
    fn parses_parent_singular_and_list() {
        let src = "\
// @aristo intent(\"child\", parent = \"ancestor\")
int a(void) { return 0; }
// @aristo intent(\"multi\", parent = [\"one\", \"two\"])
int b(void) { return 0; }
";
        let ann = extract(src);
        assert_eq!(ann.len(), 2);
        assert_eq!(ann[0].parent, Some(ParentRaw::Single("ancestor".into())));
        assert_eq!(
            ann[1].parent,
            Some(ParentRaw::Multiple(vec!["one".into(), "two".into()]))
        );
    }

    #[test]
    fn resolves_name_of_pointer_returning_function() {
        let src = "\
// @aristo intent(\"allocates a buffer of n bytes\")
char *make_buf(int n) { return 0; }
";
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].site, "fn make_buf");
    }

    #[test]
    fn blank_line_between_directive_and_function_detaches() {
        let src = "\
// @aristo intent(\"detached — a blank line breaks adjacency\")

int f(void) { return 0; }
";
        assert!(extract(src).is_empty());
    }

    #[test]
    fn non_directive_comment_is_ignored() {
        let src = "\
// an ordinary comment
int f(void) { return 0; }
";
        assert!(extract(src).is_empty());
    }

    #[test]
    fn stacked_directives_both_attach() {
        let src = "\
// @aristo intent(\"describes behaviour\")
// @aristo assume(\"relies on an external invariant\")
int f(void) { return 0; }
";
        let ann = extract(src);
        assert_eq!(ann.len(), 2, "a contiguous run of directives all attach");
        assert_eq!(ann[0].kind, AnnotationKind::Intent);
        assert_eq!(ann[1].kind, AnnotationKind::Assume);
        assert_eq!(ann[0].site, "fn f");
        assert_eq!(ann[1].site, "fn f");
    }

    #[test]
    fn returns_annotations_in_source_order() {
        let src = "\
// @aristo intent(\"first\")
int a(void) { return 0; }
// @aristo intent(\"second\")
int b(void) { return 0; }
// @aristo intent(\"third\")
int c(void) { return 0; }
";
        let ann = extract(src);
        assert_eq!(ann.len(), 3);
        assert_eq!(ann[0].text, "first");
        assert_eq!(ann[1].text, "second");
        assert_eq!(ann[2].text, "third");
        assert!(ann[0].line < ann[1].line);
        assert!(ann[1].line < ann[2].line);
    }

    #[test]
    fn source_without_directives_returns_empty() {
        let src = "int plain(void) { return 42; }\n";
        assert!(extract(src).is_empty());
    }

    #[test]
    fn malformed_args_silently_skipped() {
        // No positional text string — mirrors the Rust extractor: the
        // compile/lint layer reports; the extractor does not double-report.
        let src = "\
// @aristo intent(no_text = \"x\")
int bad(void) { return 0; }
";
        assert!(extract(src).is_empty());
    }

    #[test]
    fn text_hash_is_populated() {
        let ann = extract("// @aristo intent(\"hello\")\nint x(void) { return 0; }\n");
        assert_eq!(ann[0].text_hash, text_hash("hello"));
    }

    #[test]
    fn body_hash_changes_with_body_but_not_with_text() {
        let a = extract("// @aristo intent(\"v1\")\nint f(void) { return 1; }\n");
        let b = extract("// @aristo intent(\"v2\")\nint f(void) { return 1; }\n");
        let c = extract("// @aristo intent(\"v1\")\nint f(void) { return 2; }\n");
        // Text-only edit: body_hash stable, text_hash differs.
        assert_ne!(a[0].text_hash, b[0].text_hash);
        assert_eq!(
            a[0].body_hash, b[0].body_hash,
            "body unchanged → hash stable"
        );
        // Body edit: body_hash flips.
        assert_ne!(
            a[0].body_hash, c[0].body_hash,
            "body changed → hash changes"
        );
    }

    // ─── type sites (C-2): struct / union / enum ─────────────────────────

    #[test]
    fn extracts_directive_on_tagged_struct() {
        let src = "\
// @aristo intent(\"a live key maps to the newest record's location\", id = \"keydir_entry\")
struct Entry {
    unsigned long file_id;
    unsigned long offset;
};
";
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].site, "struct Entry");
        assert_eq!(ann[0].covered_region, CoveredRegion::Type);
    }

    #[test]
    fn extracts_directive_on_enum_and_union() {
        let src = "\
// @aristo intent(\"record tag discriminates a put from a tombstone\")
enum RecordTag { PUT, TOMBSTONE };
// @aristo intent(\"payload is interpreted per the record tag\")
union Payload { long as_int; double as_float; };
";
        let ann = extract(src);
        assert_eq!(ann.len(), 2);
        assert_eq!(ann[0].site, "enum RecordTag");
        assert_eq!(ann[0].covered_region, CoveredRegion::Type);
        assert_eq!(ann[1].site, "union Payload");
        assert_eq!(ann[1].covered_region, CoveredRegion::Type);
    }

    #[test]
    fn extracts_directive_on_typedef_struct() {
        let src = "\
// @aristo intent(\"opaque store handle; single-writer\", id = \"db_handle\")
typedef struct {
    int fd;
    unsigned long next_seqno;
} Db;
";
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].site, "struct Db");
        assert_eq!(ann[0].covered_region, CoveredRegion::Type);
    }

    #[test]
    fn type_body_hash_changes_with_fields_but_not_directive_text() {
        let a = extract("// @aristo intent(\"v1\")\nstruct S { int a; };\n");
        let b = extract("// @aristo intent(\"v2\")\nstruct S { int a; };\n");
        let c = extract("// @aristo intent(\"v1\")\nstruct S { int a; int b; };\n");
        assert_ne!(a[0].text_hash, b[0].text_hash);
        assert_eq!(a[0].body_hash, b[0].body_hash, "field list unchanged");
        assert_ne!(a[0].body_hash, c[0].body_hash, "field added → hash changes");
    }

    #[test]
    fn directive_on_a_plain_variable_declaration_does_not_attach() {
        // `struct Foo x;` is a reference, not a definition — no body to cover.
        let src = "\
struct Foo { int a; };
// @aristo intent(\"not a definition\")
struct Foo x;
";
        // Only the definition would attach IF annotated; the directive here
        // sits above a variable declaration and must be dropped.
        let ann = extract(src);
        assert!(ann.is_empty());
    }

    // ─── explicit target selection (C-2): site = "..." ──────────────────
    // `site = "name"` must be the FIRST argument — it is peeled off before the
    // shared grammar (Option B; see CArgs).

    #[test]
    fn site_targets_a_function_not_adjacent() {
        // The directive sits above a variable declaration (which adjacency
        // cannot attach to), but `site` names the function below it.
        let src = "\
// @aristo intent(site = \"db_open\", \"open recovers the durable prefix\", verify = \"test\", id = \"recover\")
static int internal_state;

int db_open(const char *dir) { return 0; }
";
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].site, "fn db_open");
        assert_eq!(ann[0].id.as_deref(), Some("recover"));
        assert_eq!(ann[0].verify.as_deref(), Some("\"test\""));
    }

    #[test]
    fn site_reaches_past_an_intervening_comment() {
        // A kernel-doc / Doxygen block between the directive and the function
        // breaks adjacency; `site` reaches the target anyway.
        let src = "\
// @aristo intent(site = \"clamp\", \"clamps the value into the range\")
// an ordinary doc comment that would break plain adjacency
int clamp(int x) { return x; }
";
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].site, "fn clamp");
    }

    #[test]
    fn site_targets_a_type_by_name() {
        let src = "\
// @aristo intent(site = \"Entry\", \"a live key maps to its newest record\")
int unrelated(void) { return 0; }
typedef struct { unsigned long off; } Entry;
";
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].site, "struct Entry");
        assert_eq!(ann[0].covered_region, CoveredRegion::Type);
    }

    #[test]
    fn site_to_unknown_name_is_dropped() {
        // `site` present but no such item — adjacency is not a fallback.
        let src = "\
// @aristo intent(site = \"nope\", \"targets a nonexistent item\")
int real(void) { return 0; }
";
        assert!(extract(src).is_empty());
    }

    #[test]
    fn site_overrides_adjacency() {
        let src = "\
// @aristo intent(\"attaches by adjacency\", id = \"adj\")
int alpha(void) { return 0; }
// @aristo intent(site = \"alpha\", \"overrides adjacency to target alpha\", id = \"exp\")
int beta(void) { return 0; }
";
        let ann = extract(src);
        assert_eq!(ann.len(), 2);
        assert_eq!(ann[0].id.as_deref(), Some("adj"));
        assert_eq!(ann[0].site, "fn alpha");
        // The second directive is adjacent to `beta` but `site` retargets it.
        assert_eq!(ann[1].id.as_deref(), Some("exp"));
        assert_eq!(ann[1].site, "fn alpha");
    }

    // ─── statement-form directives (C-2): the intent_stmt analog ─────────

    #[test]
    fn extracts_stmt_directive_before_a_loop() {
        let src = "\
int checksum(const char *buf, int n) {
    int sum = 0;
    // @aristo intent(\"each byte contributes once; no index is read twice\", verify = \"test\")
    for (int i = 0; i < n; i++) {
        sum += buf[i];
    }
    return sum;
}
";
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].form, AnnotationForm::Statement);
        assert_eq!(ann[0].covered_region, CoveredRegion::Statement);
        assert_eq!(ann[0].site, "fn checksum", "site is the enclosing function");
    }

    #[test]
    fn extracts_stmt_directive_nested_in_a_loop_body() {
        let src = "\
int f(int n) {
    for (int i = 0; i < n; i++) {
        // @aristo intent(\"the accumulator never overflows for valid n\")
        long acc = i;
        (void) acc;
    }
    return 0;
}
";
        let ann = extract(src);
        assert_eq!(
            ann.len(),
            1,
            "a directive nested in a loop body must attach"
        );
        assert_eq!(ann[0].form, AnnotationForm::Statement);
        assert_eq!(ann[0].site, "fn f");
    }

    #[test]
    fn stmt_directive_body_hash_tracks_the_statement() {
        let a = extract("int f() {\n// @aristo intent(\"x\")\nint y = 1;\nreturn y;\n}\n");
        let b = extract("int f() {\n// @aristo intent(\"x\")\nint y = 2;\nreturn y;\n}\n");
        assert_eq!(a.len(), 1);
        assert_ne!(
            a[0].body_hash, b[0].body_hash,
            "the covered statement changed"
        );
    }

    #[test]
    fn function_and_statement_directives_come_out_in_source_order() {
        let src = "\
// @aristo intent(\"function-level\", id = \"fn_level\")
int g(int n) {
    // @aristo intent(\"statement-level\", id = \"stmt_level\")
    int t = n;
    return t;
}
";
        let ann = extract(src);
        assert_eq!(ann.len(), 2);
        assert_eq!(ann[0].id.as_deref(), Some("fn_level"));
        assert_eq!(ann[1].id.as_deref(), Some("stmt_level"));
        assert!(ann[0].line < ann[1].line);
    }

    #[test]
    fn block_comment_is_not_a_directive() {
        let src = "\
/* @aristo intent(\"block comments are not directives\") */
int f(void) { return 0; }
";
        assert!(extract(src).is_empty());
    }
}
