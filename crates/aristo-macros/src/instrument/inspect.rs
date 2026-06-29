//! `#[derive(Inspect)]` — emits snapshot accessors over tagged fields.
//!
//! Each `#[inspect]`-tagged field becomes a `pub fn inspect_<name>(&self)`
//! method returning an owned, point-in-time snapshot of that one field. The
//! derive is **type-agnostic**: it never inspects the field's type. Two
//! modes, chosen purely at the attribute level.
//!
//! - **Clone mode** — bare `#[inspect]` (optionally `#[inspect(name = "…")]`):
//!
//!   ```ignore
//!   pub fn inspect_<name>(&self) -> <FieldType> { self.<field>.clone() }
//!   ```
//!
//!   The return type is the field's own declared type, echoed verbatim; the
//!   `Clone` bound is deferred to rustc. Works for any `Clone` field —
//!   scalars, `Option<T>`, `BTreeMap`, a foreign `SkipMap`, … — with no
//!   per-type codegen.
//!
//! - **Projection mode** — `#[inspect(ret = SnapTy, with = expr)]`
//!   (optionally `+ name = "…"`):
//!
//!   ```ignore
//!   pub fn inspect_<name>(&self) -> SnapTy {
//!       let __project: &dyn Fn(&<FieldType>) -> SnapTy = &(expr);
//!       __project(&self.<field>)
//!   }
//!   ```
//!
//!   `expr` is any `Fn(&FieldType) -> SnapTy` — a path to a named projector
//!   or an inline closure. `ret` is required and echoed as the return type
//!   verbatim: a syntactic proc-macro cannot infer a closure's return type,
//!   so this ascription is genuine and unremovable.
//!
//!   This form is **orphan-safe for any foreign field**. The macro emits an
//!   inherent method on the consumer struct and names the projection as an
//!   arbitrary expression, so the foreign type is never the `Self` of a
//!   foreign trait — no `impl` on the field type is ever required. Because
//!   the projector sees the whole field, it can FILTER and FAN-OUT (one map
//!   entry → N snapshot rows), which the prior per-entry form could not.
//!
//! Untagged fields are silently ignored. The `inspect_` prefix is always
//! automatic; `name = "…"` overrides only the suffix.

use proc_macro::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Expr, Fields, Type};

pub(crate) fn derive(input: TokenStream) -> TokenStream {
    derive_impl(input.into()).into()
}

fn derive_impl(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let input = match syn::parse2::<DeriveInput>(input) {
        Ok(parsed) => parsed,
        Err(err) => return err.to_compile_error(),
    };

    let struct_name = &input.ident;
    // Thread the struct's generics / where-clause through the emitted impl
    // header so `impl Store<Clock>` (not bare `impl Store`) lands. The
    // projection-mode `let __project: &dyn Fn(&FieldType) -> Ret` ascription
    // may mention these generics — which is exactly why a `let` binding is
    // used rather than an inner helper `fn` (a helper could not name the
    // struct's generics). See the `inspect_projection_generic_struct` case.
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let named_fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    struct_name,
                    "`#[derive(Inspect)]` requires a struct with named fields",
                )
                .to_compile_error();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                struct_name,
                "`#[derive(Inspect)]` only supports structs",
            )
            .to_compile_error();
        }
    };

    let mut accessors = Vec::new();
    for field in named_fields {
        let args = match parse_inspect_attr(field) {
            Ok(Some(a)) => a,
            Ok(None) => continue,
            Err(err) => return err.to_compile_error(),
        };

        let field_name = field.ident.as_ref().expect("named field has an ident");
        let field_ty = &field.ty;
        let method_suffix = args.name.unwrap_or_else(|| field_name.to_string());
        let method_ident = format_ident!("inspect_{}", method_suffix);

        let accessor = match args.mode {
            InspectMode::Clone => quote_spanned! { field_ty.span() =>
                // Span the accessor at the field's type so a deferred
                // `FieldTy: Clone is not satisfied` error points at the
                // offending field rather than the whole `#[derive(Inspect)]`.
                // The remedy is projection mode (`ret = …, with = …`); see the
                // `inspect_clone_non_clone_field` UI fixture. (aretta-bench C4.)
                pub fn #method_ident(&self) -> #field_ty {
                    ::core::clone::Clone::clone(&self.#field_name)
                }
            },
            InspectMode::Project(p) => {
                let Projection { ret, with } = &*p;
                quote! {
                    pub fn #method_ident(&self) -> #ret {
                        // Pin the projector's input/output types via the `let`
                        // ascription so an un-annotated inline closure can infer
                        // its parameter — the naive `(expr)(&self.x)` fails
                        // E0282 because closure-param inference does not flow
                        // back from an immediately-applied argument. The
                        // annotation may name the struct's generics; an inner
                        // helper `fn` could not, so a `&dyn Fn` binding is used.
                        let __project: &dyn ::core::ops::Fn(&#field_ty) -> #ret = &(#with);
                        __project(&self.#field_name)
                    }
                }
            }
        };
        accessors.push(accessor);
    }

    let expanded = quote! {
        // A projection `ret` (or a clone-mode field type) can be arbitrarily
        // nested, e.g. `Vec<Vec<(Vec<u8>, Vec<u8>, u32)>>`. That trips
        // `clippy::type_complexity` on the generated accessor — which the
        // consumer cannot silence from the annotation site (a derive-helper
        // `#[allow]` does not propagate into generated code). A macro is
        // responsible for its own codegen being lint-clean, so the allow
        // rides on the generated impl. (aretta-bench finding C5.)
        #[allow(clippy::type_complexity)]
        impl #impl_generics #struct_name #ty_generics #where_clause {
            #(#accessors)*
        }
    };
    expanded
}

