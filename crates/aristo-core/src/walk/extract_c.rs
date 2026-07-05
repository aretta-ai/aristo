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
//! - **Function and type-definition directives.** A directive attaches to the
//!   function, or the `struct` / `union` / `enum` definition (tagged or
//!   `typedef`), on the next line. Statement-form directives land in a later
//!   slice.
//! - **Attachment is by adjacency** — the directive must be on the line directly
//!   above the function definition, with no blank line between. A gap detaches
//!   the directive (it is dropped). The explicit `site = "..."` escape hatch for
//!   macro-defined / non-adjacent targets lands in a later slice.
//! - **Block comments `/* ... */` are not directives** — only `//` line comments,
//!   because a block comment can float anywhere (mid-expression, mid-argument).
//! - **The covered region is the item's brace-delimited body** (`{ ... }`) — a
//!   function's block, or a type's field / enumerator list — hashed verbatim,
//!   like a Rust `fn` block or struct body, so a code edit flips `body_hash`
//!   while a prose-only edit to the directive does not.

use tree_sitter::{Node, Parser};

use crate::index::{AnnotationKind, CoveredRegion};
use crate::walk::extract::{
    make_annotation, AnnotationArgs, AnnotationForm, ExtractError, ExtractedAnnotation,
};

/// A recognized `// @aristo <kind>(<args>)` directive, before the argument
/// list is parsed. `args` is the raw substring between the outer parentheses,
/// fed verbatim to the shared Rust argument grammar.
struct CDirective {
    kind: AnnotationKind,
    args: String,
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

    let mut found = Vec::new();

    // Comments are tree-sitter "extra" nodes that appear as siblings between
    // top-level declarations, so a directive comment and the item it annotates
    // are adjacent children of the translation unit. Walk them in source order,
    // accumulating a contiguous run of directive comments; when a supported
    // item (function, or struct/union/enum definition) follows immediately
    // below the run, the whole run attaches to it (mirrors Rust allowing
    // multiple `#[aristo::*]` on one item).
    let mut pending: Vec<(CDirective, Node)> = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "comment" {
            let text = child.utf8_text(src_bytes).unwrap_or("");
            match parse_directive(text) {
                Some(dir) => {
                    // Keep the run contiguous: a directive not on the line
                    // immediately below the previous one starts a fresh run.
                    if let Some((_, last)) = pending.last() {
                        if child.start_position().row != last.end_position().row + 1 {
                            pending.clear();
                        }
                    }
                    pending.push((dir, child));
                }
                // A non-directive comment breaks the contiguous run.
                None => pending.clear(),
            }
        } else if let Some(item) = c_item(&child, source) {
            if let Some((_, last)) = pending.last() {
                // Adjacency: the item starts on the line directly below the
                // last directive comment. Any gap detaches the run.
                if child.start_position().row == last.end_position().row + 1 {
                    for (dir, comment) in &pending {
                        if let Some(ann) = build_c_annotation(dir, comment, &item, source) {
                            found.push(ann);
                        }
                    }
                }
            }
            pending.clear();
        } else {
            // Any other top-level node breaks a dangling directive pairing.
            pending.clear();
        }
    }

    Ok(found)
}

/// Recognize `// @aristo intent(<args>)` / `// @aristo assume(<args>)`.
/// Returns the kind and the raw argument substring on a hit, `None` otherwise.
/// Only `//` line comments are directives; block comments are ignored.
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
    Some(CDirective {
        kind,
        args: inner.to_string(),
    })
}

/// Build one [`ExtractedAnnotation`] from a directive attached to a C item.
/// Reuses the Rust-side argument grammar and annotation builder verbatim, so
/// hashing and field semantics are identical across languages. Malformed
/// argument lists are skipped silently — same policy as the Rust extractor,
/// where the compile/lint layer is responsible for the diagnostic.
fn build_c_annotation(
    dir: &CDirective,
    comment: &Node,
    item: &CItem,
    source: &str,
) -> Option<ExtractedAnnotation> {
    // Body = the item's brace-delimited region (a function's `{ ... }` block,
    // or a type's `{ ... }` field / enumerator list), excluding the signature
    // and the directive — so a prose-only edit to the directive leaves
    // body_hash unchanged, and only a code edit to the body flips it.
    let body_text = source
        .get(item.body.start_byte()..item.body.end_byte())?
        .to_string();
    let line = comment.start_position().row + 1; // 1-indexed, like the Rust extractor
    let args: AnnotationArgs = syn::parse_str(&dir.args).ok()?;
    Some(make_annotation(
        dir.kind,
        AnnotationForm::Attribute,
        args,
        &item.site,
        line,
        item.region,
        body_text,
    ))
}

/// A C item an annotation can attach to: the display site label, the node
/// whose bytes feed `body_hash`, and the covered-region kind.
struct CItem<'t> {
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
        .ok()?;
    // A specifier with no `{ ... }` body is a *reference* (`struct Foo x;`),
    // not a definition — there is nothing to cover, so it does not attach.
    let body = spec.child_by_field_name("body")?;
    Some(CItem {
        site: format!("{keyword} {name}"),
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
        .ok()?;
    Some(CItem {
        site: format!("{keyword} {name}"),
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

    #[test]
    fn block_comment_is_not_a_directive() {
        let src = "\
/* @aristo intent(\"block comments are not directives\") */
int f(void) { return 0; }
";
        assert!(extract(src).is_empty());
    }
}
