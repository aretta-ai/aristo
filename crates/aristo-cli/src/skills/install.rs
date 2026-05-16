//! Skill install backends. Two install models per K4:
//!
//! 1. **File copy** (Claude Code, Cursor, Antigravity) — write the skill
//!    content verbatim to a per-agent path under the project (or user-
//!    scope) directory. Different agents use different extensions and
//!    layouts; the path is the agent's responsibility, not ours.
//!
//! 2. **AGENTS.md section injection** (Codex, OpenCode) — append or
//!    replace a marker-delimited block inside an existing `AGENTS.md`
//!    file, preserving everything outside the markers. Lets users
//!    hand-edit the rest of AGENTS.md without losing their work on
//!    `aristo install-skills --update`.
//!
//! The CLI dispatcher in slice 13 calls these helpers; this module has no
//! CLI surface of its own.

use std::fs;
use std::io;
use std::path::Path;

use super::Skill;

/// Marker boundaries for the AGENTS.md-style section. Versioned in the
/// START marker so a future format bump can detect old blocks during
/// `--update`.
pub(crate) const SECTION_START: &str = "<!-- ARISTO-SKILLS START v1 -->";
pub(crate) const SECTION_END: &str = "<!-- ARISTO-SKILLS END -->";

/// Outcome of an install operation, so callers can emit the right
/// "ok: wrote ..." vs "ok: updated ..." vs "note: unchanged" message.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InstallOutcome {
    /// File didn't exist; we wrote it.
    Created,
    /// File existed with different content; we replaced it.
    Updated,
    /// File existed with identical content; nothing changed.
    Unchanged,
}

#[aristo::intent(
    "A second invocation with identical content leaves the target \
     byte-identical and returns `Unchanged`. Created (file did not \
     exist) and Updated (content differed) are distinct outcomes; \
     idempotence is the Unchanged case specifically.",
    verify = "test",
    id = "file_copy_install_idempotent"
)]
pub(crate) fn file_copy_install(target: &Path, skill: &Skill) -> io::Result<InstallOutcome> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let resolved = skill.resolved_content();
    if target.exists() {
        let existing = fs::read_to_string(target)?;
        if existing == resolved {
            return Ok(InstallOutcome::Unchanged);
        }
        fs::write(target, &resolved)?;
        return Ok(InstallOutcome::Updated);
    }

    fs::write(target, &resolved)?;
    Ok(InstallOutcome::Created)
}

#[aristo::intent(
    "Removes only the file we wrote — no sibling deletion, no \
     parent-dir cleanup. Absence of the target is not an error; \
     uninstall-of-already-uninstalled is the idempotent case.",
    verify = "test",
    id = "file_copy_uninstall_idempotent"
)]
pub(crate) fn file_copy_uninstall(target: &Path) -> io::Result<bool> {
    if !target.exists() {
        return Ok(false);
    }
    fs::remove_file(target)?;
    Ok(true)
}

/// Inject (or update) the marker-delimited Aristo block inside an
/// AGENTS.md-style file. Content outside the markers is preserved
/// verbatim; if the file doesn't exist, it's created with just the block.
#[aristo::intent(
    "Content outside the marker boundaries is preserved byte-for-byte \
     across install and update. Users who hand-edit AGENTS.md alongside \
     the auto-generated block don't lose their work to a normalization \
     or reformat pass.",
    verify = "test",
    id = "agents_md_install_preserves_outside_markers"
)]
pub(crate) fn agents_md_install(target: &Path, skills: &[&Skill]) -> io::Result<InstallOutcome> {
    let new_block = render_agents_md_block(skills);

    if !target.exists() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, &new_block)?;
        return Ok(InstallOutcome::Created);
    }

    let existing = fs::read_to_string(target)?;
    let updated = match find_block(&existing) {
        Some((start, end)) => {
            let mut buf = String::with_capacity(existing.len() + new_block.len());
            buf.push_str(&existing[..start]);
            buf.push_str(new_block.trim_end());
            buf.push_str(&existing[end..]);
            buf
        }
        None => {
            let mut buf = existing.clone();
            if !buf.ends_with('\n') {
                buf.push('\n');
            }
            buf.push('\n');
            buf.push_str(&new_block);
            buf
        }
    };

    if updated == existing {
        return Ok(InstallOutcome::Unchanged);
    }
    fs::write(target, updated)?;
    Ok(InstallOutcome::Updated)
}

