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
    // Row of the last `// @aristo` directive of ANY kind — adjacency to the
    // item below the block is measured from here, so a foreign directive
    // (e.g. inspect) between an intent and its item does not detach it.
    let mut last_directive_row: Option<usize> = None;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "comment" {
            // A comment not directly below the previous directive breaks the block.
            if let Some(last) = last_directive_row {
                if child.start_position().row != last + 1 {
                    resolve_run(
                        &run,
                        last_directive_row,
                        &items,
                        &by_name,
                        &item_at_row,
                        source,
                        &mut found,
                    );
                    run.clear();
                    last_directive_row = None;
                }
            }
            let text = child.utf8_text(src_bytes).unwrap_or("");
            if is_aristo_directive(text) {
                // An intent/assume joins the run; a foreign aristo directive
                // (inspect/expose) is transparent — it neither joins nor breaks.
                if let Some(dir) = parse_directive(text) {
                    run.push(CPendingDirective {
                        dir,
                        start_row: child.start_position().row,
                        end_row: child.end_position().row,
                    });
                }
                last_directive_row = Some(child.end_position().row);
            } else {
                // A plain comment breaks the block.
                resolve_run(
                    &run,
                    last_directive_row,
                    &items,
                    &by_name,
                    &item_at_row,
                    source,
                    &mut found,
                );
                run.clear();
                last_directive_row = None;
            }
        } else {
            // Any non-comment node ends the run; its adjacent item (if any) was
            // recorded in pass 1, so `resolve_run` finds it by row.
            resolve_run(
                &run,
                last_directive_row,
                &items,
                &by_name,
                &item_at_row,
                source,
                &mut found,
            );
            run.clear();
            last_directive_row = None;
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
    resolve_run(
        &run,
        last_directive_row,
        &items,
        &by_name,
        &item_at_row,
        source,
        &mut found,
    );

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
    block_end_row: Option<usize>,
    items: &[CItem],
    by_name: &HashMap<String, usize>,
    item_at_row: &HashMap<usize, usize>,
    source: &str,
    found: &mut Vec<ExtractedAnnotation>,
) {
    if run.is_empty() {
        return;
    }
    // Adjacency is measured from the last aristo directive of any kind, so an
    // interleaved foreign directive (inspect/expose) does not detach the run.
    let adjacent = block_end_row.and_then(|r| item_at_row.get(&(r + 1)).copied());
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

// ─── inspect directives (I-2): field-accessor codegen input ──────────────

/// A parsed `// @aristo inspect(...)` directive: the codegen input for one
/// read-only field accessor. Unlike `intent`/`assume` (which feed the index),
/// `inspect` feeds `aristo instrument gen-c`, so it is extracted separately
/// and carries no proof fields — only what codegen needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CInspectDirective {
    /// The enclosing type's name — the `<Type>` in `aristo_inspect_<Type>_<field>`.
    pub type_name: String,
    /// The struct member to snapshot (required).
    pub field: String,
    /// The accessor's C return type, verbatim (required). Clone mode needs a
    /// public/standard type; projection mode uses the projector's return type.
    pub ret: String,
    /// Projection mode: the name of an author-written pure function
    /// `ret with(const <FieldType> *)`. `None` = clone mode (`return self->field`).
    pub with: Option<String>,
    /// Optional override for the `<field>` suffix of the accessor name.
    pub name: Option<String>,
    /// 1-indexed line of the directive.
    pub line: usize,
}

impl CInspectDirective {
    /// The generated accessor's name: `aristo_inspect_<Type>_<suffix>`, where
    /// `suffix` is `name` if given, else `field`.
    pub fn accessor_name(&self) -> String {
        let suffix = self.name.as_deref().unwrap_or(&self.field);
        format!("aristo_inspect_{}_{}", self.type_name, suffix)
    }
}

/// Parsed key=value args of an inspect directive, before type-attachment.
struct InspectArgs {
    field: String,
    ret: String,
    with: Option<String>,
    name: Option<String>,
}

