use aci_diff::{
    ChangeKind, ChangedSymbol, DependencyChange, DiffOptions, DiffReport, FileChange, ImpactedFile,
    SymbolSummary, diff_refs,
};
use anyhow::Result;
use std::io::Write;
use std::time::Instant;

use crate::args::{DiffArgs, QueryFormat};
use crate::output::{Output, TableStyle, format_duration, format_location, print_table};

pub fn run_diff(args: DiffArgs) -> Result<()> {
    let started = Instant::now();
    let color = args.color.enabled();
    let out = Output::new(color);
    let workers = args.workers.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
    });
    let options = DiffOptions::new(args.base, args.head)
        .with_repo_root(args.repo)
        .with_workers(workers);
    let report = diff_refs(options)?;
    match args.format {
        QueryFormat::Json => {
            if args.pretty {
                serde_json::to_writer_pretty(std::io::stdout(), &report)?;
                println!();
            } else {
                serde_json::to_writer(std::io::stdout(), &report)?;
                println!();
            }
        }
        QueryFormat::Text => print_text_report(&report, args.pretty, color),
    }
    std::io::stdout().flush()?;
    let timing = format!("diff completed in {}", format_duration(started.elapsed()));
    if color {
        eprintln!("{}", out.dim(&timing));
    } else {
        eprintln!("{timing}");
    }
    Ok(())
}

fn print_text_report(report: &DiffReport, pretty: bool, color: bool) {
    println!(
        "{} {}..{}",
        summary_line(report),
        short_commit(&report.base.commit),
        short_commit(&report.head.commit)
    );
    println!();
    if pretty {
        print_pretty(report, color);
    } else {
        print_plain(report);
    }
}

fn summary_line(report: &DiffReport) -> String {
    format!(
        "{} files, {} symbols, {} public API, {} dependencies, {} impacted",
        report.changed_files.len(),
        report.changed_symbols.len(),
        report.public_api_changes.len(),
        report.dependency_changes.len(),
        report.impacted_files.len()
    )
}

fn print_plain(report: &DiffReport) {
    print_section("Changed files", report.changed_files.iter(), file_line);
    print_section(
        "Changed symbols",
        report.changed_symbols.iter(),
        symbol_line,
    );
    print_section(
        "Public API changes",
        report.public_api_changes.iter(),
        symbol_line,
    );
    print_section(
        "Dependency changes",
        report.dependency_changes.iter(),
        dependency_line,
    );
    print_section("Impacted files", report.impacted_files.iter(), impact_line);
    if !report.diagnostics.is_empty() {
        println!("Diagnostics");
        for diagnostic in &report.diagnostics {
            let file = diagnostic.file.as_deref().unwrap_or("<unknown>");
            println!(
                "{}\t{:?}\t{}\t{}",
                side_label(diagnostic.reference),
                diagnostic.severity,
                file,
                diagnostic.message
            );
        }
    }
}

fn print_pretty(report: &DiffReport, color: bool) {
    let style = TableStyle::new(color);
    print_table(
        &["Change", "Path", "Previous"],
        report.changed_files.iter().map(|file| {
            vec![
                change_label(file.change).to_string(),
                file.path.clone(),
                file.old_path.clone().unwrap_or_default(),
            ]
        }),
        style,
    );
    print_table(
        &["Change", "Symbol", "Kind", "Location", "Risk"],
        report.changed_symbols.iter().map(symbol_row),
        style,
    );
    print_table(
        &["Change", "File", "Dependency", "Kind"],
        report.dependency_changes.iter().map(|dependency| {
            vec![
                change_label(dependency.change).to_string(),
                dependency.file.clone(),
                dependency.dependency.clone(),
                format!("{:?}", dependency.edge_kind),
            ]
        }),
        style,
    );
    print_table(
        &["Impacted file", "Reason"],
        report
            .impacted_files
            .iter()
            .map(|impact| vec![impact.path.clone(), impact.reasons.join(", ")]),
        style,
    );
}

fn print_section<'a, T, I, F>(title: &str, values: I, line: F)
where
    I: IntoIterator<Item = &'a T>,
    T: 'a,
    F: Fn(&T) -> String,
{
    let rows = values.into_iter().map(line).collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }
    println!("{title}");
    for row in rows {
        println!("{row}");
    }
    println!();
}

fn file_line(file: &FileChange) -> String {
    match &file.old_path {
        Some(old_path) => format!("{}\t{}\t{}", change_label(file.change), file.path, old_path),
        None => format!("{}\t{}", change_label(file.change), file.path),
    }
}

fn symbol_line(symbol: &ChangedSymbol) -> String {
    let display = symbol.after.as_ref().or(symbol.before.as_ref());
    let location = display
        .and_then(|summary| format_location(Some(summary.file.as_ref()), summary.span.as_ref()))
        .unwrap_or_default();
    let name = display.map(symbol_name).unwrap_or_default();
    format!(
        "{}\t{}\t{}\t{:?}\t{}",
        change_label(symbol.change),
        name,
        location,
        symbol.risk,
        symbol.reason
    )
}

fn dependency_line(dependency: &DependencyChange) -> String {
    format!(
        "{}\t{}\t{}\t{:?}",
        change_label(dependency.change),
        dependency.file,
        dependency.dependency,
        dependency.edge_kind
    )
}

fn impact_line(impact: &ImpactedFile) -> String {
    format!("{}\t{}", impact.path, impact.reasons.join(", "))
}

fn symbol_row(symbol: &ChangedSymbol) -> Vec<String> {
    let display = symbol.after.as_ref().or(symbol.before.as_ref());
    let location = display
        .and_then(|summary| format_location(Some(summary.file.as_ref()), summary.span.as_ref()))
        .unwrap_or_default();
    vec![
        change_label(symbol.change).to_string(),
        display.map(symbol_name).unwrap_or_default(),
        display
            .and_then(|summary| summary.kind)
            .map(|kind| format!("{kind:?}"))
            .unwrap_or_default(),
        location,
        format!("{:?}", symbol.risk),
    ]
}

fn symbol_name(symbol: &SymbolSummary) -> String {
    symbol
        .qualified_name
        .clone()
        .unwrap_or_else(|| symbol.name.clone())
}

fn change_label(change: ChangeKind) -> &'static str {
    match change {
        ChangeKind::Added => "added",
        ChangeKind::Removed => "removed",
        ChangeKind::Modified => "modified",
        ChangeKind::Renamed => "renamed",
        ChangeKind::TypeChanged => "type-changed",
        ChangeKind::Copied => "copied",
    }
}

fn side_label(side: aci_diff::RefSide) -> &'static str {
    match side {
        aci_diff::RefSide::Base => "base",
        aci_diff::RefSide::Head => "head",
    }
}

fn short_commit(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}
