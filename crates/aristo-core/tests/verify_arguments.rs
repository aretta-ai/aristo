use aristo_core::index::{VerifyLevel, VerifyMethod};
use aristo_core::language::VerifyArgument;

#[test]
fn decoded_verify_values_resolve_without_language_syntax() {
    for (argument, expected) in [
        (VerifyArgument::Omitted, VerifyLevel::Bool(true)),
        (VerifyArgument::Bool(true), VerifyLevel::Bool(true)),
        (VerifyArgument::Bool(false), VerifyLevel::Bool(false)),
        (
            VerifyArgument::Text("test".into()),
            VerifyLevel::Method(VerifyMethod::Test),
        ),
        (
            VerifyArgument::Text("neural".into()),
            VerifyLevel::Method(VerifyMethod::Neural),
        ),
        (
            VerifyArgument::Text("full".into()),
            VerifyLevel::Method(VerifyMethod::Full),
        ),
    ] {
        assert_eq!(VerifyLevel::try_from(argument), Ok(expected));
    }
}

#[test]
fn invalid_decoded_values_never_fall_back_to_the_default() {
    for value in [
        "", "true", "false", "True", "False", "Test", " test ", "\"test\"", "unknown",
    ] {
        let error = VerifyLevel::try_from(VerifyArgument::Text(value.into())).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("unknown verification method `{value}`")
        );
    }
}

#[test]
fn method_serde_spellings_resolve_to_the_same_method() {
    for method in [VerifyMethod::Neural, VerifyMethod::Test, VerifyMethod::Full] {
        let serde_json::Value::String(value) = serde_json::to_value(method).unwrap() else {
            panic!("method must serialize as a string");
        };
        assert_eq!(
            VerifyLevel::try_from(VerifyArgument::Text(value)),
            Ok(VerifyLevel::Method(method))
        );
    }
}
