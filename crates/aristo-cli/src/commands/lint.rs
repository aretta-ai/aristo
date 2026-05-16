//! `aristo lint --check [--strict]` — static text-quality pass over
//! every annotation in the index.
//!
//! Pure text analysis per mockup 07 D3. No LLM, no I/O beyond reading
//! the index. Rule catalog is a closed set in [`rules`]; per-project
//! severity overrides come from `aristo.toml [lint.rules.<name>]`.
//!
//! `--check` exits non-zero iff at least one finding is `error`
//! severity (or, with `--strict`, `warn` severity).
//!
//! `--fix` walks source files and applies mechanical whitespace fixes
//! in place (`doubled_spaces`, `trailing_whitespace`). Never applies
//! semantic rewrites.

use std::cmp::Ordering;

use aristo_core::config::ConfigFile;
use aristo_core::index::{AnnotationId, IndexEntry};

use crate::commands::index::workspace_or_error;
use crate::commands::show::read_index;
use crate::preflight::{emit_advisory_if_stale, freshness_check};
use crate::{CliError, CliResult};

mod fix;
mod rules;

use rules::Severity;

pub(crate) fn run(check: bool, fix: bool, strict: bool) -> CliResult<()> {
    if fix {
        let ws = workspace_or_error()?;
        let (issues, files) = self::fix::run_fix(&ws)?;
        println!(
            "fixed: {issues} whitespace {issue_word} across {files} {file_word}",
            issue_word = if issues == 1 { "issue" } else { "issues" },
            file_word = if files == 1 { "file" } else { "files" },
        );
        return Ok(());
    }
    if !check {
        // Default invocation (no flags) acts like --check for slice 20.
        // J6's pre_commit dispatch may grow other defaults later.
    }
    let ws = workspace_or_error()?;
    emit_advisory_if_stale(&freshness_check(&ws));
    let index = read_index(&ws.index_path())?;
    let overrides = load_rule_overrides(&ws);

    let mut findings: Vec<Finding> = Vec::new();
    for (id, entry) in &index.entries {
        let text = entry_text(entry);
        let file = entry_file(entry).to_string();
        for outcome in rules::run_check_rules(text, &overrides) {
            findings.push(Finding {
                id: id.clone(),
                file: file.clone(),
                rule: outcome.rule_name,
                severity: outcome.severity,
                message: outcome.message,
            });
        }
    }

    findings.sort_by(|a, b| match a.id.cmp(&b.id) {
        Ordering::Equal => a.rule.cmp(b.rule),
        other => other,
    });

    report(&findings, strict)
}

fn report(findings: &[Finding], strict: bool) -> CliResult<()> {
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warns = findings
        .iter()
        .filter(|f| f.severity == Severity::Warn)
        .count();
    let infos = findings
        .iter()
        .filter(|f| f.severity == Severity::Info)
        .count();

    let fail = errors > 0 || (strict && warns > 0);

    if findings.is_empty() {
        println!("ok: 0 lint findings.");
        return Ok(());
    }

    if fail {
        let triggering = if errors > 0 {
            format!("{errors} lint finding (error severity)")
        } else {
            format!("{warns} lint finding (warn severity, --strict)")
        };
        // The user-visible bullet list — matches mockup 07's J6 shape.
        println!("error: {triggering}");
        for f in findings {
            if f.severity == Severity::Error || (strict && f.severity == Severity::Warn) {
                let line = "[..]";
                println!(
                    "  \u{2022} {id} ({file}:{line}): {rule} ({msg})",
                    id = f.id,
                    file = f.file,
                    rule = f.rule,
                    msg = f.message,
                );
            }
        }
        let _ = (errors, warns, infos);
        return Err(CliError::Silent { exit_code: 1 });
    }

    println!(
        "ok: {} finding{} ({errors} error, {warns} warn, {infos} info).",
        findings.len(),
        if findings.len() == 1 { "" } else { "s" }
    );
    Ok(())
}

struct Finding {
    id: AnnotationId,
    file: String,
    rule: &'static str,
    severity: Severity,
    message: String,
}

fn entry_text(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.text,
        IndexEntry::Assume(e) => &e.text,
    }
}

fn entry_file(entry: &IndexEntry) -> &str {
    match entry {
        IndexEntry::Intent(e) => &e.file,
        IndexEntry::Assume(e) => &e.file,
    }
}

fn load_rule_overrides(ws: &crate::Workspace) -> rules::Overrides {
    let path = ws.config_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return rules::Overrides::default();
    };
    let cfg: Result<ConfigFile, _> = toml::from_str(&text);
    let Ok(cfg) = cfg else {
        return rules::Overrides::default();
    };
    rules::Overrides::from_config(&cfg)
}
