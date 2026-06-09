//! `#[expose_pub]` — emits a feature-gated `pub` wrapper around a
//! `pub(crate)` function (slice 38), or a sibling `pub` twin
//! declaration of a `pub(crate)` type / impl block (slice 39).
//!
//! ### Function form (slice 38)
//!
//! Applied to a free function, a method, or an associated function
//! with `as = "<wrapper_name>"`:
//!
//! ```ignore
//! mod inner {
//!     #[aristo::instrument::expose_pub(as = "new_for_test")]
//!     pub(crate) fn new(buf_size: usize) -> Self { /* body */ }
//! }
//! ```
//!
//! Expands to the original item **plus** a `pub` wrapper that calls
//! through. The wrapper preserves the original signature (generics,
//! lifetimes, `where` clauses, receiver kind, `Self` references) and
//! adapts the call shape:
//!
//! - **Method with receiver** (`&self` / `&mut self` / `self`): emits
//!   `self.<orig>(<args>)`.
//! - **No receiver + `Self` in signature**: emits
//!   `Self::<orig>(<args>)` (assumed impl-block context).
//! - **No receiver + no `Self`**: emits `<orig>(<args>)` (assumed free
//!   function context).
//!
//! `Self`-reference detection drives the impl-vs-free disambiguation
//! because syntactically `ItemFn` and `ImplItemFn` are identical, so
//! the macro can't see the surrounding context from tokens alone. A
//! context-free associated function inside an `impl` block (no receiver,
//! no `Self` anywhere in the signature) defaults to the free-fn shape
//! — users in that corner can hand-write the wrapper or include a
//! `Self` reference in the signature to opt in.
//!
//! `as = "..."` is **required** on functions; without it the wrapper
//! would collide with the original by name. Type / impl-block forms
//! (slice 39) follow the inverse rule — they forbid `as = "..."`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{FnArg, Ident, ItemFn, LitStr, Pat, Signature};

pub(crate) fn attribute(attr: TokenStream, item: TokenStream) -> TokenStream {
    let item_ts2 = TokenStream2::from(item.clone());

    // `ItemFn` parses any function-shaped item (free fn, method,
    // associated fn) — the syntactic shape is identical across all
    // three. Other item kinds (struct/enum/impl-block/etc.) fall
    // through to the deferred-to-slice-39 error below.
    if let Ok(item_fn) = syn::parse::<ItemFn>(item) {
        let args = match syn::parse::<FnAttrArgs>(attr) {
            Ok(a) => a,
            Err(e) => return e.to_compile_error().into(),
        };
        return expand_fn(args, item_fn).into();
    }

    syn::Error::new_spanned(
        item_ts2,
        "`#[expose_pub]` on this item kind is not yet supported \
         (type / impl-block forms land in slice 39 of docs/ROADMAP.md)",
    )
    .to_compile_error()
    .into()
}

/// Parsed `#[expose_pub(as = "...")]` for function form.
struct FnAttrArgs {
    wrapper_name: LitStr,
}

impl Parse for FnAttrArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(input.error(
                "`#[expose_pub]` on a function requires `as = \"<wrapper_name>\"` \
                 (pick a distinct name so the wrapper doesn't collide with the original)",
            ));
        }
        // `as` is a Rust keyword; `parse_any` accepts it in ident
        // position. Other idents are rejected explicitly.
        let key = Ident::parse_any(input)?;
        if key != "as" {
            return Err(syn::Error::new(
                key.span(),
                format!(
                    "unknown `expose_pub` argument `{key}`; expected `as = \"<wrapper_name>\"`"
                ),
            ));
        }
        input.parse::<syn::Token![=]>()?;
        let wrapper_name: LitStr = input.parse()?;
        if !input.is_empty() {
            return Err(input
                .error("`#[expose_pub(as = \"...\")]` takes one argument; nothing else expected"));
        }
        Ok(FnAttrArgs { wrapper_name })
    }
}

fn expand_fn(args: FnAttrArgs, item_fn: ItemFn) -> TokenStream2 {
    let orig_ident = item_fn.sig.ident.clone();
    let wrapper_ident = format_ident!("{}", args.wrapper_name.value());

    let mut wrapper_sig = item_fn.sig.clone();
    wrapper_sig.ident = wrapper_ident;

    let call_args = forward_args(&item_fn.sig);
    let call_expr = build_call_expr(&item_fn, &orig_ident, &call_args);

    quote! {
        #item_fn

        #[doc(hidden)]
        pub #wrapper_sig {
            #call_expr
        }
    }
}

/// Pick the wrapper-body call expression for the original function:
/// - Receiver present → `self.<orig>(<args>)`.
/// - No receiver, item mentions `Self` → `Self::<orig>(<args>)`
///   (associated-function-in-impl shape; `Self` is a Rust keyword that
///   only resolves in impl context, so its presence anywhere in sig or
///   body proves the context).
/// - No receiver, no `Self` → `<orig>(<args>)` (free-fn shape).
fn build_call_expr(item_fn: &ItemFn, orig: &Ident, args: &[TokenStream2]) -> TokenStream2 {
    let has_receiver = matches!(item_fn.sig.inputs.first(), Some(FnArg::Receiver(_)));
    if has_receiver {
        return quote!(self.#orig(#(#args),*));
    }
    if item_mentions_self_type(item_fn) {
        return quote!(Self::#orig(#(#args),*));
    }
    quote!(#orig(#(#args),*))
}

/// Heuristic: render the signature + body as tokens and check for the
/// `Self` keyword. `Self` is only valid inside an impl block; its
/// presence anywhere proves impl-context. Misses the corner case of an
/// associated fn that never names `Self` (e.g. `fn compute(x: u32) ->
/// u32`); those users add a `Self` annotation or hand-write the
/// wrapper. Documented in the module-level docs.
fn item_mentions_self_type(item_fn: &ItemFn) -> bool {
    let mut tokens = TokenStream2::new();
    item_fn.sig.to_tokens(&mut tokens);
    item_fn.block.to_tokens(&mut tokens);
    tokens.to_string().contains("Self")
}

/// Collect the argument identifiers from a signature, skipping the
/// receiver. Patterns that aren't a plain ident emit a span-attached
/// compile error — slice 38 supports `fn foo(a: T)` only, not
/// destructuring patterns like `fn foo((a, b): (T, U))`.
fn forward_args(sig: &Signature) -> Vec<TokenStream2> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Receiver(_) => None,
            FnArg::Typed(pat_type) => match &*pat_type.pat {
                Pat::Ident(pi) => {
                    let ident = &pi.ident;
                    Some(quote!(#ident))
                }
                _ => {
                    let span = pat_type.span();
                    Some(quote_spanned! { span =>
                        compile_error!(
                            "`#[expose_pub]` v1 supports plain identifier args (`fn foo(a: T)`); \
                             destructuring patterns are not yet supported"
                        )
                    })
                }
            },
        })
        .collect()
}
