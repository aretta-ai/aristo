//! Slice 27.7 commit 5 — `aristo critique --staged` end-to-end.
//!
//! Drives a real `git init` + add + critique cycle in a temp directory.
//! Pins two load-bearing behaviors:
//! - `--staged` alone restricts scope to files in `git diff --cached --name-only`
//! - `--staged` intersects with `--filter` (composition, not replacement)
//!
//! Intent pinned: `critique_staged_filter_intersects_with_explicit_filter`.

use assert_cmd::Command;
use std::path::Path;

fn git_with_path(repo: &Path) -> Command {
    let aristo_bin = assert_cmd::cargo::cargo_bin("aristo");
    let aristo_dir = aristo_bin.parent().unwrap();
    let new_path = match std::env::var_os("PATH") {
        Some(existing) => {
            let mut paths = vec![aristo_dir.to_path_buf()];
            paths.extend(std::env::split_paths(&existing));
            std::env::join_paths(paths).unwrap()
        }
        None => aristo_dir.as_os_str().to_owned(),
    };
    let mut cmd = Command::new("git");
    cmd.current_dir(repo).env("PATH", new_path);
    cmd
}

fn aristo_in(repo: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aristo").unwrap();
    cmd.current_dir(repo);
    cmd
}

/// Initialize a fresh git+cargo+aristo project with two annotations,
/// one in src/a.rs and one in src/b.rs. Returns the repo path.
fn make_two_file_project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();

    git_with_path(repo)
        .args(["init", "--quiet"])
        .assert()
        .success();
    git_with_path(repo)
        .args(["config", "user.email", "test@aretta.dev"])
        .assert()
        .success();
    git_with_path(repo)
        .args(["config", "user.name", "Test"])
        .assert()
        .success();
    git_with_path(repo)
        .args(["config", "commit.gpgsign", "false"])
        .assert()
        .success();

    std::fs::write(
        repo.join("Cargo.toml"),
        r#"[package]
name = "staged-test"
version = "0.0.0"
edition = "2021"
"#,
    )
    .unwrap();
    std::fs::create_dir(repo.join("src")).unwrap();
    // src/lib.rs must exist for cargo metadata; not annotated.
    std::fs::write(repo.join("src/lib.rs"), "pub mod a;\npub mod b;\n").unwrap();
    std::fs::write(
        repo.join("src/a.rs"),
        r#"#[aristo::intent("a-file annotation", verify = "neural", id = "ann_in_a")]
pub fn a() {}
"#,
    )
    .unwrap();
    std::fs::write(
        repo.join("src/b.rs"),
        r#"#[aristo::intent("b-file annotation", verify = "neural", id = "ann_in_b")]
pub fn b() {}
"#,
    )
    .unwrap();

    aristo_in(repo).arg("init").assert().success();
    aristo_in(repo).arg("stamp").assert().success();

    tmp
}

fn drain_queue(repo: &Path) {
    let pending = repo.join(".aristo/critique-queue/pending");
    if pending.is_dir() {
        for entry in std::fs::read_dir(&pending).unwrap().flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn queue_ids(repo: &Path) -> Vec<String> {
    let pending = repo.join(".aristo/critique-queue/pending");
    if !pending.is_dir() {
        return vec![];
    }
    let mut ids: Vec<String> = std::fs::read_dir(&pending)
        .unwrap()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension()? != "toml" {
                return None;
            }
            path.file_stem()?.to_str().map(|s| s.to_string())
        })
        .collect();
    ids.sort();
    ids
}

#[test]
fn staged_alone_enqueues_only_annotations_in_staged_files() {
    let tmp = make_two_file_project();
    let repo = tmp.path();

    // Stage src/a.rs only. src/b.rs is unstaged.
    git_with_path(repo)
        .args(["add", "src/a.rs"])
        .assert()
        .success();

    aristo_in(repo)
        .args(["critique", "--staged"])
        .assert()
        .success();

    let ids = queue_ids(repo);
    assert_eq!(ids, vec!["ann_in_a"], "got: {ids:?}");
}

#[test]
fn staged_with_no_files_staged_enqueues_nothing() {
    let tmp = make_two_file_project();
    let repo = tmp.path();
    // Nothing staged.
    aristo_in(repo)
        .args(["critique", "--staged"])
        .assert()
        .success()
        .stdout(predicates::str::contains("0 annotations matched"));
    assert!(queue_ids(repo).is_empty());
}

#[test]
fn staged_intersects_with_filter_file_clause() {
    // Both files staged but --filter file=src/a.rs explicitly chosen.
    // Intersection: only ann_in_a (matches BOTH staged set AND filter).
    let tmp = make_two_file_project();
    let repo = tmp.path();

    git_with_path(repo)
        .args(["add", "src/a.rs", "src/b.rs"])
        .assert()
        .success();
    drain_queue(repo);

    aristo_in(repo)
        .args(["critique", "--staged", "--filter", "file=src/a.rs"])
        .assert()
        .success();

    let ids = queue_ids(repo);
    assert_eq!(
        ids,
        vec!["ann_in_a"],
        "intersection must not include ann_in_b (it'd be a union otherwise); got: {ids:?}"
    );
}

#[test]
fn staged_intersects_with_filter_id_clause() {
    // Both files staged, --filter id=ann_in_b. Intersection enqueues just b.
    let tmp = make_two_file_project();
    let repo = tmp.path();

    git_with_path(repo)
        .args(["add", "src/a.rs", "src/b.rs"])
        .assert()
        .success();
    drain_queue(repo);

    aristo_in(repo)
        .args(["critique", "--staged", "--filter", "id=ann_in_b"])
        .assert()
        .success();

    let ids = queue_ids(repo);
    assert_eq!(ids, vec!["ann_in_b"], "got: {ids:?}");
}

