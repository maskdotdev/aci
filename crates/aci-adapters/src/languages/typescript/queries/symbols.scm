(function_declaration
  name: (identifier) @definition.function)

(generator_function_declaration
  name: (identifier) @definition.function)

(class_declaration
  name: (type_identifier) @definition.class)

(method_definition
  name: (property_identifier) @definition.method)

(public_field_definition
  name: (property_identifier) @definition.field)

(interface_declaration
  name: (type_identifier) @definition.interface)

(type_alias_declaration
  name: (type_identifier) @definition.type)

(enum_declaration
  name: (identifier) @definition.enum)

(variable_declarator
  name: (identifier) @definition.variable)
