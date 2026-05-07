(call_expression
  function: (identifier) @reference.call)

(call_expression
  function: (field_expression
    field: (field_identifier) @reference.call))

(macro_invocation
  macro: (identifier) @reference.macro)
