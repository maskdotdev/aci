use crate::languages::brace::{self, BraceLanguage};
use crate::tree_sitter::{ParserPool, java_language};
use aci_core::{GraphPartition, SourceFile};
use std::sync::OnceLock;

static JAVA_POOL: OnceLock<ParserPool> = OnceLock::new();

const JAVA_CONFIG: BraceLanguage = BraceLanguage {
    name: "Java",
    scope_separator: ".",
    module_fallback: "module",
    imports: &["import_declaration"],
    functions: &[],
    methods: &["method_declaration", "constructor_declaration"],
    classes: &["class_declaration", "record_declaration"],
    interfaces: &["interface_declaration", "annotation_type_declaration"],
    enums: &["enum_declaration"],
    type_aliases: &[],
    variables: &["variable_declarator", "field_declaration"],
    scopes: &["package_declaration"],
    calls: &["method_invocation", "object_creation_expression"],
};

pub fn extract_java(file: &SourceFile) -> GraphPartition {
    let pool = JAVA_POOL.get_or_init(|| ParserPool::new(java_language()));
    brace::extract(file, pool, &JAVA_CONFIG)
}
