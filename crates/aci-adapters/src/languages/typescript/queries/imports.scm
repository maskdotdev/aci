(import_statement
  source: (string) @import.module)

(import_specifier
  name: (identifier) @import.name)

(import_specifier
  alias: (identifier) @import.alias)

(namespace_import
  (identifier) @import.alias)

(export_statement
  source: (string) @export.module)

(call_expression
  function: (import)
  arguments: (arguments
    (string) @import.module))