/// A parsed `#[inspect]` tag. The mode is selected at the attribute level:
/// bare (or `name`-only) → clone the whole field; `ret = T, with = expr` →
/// project the field through `expr: Fn(&FieldType) -> T`.
struct InspectArgs {
    /// Override for the method-name suffix (the `inspect_` prefix is always
    /// automatic). `None` → use the field's identifier.
    name: Option<String>,
    mode: InspectMode,
}

enum InspectMode {
    /// Bare `#[inspect]`: `inspect_<name>(&self) -> FieldType`, cloning the field.
    Clone,
    /// `#[inspect(ret = T, with = expr)]`: project the field through `expr`.
    /// Boxed so the enum's variants stay size-balanced (`Type` + `Expr` are
    /// large `syn` nodes; `Clone` is a unit).
    Project(Box<Projection>),
}

/// The `ret =` / `with =` payload of projection mode.
struct Projection {
    /// Verbatim return type of the generated accessor.
    ret: Type,
    /// A `Fn(&FieldType) -> ret` projector — a named path or inline closure.
    with: Expr,
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

    // Bare `#[inspect]` (no parentheses) is clone mode. `syn::Meta::Path`
    // matches `#[inspect]` exactly.
    if matches!(attr.meta, syn::Meta::Path(_)) {
        return Ok(Some(InspectArgs {
            name: None,
            mode: InspectMode::Clone,
        }));
    }

    let mut ret: Option<Type> = None;
    let mut with: Option<Expr> = None;
    let mut name: Option<String> = None;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("ret") {
            ret = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("with") {
            with = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("name") {
            let lit: syn::LitStr = meta.value()?.parse()?;
            name = Some(lit.value());
            Ok(())
        } else if meta.path.is_ident("snapshot") {
            Err(meta.error(
                "`snapshot = T` was removed in 0.3.0; use \
                 `#[inspect(ret = T, with = <projector>)]` where \
                 `<projector>: Fn(&FieldType) -> T`",
            ))
        } else {
            Err(meta.error(
                "unsupported `inspect` argument. Use bare `#[inspect]` to clone \
                 the field, or `#[inspect(ret = T, with = <projector>)]` to \
                 project it. The positional projection type `#[inspect(T)]` and \
                 `snapshot = T` were removed in 0.3.0.",
            ))
        }
    })?;

    let mode = match (ret, with) {
        (None, None) => InspectMode::Clone,
        (Some(ret), Some(with)) => InspectMode::Project(Box::new(Projection { ret, with })),
        (Some(_), None) => {
            return Err(syn::Error::new_spanned(
                attr,
                "projection mode requires `with = <projector>` alongside `ret = T`",
            ));
        }
        (None, Some(_)) => {
            return Err(syn::Error::new_spanned(
                attr,
                "projection mode requires `ret = T` alongside `with = <projector>`",
            ));
        }
    };

    Ok(Some(InspectArgs { name, mode }))
}

#[cfg(test)]
mod tests {
    use super::derive_impl;
    use quote::quote;

    /// A projection whose `ret` is a complex nested type trips
    /// `clippy::type_complexity` on the *generated* accessor — a spot the
    /// consumer cannot reach with an `#[allow(...)]`. A macro owns its
    /// codegen's lint-cleanliness, so the derive must emit the allow itself.
    /// Regression guard for aretta-bench finding C5.
    #[test]
    fn generated_impl_carries_type_complexity_allow() {
        let input = quote! {
            struct Db {
                #[inspect(
                    ret = Vec<Vec<(Vec<u8>, Vec<u8>, u32)>>,
                    with = |s| s.clone()
                )]
                ssts: Vec<Vec<(Vec<u8>, Vec<u8>, u32)>>,
            }
        };
        let out = derive_impl(input).to_string();
        assert!(
            out.contains("allow") && out.contains("type_complexity"),
            "generated impl must carry #[allow(clippy::type_complexity)]; got:\n{out}"
        );
    }
}
