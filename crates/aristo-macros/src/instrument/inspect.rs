//! `#[derive(Inspect)]` — emits snapshot accessors over `SkipMap<K, V>`
//! fields.
//!
//! Each tagged field becomes a `pub fn inspect_<name>(&self) -> Vec<(K, T)>`
//! method on the struct. Two modes, picked at the attribute level:
//!
//! - **Clone mode**: `#[inspect]` (or `#[inspect(name = "...")]`). The
//!   macro iterates the `SkipMap` and clones each `V` into the
//!   snapshot. T = V. Requires `K: Copy`, `V: Clone`.
//! - **Project mode**: `#[inspect(SnapshotType)]` (or
//!   `#[inspect(SnapshotType, name = "...")]`). The macro iterates and
//!   applies the user-supplied `impl From<&V> for SnapshotType` per
//!   entry. T = SnapshotType. Requires `K: Copy` and that the user
//!   provide the `From` impl.
//!
//! In both modes the returned `Vec` is owned and point-in-time — the
//! harness can't write back to the SUT through it.
//!
//! Untagged fields are silently ignored (no codegen). `name = "..."`
//! overrides the default `inspect_<field>` method-name suffix; the
//! `inspect_` prefix itself is always automatic.
//!
//! v1 supports `SkipMap<K, V>` fields only. Other collection types
//! (`BTreeMap`, `HashMap`, `Vec`) and scalar fields are deferred —
//! attempting to tag them produces a clear error pointing at the
//! field. `K: Copy` is required because the generated body dereferences
//! the SkipMap entry's borrowed key (`*e.key()`); a non-Copy K surfaces
//! at the rustc level as a "cannot move out of `*e.key()`" error.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type, TypePath};

pub(crate) fn derive(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);

    let struct_name = &input.ident;
    // Thread the struct's generic params, ty-params, and where-clause
    // through the emitted impl header so `impl Store<Clock>` (not bare
    // `impl Store`) lands. The standard `syn` idiom; required for
    // structs with generic / lifetime / `where` parameters. See the
    // `inspect_generic_struct` regression case (v0.2.6).
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let named_fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    struct_name,
                    "`#[derive(Inspect)]` requires a struct with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                struct_name,
                "`#[derive(Inspect)]` only supports structs",
            )
            .to_compile_error()
            .into();
        }
    };

    let mut accessors = Vec::new();
    for field in named_fields {
        let args = match parse_inspect_attr(field) {
            Ok(Some(a)) => a,
            Ok(None) => continue,
            Err(err) => return err.to_compile_error().into(),
        };

        let field_name = field.ident.as_ref().expect("named field has an ident");
        let (key_ty, value_ty) = match extract_skipmap_kv(&field.ty) {
            Some(kv) => kv,
            None => {
                return syn::Error::new_spanned(
                    field_name,
                    "`#[inspect(...)]` only supports `SkipMap<K, V>` fields in v1 \
                     (other collection types and scalars are deferred to a future release)",
                )
                .to_compile_error()
                .into();
            }
        };

        let method_suffix = args.name.unwrap_or_else(|| field_name.to_string());
        let method_ident = format_ident!("inspect_{}", method_suffix);

        let (return_elem_ty, projection_expr) = match &args.snapshot_ty {
            Some(t) => (
                quote!(#t),
                quote!(<#t as ::core::convert::From<&#value_ty>>::from(e.value())),
            ),
            None => (
                quote!(#value_ty),
                quote!(::core::clone::Clone::clone(e.value())),
            ),
        };

        accessors.push(quote! {
            pub fn #method_ident(&self) -> ::std::vec::Vec<(#key_ty, #return_elem_ty)> {
                self.#field_name
                    .iter()
                    .map(|e| (*e.key(), #projection_expr))
                    .collect()
            }
        });
    }

    let expanded = quote! {
        impl #impl_generics #struct_name #ty_generics #where_clause {
            #(#accessors)*
        }
    };
    expanded.into()
}

/// Parsed `#[inspect]`, `#[inspect(T)]`, `#[inspect(name = "x")]`, or
/// `#[inspect(T, name = "x")]`. The absence of `T` selects clone mode;
/// its presence selects project mode.
struct InspectArgs {
    /// `Some(T)` → project via `impl From<&V> for T`. `None` → clone V.
    snapshot_ty: Option<Type>,
    /// Override for the method-name suffix (the `inspect_` prefix is
    /// always automatic). `None` → use the field's identifier.
    name: Option<String>,
}

fn parse_inspect_attr(field: &syn::Field) -> syn::Result<Option<InspectArgs>> {
    let mut found: Option<&syn::Attribute> = None;
    for attr in &field.attrs {
        if attr.path().is_ident("inspect") {
            if found.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "multiple `#[inspect(...)]` attributes on one field; combine them",
                ));
            }
            found = Some(attr);
        }
    }
    let Some(attr) = found else { return Ok(None) };

    let mut snapshot_ty: Option<Type> = None;
    let mut name: Option<String> = None;

    // Bare `#[inspect]` (no parentheses) is the simplest clone form.
    // syn::Meta::Path matches `#[inspect]` exactly.
    if matches!(attr.meta, syn::Meta::Path(_)) {
        return Ok(Some(InspectArgs { snapshot_ty, name }));
    }

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("name") {
            let value = meta.value()?;
            let lit: syn::LitStr = value.parse()?;
            name = Some(lit.value());
            Ok(())
        } else if meta.input.peek(syn::Token![=]) {
            // `<ident> = ...` for some unknown <ident> — error.
            Err(meta.error(
                "unknown `inspect` argument; expected positional `T` (projection type) or `name = \"...\"`",
            ))
        } else if snapshot_ty.is_none() {
            // First non-keyword item is the positional snapshot type.
            // `parse_nested_meta` gives us `meta.path` already; reuse it
            // as a Type (we only support type paths in this position).
            let path = meta.path.clone();
            snapshot_ty = Some(syn::parse_quote!(#path));
            Ok(())
        } else {
            Err(meta.error(
                "`#[inspect(...)]` accepts at most one positional projection type",
            ))
        }
    })?;

    Ok(Some(InspectArgs { snapshot_ty, name }))
}

/// Detect a `SkipMap<K, V>` type by trailing-path-segment match and
/// return `(K, V)`. Path prefix (`crossbeam_skiplist::SkipMap`,
/// `crossbeam_skiplist::map::SkipMap`, bare `SkipMap`) is ignored —
/// users can have it in scope under any name.
fn extract_skipmap_kv(ty: &Type) -> Option<(Type, Type)> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return None;
    };
    let last = path.segments.last()?;
    if last.ident != "SkipMap" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let mut iter = args.args.iter();
    let k = match iter.next()? {
        GenericArgument::Type(t) => t.clone(),
        _ => return None,
    };
    let v = match iter.next()? {
        GenericArgument::Type(t) => t.clone(),
        _ => return None,
    };
    Some((k, v))
}
