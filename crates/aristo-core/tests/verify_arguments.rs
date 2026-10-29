use aristo_core::index::{VerifyLevel, VerifyMethod};
use aristo_core::language::VerifyArgument;

#[test]
fn decoded_verify_values_resolve_without_language_syntax() {
    for (argument, expected) in [
        (VerifyArgument::Omitted, VerifyLevel::Bool(true)),
        (VerifyArgument::Bool(true), VerifyLevel::Bool(true)),
        (VerifyArgument::Bool(false), VerifyLevel::Bool(false)),
        (
            VerifyArgument::String("test".into()),
            VerifyLevel::Method(VerifyMethod::Test),
        ),
        (
            VerifyArgument::String("neural".into()),
            VerifyLevel::Method(VerifyMethod::Neural),
        ),
        (
            VerifyArgument::String("full".into()),
            VerifyLevel::Method(VerifyMethod::Full),
        ),
    ] {
        assert_eq!(argument.resolve(), Some(expected));
    }
}

#[test]
fn invalid_decoded_values_never_fall_back_to_the_default() {
    for value in [
        "", "true", "false", "True", "False", "Test", " test ", "\"test\"", "unknown",
    ] {
        assert_eq!(
            VerifyArgument::String(value.into()).resolve(),
            None,
            "{value:?}"
        );
    }
}
