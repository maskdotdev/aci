use crate::{
    AgentDiffReport, AgentDiffStats, AgentReviewFocus, AgentTopChange, ChangeKind, ChangedSymbol,
    DiffDiagnostic, DiffReport, FileChange, RiskLevel, labels::change_label,
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_TOP_CHANGES: usize = 8;
const MAX_REVIEW_FOCUS: usize = 8;

pub fn summarize_for_agent(report: &DiffReport) -> AgentDiffReport {
    let tests_changed = report
        .changed_files
        .iter()
        .any(|file| is_test_path(&file.path));
    let changed_paths = changed_paths(report);
    let relevant_diagnostics = relevant_diagnostics(report, &changed_paths);
    let important_symbols = important_symbols(report);
    let risk = overall_risk(
        report,
        tests_changed,
        important_symbols.len(),
        relevant_diagnostics.len(),
    );
    let top_changes = top_changes(report, &important_symbols);
    let review_focus = review_focus(report, &top_changes);
    let notes = notes(report, tests_changed, relevant_diagnostics.len());
    AgentDiffReport {
        base: report.base.clone(),
        head: report.head.clone(),
        risk,
        summary: AgentDiffStats {
            files_changed: report.changed_files.len(),
            public_api_changes: report.public_api_changes.len(),
            important_symbol_changes: important_symbols.len(),
            dependency_changes: report.dependency_changes.len(),
            diagnostics: relevant_diagnostics.len(),
            tests_changed,
        },
        top_changes,
        review_focus,
        notes,
    }
}

fn important_symbols(report: &DiffReport) -> Vec<&ChangedSymbol> {
    let mut symbols = report
        .changed_symbols
        .iter()
        .filter(|symbol| {
            symbol.public_api
                || symbol.risk != RiskLevel::Low
                || matches!(symbol.change, ChangeKind::Added | ChangeKind::Removed)
        })
        .collect::<Vec<_>>();
    symbols.sort_by(|left, right| {
        symbol_score(right)
            .cmp(&symbol_score(left))
            .then(symbol_name(left).cmp(&symbol_name(right)))
    });
    symbols
}

fn top_changes(report: &DiffReport, important_symbols: &[&ChangedSymbol]) -> Vec<AgentTopChange> {
    let mut paths = BTreeMap::<String, PathSignal>::new();
    for file in &report.changed_files {
        let signal = paths.entry(file.path.clone()).or_default();
        signal.score += file_score(file);
        signal.risk = max_risk(signal.risk, file_risk(file));
        signal.kind = format!("{}-file", change_label(file.change));
        signal.reasons.insert(file_reason(file));
    }
    for symbol in important_symbols {
        let Some(path) = symbol_path(symbol) else {
            continue;
        };
        let signal = paths.entry(path).or_default();
        signal.score += symbol_score(symbol);
        signal.risk = max_risk(signal.risk, symbol.risk);
        signal.symbols.insert(symbol_name(symbol));
        signal.reasons.insert(symbol_reason(symbol));
    }
    for dependency in &report.dependency_changes {
        let signal = paths.entry(dependency.file.clone()).or_default();
        signal.score += 8;
        signal.risk = max_risk(signal.risk, RiskLevel::Medium);
        signal.kind = format!("{}-dependency", change_label(dependency.change));
        signal
            .reasons
            .insert("dependency surface changed".to_string());
    }
    for diagnostic in &report.diagnostics {
        let Some(path) = &diagnostic.file else {
            continue;
        };
        if !paths.contains_key(path) {
            continue;
        }
        let signal = paths.entry(path.clone()).or_default();
        signal.score += 6;
        signal.risk = max_risk(signal.risk, RiskLevel::Medium);
        signal
            .reasons
            .insert("changed file has parser or indexing diagnostics".to_string());
    }
    let mut ranked = paths.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|(left_path, left), (right_path, right)| {
        right.score.cmp(&left.score).then(left_path.cmp(right_path))
    });
    ranked
        .into_iter()
        .take(MAX_TOP_CHANGES)
        .map(|(path, signal)| AgentTopChange {
            kind: signal.kind,
            path,
            symbol: symbol_summary(signal.symbols),
            risk: signal.risk,
            why_it_matters: signal.reasons.into_iter().collect::<Vec<_>>().join("; "),
        })
        .collect()
}

