//! Shared meaning of annotation fields after source-language decoding.

use crate::index::{VerifyLevel, VerifyMethod};

/// An annotation's decoded `verify` argument, before default resolution.
///
/// Frontends decode native literals; this type contains no source tokens.
/// Language-specific compatibility spellings belong in those frontends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyArgument {
    Omitted,
    Bool(bool),
    String(String),
}

impl VerifyArgument {
    /// Resolve an intent's argument into the existing index representation.
    ///
    /// `None` means an invalid method name; callers retain the original source
    /// spelling and location for diagnostics. Check annotation-kind rules
    /// before calling this: omission becomes the project-default marker.
    #[aristo::intent(
        "An omitted verification argument retains the project-default marker. \
         An unrecognized string must never fall back to that default.",
        verify = "test",
        id = "verify_argument_invalid_never_defaults"
    )]
    pub fn resolve(self) -> Option<VerifyLevel> {
        match self {
            Self::Omitted => Some(VerifyLevel::Bool(true)),
            Self::Bool(value) => Some(VerifyLevel::Bool(value)),
            Self::String(value) => match value.as_str() {
                "test" => Some(VerifyLevel::Method(VerifyMethod::Test)),
                "neural" => Some(VerifyLevel::Method(VerifyMethod::Neural)),
                "full" => Some(VerifyLevel::Method(VerifyMethod::Full)),
                _ => None,
            },
        }
    }
}
