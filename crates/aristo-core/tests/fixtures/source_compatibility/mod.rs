//! Fixed Rust/C reference outputs for the source-provider refactor.
//!
//! Known issues pinned deliberately, not intended semantics:
//! Q1: Rust statement hashes cover only the marker, not the following code.
//!     https://github.com/aretta-ai/aristo/issues/108
//! Q2: Rust reconstructs LF bodies; C retains original CRLF bytes.
//!     https://github.com/aretta-ai/aristo/issues/109
//! Q3: Global collision ordinals let edits in a.rs rename unchanged b.c claims.
//!     https://github.com/aretta-ai/aristo/issues/110
//! Fixes must migrate identity/evidence and update these references explicitly.

use serde_json::Value;
use std::fs;

const EXPECTED: &str = include_str!("expected.json");

pub fn workspace(crlf: bool) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("aristo.toml"),
        "[doc]\ncommit_artifacts = false\n",
    )
    .unwrap();
    for (name, source) in [
        ("a.rs", include_str!("a.rs")),
        ("b.c", include_str!("b.c")),
        ("c.rs", include_str!("c.rs")),
        ("d.h", include_str!("d.h")),
    ] {
        // Construct both byte profiles independently of Git's checkout settings.
        let source = source.replace("\r\n", "\n");
        fs::write(
            tmp.path().join(name),
            if crlf {
                source.replace('\n', "\r\n")
            } else {
                source
            },
        )
        .unwrap();
    }
    tmp
}

pub fn expected(crlf: bool) -> Vec<Value> {
    let mut rows: Vec<Value> = serde_json::from_str(EXPECTED).unwrap();
    for row in &mut rows {
        if let Some(hash) = row.as_object_mut().unwrap().remove("crlf_body_hash") {
            if crlf {
                row["body_hash"] = hash;
            }
        }
    }
    rows
}
