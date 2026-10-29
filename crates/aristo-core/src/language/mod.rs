//! Source language selection and shared annotation-field semantics.

mod fields;
mod registry;

pub use fields::VerifyArgument;
pub use registry::{built_in_languages, language_for_path, LanguageId, SourceLanguage};