impl syn::parse::Parse for InspectArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let (mut field, mut ret, mut with, mut name) = (None, None, None, None);
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            let val: syn::LitStr = input.parse()?;
            match key.to_string().as_str() {
                "field" => field = Some(val.value()),
                "ret" => ret = Some(val.value()),
                "with" => with = Some(val.value()),
                "name" => name = Some(val.value()),
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown inspect arg `{other}`"),
                    ))
                }
            }
            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            }
        }
        Ok(InspectArgs {
            field: field.ok_or_else(|| input.error("inspect requires `field`"))?,
            ret: ret.ok_or_else(|| input.error("inspect requires `ret`"))?,
            with,
            name,
        })
    }
}

/// True if a `//` comment is any `// @aristo …` directive (of any kind).
fn is_aristo_directive(comment_text: &str) -> bool {
    comment_text
        .strip_prefix("//")
        .map(|b| b.trim_start().starts_with("@aristo"))
        .unwrap_or(false)
}

/// Strip a directive keyword only at a real word boundary: `strip_keyword("inspect(…", "inspect")`
/// succeeds, but `strip_keyword("inspected…", "inspect")` returns `None` — an
/// `inspect`-prefixed longer identifier is not the `inspect` keyword.
fn strip_keyword<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    match rest.chars().next() {
        Some(c) if c.is_alphanumeric() || c == '_' => None,
        _ => Some(rest),
    }
}

/// Recognize `// @aristo inspect(...)` and parse its args. Returns `None` when
/// the comment is not an inspect directive at all; `Some(Ok)` when it parses;
/// `Some(Err(msg))` when it IS an inspect directive but its args are malformed
/// — which is a hard error, because gen-c is the only thing that validates a
/// comment directive (unlike intent/assume, whose macro layer reports).
fn parse_inspect_directive(comment_text: &str) -> Option<Result<InspectArgs, String>> {
    let body = comment_text.strip_prefix("//")?.trim_start();
    let rest = body.strip_prefix("@aristo")?.trim_start();
    let rest = strip_keyword(rest, "inspect")?.trim();
    // A recognized `inspect` keyword commits: from here, any parse failure is a
    // malformed inspect directive, not a "different directive".
    let inner = match rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        Some(inner) => inner,
        None => return Some(Err("expected `inspect(...)`".to_string())),
    };
    Some(syn::parse_str::<InspectArgs>(inner).map_err(|e| e.to_string()))
}

/// Collect the direct field names of a struct/union body, and whether every
/// member was understood. Returns `(names, complete)`: `complete` is false if
/// any member could not be reduced to a simple field name (anonymous member,
/// bitfield shape we don't model, etc.). Callers must only reject an unknown
/// field when `complete` is true — a codegen tool must never reject a *valid*
/// field it merely failed to parse.
#[aristo::intent(
    "`complete` is false whenever ANY struct member could not be reduced to a \
     plain field name — an anonymous union/struct member, a bitfield, or any \
     shape this walker does not model. The unknown-field check MUST gate on \
     `complete`: rejecting a field when the member list is only partially \
     understood would fail a build on VALID code (a false negative), which for \
     a codegen tool is worse than the silent-drop it replaced. Widening the \
     set of members treated as \"understood\" without proving they are truly \
     enumerable re-opens the false-reject hole.",
    verify = "test",
    id = "c_struct_field_completeness_gates_unknown_field_rejection"
)]
fn c_struct_field_names(body: &Node, source: &str) -> (Vec<String>, bool) {
    let mut names = Vec::new();
    let mut complete = true;
    let mut cursor = body.walk();
    for decl in body.named_children(&mut cursor) {
        if decl.kind() != "field_declaration" {
            // A non-field member (nested struct def, static assert, etc.) — we
            // can't vouch for completeness.
            complete = false;
            continue;
        }
        let mut found_one = false;
        let mut dc = decl.walk();
        for child in decl.named_children(&mut dc) {
            if let Some(n) = field_declarator_name(&child, source) {
                names.push(n);
                found_one = true;
            }
        }
        // A field_declaration with no resolvable declarator (anonymous union
        // member, bitfield-only) means we didn't see everything.
        if !found_one {
            complete = false;
        }
    }
    (names, complete)
}

