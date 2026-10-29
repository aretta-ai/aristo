//! Shared meaning of annotation fields after source-language decoding.

use crate::index::{VerifyLevel, VerifyMethod};

/// An annotation's decoded `verify` argument, before default resolution.
///
/// Frontends decode native literals; this type contains no source tokens.
/// Language-specific compatibility spellings belong in those frontends.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VerifyArgument {
    Omitted,
    Bool(bool),
    Text(String),
}

/// A decoded string that does not name a supported verification method.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown verification method `{0}`")]
pub struct InvalidVerifyMethod(String);

impl TryFrom<VerifyArgument> for VerifyLevel {
    type Error = InvalidVerifyMethod;

    /// Convert decoded values into the index representation. Omission retains
    /// the project-default marker; unknown strings return their decoded value
    /// in the error. Annotation-kind validation belongs to the caller.
    #[aristo::intent(
        "An omitted verification argument retains the project-default marker. \
         An unrecognized string must never fall back to that default.",
        verify = "test",
        id = "verify_argument_invalid_never_defaults"
    )]
    fn try_from(argument: VerifyArgument) -> Result<Self, Self::Error> {
        match argument {
            VerifyArgument::Omitted => Ok(Self::Bool(true)),
            VerifyArgument::Bool(value) => Ok(Self::Bool(value)),
            VerifyArgument::Text(value) => {
                serde_json::from_value::<VerifyMethod>(serde_json::Value::String(value.clone()))
                    .map(Self::Method)
                    .map_err(|_| InvalidVerifyMethod(value))
            }
        }
    }
}
