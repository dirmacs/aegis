use std::path::Path;

use anyhow::{Context, Result};
use similar::{ChangeTag, TextDiff};

/// Generate a unified diff between two files.
pub fn diff_files(source: &Path, target: &Path) -> Result<DiffResult> {
    let source_content = std::fs::read_to_string(source)
        .with_context(|| format!("reading {}", source.display()))?;
    let target_content = std::fs::read_to_string(target)
        .with_context(|| format!("reading {}", target.display()))?;

    Ok(diff_strings(
        &source_content,
        &target_content,
        &source.display().to_string(),
        &target.display().to_string(),
    ))
}

/// Generate a unified diff between two strings.
pub fn diff_strings(old: &str, new: &str, old_label: &str, new_label: &str) -> DiffResult {
    let text_diff = TextDiff::from_lines(old, new);

    let mut hunks = Vec::new();
    for hunk in text_diff.unified_diff().context_radius(3).iter_hunks() {
        let mut lines = Vec::new();
        for change in hunk.iter_changes() {
            let tag = match change.tag() {
                ChangeTag::Delete => DiffTag::Remove,
                ChangeTag::Insert => DiffTag::Add,
                ChangeTag::Equal => DiffTag::Context,
            };
            lines.push(DiffLine {
                tag,
                content: change.value().to_string(),
            });
        }
        hunks.push(DiffHunk { lines });
    }

    let has_changes = hunks.iter().any(|h| {
        h.lines
            .iter()
            .any(|l| l.tag == DiffTag::Add || l.tag == DiffTag::Remove)
    });

    DiffResult {
        old_label: old_label.to_string(),
        new_label: new_label.to_string(),
        hunks,
        has_changes,
    }
}

#[derive(Debug)]
pub struct DiffResult {
    pub old_label: String,
    pub new_label: String,
    pub hunks: Vec<DiffHunk>,
    pub has_changes: bool,
}

#[derive(Debug)]
pub struct DiffHunk {
    pub lines: Vec<DiffLine>,
}

#[derive(Debug)]
pub struct DiffLine {
    pub tag: DiffTag,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffTag {
    Add,
    Remove,
    Context,
}

impl std::fmt::Display for DiffResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.has_changes {
            return write!(f, "no changes");
        }
        writeln!(f, "--- {}", self.old_label)?;
        writeln!(f, "+++ {}", self.new_label)?;
        for hunk in &self.hunks {
            for line in &hunk.lines {
                let prefix = match line.tag {
                    DiffTag::Add => "+",
                    DiffTag::Remove => "-",
                    DiffTag::Context => " ",
                };
                write!(f, "{prefix}{}", line.content)?;
            }
        }
        Ok(())
    }
}
