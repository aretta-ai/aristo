//! Function counter — enumerate the per-module function surface a project
//! exposes, scoped to what behaves at runtime in production.
//!
//! Sister to [`crate::walk::fs`]; reuses the same root-walk skip set
//! (`target/`, `.git/`, `.aristo/`, `node_modules/`) and exclude-glob
//! filter so an `aristo badge` invocation sees the same source tree that
//! `aristo stamp` would index. The counter only reads each `.rs` file
//! and parses it with `syn`; no extraction, no hashing.
//!
//! Counting rules (per `docs/decisions/badge-tier-scheme.md` D7):
//!
//! - `Item::Fn` at any module depth → counted.
//! - `ImplItem::Fn` inside `Item::Impl` → counted.
//! - `TraitItem::Fn` WITH a `default` block → counted (default methods
//!   carry behaviour; trait declarations without defaults are contracts
//!   and contribute zero verifiable surface here).
//! - `#[cfg(test)]`-gated items (fns, impls, trait declarations, or
//!   enclosing modules) → excluded recursively. The badge surface counts
//!   production behaviour, not test scaffolding.
//! - Closures, async blocks, macro-generated fns → excluded (the syn
//!   AST never sees the post-macro form, so this is automatic).
//!
//! Output is `BTreeMap<PathBuf, u32>` keyed by walk-root-relative path,
//! so iteration order is deterministic.
//!
//! Files with zero counted fns are still keyed in the map with `0`, so
//! callers can distinguish "module exists, no fn surface" from "module
//! not walked." This matters for the coverage-score denominator — pure
//! type modules are skipped by the badge formula, but the walker should
//! report them faithfully and let the formula apply its filter.
//!
//! [`crate::walk::fs`]: ../fs/index.html

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

use crate::walk::extract::ExtractError;
use crate::walk::fs::{walk_for_freshness_with, FsWalkError, WalkOptions};

/// Per-module fn count. `BTreeMap` for deterministic ordering across
/// runs and machines.
pub type FnCountByModule = BTreeMap<PathBuf, u32>;

/// Walk `root`, parse every `.rs` file, and count behaviour-bearing
/// functions per file. See module docs for the inclusion rules.
pub fn count_fns_per_module(root: &Path) -> Result<FnCountByModule, FsWalkError> {
    count_fns_per_module_with(root, &WalkOptions::none())
}

/// Same as [`count_fns_per_module`] but honors `opts` (the same exclude
/// globs `aristo stamp` applies).
pub fn count_fns_per_module_with(
    root: &Path,
    opts: &WalkOptions,
) -> Result<FnCountByModule, FsWalkError> {
    if !root.is_dir() {
        return Err(FsWalkError::BadRoot(root.to_path_buf()));
    }
    let paths = walk_for_freshness_with(root, opts)?;

    let mut out = FnCountByModule::new();
    for abs in paths {
        let source = std::fs::read_to_string(&abs).map_err(|source| FsWalkError::Io {
            path: abs.clone(),
            source,
        })?;
        let count = count_fns_in_source(&source).map_err(|source| FsWalkError::Parse {
            path: abs.clone(),
            source,
        })?;
        let rel = abs.strip_prefix(root).unwrap_or(&abs).to_path_buf();
        out.insert(rel, count);
    }
    Ok(out)
}

/// Pure-function counter on a single Rust source string. Exposed for unit
/// tests and for callers that have already loaded the source.
pub fn count_fns_in_source(source: &str) -> Result<u32, ExtractError> {
    let file: syn::File = syn::parse_str(source)?;
    let mut counter = FnCounter::default();
    counter.visit_file(&file);
    Ok(counter.count)
}

#[derive(Default)]
struct FnCounter {
    count: u32,
}

