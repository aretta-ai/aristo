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
    /// Match by file path, optionally restricted to a closed line
    /// range. Syntax: `file=<path>` or `file=<path>:<LO>-<HI>`.
    File {
        path: String,
        line_range: Option<(u32, u32)>,
    },
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
    /// `file=<path>:<LO>-<HI>` parse failed (LO/HI not integers, or
    /// LO > HI, or syntax doesn't match).
    BadLineRange { input: String, detail: String },
}

impl std::fmt::Display for FilterParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterParseError::MissingEquals { input } => write!(
                f,
                "filter `{input}` is missing `=`; expected one of: \
                 id=<id>, file=<path>[:<LO>-<HI>], parent=<id>, status=<state>"
            ),
            FilterParseError::UnknownKey { key } => write!(
                f,
                "unknown filter key `{key}`; expected one of: id, file, parent, status"
            ),
            FilterParseError::EmptyValue { key } => {
                write!(f, "filter `{key}=` has no value")
            }
            FilterParseError::BadLineRange { input, detail } => write!(
                f,
                "filter `{input}` has a bad line range: {detail} \
                 (expected `file=<path>:<LO>-<HI>` where LO and HI are positive \
                 integers and LO ≤ HI)"
            ),
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
            "file" => parse_file_value(s, value),
            "parent" => Ok(Filter::Parent(value.to_string())),
            "status" => Ok(Filter::Status(value.to_string())),
            other => Err(FilterParseError::UnknownKey {
                key: other.to_string(),
            }),
        }
    }
}

/// Split `<path>[:<LO>-<HI>]` into a `Filter::File`. The `:` separator
/// for the range is the LAST `:` in the string so paths containing `:`
/// (Windows drive letters, future namespacing) are accepted as-is when
/// no range follows.
fn parse_file_value(full_input: &str, value: &str) -> Result<Filter, FilterParseError> {
    // Detect range presence via a trailing `:N-M` suffix on the value.
    if let Some((path, range_str)) = split_off_trailing_range(value) {
        let (lo, hi) =
            parse_line_range(range_str).map_err(|detail| FilterParseError::BadLineRange {
                input: full_input.to_string(),
                detail,
            })?;
        Ok(Filter::File {
            path: path.to_string(),
            line_range: Some((lo, hi)),
        })
    } else {
        Ok(Filter::File {
            path: value.to_string(),
            line_range: None,
        })
    }
}

/// Returns Some((path, "LO-HI")) iff value ends with `:<digits>-<digits>`.
/// Returns None otherwise (treat as path-only — including paths that
/// contain `:` not followed by a digit-range).
fn split_off_trailing_range(value: &str) -> Option<(&str, &str)> {
    let colon = value.rfind(':')?;
    let candidate = &value[colon + 1..];
    // Range suffix must contain a `-` separating two digit sequences.
    let dash = candidate.find('-')?;
    let (lo_str, hi_str) = (&candidate[..dash], &candidate[dash + 1..]);
    if lo_str.is_empty() || hi_str.is_empty() {
        return None;
    }
    if !lo_str.bytes().all(|b| b.is_ascii_digit()) || !hi_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((&value[..colon], candidate))
}

fn parse_line_range(range_str: &str) -> Result<(u32, u32), String> {
    let (lo_str, hi_str) = range_str
        .split_once('-')
        .ok_or_else(|| format!("range `{range_str}` is missing `-` separator"))?;
    let lo: u32 = lo_str
        .parse()
        .map_err(|e| format!("LO `{lo_str}` is not a u32: {e}"))?;
    let hi: u32 = hi_str
        .parse()
        .map_err(|e| format!("HI `{hi_str}` is not a u32: {e}"))?;
    if lo == 0 || hi == 0 {
        return Err("LO and HI must be ≥ 1 (line numbers are 1-indexed)".into());
    }
    if lo > hi {
        return Err(format!("LO ({lo}) is greater than HI ({hi})"));
    }
    Ok((lo, hi))
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
            Filter::File {
                path: "src/lib.rs".into(),
                line_range: None,
            }
        );
    }

    #[test]
    fn parses_file_with_line_range() {
        assert_eq!(
            "file=src/lib.rs:10-50".parse::<Filter>().unwrap(),
            Filter::File {
                path: "src/lib.rs".into(),
                line_range: Some((10, 50)),
            }
        );
    }

    #[test]
    fn parses_file_with_single_line_range() {
        // LO == HI is a one-line range; useful for `file=x.rs:42-42`.
        assert_eq!(
            "file=src/lib.rs:42-42".parse::<Filter>().unwrap(),
            Filter::File {
                path: "src/lib.rs".into(),
                line_range: Some((42, 42)),
            }
        );
    }

    #[test]
    fn file_with_colon_but_no_range_treated_as_path() {
        // Path containing `:` not followed by a digit-range is taken
        // verbatim (rare on Unix, but Windows drive letters & future
        // namespacing shouldn't be eaten by the range parser).
        assert_eq!(
            "file=C:Users/foo.rs".parse::<Filter>().unwrap(),
            Filter::File {
                path: "C:Users/foo.rs".into(),
                line_range: None,
            }
        );
    }

    #[test]
    fn file_range_zero_rejected() {
        let err = "file=src/x.rs:0-10".parse::<Filter>().unwrap_err();
        assert!(matches!(err, FilterParseError::BadLineRange { .. }));
        assert!(err.to_string().contains("≥ 1"));
    }

    #[test]
    fn file_range_inverted_rejected() {
        let err = "file=src/x.rs:50-10".parse::<Filter>().unwrap_err();
        assert!(matches!(err, FilterParseError::BadLineRange { .. }));
        assert!(err.to_string().contains("greater than"));
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