fn review_focus(report: &DiffReport, top_changes: &[AgentTopChange]) -> Vec<AgentReviewFocus> {
    let mut scores = BTreeMap::<String, (u32, BTreeSet<String>)>::new();
    let changed_paths = changed_paths(report);
    for change in top_changes {
        let (score, reasons) = scores.entry(change.path.clone()).or_default();
        *score += risk_score(change.risk) + 3;
        for reason in change.why_it_matters.split("; ") {
            reasons.insert(reason.to_string());
        }
    }
    for file in &report.changed_files {
        let (score, reasons) = scores.entry(file.path.clone()).or_default();
        *score += file_score(file);
        reasons.insert(file_reason(file));
    }
    for impact in &report.impacted_files {
        if !changed_paths.contains(impact.path.as_str()) {
            continue;
        }
        let (score, reasons) = scores.entry(impact.path.clone()).or_default();
        *score += 1;
        reasons.insert("impacted by changed graph facts".to_string());
    }
    let mut focus = scores.into_iter().collect::<Vec<_>>();
    focus.sort_by(
        |(left_path, (left_score, _)), (right_path, (right_score, _))| {
            right_score.cmp(left_score).then(left_path.cmp(right_path))
        },
    );
    focus
        .into_iter()
        .take(MAX_REVIEW_FOCUS)
        .map(|(path, (_, reasons))| AgentReviewFocus {
            path,
            reason: reasons.into_iter().collect::<Vec<_>>().join("; "),
        })
        .collect()
}

fn notes(report: &DiffReport, tests_changed: bool, relevant_diagnostics: usize) -> Vec<String> {
    let mut notes = Vec::new();
    if !tests_changed {
        notes.push(
            "No test files changed; verify existing coverage exercises this diff.".to_string(),
        );
    }
    if report.changed_symbols.len() > report.public_api_changes.len() + 20 {
        notes.push(
            "Raw symbol diff is broad; prefer top_changes and review_focus for agent triage."
                .to_string(),
        );
    }
    if relevant_diagnostics > 0 {
        notes.push(
            "Changed files have parser or indexing diagnostics; inspect diagnostics before trusting impact."
                .to_string(),
        );
    }
    notes
}

fn overall_risk(
    report: &DiffReport,
    tests_changed: bool,
    important_symbols: usize,
    relevant_diagnostics: usize,
) -> RiskLevel {
    if report
        .public_api_changes
        .iter()
        .any(|symbol| symbol.risk == RiskLevel::High)
        || report
            .changed_files
            .iter()
            .any(|file| file.change == ChangeKind::Removed && !is_test_path(&file.path))
    {
        return RiskLevel::High;
    }
    if !report.public_api_changes.is_empty()
        || !report.dependency_changes.is_empty()
        || relevant_diagnostics > 0
        || !tests_changed
        || important_symbols > 8
    {
        return RiskLevel::Medium;
    }
    RiskLevel::Low
}

struct PathSignal {
    score: u32,
    kind: String,
    risk: RiskLevel,
    symbols: BTreeSet<String>,
    reasons: BTreeSet<String>,
}

impl Default for PathSignal {
    fn default() -> Self {
        Self {
            score: 0,
            kind: "changed-file".to_string(),
            risk: RiskLevel::Low,
            symbols: BTreeSet::new(),
            reasons: BTreeSet::new(),
        }
    }
}

fn changed_paths(report: &DiffReport) -> BTreeSet<&str> {
    report
        .changed_files
        .iter()
        .map(|file| file.path.as_str())
        .collect()
}

fn relevant_diagnostics<'a>(
    report: &'a DiffReport,
    changed_paths: &BTreeSet<&str>,
) -> Vec<&'a DiffDiagnostic> {
    report
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .file
                .as_deref()
                .is_some_and(|file| changed_paths.contains(file))
        })
        .collect()
}

fn symbol_score(symbol: &ChangedSymbol) -> u32 {
    risk_score(symbol.risk)
        + if symbol.public_api { 10 } else { 0 }
        + match symbol.change {
            ChangeKind::Removed => 8,
            ChangeKind::Added => 5,
            ChangeKind::Modified | ChangeKind::TypeChanged => 3,
            ChangeKind::Renamed | ChangeKind::Copied => 2,
        }
}

fn file_score(file: &FileChange) -> u32 {
    match file.change {
        ChangeKind::Removed => 12,
        ChangeKind::Added | ChangeKind::Renamed => 8,
        ChangeKind::TypeChanged => 6,
        ChangeKind::Modified | ChangeKind::Copied => 2,
    }
}