impl<'ast> Visit<'ast> for FnCounter {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if attrs_imply_cfg_test(&node.attrs) {
            return;
        }
        self.count += 1;
        // Don't descend — closures inside the body are not `Item::Fn`
        // and the rules exclude them anyway.
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if attrs_imply_cfg_test(&node.attrs) {
            return;
        }
        for item in &node.items {
            if let syn::ImplItem::Fn(m) = item {
                if !attrs_imply_cfg_test(&m.attrs) {
                    self.count += 1;
                }
            }
        }
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if attrs_imply_cfg_test(&node.attrs) {
            return;
        }
        for item in &node.items {
            if let syn::TraitItem::Fn(m) = item {
                // Only trait DEFAULT methods carry behaviour. Plain
                // declarations are contracts.
                if m.default.is_some() && !attrs_imply_cfg_test(&m.attrs) {
                    self.count += 1;
                }
            }
        }
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if attrs_imply_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }
}

/// True iff any of `attrs` is `#[cfg(test)]` or a nested form that
/// implies the item is test-only (e.g. `#[cfg(all(test, foo))]`).
///
/// `#[cfg(feature = "test")]`-style attributes are NOT matched — `test`
/// there is a string literal, not the `test` predicate, so the item
/// ships in non-test builds and counts toward the production surface.
fn attrs_imply_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(attr_is_cfg_test)
}

fn attr_is_cfg_test(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("cfg") {
        return false;
    }
    let mut found = false;
    let _ = attr.parse_nested_meta(|nested| nested_meta_implies_test(&nested, &mut found));
    found
}

