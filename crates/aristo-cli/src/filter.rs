//! J2 unified filter grammar shared across `aristo list` / `verify` /
//! `graph` / `critique`.
//!
//! Form: `<key>=<value>`. Allowed keys: `id`, `file`, `parent`, `status`.
//! Multiple `--filter` flags AND together at the call site (not modeled
//! here — this type represents a single filter clause).

use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    Id(String),
    File(String),
    Parent(String),
    Status(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum FilterParseError {
    /// No `=` separator in the filter expression.
    MissingEquals { input: String },
    /// Unknown left-hand side (typo or unsupported key).
    UnknownKey { key: String },
    /// Right-hand side is empty (e.g. `id=`).
    EmptyValue { key: String },
}

impl std::fmt::Display for FilterParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterParseError::MissingEquals { input } => write!(
                f,
                "filter `{input}` is missing `=`; expected one of: \
                 id=<id>, file=<path>, parent=<id>, status=<state>"
            ),
            FilterParseError::UnknownKey { key } => write!(
                f,
                "unknown filter key `{key}`; expected one of: id, file, parent, status"
            ),
            FilterParseError::EmptyValue { key } => {
                write!(f, "filter `{key}=` has no value")
            }
        }
    }
}

impl std::error::Error for FilterParseError {}

impl FromStr for Filter {
    type Err = FilterParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (key, value) = s
            .split_once('=')
            .ok_or_else(|| FilterParseError::MissingEquals {
                input: s.to_string(),
            })?;
        if value.is_empty() {
            return Err(FilterParseError::EmptyValue {
                key: key.to_string(),
            });
        }
        match key {
            "id" => Ok(Filter::Id(value.to_string())),
            "file" => Ok(Filter::File(value.to_string())),
            "parent" => Ok(Filter::Parent(value.to_string())),
            "status" => Ok(Filter::Status(value.to_string())),
            other => Err(FilterParseError::UnknownKey {
                key: other.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_id() {
        assert_eq!(
            "id=foo".parse::<Filter>().unwrap(),
            Filter::Id("foo".into())
        );
    }

    #[test]
    fn parses_file_with_slashes() {
        assert_eq!(
            "file=src/lib.rs".parse::<Filter>().unwrap(),
            Filter::File("src/lib.rs".into())
        );
    }

    #[test]
    fn parses_parent() {
        assert_eq!(
            "parent=root_invariants".parse::<Filter>().unwrap(),
            Filter::Parent("root_invariants".into())
        );
    }

    #[test]
    fn parses_status() {
        assert_eq!(
            "status=verified".parse::<Filter>().unwrap(),
            Filter::Status("verified".into())
        );
    }

    #[test]
    fn value_may_contain_equals_sign() {
        // split_once('=') is greedy on the first `=`, so values with an `=`
        // inside (rare for ids/paths but possible) survive.
        assert_eq!(
            "id=foo=bar".parse::<Filter>().unwrap(),
            Filter::Id("foo=bar".into())
        );
    }

    #[test]
    fn aristos_namespaced_id_parses() {
        // `aristos:` prefix contains a colon, not an equals — must round-trip.
        assert_eq!(
            "id=aristos:my_thing".parse::<Filter>().unwrap(),
            Filter::Id("aristos:my_thing".into())
        );
    }

    #[test]
    fn missing_equals_rejected() {
        let err = "id".parse::<Filter>().unwrap_err();
        assert!(matches!(err, FilterParseError::MissingEquals { .. }));
        assert!(err.to_string().contains("missing `=`"));
    }

    #[test]
    fn unknown_key_rejected_with_helpful_message() {
        let err = "kind=intent".parse::<Filter>().unwrap_err();
        assert!(matches!(err, FilterParseError::UnknownKey { .. }));
        let msg = err.to_string();
        assert!(msg.contains("kind"));
        assert!(msg.contains("id, file, parent, status"));
    }

    #[test]
    fn empty_value_rejected() {
        let err = "id=".parse::<Filter>().unwrap_err();
        assert!(matches!(err, FilterParseError::EmptyValue { .. }));
    }
}
