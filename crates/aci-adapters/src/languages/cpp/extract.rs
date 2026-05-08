use crate::languages::brace::{self, BraceLanguage};
use crate::tree_sitter::{ParserPool, cpp_language};
use aci_core::{GraphPartition, SourceFile};
use std::sync::OnceLock;

static CPP_POOL: OnceLock<ParserPool> = OnceLock::new();

const CPP_CONFIG: BraceLanguage = BraceLanguage {
    name: "C++",
    scope_separator: "::",
    module_fallback: "module",
    imports: &["preproc_include"],
    functions: &["function_definition"],
    methods: &[],
    classes: &["class_specifier", "struct_specifier"],
    interfaces: &[],
    enums: &["enum_specifier"],
    type_aliases: &["type_definition", "alias_declaration"],
    variables: &["init_declarator"],
    scopes: &["namespace_definition"],
    calls: &["call_expression"],
};

pub fn extract_cpp(file: &SourceFile) -> GraphPartition {
    let pool = CPP_POOL.get_or_init(|| ParserPool::new(cpp_language()));
    brace::extract(file, pool, &CPP_CONFIG)
}
