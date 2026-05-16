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
        let output = git_output_bytes(
            &self.root,
            ["diff", "--name-status", "-z", "-M", base, head],
        )?;
        let mut changes = parse_name_status_z(&output)?;
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
        let base_guard = WorktreeGuard::new(self.root.clone(), base_root.clone());
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
        let head_guard = WorktreeGuard::new(self.root.clone(), head_root.clone());
        Ok(CheckedOutRefs {
            base_root: base_root.clone(),
            head_root: head_root.clone(),
            _base: base_guard,
            _head: head_guard,
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

fn parse_name_status_z(output: &[u8]) -> Result<Vec<FileChange>> {
    let mut fields = output
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty());
    let mut changes = Vec::new();
    while let Some(status) = fields.next() {
        let change = change_from_status(status)?;
        let (old_path, path) = if matches!(change, ChangeKind::Renamed | ChangeKind::Copied) {
            let old_path = next_path(&mut fields, status, "old path")?;
            let path = next_path(&mut fields, status, "new path")?;
            (Some(old_path), path)
        } else {
            (None, next_path(&mut fields, status, "path")?)
        };
        changes.push(FileChange {
            change,
            path,
            old_path,
        });
    }
    Ok(changes)
}

fn change_from_status(status: &[u8]) -> Result<ChangeKind> {
    let change = match status.first().copied() {
        Some(b'A') => ChangeKind::Added,
        Some(b'D') => ChangeKind::Removed,
        Some(b'M') => ChangeKind::Modified,
        Some(b'R') => ChangeKind::Renamed,
        Some(b'C') => ChangeKind::Copied,
        Some(b'T') => ChangeKind::TypeChanged,
        _ => {
            return Err(AciError::Message(format!(
                "unsupported git diff status: {}",
                path_string(status)
            )));
        }
    };
    Ok(change)
}

fn next_path<'a>(
    fields: &mut impl Iterator<Item = &'a [u8]>,
    status: &[u8],
    label: &str,
) -> Result<String> {
    fields.next().map(path_string).ok_or_else(|| {
        AciError::Message(format!(
            "missing {label} after git diff status {}",
            path_string(status)
        ))
    })
}

fn git_output<const N: usize>(cwd: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(AciError::Message(git_error(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn git_output_bytes<const N: usize>(cwd: &Path, args: [&str; N]) -> Result<Vec<u8>> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(AciError::Message(git_error(&output)));
    }
    Ok(output.stdout)
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

fn path_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nul_separated_name_status_with_special_paths() {
        let input = b"M\0src/has space.ts\0A\0src/has\ttab.ts\0D\0src/has\nnewline.ts\0";
        let changes = parse_name_status_z(input).expect("parse name-status");

        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].change, ChangeKind::Modified);
        assert_eq!(changes[0].path, "src/has space.ts");
        assert_eq!(changes[1].change, ChangeKind::Added);
        assert_eq!(changes[1].path, "src/has\ttab.ts");
        assert_eq!(changes[2].change, ChangeKind::Removed);
        assert_eq!(changes[2].path, "src/has\nnewline.ts");
    }

    #[test]
    fn parses_nul_separated_renames() {
        let input = b"R100\0src/old name.ts\0src/new name.ts\0";
        let changes = parse_name_status_z(input).expect("parse rename");

        assert_eq!(
            changes,
            vec![FileChange {
                change: ChangeKind::Renamed,
                old_path: Some("src/old name.ts".to_string()),
                path: "src/new name.ts".to_string(),
            }]
        );
    }
}