/// Descend a struct member's declarator to its `field_identifier`, past pointer
/// / array wrappers. `None` for a member with no plain field name.
fn field_declarator_name(node: &Node, source: &str) -> Option<String> {
    let mut cur = *node;
    for _ in 0..16 {
        if cur.kind() == "field_identifier" {
            return cur.utf8_text(source.as_bytes()).ok().map(|s| s.to_string());
        }
        cur = cur.child_by_field_name("declarator")?;
    }
    None
}

#[aristo::intent(
    "inspect directives attach to the type on the line directly below a \
     contiguous block of `// @aristo` directive lines — an intervening \
     intent/assume directive does NOT break the block (adjacency is measured \
     from the last aristo directive of any kind), but a plain comment or a \
     blank-line gap does. This keeps a struct's intent and its inspect \
     directives freely interleavable above it while a reformatter that \
     inserts a blank line still (correctly) detaches them.",
    verify = "test",
    id = "extract_c_inspect_attaches_across_mixed_directive_block"
)]
pub fn extract_c_inspect_directives(source: &str) -> Result<Vec<CInspectDirective>, ExtractError> {
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
    // Every malformed inspect directive (F1) and unknown-field reference (F2)
    // is collected here and reported together — a codegen input error, since
    // nothing else validates a comment directive.
    let mut problems: Vec<String> = Vec::new();
    // A block of contiguous `// @aristo` directive lines; we collect the
    // inspect args and track the row of the last directive of ANY kind so a
    // Type item directly below the block attaches.
    let mut run: Vec<(InspectArgs, usize)> = Vec::new(); // (args, start_row)
    let mut last_directive_row: Option<usize> = None;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "comment" {
            // A comment not directly below the previous directive breaks the
            // block. (`last_directive_row` is reassigned below in both arms, so
            // only the run needs clearing here.)
            if let Some(last) = last_directive_row {
                if child.start_position().row != last + 1 {
                    run.clear();
                }
            }
            let text = child.utf8_text(src_bytes).unwrap_or("");
            if is_aristo_directive(text) {
                match parse_inspect_directive(text) {
                    Some(Ok(args)) => run.push((args, child.start_position().row)),
                    Some(Err(msg)) => {
                        problems.push(format!("line {}: {msg}", child.start_position().row + 1))
                    }
                    None => {} // a non-inspect aristo directive (intent/assume/expose)
                }
                last_directive_row = Some(child.end_position().row);
            } else {
                // A plain comment breaks the block.
                run.clear();
                last_directive_row = None;
            }
        } else if let Some(item) = c_item(&child, source) {
            if item.region == CoveredRegion::Type && !run.is_empty() {
                if let Some(last) = last_directive_row {
                    if child.start_position().row == last + 1 {
                        // F2: reject a field the struct doesn't declare — but
                        // only when we're sure we enumerated all fields, never
                        // rejecting a valid field we merely failed to parse.
                        let (fields, complete) = c_struct_field_names(&item.body, source);
                        for (args, start_row) in &run {
                            if complete && !fields.contains(&args.field) {
                                problems.push(format!(
                                    "line {}: field `{}` is not declared in `{}`",
                                    start_row + 1,
                                    args.field,
                                    item.name
                                ));
                                continue;
                            }
                            found.push(CInspectDirective {
                                type_name: item.name.clone(),
                                field: args.field.clone(),
                                ret: args.ret.clone(),
                                with: args.with.clone(),
                                name: args.name.clone(),
                                line: start_row + 1,
                            });
                        }
                    }
                }
            }
            run.clear();
            last_directive_row = None;
        } else {
            run.clear();
            last_directive_row = None;
        }
    }
    if !problems.is_empty() {
        return Err(ExtractError::CInspectInvalid(problems.join("\n")));
    }
    Ok(found)
}

// ─── expose directives (I-4): reach a TU-local function from a harness ────

