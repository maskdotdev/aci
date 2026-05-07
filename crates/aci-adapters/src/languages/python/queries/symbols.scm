(module) @definition.module

(class_definition
  name: (identifier) @definition.class)

(function_definition
  name: (identifier) @definition.function)

(assignment
  left: (identifier) @definition.variable)

(assignment
  left: (pattern_list
    (identifier) @definition.variable))
