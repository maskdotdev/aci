# Tree-sitter Production Plan

The scanner adapters got ACI to a working baseline. To get close to
production-ready structural data, Tree-sitter should become the primary
extractor for supported languages, with the scanner retained as a fallback and
SCIP/LSP retained for semantic enrichment.

## Target Data Quality

Tree-sitter extraction should be considered production-ready for structural
indexing when it reliably emits:

- File, module, class, function, method, field, variable, interface, import,
  export, call, and reference nodes with stable deterministic IDs.
- Parent/child scope relationships for nested declarations.
- Import/export relationships with module specifiers and local alias names.
- Best-effort call/reference facts that preserve lexical location even when
  cross-file resolution is not known.
- Diagnostics for parser errors, unsupported syntax, and fallback paths.
- Provenance and confidence on every emitted fact.

Tree-sitter does not replace semantic tooling. It should answer "what structure
is present in this file?" SCIP, LSP, compiler, or type-checker enrichment should
answer "what does this reference resolve to?"

## Architecture

Extraction should use a layered pipeline:

1. Detect language by extension, filename, and shebang.
2. Parse with Tree-sitter grammar for the language.
3. Execute versioned query files from `languages/<language>/queries/`.
4. Convert captures into the neutral graph model from `aci-core`.
5. Run language-specific resolution inside the file for scope, aliases, and
   import/export relationships.
6. If parsing or query execution fails, fall back to the existing scanner.
7. Mark facts with provenance:
   - scanner fallback: `StructuralScanner`, `Low` or `Medium`
   - Tree-sitter structural facts: `TreeSitter`, `High`
   - SCIP/LSP/compiler facts: semantic provenance with `High` or `Exact`

## Phase TS-0: Shared Tree-sitter Runtime

- Add grammar dependencies for `tree-sitter-python`,
  `tree-sitter-javascript`, and `tree-sitter-typescript`.
- Replace the current utility-only wrapper with a reusable parser pool.
- Add helpers for:
  - byte-to-line/column span conversion from Tree-sitter ranges
  - capture iteration with typed capture names
  - node text extraction without extra allocations where possible
  - query compile-time or startup validation
  - parse diagnostics and error-node reporting
- Add tests that compile every `.scm` query file at startup.

Acceptance criteria:

- Query files fail tests if captures are renamed or malformed.
- A parse failure never aborts indexing a repository.
- Parser setup is amortized across files and worker threads.

## Phase TS-1: Python Structural Extractor

- Implement `symbols.scm` for:
  - modules
  - classes
  - functions
  - async functions
  - methods
  - assignments
  - annotated assignments
  - imports and aliases
- Implement `imports.scm` for:
  - `import x`
  - `import x as y`
  - `from x import y`
  - `from x import y as z`
  - relative imports
- Implement `calls.scm` for:
  - direct calls
  - attribute calls
  - constructor-looking calls
  - decorator references
- Add resolver logic for:
  - lexical nesting
  - class/member qualification
  - imported local alias to module specifier
  - file-local definitions and references

Acceptance criteria:

- Fixtures cover decorators, async functions, nested functions, class methods,
  assignments, annotations, aliases, relative imports, syntax errors, and large
  files.
- Extracted symbol counts and qualified names are asserted in tests.
- Existing scanner output is only used when the Tree-sitter path fails or is
  explicitly disabled.

## Phase TS-2: JavaScript and TypeScript Structural Extractor

- Add separate grammar support for JavaScript, TypeScript, TSX, and JSX.
- Implement `symbols.scm` for:
  - functions
  - arrow functions
  - classes
  - methods
  - fields
  - interfaces
  - type aliases
  - enums
  - variables
  - default exports
- Implement `imports.scm` for:
  - static imports
  - type-only imports
  - namespace imports
  - default imports
  - named imports and aliases
  - re-exports
  - dynamic import specifiers when literal
  - CommonJS `require`
- Implement `calls.scm` for:
  - function calls
  - method calls
  - constructor calls
  - JSX component references
  - decorator references
- Add resolver logic for:
  - lexical nesting
  - class/member qualification
  - import aliases
  - export aliases
  - local definition/reference binding where unambiguous

Acceptance criteria:

- Fixtures cover ESM, CommonJS, TS types, interfaces, classes, methods, React
  components, JSX/TSX, re-exports, dynamic imports, syntax errors, and large
  files.
- TypeScript-specific syntax does not degrade JavaScript extraction.
- Tree-sitter extraction remains deterministic across runs.

## Phase TS-3: Graph Fidelity And Query Semantics

- Add graph invariants:
  - every symbol has a file
  - every span belongs to its source file
  - every `defines`, `imports`, `exports`, `calls`, and `references` edge points
    to an existing node or an explicit external node
  - qualified names are stable across repeated indexes
- Add golden graph fixtures for Python and TypeScript.
- Add JSONL snapshot tests to catch accidental graph shape changes.
- Extend queries to distinguish:
  - definitions
  - declarations
  - imports
  - exports
  - unresolved external references
  - file-local references

Acceptance criteria:

- Golden fixture diffs are readable and intentional.
- Structural graph facts can be exported/imported without loss.
- Query results prefer higher-provenance facts when enrichment is present.

## Phase TS-4: Performance Guardrails

- Add benchmark variants:
  - scanner-only
  - Tree-sitter-only
  - Tree-sitter with scanner fallback
  - Tree-sitter plus SCIP/LSP enrichment
- Track:
  - parse time per file
  - extraction time per file
  - allocations per file where practical
  - cold index throughput
  - single-file incremental update latency
  - memory RSS at 100k-file scale
- Add limits:
  - per-file parser timeout or byte-size guardrail
  - fallback for very large generated-looking files
  - query capture count guardrail to avoid pathological files

Acceptance criteria:

- 100k-file cold structural indexing remains under the existing 60 second
  budget on the benchmark fixture.
- Single-file structural updates remain under 250 ms.
- Memory remains under the existing 1-2 GB budget.
- Tree-sitter is allowed to be slower than the scanner, but not unbounded.

## Phase TS-5: Semantic Enrichment Boundary

- Keep Tree-sitter as structural truth for source shape.
- Keep SCIP/LSP/compiler facts as semantic truth for cross-file references.
- Add merge tests where Tree-sitter and SCIP disagree.
- Add query behavior tests showing that higher-confidence semantic facts win
  without deleting lower-confidence structural facts.

Acceptance criteria:

- A reference can retain its Tree-sitter lexical location while resolving to a
  SCIP/LSP target.
- Conflict resolution is deterministic and tested.
- Enrichment can be disabled without breaking structural indexing.

## Rollout Strategy

1. Land shared runtime and query compilation tests.
2. Ship Python Tree-sitter extraction behind a feature flag or runtime option.
3. Compare Python scanner vs Tree-sitter fixture output.
4. Make Python Tree-sitter the default when accuracy and performance gates pass.
5. Repeat for JavaScript/TypeScript.
6. Remove scanner as the default path, but keep it as fallback.

## Non-goals

- Full type checking inside ACI.
- Package-manager-specific module resolution in the Tree-sitter layer.
- Replacing SCIP/LSP for exact cross-file semantic references.
- Perfect extraction from generated, minified, or intentionally invalid source.
