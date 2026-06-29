//! Aristo proc-macros.
//!
//! Intentionally thin: this crate runs during downstream compile time, so
//! heavy work (project-wide cycle detection, B5b signature validation,
//! index IO) lives in `aristo-cli`. The macros here do single-annotation
//! validation (when the `aristo_check` cargo feature is on — default) and
//! `include_str!` injection (`aristo_doc`, slice 30).
//!
//! The macros parse their arguments into `IntentArgs` / `AssumeArgs`, run
//! validation per `validate.rs`, and (on success) emit the wrapped item
//! unchanged — they have no runtime effect, only compile-time signal. The
//! argument shape mirrors the subset of `aristo_core::index::IntentEntry` /
//! `AssumeEntry` that the developer writes by hand (text, verify, parent,
//! id) — `aristo stamp` populates the rest from source position.

mod inject;
#[cfg(feature = "aristo_instrument")]
mod instrument;
mod validate;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, LitStr, Token};

/// Parsed `#[aristo::intent("text", verify = ..., parent = ..., id = ...)]`.
///
/// Parsing is always-on; validation is gated by the `aristo_check` cargo
/// feature (see `validate.rs`).
#[derive(Default)]
pub(crate) struct IntentArgs {
    pub(crate) text: Option<LitStr>,
    pub(crate) verify: Option<Expr>,
    #[allow(dead_code)] // parent shape validation lands with slice 32
    pub(crate) parent: Option<Expr>,
    pub(crate) id: Option<LitStr>,
}

impl Parse for IntentArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = IntentArgs::default();
        if input.is_empty() {
            return Ok(args);
        }

        // First argument is positional: a string literal carrying the
        // annotation text. (Mockup 01 form; required at validation time.)
        args.text = Some(input.parse::<LitStr>()?);

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break; // trailing comma
            }
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "verify" => args.verify = Some(input.parse()?),
                "parent" => args.parent = Some(input.parse()?),
                "id" => args.id = Some(input.parse()?),
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown `intent` argument `{other}`; expected one of: verify, parent, id"
                        ),
                    ));
                }
            }
        }
        Ok(args)
    }
}

/// `#[aristo::intent("...", verify = ..., parent = ..., id = ...)]`
///
/// Item-level annotation describing what a function / module / struct /
/// impl / trait does. Pass-through during slice 6 — emits the wrapped item
/// unchanged.
#[proc_macro_attribute]
pub fn intent(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match syn::parse::<IntentArgs>(attr) {
        Ok(a) => a,
        Err(err) => return err.to_compile_error().into(),
    };
    if let Err(err) = validate::validate_intent(&args) {
        return err.to_compile_error().into();
    }
    match inject::doc_attribute_or_error(args.id.as_ref()) {
        Ok(prefix) => {
            let item_ts = proc_macro2::TokenStream::from(item);
            quote!(#prefix #item_ts).into()
        }
        Err(err) => err.to_compile_error().into(),
    }
}

/// `aristo::intent_stmt!("...", verify = ..., parent = ..., id = ...);`
///
/// Sub-item annotation: used inside a function body to attach intent to a
/// statement, block, or loop that the attribute form can't reach. Per
/// mockup 01 the parameter shape is identical to the attribute form;
/// expansion is empty (compile-time annotation only — no runtime trace).
///
/// Naming note: Rust requires distinct fn names for attribute and function-
/// like proc-macros within a single crate (E0428). Convention in the
/// ecosystem (tokio: `#[tokio::main]` + `tokio::select!`; tracing:
/// `#[tracing::instrument]` + `tracing::trace!`) is to use different names
/// per kind. We follow that with the `_stmt` suffix to make the statement-
/// position context explicit at the call site.
#[proc_macro]
pub fn intent_stmt(input: TokenStream) -> TokenStream {
    match syn::parse::<IntentArgs>(input).and_then(|args| validate::validate_intent(&args)) {
        Ok(()) => TokenStream::new(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Parsed `#[aristo::assume("text", parent = ..., id = ...)]`.
///
/// `assume` is `intent` minus `verify` per A5 — assumptions describe
/// invariants the code RELIES ON (OS guarantees, library contracts,
/// upstream invariants); they aren't verification targets, so passing
/// `verify` is a category error caught at parse time with a friendly
/// message (the user is probably reaching for `intent`).
#[derive(Default)]
pub(crate) struct AssumeArgs {
    pub(crate) text: Option<LitStr>,
    #[allow(dead_code)] // parent shape validation lands with slice 32
    pub(crate) parent: Option<Expr>,
    pub(crate) id: Option<LitStr>,
}

impl Parse for AssumeArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = AssumeArgs::default();
        if input.is_empty() {
            return Ok(args);
        }
        args.text = Some(input.parse::<LitStr>()?);
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "parent" => args.parent = Some(input.parse()?),
                "id" => args.id = Some(input.parse()?),
                "verify" => {
                    return Err(syn::Error::new(
                        key.span(),
                        "`verify` is not allowed on `assume` (A5): assumptions describe \
                         invariants you rely on, not properties to be verified. \
                         Use `intent` if you meant a verifiable claim.",
                    ));
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown `assume` argument `{other}`; expected one of: parent, id"),
                    ));
                }
            }
        }
        Ok(args)
    }
}

