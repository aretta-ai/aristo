//! Built-in source language selection shared by discovery and CLI detection.

mod registry;

pub use registry::{built_in_languages, language_for_path, LanguageId, SourceLanguage};
