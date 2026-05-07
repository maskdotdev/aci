(call_expression
  function: (identifier) @reference.call)

(call_expression
  function: (member_expression
    property: (property_identifier) @reference.call))

(new_expression
  constructor: (identifier) @reference.call)

(decorator) @reference.decorator
