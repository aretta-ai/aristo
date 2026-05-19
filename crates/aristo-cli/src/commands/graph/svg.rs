//! SVG emitter — shells out to Graphviz `dot -Tsvg` against the DOT
//! representation produced by `super::dot`.
//!
//! Aristo doesn't bundle Graphviz; if `dot` isn't on PATH we emit a
//! friendly error with platform-specific install hints AND the two
//! alternative formats that don't require Graphviz (DOT and Mermaid).

use std::io::Write;
use std::process::{Command, Stdio};

use super::model::Graph;
use crate::{CliError, CliResult};

pub(crate) fn render(g: &Graph) -> CliResult<String> {
    let dot_source = super::dot::render(g);
    let mut child = match Command::new("dot")
        .arg("-Tsvg")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(CliError::Other {
                message: dot_not_found_message(),
                exit_code: 2,
            });
        }
        Err(e) => {
            return Err(CliError::Other {
                message: format!("failed to spawn `dot`: {e}"),
                exit_code: 1,
            });
        }
    };

    {
        let stdin = child.stdin.as_mut().expect("piped stdin captured above");
        stdin
            .write_all(dot_source.as_bytes())
            .map_err(|e| CliError::Other {
                message: format!("writing DOT source to dot subprocess stdin: {e}"),
                exit_code: 1,
            })?;
    }

    let output = child.wait_with_output().map_err(|e| CliError::Other {
        message: format!("waiting on dot subprocess: {e}"),
        exit_code: 1,
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CliError::Other {
            message: format!(
                "`dot -Tsvg` exited with status {}. stderr: {stderr}",
                output.status
            ),
            exit_code: 1,
        });
    }

    String::from_utf8(output.stdout).map_err(|e| CliError::Other {
        message: format!("dot produced non-UTF8 SVG (highly unusual): {e}"),
        exit_code: 1,
    })
}

fn dot_not_found_message() -> String {
    "SVG output requires Graphviz `dot`, which was not found on PATH.\n\
     \n\
     Install:\n  \
       • macOS:    brew install graphviz\n  \
       • Debian:   apt install graphviz\n  \
       • Windows:  https://graphviz.org/download/\n\
     \n\
     Alternatives:\n  \
       • aristo graph --format=dot > annotations.dot\n    \
         (then render with any Graphviz tool)\n  \
       • aristo graph --format=mermaid > annotations.mmd\n    \
         (renders in any markdown viewer that supports Mermaid)"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_dot_message_lists_three_platforms_and_two_alternatives() {
        let m = dot_not_found_message();
        // Install section: macOS / Debian / Windows.
        assert!(m.contains("brew install graphviz"));
        assert!(m.contains("apt install graphviz"));
        assert!(m.contains("https://graphviz.org/download/"));
        // Alternatives: both no-graphviz formats called out by name.
        assert!(m.contains("--format=dot"));
        assert!(m.contains("--format=mermaid"));
    }
}
