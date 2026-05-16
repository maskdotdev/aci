use crate::ExtractionOptions;
use crate::languages::brace::{self, BraceLanguage};
use crate::tree_sitter::{ParserPool, objective_c_language};
use aci_core::{GraphPartition, SourceFile};
use std::sync::OnceLock;

static OBJECTIVE_C_POOL: OnceLock<ParserPool> = OnceLock::new();

const OBJECTIVE_C_CONFIG: BraceLanguage = BraceLanguage {
    name: "Objective-C",
    scope_separator: ".",
    module_fallback: "module",
    imports: &["preproc_include", "import_declaration"],
    functions: &["function_definition"],
    methods: &["method_declaration", "method_definition"],
    classes: &[
        "class_interface",
        "implementation_definition",
        "class_declaration",
    ],
    interfaces: &["protocol_declaration"],
    enums: &["enum_specifier"],
    type_aliases: &["type_definition"],
    variables: &["init_declarator"],
    scopes: &[],
    calls: &["call_expression", "message_expression"],
};

pub fn extract_objective_c(file: &SourceFile) -> GraphPartition {
    extract_objective_c_with_options(file, ExtractionOptions::default())
}

pub fn extract_objective_c_with_options(
    file: &SourceFile,
    options: ExtractionOptions,
) -> GraphPartition {
    let pool = OBJECTIVE_C_POOL.get_or_init(|| ParserPool::new(objective_c_language()));
    brace::extract_with_options(file, pool, &OBJECTIVE_C_CONFIG, options)
}