fn risk_score(risk: RiskLevel) -> u32 {
    match risk {
        RiskLevel::High => 20,
        RiskLevel::Medium => 10,
        RiskLevel::Low => 1,
    }
}

fn max_risk(left: RiskLevel, right: RiskLevel) -> RiskLevel {
    if risk_score(right) > risk_score(left) {
        right
    } else {
        left
    }
}

fn symbol_reason(symbol: &ChangedSymbol) -> String {
    if symbol.public_api {
        return "public API changed".to_string();
    }
    match symbol.change {
        ChangeKind::Removed => "symbol removed".to_string(),
        ChangeKind::Added => "new symbol added".to_string(),
        ChangeKind::Modified | ChangeKind::TypeChanged => {
            "symbol behavior or shape changed".to_string()
        }
        ChangeKind::Renamed => "symbol renamed".to_string(),
        ChangeKind::Copied => "symbol copied".to_string(),
    }
}

fn file_reason(file: &FileChange) -> String {
    match (&file.change, &file.old_path) {
        (ChangeKind::Renamed, Some(old)) => {
            format!("file renamed from {old}")
        }
        (ChangeKind::Removed, _) => "file removed".to_string(),
        (ChangeKind::Added, _) => "new file added".to_string(),
        _ if is_test_path(&file.path) => "test coverage changed".to_string(),
        _ => "file changed".to_string(),
    }
}

fn file_risk(file: &FileChange) -> RiskLevel {
    if is_test_path(&file.path) {
        RiskLevel::Low
    } else if matches!(file.change, ChangeKind::Removed) {
        RiskLevel::High
    } else {
        RiskLevel::Medium
    }
}

fn symbol_summary(symbols: BTreeSet<String>) -> Option<String> {
    let len = symbols.len();
    let mut symbols = symbols.into_iter();
    let first = symbols.next()?;
    if len == 1 {
        Some(first)
    } else {
        Some(format!("{first} (+{} more)", len - 1))
    }
}

fn symbol_name(symbol: &ChangedSymbol) -> String {
    symbol
        .after
        .as_ref()
        .or(symbol.before.as_ref())
        .map(|summary| {
            summary
                .qualified_name
                .clone()
                .unwrap_or_else(|| summary.name.clone())
        })
        .unwrap_or_default()
}

fn symbol_path(symbol: &ChangedSymbol) -> Option<String> {
    symbol
        .after
        .as_ref()
        .or(symbol.before.as_ref())
        .map(|summary| summary.file.clone())
}

fn is_test_path(path: &str) -> bool {
    path.contains("/tests/")
        || path.ends_with("_test.rs")
        || path.ends_with("_test.ts")
        || path.ends_with("_test.py")
        || path.ends_with(".test.ts")
        || path.ends_with(".spec.ts")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiffStats, RefSummary, SymbolSummary};
    use aci_core::{Language, SymbolKind};

    #[test]
    fn agent_summary_prioritizes_public_api_and_review_focus() {
        let report = DiffReport {
            base: RefSummary {
                name: "main".to_string(),
                commit: "base".to_string(),
            },
            head: RefSummary {
                name: "feature".to_string(),
                commit: "head".to_string(),
            },
            changed_files: vec![FileChange {
                change: ChangeKind::Modified,
                path: "src/lib.ts".to_string(),
                old_path: None,
            }],
            changed_symbols: vec![ChangedSymbol {
                change: ChangeKind::Modified,
                before: None,
                after: Some(SymbolSummary {
                    name: "run".to_string(),
                    qualified_name: Some("lib.run".to_string()),
                    kind: Some(SymbolKind::Function),
                    language: Language::TypeScript,
                    file: "src/lib.ts".to_string(),
                    span: None,
                }),
                public_api: true,
                risk: RiskLevel::Medium,
                reason: "symbol body or span changed".to_string(),
            }],
            public_api_changes: Vec::new(),
            dependency_changes: Vec::new(),
            impacted_files: Vec::new(),
            diagnostics: Vec::new(),
            stats: DiffStats::default(),
        };

        let summary = summarize_for_agent(&report);

        assert_eq!(summary.risk, RiskLevel::Medium);
        assert_eq!(summary.summary.important_symbol_changes, 1);
        assert_eq!(summary.top_changes[0].symbol.as_deref(), Some("lib.run"));
        assert_eq!(summary.review_focus[0].path, "src/lib.ts");
    }
}
