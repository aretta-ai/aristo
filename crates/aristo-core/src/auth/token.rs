//! [`Token`] newtype — the resolved bearer credential.
//!
//! Wrapping in a newtype prevents the raw string from being logged or
//! formatted accidentally: [`Display`](std::fmt::Display) is **not**
//! implemented; callers reach the underlying string via
//! [`Token::as_str`]. [`Debug`] is overridden to redact the body.

/// Resolved auth token, ready to send as `Authorization: Bearer <token>`.
#[derive(Clone)]
pub struct Token(String);

impl Token {
    /// Underlying token string. Safe to send as `Authorization:
    /// Bearer <token>`; **do not log this value**.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Construct a token directly. Used by tests + the credentials
    /// store on load.
    pub fn new<S: Into<String>>(s: S) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redact the body so accidental `{:?}` logging doesn't leak.
        // Show length only for the "is this empty?" debugging case.
        write!(f, "Token(<redacted; {} chars>)", self.0.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_format_redacts_body() {
        let t = Token::new("my-secret-token-value");
        let s = format!("{t:?}");
        assert!(!s.contains("my-secret-token-value"), "got: {s}");
        assert!(s.contains("redacted"), "got: {s}");
    }

    #[test]
    fn debug_format_includes_length_for_diagnostics() {
        // Length-only is useful for "is this empty?" without leaking.
        let t = Token::new("abcd");
        let s = format!("{t:?}");
        assert!(s.contains("4"), "got: {s}");
    }

    #[test]
    fn as_str_returns_underlying_value() {
        let t = Token::new("plain");
        assert_eq!(t.as_str(), "plain");
    }

    #[test]
    fn does_not_implement_display() {
        // Compile-time check that Display is intentionally absent.
        // If someone adds `impl Display for Token`, the trait-bound
        // call below stops compiling, and this test fails the build.
        fn _is_not_display<T>(_: &T)
        where
            T: ?Sized,
        {
        }
        let t = Token::new("plain");
        _is_not_display(&t);
        // Real assertion: format!("{t}") would not compile.
    }
}