fn nested_meta_implies_test(
    nested: &syn::meta::ParseNestedMeta<'_>,
    found: &mut bool,
) -> syn::Result<()> {
    if nested.path.is_ident("test") {
        *found = true;
        return Ok(());
    }
    if nested.input.peek(syn::token::Paren) {
        nested.parse_nested_meta(|inner| nested_meta_implies_test(&inner, found))?;
    } else if nested.input.peek(syn::Token![=]) {
        // Skip name = value forms (`feature = "test"`, `target_os = "linux"`).
        // Consume the rhs without inspecting it so the recursion doesn't
        // misread a string literal as the bare `test` predicate.
        let _ = nested.value();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn empty_source_counts_zero() {
        assert_eq!(count_fns_in_source("").unwrap(), 0);
    }

    #[test]
    fn single_free_fn_counts_one() {
        assert_eq!(count_fns_in_source("fn x() {}").unwrap(), 1);
    }

    #[test]
    fn multiple_free_fns_counted() {
        let src = "fn a() {} fn b() {} fn c() {}";
        assert_eq!(count_fns_in_source(src).unwrap(), 3);
    }

    #[test]
    fn impl_methods_counted() {
        let src = r#"
            struct S;
            impl S {
                fn a(&self) {}
                fn b(&self) {}
            }
        "#;
        assert_eq!(count_fns_in_source(src).unwrap(), 2);
    }

    #[test]
    fn trait_default_methods_counted() {
        let src = r#"
            trait T {
                fn declared(&self);
                fn defaulted(&self) {
                    let _ = ();
                }
            }
        "#;
        assert_eq!(count_fns_in_source(src).unwrap(), 1);
    }

    #[test]
    fn cfg_test_fn_excluded() {
        let src = r#"
            fn prod() {}
            #[cfg(test)]
            fn under_test() {}
        "#;
        assert_eq!(count_fns_in_source(src).unwrap(), 1);
    }

    #[test]
    fn cfg_test_module_excludes_all_its_fns() {
        let src = r#"
            fn prod() {}
            #[cfg(test)]
            mod tests {
                fn helper_one() {}
                fn helper_two() {}
                impl super::S {
                    fn helper_method(&self) {}
                }
            }
        "#;
        assert_eq!(count_fns_in_source(src).unwrap(), 1);
    }

    #[test]
    fn cfg_test_impl_block_excludes_all_methods() {
        let src = r#"
            struct S;
            #[cfg(test)]
            impl S {
                fn helper_a(&self) {}
                fn helper_b(&self) {}
            }
        "#;
        assert_eq!(count_fns_in_source(src).unwrap(), 0);
    }

    #[test]
    fn cfg_all_test_extra_excluded() {
        let src = r#"
            #[cfg(all(test, target_os = "linux"))]
            fn x() {}
        "#;
        assert_eq!(count_fns_in_source(src).unwrap(), 0);
    }

    #[test]
    fn cfg_any_test_extra_excluded() {
        let src = r#"
            #[cfg(any(test, debug_assertions))]
            fn x() {}
        "#;
        assert_eq!(count_fns_in_source(src).unwrap(), 0);
    }

    #[test]
    fn cfg_feature_test_string_not_matched() {
        // `feature = "test"` is a feature predicate, not the test
        // predicate — items ship in non-test builds and count toward
        // the production surface.
        let src = r#"
            #[cfg(feature = "test")]
            fn shipped_in_test_feature_builds() {}
        "#;
        assert_eq!(count_fns_in_source(src).unwrap(), 1);
    }

    #[test]
    fn nested_modules_descended_into() {
        let src = r#"
            fn root() {}
            mod inner {
                fn a() {}
                mod deeper {
                    fn b() {}
                }
            }
        "#;
        assert_eq!(count_fns_in_source(src).unwrap(), 3);
    }

    #[test]
    fn closures_inside_fn_body_not_counted() {
        let src = r#"
            fn outer() {
                let _f = |x: i32| x + 1;
                let _g = || ();
            }
        "#;
        assert_eq!(count_fns_in_source(src).unwrap(), 1);
    }

    #[test]
    fn count_fns_per_module_keys_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "fn a() {}");
        write(tmp.path(), "src/inner/mod.rs", "fn b() {} fn c() {}");
        let counts = count_fns_per_module(tmp.path()).unwrap();
        assert_eq!(counts.get(&PathBuf::from("src/lib.rs")), Some(&1));
        assert_eq!(counts.get(&PathBuf::from("src/inner/mod.rs")), Some(&2));
    }

    #[test]
    fn count_fns_per_module_includes_zero_fn_files() {
        // Pure-type modules still appear in the map so the badge formula
        // can decide whether to skip them.
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "struct A; struct B;");
        let counts = count_fns_per_module(tmp.path()).unwrap();
        assert_eq!(counts.get(&PathBuf::from("src/lib.rs")), Some(&0));
    }

    #[test]
    fn count_fns_per_module_skips_default_ignored_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "fn a() {}");
        write(tmp.path(), "target/debug/build.rs", "fn b() {}");
        write(tmp.path(), ".aristo/scratch.rs", "fn c() {}");
        let counts = count_fns_per_module(tmp.path()).unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get(&PathBuf::from("src/lib.rs")), Some(&1));
    }

    #[test]
    fn count_fns_per_module_errors_on_bad_root() {
        let nope = std::env::temp_dir().join("definitely-not-here-fn-count-test");
        assert!(matches!(
            count_fns_per_module(&nope),
            Err(FsWalkError::BadRoot(_))
        ));
    }

    #[test]
    fn count_fns_per_module_errors_on_unparseable_file() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/broken.rs", "fn unbalanced(");
        match count_fns_per_module(tmp.path()) {
            Err(FsWalkError::Parse { path, .. }) => {
                assert!(path.ends_with("broken.rs"), "got: {}", path.display());
            }
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn count_fns_per_module_honors_exclude_globs() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "src/lib.rs", "fn keep() {}");
        write(tmp.path(), "tests/ui/fixture.rs", "fn drop_me() {}");
        let opts = WalkOptions::from_patterns(&["**/tests/ui/**"]).unwrap();
        let counts = count_fns_per_module_with(tmp.path(), &opts).unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get(&PathBuf::from("src/lib.rs")), Some(&1));
    }
}