/// Strip the marker-delimited Aristo block from an AGENTS.md-style file.
/// Returns `Ok(false)` if the file or block is absent (idempotent).
#[aristo::intent(
    "Only the marker-delimited block is stripped; surrounding content \
     is preserved byte-for-byte. Absent file or absent block is not an \
     error — idempotent.",
    verify = "test",
    id = "agents_md_uninstall_preserves_outside_markers"
)]
pub(crate) fn agents_md_uninstall(target: &Path) -> io::Result<bool> {
    if !target.exists() {
        return Ok(false);
    }
    let existing = fs::read_to_string(target)?;
    let Some((start, end)) = find_block(&existing) else {
        return Ok(false);
    };

    // Trim a trailing newline immediately after the block so removal
    // doesn't leave a doubled blank line.
    let mut trim_end = end;
    if existing[trim_end..].starts_with('\n') {
        trim_end += 1;
    }
    // And a leading newline before, for the same reason.
    let mut trim_start = start;
    if trim_start > 0 && existing[..trim_start].ends_with('\n') {
        trim_start -= 1;
    }

    let mut buf = String::with_capacity(existing.len());
    buf.push_str(&existing[..trim_start]);
    buf.push_str(&existing[trim_end..]);
    fs::write(target, buf)?;
    Ok(true)
}

fn render_agents_md_block(skills: &[&Skill]) -> String {
    let mut buf = String::new();
    buf.push_str(SECTION_START);
    buf.push('\n');
    for s in skills {
        buf.push_str("\n## ");
        buf.push_str(s.name);
        buf.push_str("\n\n");
        buf.push_str(s.resolved_content().trim());
        buf.push('\n');
    }
    buf.push('\n');
    buf.push_str(SECTION_END);
    buf.push('\n');
    buf
}

