use crate::{ChangeKind, RefSide, RiskLevel};
use aci_core::{EdgeKind, SymbolKind};

pub(crate) fn change_label(change: ChangeKind) -> &'static str {
    match change {
        ChangeKind::Added => "added",
        ChangeKind::Removed => "removed",
        ChangeKind::Modified => "modified",
        ChangeKind::Renamed => "renamed",
        ChangeKind::TypeChanged => "type-changed",
        ChangeKind::Copied => "copied",
    }
}

pub fn edge_kind_label(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Contains => "contains",
        EdgeKind::Defines => "defines",
        EdgeKind::Imports => "imports",
        EdgeKind::Exports => "exports",
        EdgeKind::Calls => "calls",
        EdgeKind::References => "references",
        EdgeKind::Extends => "extends",
        EdgeKind::Implements => "implements",
        EdgeKind::Overrides => "overrides",
        EdgeKind::DependsOn => "depends-on",
        EdgeKind::Tests => "tests",
    }
}

pub(crate) fn edge_impact_label(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Imports => "imports",
        EdgeKind::DependsOn => "depends on",
        EdgeKind::Calls => "calls",
        EdgeKind::References => "references",
        _ => "uses",
    }
}

pub fn risk_label(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
    }
}

pub fn ref_side_label(side: RefSide) -> &'static str {
    match side {
        RefSide::Base => "base",
        RefSide::Head => "head",
    }
}

pub fn symbol_kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Class => "class",
        SymbolKind::Interface => "interface",
        SymbolKind::TypeAlias => "type-alias",
        SymbolKind::Enum => "enum",
        SymbolKind::Variable => "variable",
        SymbolKind::Module => "module",
        SymbolKind::Field => "field",
        SymbolKind::Unknown => "unknown",
    }
}
