//! Aristo proc-macros.
//!
//! Intentionally thin: this crate runs during downstream compile time, so
//! heavy work (project-wide cycle detection, B5b signature validation,
//! index IO) lives in `aristo-cli`. The macros here only do single-
//! annotation validation (when the `aristo_check` cargo feature is on,
//! landing in slice 8) and `include_str!` injection (`aristo_doc`, slice 30).
//!
//! Slice 6: pass-through expansion. The macros parse their arguments and
//! emit the wrapped item unchanged. The argument shape mirrors the subset
//! of `aristo_core::index::IntentEntry` / `AssumeEntry` that the developer
//! writes by hand (text, verify, parent, id) — `aristo stamp` populates the
//! rest from source position.

use proc_macro::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, LitStr, Token};

/// Parsed `#[aristo::intent("text", verify = ..., parent = ..., id = ...)]`.
///
/// Slice 6 parses but does not validate. Slice 8 (the `aristo_check` cargo
/// feature) reuses this same parser and adds value validation.
#[derive(Default)]
struct IntentArgs {
    text: Option<LitStr>,
    verify: Option<Expr>,
    parent: Option<Expr>,
    id: Option<LitStr>,
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
    match syn::parse::<IntentArgs>(attr) {
        Ok(_args) => item,
        Err(err) => err.to_compile_error().into(),
    }
}