/// A parsed `// @aristo expose` directive: a request to emit a type-checked
/// prototype for one function so a harness can call it. The function is marked
/// `ARISTO_TU_LOCAL` (external linkage when instrumented); this makes its
/// signature callable without the harness hand-writing an unchecked prototype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CExposeDirective {
    /// The function's name (for ordering / diagnostics).
    pub name: String,
    /// The verbatim source signature — everything from the function's first
    /// token up to (not including) the `{`. It keeps any `ARISTO_TU_LOCAL`
    /// prefix, which expands to nothing when the harness compiles instrumented,
    /// so the emitted `<signature>;` is exactly the real declaration.
    pub signature: String,
    /// 1-indexed line of the directive.
    pub line: usize,
}

/// Recognize `// @aristo expose`. `None` = not an expose directive; `Some(Ok)`
/// = the bare form we support; `Some(Err)` = the `expose(as = "...")` forwarder
/// form, which renames the symbol and is not built yet (documented, deferred).
fn parse_expose_directive(comment_text: &str) -> Option<Result<(), String>> {
    let body = comment_text.strip_prefix("//")?.trim_start();
    let rest = body.strip_prefix("@aristo")?.trim_start();
    let rest = strip_keyword(rest, "expose")?.trim();
    if rest.is_empty() {
        return Some(Ok(()));
    }
    match rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        Some(inner) if inner.trim().is_empty() => Some(Ok(())),
        Some(inner) => Some(Err(format!(
            "the `expose({})` forwarder form is not yet supported; use bare \
             `// @aristo expose` on a function marked `ARISTO_TU_LOCAL`",
            inner.trim()
        ))),
        None => Some(Err("expected `// @aristo expose` (bare)".to_string())),
    }
}

/// The verbatim signature of a function definition: source bytes from the
/// function's start up to its body's opening brace, trimmed. Robust to the
/// `ARISTO_TU_LOCAL` macro prefix (which tree-sitter mis-parses into an ERROR
/// node) precisely because it uses byte ranges, not the mangled parse tree.
fn c_function_signature(func: &Node, source: &str) -> Option<String> {
    let body = func.child_by_field_name("body")?;
    let sig = source.get(func.start_byte()..body.start_byte())?;
    Some(sig.trim().to_string())
}

