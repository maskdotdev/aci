use crate::{ChangeKind, FileChange};
use aci_core::{AciError, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub(crate) struct GitRepository {
    root: PathBuf,
}

impl GitRepository {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let output = git_output(path, ["rev-parse", "--show-toplevel"])?;
        Ok(Self {
            root: PathBuf::from(output.trim()),
        })
    }

    pub(crate) fn resolve_ref(&self, reference: &str) -> Result<String> {
        let rev = format!("{reference}^{{commit}}");
        let output = git_output(&self.root, ["rev-parse", "--verify", &rev])?;
        Ok(output.trim().to_string())
    }

    pub(crate) fn diff_name_status(&self, base: &str, head: &str) -> Result<Vec<FileChange>> {
        let output = git_output(&self.root, ["diff", "--name-status", "-M", base, head])?;
        let mut changes = Vec::new();
        for line in output.lines().filter(|line| !line.trim().is_empty()) {
            changes.push(parse_name_status_line(line)?);
        }
        changes.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.old_path.cmp(&right.old_path))
        });
        Ok(changes)
    }

    pub(crate) fn checkout_pair(&self, base: &str, head: &str) -> Result<CheckedOutRefs> {
        let temp = tempfile::tempdir()?;
        let base_root = temp.path().join("base");
        let head_root = temp.path().join("head");
        let base_root_arg = path_arg(&base_root);
        let head_root_arg = path_arg(&head_root);
        git_status(
            &self.root,
            [
                "worktree",
                "add",
                "--detach",
                "--quiet",
                base_root_arg.as_str(),
                base,
            ],
        )?;
        git_status(
            &self.root,
            [
                "worktree",
                "add",
                "--detach",
                "--quiet",
                head_root_arg.as_str(),
                head,
            ],
        )?;
        Ok(CheckedOutRefs {
            base_root: base_root.clone(),
            head_root: head_root.clone(),
            _base: WorktreeGuard::new(self.root.clone(), base_root),
            _head: WorktreeGuard::new(self.root.clone(), head_root),
            _temp: temp,
        })
    }
}

pub(crate) struct CheckedOutRefs {
    pub(crate) base_root: PathBuf,
    pub(crate) head_root: PathBuf,
    _base: WorktreeGuard,
    _head: WorktreeGuard,
    _temp: TempDir,
}

struct WorktreeGuard {
    repo_root: PathBuf,
    worktree: PathBuf,
}

impl WorktreeGuard {
    fn new(repo_root: PathBuf, worktree: PathBuf) -> Self {
        Self {
            repo_root,
            worktree,
        }
    }
}

impl Drop for WorktreeGuard {
    fn drop(&mut self) {
        let _ = Command::new("git")
            .arg("worktree")
            .arg("remove")
            .arg("--force")
            .arg(&self.worktree)
            .current_dir(&self.repo_root)
            .status();
    }
}

fn parse_name_status_line(line: &str) -> Result<FileChange> {
    let parts = line.split('\t').collect::<Vec<_>>();
    let status = parts
        .first()
        .ok_or_else(|| AciError::Message("empty git diff status line".to_string()))?;
    let change = match status.chars().next() {
        Some('A') => ChangeKind::Added,
        Some('D') => ChangeKind::Removed,
        Some('M') => ChangeKind::Modified,
        Some('R') => ChangeKind::Renamed,
        Some('C') => ChangeKind::Copied,
        Some('T') => ChangeKind::TypeChanged,
        _ => {
            return Err(AciError::Message(format!(
                "unsupported git diff status: {status}"
            )));
        }
    };
    let (old_path, path) = if matches!(change, ChangeKind::Renamed | ChangeKind::Copied) {
        let old_path = parts.get(1).ok_or_else(|| {
            AciError::Message(format!("missing old path in git diff status line: {line}"))
        })?;
        let path = parts.get(2).ok_or_else(|| {
            AciError::Message(format!("missing new path in git diff status line: {line}"))
        })?;
        (Some((*old_path).to_string()), (*path).to_string())
    } else {
        let path = parts.get(1).ok_or_else(|| {
            AciError::Message(format!("missing path in git diff status line: {line}"))
        })?;
        (None, (*path).to_string())
    };
    Ok(FileChange {
        change,
        path,
        old_path,
    })
}

fn git_output<const N: usize>(cwd: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(AciError::Message(git_error(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn git_status<const N: usize>(cwd: &Path, args: [&str; N]) -> Result<()> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AciError::Message(git_error(&output)))
    }
}

fn git_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if detail.is_empty() {
        "git command failed".to_string()
    } else {
        detail.to_string()
    }
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
