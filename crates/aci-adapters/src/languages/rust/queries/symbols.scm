(function_item
  name: (identifier) @definition.function)

(struct_item
  name: (type_identifier) @definition.struct)

(enum_item
  name: (type_identifier) @definition.enum)

(trait_item
  name: (type_identifier) @definition.trait)

(type_item
  name: (type_identifier) @definition.type)

(let_declaration
  pattern: (identifier) @definition.variable)
