module.exports = grammar({
  name: 'twee3',

  extras: $ => [
    /[ \t\r]+/
  ],

  conflicts: $ => [
    [$.passage_header],
    [$._link_simple, $._link_piped, $._link_right, $._link_left],
    [$.link_text],
    [$.image_source, $._image_linked, $._image_simple],
    [$.image_source],
    [$.variable]
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
      $.macro_close,
      $.link,
      $.image,
      $.variable,
      $.text_formatting
    )),

    text: $ => choice(
      /[^<\[:{\n$'_/=^~]+/,
      ':',
      '<',
      '[',
      '{',
      '\n',
      '$',
      "'",
      '_',
      '/',
      '=',
      '^',
      '~'
    ),

    macro: $ => seq(
      '<<',
      $.macro_name,
      repeat($.macro_arg),
      '>>'
    ),

    macro_close: $ => seq(
      '<</',
      $.macro_name,
      '>>'
    ),

    macro_name: $ => /[a-zA-Z][a-zA-Z0-9_\-]*/,

    macro_arg: $ => choice(
      $.string,
      $.variable,
      $.number,
      $.boolean,
      $.keyword_operator,
      $.operator,
      $.bracket,
      /[^\s"'=,\[\]{}<>:a-zA-Z0-9_]+/
    ),

    keyword_operator: $ => choice(
      'to', 'eq', 'neq', 'is', 'not', 'and', 'or', 'lt', 'lte', 'gt', 'gte', 'def', 'ndef'
    ),

    operator: $ => choice(
      '=', '==', '===', '!=', '!==', '>', '>=', '<', '<=', '+', '-', '*', '/', '%', '+=', '-=', '*=', '/=', '%=', '!', '&&', '||', '?', ':'
    ),

    bracket: $ => choice(
      '(', ')', '[', ']', '{', '}'
    ),

    string: $ => choice(
      seq('"', /[^"]*/, '"'),
      seq("'", /[^']*/, "'"),
      seq("`", /[^`]*/, "`")
    ),

    variable: $ => choice(
      seq('$', $.identifier, optional($._property_access)),
      seq('_', $.identifier, optional($._property_access))
    ),

    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    _property_access: $ => repeat1(choice(
      seq('.', $.identifier),
      seq('[', choice($.string, $.number, $.variable), ']')
    )),

    number: $ => /\d+(?:\.\d+)?/,

    boolean: $ => choice('true', 'false'),

    link: $ => seq(
      '[[',
      choice(
        $._link_right,
        $._link_left,
        $._link_piped,
        $._link_simple
      ),
      optional($.link_setter),
      ']]'
    ),

    _link_simple: $ => field('passage', $.link_text),
    
    _link_piped: $ => seq(
      field('text', $.link_text),
      '|',
      field('passage', $.link_text)
    ),

    _link_right: $ => seq(
      field('text', $.link_text),
      '->',
      field('passage', $.link_text)
    ),

    _link_left: $ => seq(
      field('passage', $.link_text),
      '<-',
      field('text', $.link_text)
    ),

    link_text: $ => repeat1(choice(
      /[^\]\[\|<\-]+/,
      '<',
      '-',
      '['
    )),

    link_setter: $ => seq(
      '][',
      repeat($.macro_arg)
    ),

    image: $ => seq(
      '[img[',
      choice(
        $._image_linked,
        $._image_simple
      ),
      ']]'
    ),

    _image_simple: $ => $.image_source,

    _image_linked: $ => seq(
      $.image_source,
      '[',
      field('link', $.link_text),
      ']'
    ),

    image_source: $ => repeat1(choice(
      /[^\]\[\|]+/,
      '|',
      '[',
      ']'
    )),

    text_formatting: $ => choice(
      seq("''", repeat(choice($.text, $.variable)), "''"),
      seq("//", repeat(choice($.text, $.variable)), "//"),
      seq("__", repeat(choice($.text, $.variable)), "__"),
      seq("==", repeat(choice($.text, $.variable)), "=="),
      seq("^^", repeat(choice($.text, $.variable)), "^^"),
      seq("~~", repeat(choice($.text, $.variable)), "~~")
    )
  }
});