#[test]
fn filter_id_comma_list_enqueues_each_listed_id() {
    // J2 unified grammar: `id=a,b` is a comma-separated OR set, NOT a
    // single literal id "a,b". Regression for the bug where the whole
    // value was kept verbatim, matched no annotation, and left the queue
    // empty. End-to-end via the CLI so it survives the Filter refactor.
    let tmp = make_two_file_project();
    let repo = tmp.path();

    aristo_in(repo)
        .args(["critique", "--filter", "id=ann_in_a,ann_in_b"])
        .assert()
        .success();

    assert_eq!(
        queue_ids(repo),
        vec!["ann_in_a", "ann_in_b"],
        "comma-list must enqueue BOTH ids; the pre-fix bug matched neither",
    );
}

#[test]
fn critique_without_filter_or_staged_errors_with_actionable_message() {
    // The filter-required guard still fires when neither --filter nor
    // --staged nor --all is provided.
    let tmp = make_two_file_project();
    let repo = tmp.path();

    aristo_in(repo)
        .args(["critique"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "requires `--filter`, `--staged`, or `--all`",
        ));
}

#[test]
fn all_without_yes_prints_cost_estimate_and_exits_two() {
    // The cost-gate: --all alone must show the count + dollar estimate
    // and refuse to enqueue. Intent
    // critique_all_flag_requires_confirmation_or_yes.
    let tmp = make_two_file_project();
    let repo = tmp.path();

    aristo_in(repo)
        .args(["critique", "--all"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("will enqueue 2 annotations"))
        .stderr(predicates::str::contains("$0.02"))
        .stderr(predicates::str::contains("--all --yes"));

    // Nothing was enqueued.
    assert!(
        queue_ids(repo).is_empty(),
        "--all without --yes must not enqueue"
    );
}

#[test]
fn all_with_yes_enqueues_every_verifiable_intent() {
    let tmp = make_two_file_project();
    let repo = tmp.path();

    aristo_in(repo)
        .args(["critique", "--all", "--yes"])
        .assert()
        .success();

    let ids = queue_ids(repo);
    assert_eq!(ids, vec!["ann_in_a", "ann_in_b"]);
}

/// Make a single-file project with two annotations on different lines.
/// Returns (tempdir, line_of_first, line_of_second).
fn make_two_annotation_one_file_project() -> (tempfile::TempDir, u32, u32) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();

    git_with_path(repo)
        .args(["init", "--quiet"])
        .assert()
        .success();
    git_with_path(repo)
        .args(["config", "user.email", "test@aretta.dev"])
        .assert()
        .success();
    git_with_path(repo)
        .args(["config", "user.name", "Test"])
        .assert()
        .success();
    git_with_path(repo)
        .args(["config", "commit.gpgsign", "false"])
        .assert()
        .success();

    std::fs::write(
        repo.join("Cargo.toml"),
        r#"[package]
name = "range-test"
version = "0.0.0"
edition = "2021"
"#,
    )
    .unwrap();
    std::fs::create_dir(repo.join("src")).unwrap();
    // Two annotations in src/lib.rs:
    //   line 1: top_ann
    //   line 5: bottom_ann
    let source = "\
#[aristo::intent(\"top\", verify = \"neural\", id = \"top_ann\")]
pub fn top() {}

// filler
#[aristo::intent(\"bottom\", verify = \"neural\", id = \"bottom_ann\")]
pub fn bottom() {}
";
    std::fs::write(repo.join("src/lib.rs"), source).unwrap();

    aristo_in(repo).arg("init").assert().success();
    aristo_in(repo).arg("stamp").assert().success();

    (tmp, 1, 5)
}

#[test]
fn line_range_filter_matches_only_in_range_annotations() {
    let (tmp, top_line, bottom_line) = make_two_annotation_one_file_project();
    let repo = tmp.path();

    // Range that includes top but not bottom.
    let range = format!("file=src/lib.rs:{}-{}", top_line, top_line);
    aristo_in(repo)
        .args(["critique", "--filter", &range])
        .assert()
        .success();
    assert_eq!(queue_ids(repo), vec!["top_ann"]);

    // Range that includes bottom but not top.
    drain_queue(repo);
    let range = format!("file=src/lib.rs:{}-{}", bottom_line, bottom_line);
    aristo_in(repo)
        .args(["critique", "--filter", &range])
        .assert()
        .success();
    assert_eq!(queue_ids(repo), vec!["bottom_ann"]);

    // Range that spans both → enqueues both.
    drain_queue(repo);
    let range = format!("file=src/lib.rs:{}-{}", top_line, bottom_line);
    aristo_in(repo)
        .args(["critique", "--filter", &range])
        .assert()
        .success();
    let mut ids = queue_ids(repo);
    ids.sort();
    assert_eq!(ids, vec!["bottom_ann", "top_ann"]);
}

#[test]
fn line_range_filter_with_no_matches_is_zero_exit_zero() {
    let (tmp, _, _) = make_two_annotation_one_file_project();
    let repo = tmp.path();

    aristo_in(repo)
        .args(["critique", "--filter", "file=src/lib.rs:100-200"])
        .assert()
        .success()
        .stdout(predicates::str::contains("0 annotations matched"));
    assert!(queue_ids(repo).is_empty());
}
