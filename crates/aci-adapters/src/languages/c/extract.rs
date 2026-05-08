use crate::languages::brace::{self, BraceLanguage};
use crate::tree_sitter::{ParserPool, c_language};
use aci_core::{GraphPartition, SourceFile};
use std::sync::OnceLock;

static C_POOL: OnceLock<ParserPool> = OnceLock::new();

const C_CONFIG: BraceLanguage = BraceLanguage {
    name: "C",
    scope_separator: "::",
    module_fallback: "module",
    imports: &["preproc_include"],
    functions: &["function_definition"],
    methods: &[],
    classes: &["struct_specifier"],
    interfaces: &[],
    enums: &["enum_specifier"],
    type_aliases: &["type_definition"],
    variables: &["init_declarator"],
    scopes: &[],
    calls: &["call_expression"],
};

pub fn extract_c(file: &SourceFile) -> GraphPartition {
    let pool = C_POOL.get_or_init(|| ParserPool::new(c_language()));
    brace::extract(file, pool, &C_CONFIG)
}