/// Locate the byte range of the Aristo block in an AGENTS.md file.
/// Returns `(start_of_marker, end_after_end_marker)` byte offsets.
fn find_block(content: &str) -> Option<(usize, usize)> {
    let start = content.find(SECTION_START)?;
    let end_marker_start = content[start..].find(SECTION_END)? + start;
    let end = end_marker_start + SECTION_END.len();
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills;
    use tempfile::TempDir;

    fn skill() -> &'static Skill {
        skills::bundled()
            .iter()
            .find(|s| s.name == "aristo-authoring")
            .expect("authoring skill must be bundled")
    }

    // ---- template resolution ----

    #[test]
    fn installed_skill_has_real_sdk_version_not_placeholder() {
        // The smoking-gun bug from v0.0.5: skill body shipped with
        // `sdk_version: 0.0.4` hardcoded, drifting on every release.
        // Install must resolve `{{SDK_VERSION}}` to the running binary's
        // version; the placeholder must never reach disk.
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("SKILL.md");
        file_copy_install(&target, skill()).unwrap();
        let on_disk = fs::read_to_string(&target).unwrap();
        assert!(
            !on_disk.contains("{{SDK_VERSION}}"),
            "placeholder leaked to installed file"
        );
        let expected = format!("sdk_version: {}", env!("CARGO_PKG_VERSION"));
        assert!(
            on_disk.contains(&expected),
            "installed frontmatter missing `{expected}`"
        );
    }

    // ---- file_copy ----

    #[test]
    fn file_copy_creates_then_unchanged_then_updated() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("a/b/c/SKILL.md");

        let r1 = file_copy_install(&target, skill()).unwrap();
        assert_eq!(r1, InstallOutcome::Created);
        assert!(target.is_file());

        let r2 = file_copy_install(&target, skill()).unwrap();
        assert_eq!(r2, InstallOutcome::Unchanged);

        // Mutate the file and re-install — should report Updated.
        fs::write(&target, "tampered").unwrap();
        let r3 = file_copy_install(&target, skill()).unwrap();
        assert_eq!(r3, InstallOutcome::Updated);
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            skill().resolved_content()
        );
    }

    #[test]
    fn file_copy_uninstall_idempotent() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("SKILL.md");

        // Absent — no error, returns false.
        assert!(!file_copy_uninstall(&target).unwrap());

        // Present — removes, returns true.
        file_copy_install(&target, skill()).unwrap();
        assert!(file_copy_uninstall(&target).unwrap());
        assert!(!target.exists());

        // Absent again — false.
        assert!(!file_copy_uninstall(&target).unwrap());
    }

    // ---- agents_md ----

    #[test]
    fn agents_md_creates_when_file_absent() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("AGENTS.md");

        let r = agents_md_install(&target, &[skill()]).unwrap();
        assert_eq!(r, InstallOutcome::Created);

        let content = fs::read_to_string(&target).unwrap();
        assert!(content.contains(SECTION_START));
        assert!(content.contains(SECTION_END));
        assert!(content.contains("## aristo-authoring"));
    }

    #[test]
    fn agents_md_appends_block_to_existing_file_preserving_user_content() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("AGENTS.md");
        let user_text = "# My agent rules\n\nUse 4-space indent.\n";
        fs::write(&target, user_text).unwrap();

        agents_md_install(&target, &[skill()]).unwrap();

        let content = fs::read_to_string(&target).unwrap();
        assert!(
            content.starts_with(user_text),
            "user content must be preserved at the start"
        );
        assert!(content.contains(SECTION_START));
    }

    #[test]
    fn agents_md_replaces_only_marker_block_on_update() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("AGENTS.md");
        let user_before = "# Before\n\n";
        let stale_block =
            format!("{SECTION_START}\n## aristo-authoring\n\nold content\n\n{SECTION_END}\n");
        let user_after = "\n# After\n\nMore user notes.\n";
        fs::write(&target, format!("{user_before}{stale_block}{user_after}")).unwrap();

        let r = agents_md_install(&target, &[skill()]).unwrap();
        assert_eq!(r, InstallOutcome::Updated);

        let content = fs::read_to_string(&target).unwrap();
        assert!(content.contains("# Before"));
        assert!(content.contains("# After"));
        assert!(content.contains("More user notes."));
        assert!(
            !content.contains("old content"),
            "stale content must be replaced"
        );
        assert!(content.contains(SECTION_START));
    }

    #[test]
    fn agents_md_install_idempotent() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("AGENTS.md");

        agents_md_install(&target, &[skill()]).unwrap();
        let r = agents_md_install(&target, &[skill()]).unwrap();
        assert_eq!(r, InstallOutcome::Unchanged);
    }

    #[test]
    fn agents_md_uninstall_strips_block_preserves_surrounding() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("AGENTS.md");
        let user_before = "# My rules\n\nUse 4-space indent.\n";
        fs::write(&target, user_before).unwrap();

        agents_md_install(&target, &[skill()]).unwrap();
        let removed = agents_md_uninstall(&target).unwrap();
        assert!(removed);

        let content = fs::read_to_string(&target).unwrap();
        assert!(!content.contains(SECTION_START));
        assert!(!content.contains(SECTION_END));
        assert!(content.contains("# My rules"));
        assert!(content.contains("Use 4-space indent."));
    }

    #[test]
    fn agents_md_uninstall_idempotent_when_file_absent_or_block_absent() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("AGENTS.md");

        // File absent.
        assert!(!agents_md_uninstall(&target).unwrap());

        // File present, no block.
        fs::write(&target, "just user content\n").unwrap();
        assert!(!agents_md_uninstall(&target).unwrap());
        assert_eq!(fs::read_to_string(&target).unwrap(), "just user content\n");
    }
}