/// `#[aristo::assume("...", parent = ..., id = ...)]`
///
/// Item-level assumption: state an invariant the code relies on but does
/// not itself enforce (an OS guarantee, a library contract, an upstream
/// caller's promise). No `verify` argument — see `AssumeArgs` doc above.
/// Pass-through during slice 6.
#[proc_macro_attribute]
pub fn assume(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match syn::parse::<AssumeArgs>(attr) {
        Ok(a) => a,
        Err(err) => return err.to_compile_error().into(),
    };
    if let Err(err) = validate::validate_assume(&args) {
        return err.to_compile_error().into();
    }
    match inject::doc_attribute_or_error(args.id.as_ref()) {
        Ok(prefix) => {
            let item_ts = proc_macro2::TokenStream::from(item);
            quote!(#prefix #item_ts).into()
        }
        Err(err) => err.to_compile_error().into(),
    }
}

/// `aristo::assume_stmt!("...", parent = ..., id = ...);`
///
/// Sub-item assumption: used inside a function body to attach an
/// assumption to a statement, block, or loop. Same shape as the attribute
/// form (no `verify` per A5); empty expansion. Naming follows the
/// `_stmt` convention from `intent_stmt!`.
#[proc_macro]
pub fn assume_stmt(input: TokenStream) -> TokenStream {
    match syn::parse::<AssumeArgs>(input).and_then(|args| validate::validate_assume(&args)) {
        Ok(()) => TokenStream::new(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// `#[derive(aristo::instrument::Inspect)]`
///
/// Derive macro emitting an `inspect_<field>()` snapshot accessor per
/// `#[inspect]`-tagged field. Type-agnostic — it never inspects the field
/// type. Bare `#[inspect]` clones the field (any `Clone` type); `#[inspect(ret
/// = T, with = <projector>)]` projects it through any `Fn(&FieldType) -> T`
/// (a named path or inline closure). `name = "..."` overrides the suffix.
/// See `instrument::inspect` for the full contract.
#[cfg(feature = "aristo_instrument")]
#[proc_macro_derive(Inspect, attributes(inspect))]
pub fn instrument_inspect(input: TokenStream) -> TokenStream {
    instrument::inspect::derive(input)
}

/// `#[aristo::instrument::expose_pub(as = "...")]`
///
/// Attribute macro that emits a feature-gated `pub` wrapper around a
/// `pub(crate)` function (slice 38) or a sibling `pub` twin of a
/// `pub(crate)` type or `impl` block (slice 39). Slice 36 is a
/// pass-through stub.
#[cfg(feature = "aristo_instrument")]
#[proc_macro_attribute]
pub fn expose_pub(attr: TokenStream, item: TokenStream) -> TokenStream {
    instrument::expose_pub::attribute(attr, item)
}

/// `aristo::instrument::yield_point!("label")`
///
/// Function-like macro that emits a call into the runtime hook
/// `aristo::instrument::__yield_point` when the `aristo_instrument`
/// feature is on; expands to nothing otherwise. Slice 36 is a stub
/// (empty expansion either way); slice 40 wires the runtime call.
#[cfg(feature = "aristo_instrument")]
#[proc_macro]
pub fn yield_point(input: TokenStream) -> TokenStream {
    instrument::yield_point::function_like(input)
}

/// `aristo::instrument::fault_point!("label")`
///
/// Function-like macro that emits a call into the fault-decision hook
/// `aristo::instrument::__fault_point`, returning the harness's `Decision`
/// for the SUT to branch on. Observe-only callers want `yield_point!`; this
/// is for injecting interior faults. Gated on `aristo_instrument`.
#[cfg(feature = "aristo_instrument")]
#[proc_macro]
pub fn fault_point(input: TokenStream) -> TokenStream {
    instrument::fault_point::function_like(input)
}
