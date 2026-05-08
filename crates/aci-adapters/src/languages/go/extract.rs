use crate::languages::brace::{self, BraceLanguage};
use crate::tree_sitter::{ParserPool, go_language};
use aci_core::{GraphPartition, SourceFile};
use std::sync::OnceLock;

static GO_POOL: OnceLock<ParserPool> = OnceLock::new();

const GO_CONFIG: BraceLanguage = BraceLanguage {
    name: "Go",
    scope_separator: ".",
    module_fallback: "package",
    imports: &["import_declaration", "import_spec"],
    functions: &["function_declaration"],
    methods: &["method_declaration"],
    classes: &[],
    interfaces: &[],
    enums: &[],
    type_aliases: &["type_spec"],
    variables: &["var_spec", "const_spec", "short_var_declaration"],
    scopes: &["package_clause"],
    calls: &["call_expression"],
};

pub fn extract_go(file: &SourceFile) -> GraphPartition {
    let pool = GO_POOL.get_or_init(|| ParserPool::new(go_language()));
    brace::extract(file, pool, &GO_CONFIG)
}
