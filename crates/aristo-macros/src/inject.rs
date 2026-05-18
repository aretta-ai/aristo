//! `aristo_doc` cargo-feature injection (slice 30).
//!
//! When the `aristo_doc` feature is OFF (default), every entrypoint in
//! this module returns an empty `TokenStream` so the proc-macros emit
//! their wrapped item unchanged.
//!
//! When ON, `doc_attribute_or_error` emits an
//! `#[doc = include_str!("<abs-path>")]` attribute that the proc-macro
//! prepends to the user's item. The absolute path is computed at
//! expansion time by walking up from `CARGO_MANIFEST_DIR` to the nearest
//! `aristo.toml`, then appending `.aristo/doc/<id-safe>.md` (id-safe
//! mirrors the slice-28 filename convention: `:` → `__`). `include_str!`
//! is the right tool because cargo tracks the embedded file as a build
//! input — touching the MD triggers recompile of the user's crate.
//!
//! v0 design choice: an EXPLICIT `id = "..."` argument is required when
//! the feature is on. The macro cannot predict the id `aristo stamp`
//! will assign for implicit-id annotations (snake_case derivation can
//! collide; opaque `aret_*` fallback is RNG-based). Pushing the burden
//! to the user keeps the macro lightweight and matches the dogfood
//! pattern. See CLAUDE.md §10 + the slice-30 ROADMAP row.

#[cfg(any(test, feature = "aristo_doc"))]
use std::path::{Path, PathBuf};

/// Filesystem-safe form of an annotation id: `:` → `__`. User-written ids
/// can't actually contain `:` (the parser rejects `aristos:` in source),
/// but the substitution is defensive and mirrors the slice-28 convention
/// so `aristo doc` and the macro agree on the filename for every id.
///
/// `cfg(any(test, feature = "aristo_doc"))` keeps the symbol alive when
/// the feature is OFF only in test builds — production builds without the
/// feature exclude it entirely, avoiding dead-code lint slop.
#[cfg(any(test, feature = "aristo_doc"))]
pub(crate) fn build_doc_path(id_value: &str, aristo_root: &Path) -> PathBuf {
    let id_safe = id_value.replace(':', "__");
    aristo_root
        .join(".aristo")
        .join("doc")
        .join(format!("{id_safe}.md"))
}

/// Walk up from `CARGO_MANIFEST_DIR` looking for `aristo.toml`. Returns
/// the directory containing it, or `None` if walking reaches the
/// filesystem root without finding one.
///
/// Resolves correctly for both single-crate projects (where
/// `CARGO_MANIFEST_DIR` == aristo root) and Cargo workspaces (where
/// member crates are nested below the workspace + aristo root).
#[cfg(feature = "aristo_doc")]
pub(crate) fn find_aristo_root_from(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        if cur.join("aristo.toml").is_file() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// Build the doc-injection attribute for an annotation. With the feature
/// OFF, returns an empty `TokenStream` and ignores `id` — the macros
/// downstream still work as a pass-through.
#[cfg(feature = "aristo_doc")]
pub(crate) fn doc_attribute_or_error(
    id: Option<&syn::LitStr>,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    use quote::quote;

    let id_lit = match id {
        Some(lit) => lit,
        None => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "the `aristo_doc` cargo feature requires an explicit `id = \"...\"` \
                 argument: the doc-injection lookup is keyed by id and the macro \
                 cannot predict the id `aristo stamp` will assign for implicit-id \
                 annotations (collision-resolved `aret_*` opaque ids depend on \
                 the RNG; snake-case derivation can collide). Add `id = \"some_name\"` \
                 to opt into doc-injection for this annotation.",
            ));
        }
    };

    let manifest_dir = match std::env::var_os("CARGO_MANIFEST_DIR") {
        Some(d) => PathBuf::from(d),
        None => {
            return Err(syn::Error::new(
                id_lit.span(),
                "CARGO_MANIFEST_DIR was not set in the build environment; the \
                 `aristo_doc` feature requires a cargo-driven build to locate \
                 `.aristo/doc/`",
            ));
        }
    };

    let aristo_root = find_aristo_root_from(&manifest_dir).ok_or_else(|| {
        syn::Error::new(
            id_lit.span(),
            format!(
                "could not locate `aristo.toml` walking up from `{}`. \
                 The `aristo_doc` feature reads `.aristo/doc/<id>.md` relative \
                 to the workspace's aristo root; run `aristo init` to bootstrap \
                 the workspace.",
                manifest_dir.display(),
            ),
        )
    })?;

    let path = build_doc_path(&id_lit.value(), &aristo_root);
    let path_str = path.to_string_lossy().into_owned();
    Ok(quote! {
        #[doc = include_str!(#path_str)]
    })
}

/// Off-feature stub — returns empty token stream, never errors. Keeps the
/// proc-macro call site uniform regardless of feature state.
#[cfg(not(feature = "aristo_doc"))]
pub(crate) fn doc_attribute_or_error(
    _id: Option<&syn::LitStr>,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    Ok(proc_macro2::TokenStream::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn build_doc_path_replaces_colon_with_double_underscore() {
        let p = build_doc_path("aristos:balance_no_duplicate_cells", Path::new("/proj"));
        assert_eq!(
            p,
            Path::new("/proj/.aristo/doc/aristos__balance_no_duplicate_cells.md")
        );
    }

    #[test]
    fn build_doc_path_leaves_local_id_unchanged() {
        let p = build_doc_path("my_local_intent", Path::new("/proj"));
        assert_eq!(p, Path::new("/proj/.aristo/doc/my_local_intent.md"));
    }

    #[test]
    fn build_doc_path_handles_relative_aristo_root() {
        let p = build_doc_path("foo", Path::new("."));
        assert_eq!(p, Path::new("./.aristo/doc/foo.md"));
    }

    #[cfg(feature = "aristo_doc")]
    #[test]
    fn find_aristo_root_walks_up_to_marker() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Workspace root has the marker.
        std::fs::write(tmp.path().join("aristo.toml"), "").unwrap();
        // Nested crate dir; walking should find the parent.
        let nested = tmp.path().join("crates").join("my-crate");
        std::fs::create_dir_all(&nested).unwrap();

        let found = find_aristo_root_from(&nested).expect("should find aristo.toml");
        // Canonicalize both — TempDir on macOS surfaces /var vs /private/var.
        let found_canon = std::fs::canonicalize(&found).unwrap();
        let expected_canon = std::fs::canonicalize(tmp.path()).unwrap();
        assert_eq!(found_canon, expected_canon);
    }

    #[cfg(feature = "aristo_doc")]
    #[test]
    fn find_aristo_root_returns_none_when_no_marker() {
        let tmp = tempfile::TempDir::new().unwrap();
        let nested = tmp.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        assert!(find_aristo_root_from(&nested).is_none());
    }

    #[cfg(feature = "aristo_doc")]
    #[test]
    fn doc_attribute_errors_when_id_missing() {
        let err = doc_attribute_or_error(None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("requires an explicit `id"),
            "expected id-required hint; got: {msg}"
        );
    }

    #[cfg(not(feature = "aristo_doc"))]
    #[test]
    fn doc_attribute_is_empty_when_feature_off() {
        let ts = doc_attribute_or_error(None).unwrap();
        assert!(
            ts.to_string().trim().is_empty(),
            "expected empty token stream when feature is off; got: {ts}"
        );
    }
}
