module.exports = grammar({
  name: 'twee3',

  extras: $ => [
    /\s+/
  ],

  conflicts: $ => [
    [$.passage_header]
  ],

  rules: {
    source_file: $ => repeat($.passage),

    passage: $ => seq(
      $.passage_header,
      optional($.passage_content)
    ),

    passage_header: $ => seq(
      '::',
      $.passage_name,
      optional($.tags),
      optional($.metadata)
    ),

    passage_name: $ => /[^\n\[\{]+/,

    tags: $ => seq(
      '[',
      optional($.tag_list),
      ']'
    ),

    tag_list: $ => repeat1(choice(
      $.tag,
      /\s+/
    )),
    
    tag: $ => /[^\s\]]+/,

    metadata: $ => seq(
      '{',
      $.json_metadata,
      '}'
    ),

    json_metadata: $ => /[^\}]+/,

    passage_content: $ => repeat1(choice(
      $.text,
      $.macro,
      $.link
    )),

    text: $ => choice(
      /[^<\[]+/,
      '<',
      '['
    ),

    macro: $ => seq(
      '<<',
      $.macro_name,
      repeat($.macro_arg),
      '>>'
    ),

    macro_name: $ => /[a-zA-Z0-9_\-\/]+/,

    macro_arg: $ => choice(
      $.string,
      $.variable,
      $.attribute,
      $.number,
      $.boolean,
      '=',
      ',',
      '[',
      ']',
      '{',
      '}',
      ':',
      /[^\s"'=,\[\]{}<>:]+/
    ),

    string: $ => choice(
      seq('"', /[^"]*/, '"'),
      seq("'", /[^']*/, "'")
    ),

    variable: $ => /\$[a-zA-Z0-9_]+/,

    attribute: $ => seq(
      $.attribute_name,
      '='
    ),

    attribute_name: $ => /[a-zA-Z0-9_\-]+/,

    number: $ => /\d+(?:\.\d+)?/,

    boolean: $ => choice('true', 'false'),

    link: $ => seq(
      '[[',
      $.link_text,
      ']]'
    ),

    link_text: $ => /(?:[^\]]|\][^\]])+/
  }
});