#[aristo::intent(
    "The exposed prototype is the function's VERBATIM source signature (bytes \
     up to the body brace), not a signature rebuilt from tree-sitter fields. A \
     function carrying the `ARISTO_TU_LOCAL` macro prefix mis-parses into an \
     ERROR node (the macro is read as the return type and the real return type \
     as an error), so reconstructing `<type> <declarator>` from the tree would \
     emit a WRONG prototype. Byte-range extraction is immune: the emitted \
     `<signature>;` is exactly the real declaration, and `ARISTO_TU_LOCAL` \
     expands to nothing when the harness compiles instrumented.",
    verify = "test",
    id = "expose_prototype_is_verbatim_signature_not_reconstructed"
)]
pub fn extract_c_expose_directives(source: &str) -> Result<Vec<CExposeDirective>, ExtractError> {
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
    let mut problems: Vec<String> = Vec::new();
    // Number of expose directives in the current contiguous `// @aristo` block.
    let mut pending_expose = 0usize;
    let mut last_directive_row: Option<usize> = None;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "comment" {
            if let Some(last) = last_directive_row {
                if child.start_position().row != last + 1 {
                    pending_expose = 0;
                }
            }
            let text = child.utf8_text(src_bytes).unwrap_or("");
            if is_aristo_directive(text) {
                match parse_expose_directive(text) {
                    Some(Ok(())) => pending_expose += 1,
                    Some(Err(msg)) => {
                        problems.push(format!("line {}: {msg}", child.start_position().row + 1))
                    }
                    None => {}
                }
                last_directive_row = Some(child.end_position().row);
            } else {
                pending_expose = 0;
                last_directive_row = None;
            }
        } else {
            if child.kind() == "function_definition" && pending_expose > 0 {
                if let Some(last) = last_directive_row {
                    if child.start_position().row == last + 1 {
                        match (
                            c_function_name(&child, source),
                            c_function_signature(&child, source),
                        ) {
                            (Some(name), Some(signature)) => {
                                // Emit once even if several expose directives stack.
                                found.push(CExposeDirective {
                                    name,
                                    signature,
                                    line: child.start_position().row + 1,
                                });
                            }
                            _ => problems.push(format!(
                                "line {}: could not resolve the exposed function's signature",
                                child.start_position().row + 1
                            )),
                        }
                    }
                }
            }
            pending_expose = 0;
            last_directive_row = None;
        }
    }
    if !problems.is_empty() {
        return Err(ExtractError::CInspectInvalid(problems.join("\n")));
    }
    Ok(found)
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

    // ─── inspect directives (I-2): codegen input, separate from the index ──

    fn inspect(s: &str) -> Vec<CInspectDirective> {
        extract_c_inspect_directives(s).expect("test source must parse as C")
    }

    #[test]
    fn extracts_clone_inspect_on_typedef_struct() {
        let src = "\
// @aristo inspect(field = \"next_seqno\", ret = \"uint64_t\")
typedef struct {
    int fd;
    unsigned long next_seqno;
} Db;
";
        let d = inspect(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].type_name, "Db");
        assert_eq!(d[0].field, "next_seqno");
        assert_eq!(d[0].ret, "uint64_t");
        assert_eq!(d[0].with, None); // clone mode
        assert_eq!(d[0].accessor_name(), "aristo_inspect_Db_next_seqno");
    }

    #[test]
    fn extracts_projection_inspect_with_and_name() {
        let src = "\
// @aristo inspect(field = \"keydir\", ret = \"size_t\", with = \"keydir_live_count\", name = \"live_keys\")
struct Db { int keydir; };
";
        let d = inspect(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].with.as_deref(), Some("keydir_live_count"));
        assert_eq!(d[0].name.as_deref(), Some("live_keys"));
        // name overrides the suffix
        assert_eq!(d[0].accessor_name(), "aristo_inspect_Db_live_keys");
    }

    #[test]
    fn multiple_inspects_stack_and_keep_source_order() {
        let src = "\
// @aristo inspect(field = \"a\", ret = \"int\")
// @aristo inspect(field = \"b\", ret = \"long\")
struct S { int a; long b; };
";
        let d = inspect(src);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].field, "a");
        assert_eq!(d[1].field, "b");
        assert!(d[0].line < d[1].line);
    }

    #[test]
    fn inspect_attaches_across_an_interleaved_intent() {
        // An intent directive between/around inspects must NOT break the block.
        let src = "\
// @aristo intent(\"the store handle; single-writer\", id = \"db_handle\")
// @aristo inspect(field = \"next_seqno\", ret = \"uint64_t\")
typedef struct { unsigned long next_seqno; } Db;
";
        let d = inspect(src);
        assert_eq!(d.len(), 1, "inspect after an intent still attaches");
        assert_eq!(d[0].accessor_name(), "aristo_inspect_Db_next_seqno");
        // And the intent still reaches the index via the other pass.
        let ann = extract(src);
        assert_eq!(ann.len(), 1);
        assert_eq!(ann[0].id.as_deref(), Some("db_handle"));
    }

    #[test]
    fn blank_line_detaches_inspect_from_the_struct() {
        let src = "\
// @aristo inspect(field = \"a\", ret = \"int\")

struct S { int a; };
";
        assert!(inspect(src).is_empty());
    }

    #[test]
    fn plain_comment_breaks_the_inspect_block() {
        let src = "\
// @aristo inspect(field = \"a\", ret = \"int\")
// an ordinary comment
struct S { int a; };
";
        assert!(inspect(src).is_empty());
    }

    #[test]
    fn inspect_missing_required_ret_is_a_hard_error() {
        // No `ret` — a recognized inspect directive with bad args is a loud
        // error (F1), never a silent drop: gen-c is the only validator.
        let src = "\
// @aristo inspect(field = \"a\")
struct S { int a; };
";
        let err = extract_c_inspect_directives(src).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("line 1"), "error must name the line: {msg}");
        assert!(
            msg.contains("ret"),
            "error must name the missing arg: {msg}"
        );
    }

    #[test]
    fn inspect_unknown_arg_is_a_hard_error() {
        let src = "\
// @aristo inspect(field = \"a\", ret = \"int\", bogus = \"x\")
struct S { int a; };
";
        assert!(extract_c_inspect_directives(src).is_err());
    }

    #[test]
    fn inspect_unknown_field_is_a_hard_error() {
        // F2: the struct doesn't declare `nope` — rejected, pointing at the
        // directive line and the type.
        let src = "\
// @aristo inspect(field = \"nope\", ret = \"int\")
struct S { int a; long b; };
";
        let err = extract_c_inspect_directives(src).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nope") && msg.contains('S'), "got: {msg}");
    }

    #[test]
    fn inspect_valid_field_passes_field_check() {
        let src = "\
// @aristo inspect(field = \"b\", ret = \"long\")
struct S { int a; long b; };
";
        let d = inspect(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].field, "b");
    }

    #[test]
    fn inspect_field_check_is_conservative_on_pointer_fields() {
        // A pointer field must still be recognized as declared (no false reject).
        let src = "\
// @aristo inspect(field = \"buf\", ret = \"const char *\")
struct S { char *buf; unsigned long len; };
";
        let d = inspect(src);
        assert_eq!(d.len(), 1, "pointer field must be recognized as declared");
    }

    #[test]
    fn inspect_directives_do_not_leak_into_the_index() {
        // The index walk (intent/assume) must ignore inspect directives.
        let src = "\
// @aristo inspect(field = \"a\", ret = \"int\")
struct S { int a; };
";
        assert!(
            extract(src).is_empty(),
            "inspect is not an index annotation"
        );
    }

    #[test]
    fn inspectfoo_is_not_an_inspect_directive() {
        // Keyword-boundary guard: `inspectfoo(...)` is not `inspect`.
        assert!(parse_inspect_directive("// @aristo inspectfoo(x = \"1\")").is_none());
    }

    // ─── expose directives (I-4): TU-local function prototypes ────────────

    fn expose(s: &str) -> Vec<CExposeDirective> {
        extract_c_expose_directives(s).expect("test source must parse as C")
    }

    #[test]
    fn exposes_a_tu_local_function_verbatim_signature() {
        let src = "\
// @aristo expose
ARISTO_TU_LOCAL int recover_replay(Db *db) { return 0; }
";
        let d = expose(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].name, "recover_replay");
        // Verbatim signature keeps the macro prefix (expands to nothing on).
        assert_eq!(d[0].signature, "ARISTO_TU_LOCAL int recover_replay(Db *db)");
    }

    #[test]
    fn exposes_a_plain_static_function() {
        let src = "\
// @aristo expose
static long tally(const char *buf, int n) { return 0; }
";
        let d = expose(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].signature, "static long tally(const char *buf, int n)");
    }

    #[test]
    fn expose_forwarder_form_is_a_hard_error() {
        // The `as=` forwarder is documented-but-unbuilt: reject loudly.
        let src = "\
// @aristo expose(as = \"recover_for_test\")
ARISTO_TU_LOCAL int recover_replay(Db *db) { return 0; }
";
        let err = extract_c_expose_directives(src).unwrap_err();
        assert!(err.to_string().contains("forwarder"), "got: {err}");
    }

    #[test]
    fn expose_without_a_function_below_is_dropped() {
        let src = "\
// @aristo expose

int detached(void) { return 0; }
";
        assert!(expose(src).is_empty());
    }

    #[test]
    fn expose_ignores_inspect_and_index_directives() {
        // Only expose directives produce expose records.
        let src = "\
// @aristo intent(\"does a thing\", id = \"x\")
int f(void) { return 0; }
// @aristo inspect(field = \"a\", ret = \"int\")
struct S { int a; };
";
        assert!(expose(src).is_empty());
    }
}
