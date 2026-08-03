module.exports = grammar({
  name: 'twee3',

  extras: $ => [
    /[ \t\r]+/
  ],

  conflicts: $ => [
    [$.passage_header]
  ],

  rules: {
    source_file: $ => seq(
      optional($._leading_newlines),
      repeat($.passage)
    ),

    _leading_newlines: $ => repeat1('\n'),

    passage: $ => seq(
      $.passage_header,
      optional($.passage_content)
    ),

    passage_header: $ => prec(1, seq(
      '::',
      $.passage_name,
      optional($.tags),
      optional($.metadata)
    )),

    passage_name: $ => seq(/(?:[^\[\{\n\\]|\\.)+/, /[ \t]*/),

    tags: $ => prec.dynamic(1, seq(
      '[',
      optional($.tag_list),
      ']',
      /[ \t]*/
    )),

    tag_list: $ => repeat1(choice(
      $.tag,
      /\s+/
    )),
    
    tag: $ => /(?:[^\]\s\\]|\\.)+/,

    metadata: $ => prec.dynamic(1, seq(
      '{',
      $.json_metadata,
      '}'
    )),

    json_metadata: $ => repeat1(choice(
      /"(?:[^"\\]|\\.)*"/,
      /[^"}]+/
    )),

    passage_content: $ => repeat1(choice(
      $.text,
      $.macro,
      $.link
    )),

    text: $ => choice(
      /[^<\[:{\n]+/,
      ':',
      '<',
      '[',
      '{',
      '\n'
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
