use aci_diff::{ChangeKind, DiffOptions, diff_refs};
use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn reports_symbol_public_api_and_impact_changes() {
    let repo = fixture_repo();
    write(
        repo.path().join("src/lib.ts"),
        "export function stable() { return 1; }\nexport function changed() { return stable(); }\n",
    );
    write(
        repo.path().join("src/app.ts"),
        "import { changed } from './lib';\nexport function app() { return changed(); }\n",
    );
    commit_all(repo.path(), "base");

    git(repo.path(), ["checkout", "-b", "feature"]);
    write(
        repo.path().join("src/lib.ts"),
        "export function stable() { return 1; }\nexport function changed() { return stable() + 1; }\nexport function added() { return changed(); }\n",
    );
    commit_all(repo.path(), "head");

    let report = diff_refs(DiffOptions::new("main", "feature").with_repo_root(repo.path()))
        .expect("diff refs");

    assert_eq!(report.stats.files_modified, 1);
    assert!(report.changed_symbols.iter().any(|symbol| {
        symbol.change == ChangeKind::Modified
            && symbol
                .after
                .as_ref()
                .is_some_and(|summary| summary.name == "changed")
    }));
    assert!(report.changed_symbols.iter().any(|symbol| {
        symbol.change == ChangeKind::Added
            && symbol
                .after
                .as_ref()
                .is_some_and(|summary| summary.name == "added")
    }));
    assert!(report.public_api_changes.iter().any(|symbol| {
        symbol
            .after
            .as_ref()
            .or(symbol.before.as_ref())
            .is_some_and(|summary| summary.name == "changed")
    }));
    assert!(
        report
            .impacted_files
            .iter()
            .any(|file| file.path == "src/app.ts")
    );
}

#[test]
fn treats_renamed_files_as_file_changes_without_symbol_churn() {
    let repo = fixture_repo();
    write(
        repo.path().join("src/old.ts"),
        "export function oldName() { return 1; }\n",
    );
    write(
        repo.path().join("src/remove.py"),
        "def doomed():\n    return 1\n",
    );
    commit_all(repo.path(), "base");

    git(repo.path(), ["checkout", "-b", "feature"]);
    git(repo.path(), ["mv", "src/old.ts", "src/renamed.ts"]);
    fs::remove_file(repo.path().join("src/remove.py")).expect("remove file");
    commit_all(repo.path(), "head");

    let report = diff_refs(DiffOptions::new("main", "feature").with_repo_root(repo.path()))
        .expect("diff refs");

    assert!(report.changed_files.iter().any(|file| {
        file.change == ChangeKind::Renamed
            && file.old_path.as_deref() == Some("src/old.ts")
            && file.path == "src/renamed.ts"
    }));
    assert!(
        report
            .changed_files
            .iter()
            .any(|file| file.change == ChangeKind::Removed && file.path == "src/remove.py")
    );
    assert!(!report.changed_symbols.iter().any(|symbol| {
        symbol
            .before
            .as_ref()
            .or(symbol.after.as_ref())
            .is_some_and(|summary| summary.name == "oldName")
    }));
}

#[test]
fn reports_dependency_changes_and_head_diagnostics() {
    let repo = fixture_repo();
    write(
        repo.path().join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    );
    commit_all(repo.path(), "base");

    git(repo.path(), ["checkout", "-b", "feature"]);
    write(
        repo.path().join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0","lodash":"4.17.21"}}"#,
    );
    write(
        repo.path().join("src/broken.ts"),
        "export function broken( {\n",
    );
    commit_all(repo.path(), "head");

    let report = diff_refs(DiffOptions::new("main", "feature").with_repo_root(repo.path()))
        .expect("diff refs");

    assert!(report.dependency_changes.iter().any(|dependency| {
        dependency.change == ChangeKind::Added && dependency.dependency == "lodash"
    }));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.file.as_deref() == Some("src/broken.ts"))
    );
}

#[test]
fn does_not_collapse_same_named_methods_in_one_file() {
    let repo = fixture_repo();
    write(
        repo.path().join("src/lib.ts"),
        "export class A { run() { return 1; } }\nexport class B { run() { return 2; } }\n",
    );
    commit_all(repo.path(), "base");

    git(repo.path(), ["checkout", "-b", "feature"]);
    write(
        repo.path().join("src/lib.ts"),
        "export class A { run() { return 99; } }\nexport class B { run() { return 2; } }\n",
    );
    commit_all(repo.path(), "head");

    let report = diff_refs(DiffOptions::new("main", "feature").with_repo_root(repo.path()))
        .expect("diff refs");

    assert!(
        report.changed_symbols.iter().any(|symbol| {
            symbol.change == ChangeKind::Modified
                && symbol
                    .after
                    .as_ref()
                    .and_then(|summary| summary.qualified_name.as_deref())
                    == Some("lib.A.run")
        }),
        "expected lib.A.run to be marked modified: {:#?}",
        report.changed_symbols
    );
    assert!(
        !report.changed_symbols.iter().any(|symbol| {
            symbol
                .after
                .as_ref()
                .or(symbol.before.as_ref())
                .and_then(|summary| summary.qualified_name.as_deref())
                == Some("lib.B.run")
        }),
        "lib.B.run should not be marked modified: {:#?}",
        report.changed_symbols
    );
}

fn fixture_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("tempdir");
    fs::create_dir(repo.path().join("src")).expect("src dir");
    git(repo.path(), ["init", "-b", "main"]);
    git(repo.path(), ["config", "user.email", "aci@example.com"]);
    git(repo.path(), ["config", "user.name", "ACI Test"]);
    repo
}

fn write(path: impl AsRef<Path>, contents: &str) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent dir");
    }
    fs::write(path, contents).expect("write fixture");
}

fn commit_all(repo: &Path, message: &str) {
    git(repo, ["add", "."]);
    git(repo, ["commit", "-m", message]);
}

fn git<const N: usize>(repo: &Path, args: [&str; N]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    if !output.status.success() {
        panic!(
            "git failed: {}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
