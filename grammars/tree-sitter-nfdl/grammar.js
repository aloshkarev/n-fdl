/**
 * tree-sitter-nfdl — IDE-track grammar for N-FDL.
 *
 * Dual-track (ADR-013): this CST MUST NOT feed verify/runtime.
 * Scaffold covers surface syntax for docs/examples (ARP minimum bar).
 */

/// <reference types="tree-sitter-cli/dsl" />

module.exports = grammar({
  name: 'nfdl',

  extras: $ => [
    /\s/,
    $.line_comment,
    $.block_comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    // `key = (expr)` : outer key tuple vs parenthesized primary expression
    [$.key_component, $._primary],
  ],

  rules: {
    source_file: $ => repeat($.protocol_decl),

    line_comment: _ => token(seq('//', /[^\n]*/)),
    block_comment: _ => token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),

    // ----- top -----
    protocol_decl: $ => seq(
      'protocol',
      field('name', $.identifier),
      '{',
      optional($.meta_decl),
      repeat($._top_item),
      '}',
    ),

    _top_item: $ => choice(
      $.message_decl,
      $.session_decl,
      $.bind_decl,
    ),

    // ----- meta -----
    meta_decl: $ => seq('meta', '{', repeat($.meta_entry), '}'),

    meta_entry: $ => choice(
      seq('endian', '=', field('value', choice('big', 'little')), ';'),
      seq('mode', '=', field('value', choice('stream', 'datagram')), ';'),
      seq('eof', '=', field('value', $.eof_source), ';'),
    ),

    eof_source: $ => choice(
      'on_fin',
      'on_close',
      seq('by_plugin', '(', $.string, ')'),
    ),

    // ----- messages -----
    message_decl: $ => seq(
      'message',
      field('name', $.identifier),
      '{',
      repeat($._statement),
      '}',
    ),

    _statement: $ => choice(
      $.field_stmt,
      $.let_stmt,
      $.validate_stmt,
      $.match_stmt,
      $.loop_stmt,
    ),

    field_stmt: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $._type),
      optional(seq('if', field('condition', $.expr))),
      ';',
    ),

    let_stmt: $ => seq(
      'let',
      field('name', $.identifier),
      '=',
      field('value', $.expr),
      ';',
    ),

    validate_stmt: $ => seq(
      'validate',
      field('predicate', $.expr),
      '->',
      field('message', $.string),
      ';',
    ),

    match_stmt: $ => seq(
      'match',
      field('scrutinee', $.expr),
      '{',
      repeat1($.match_arm),
      optional($.default_arm),
      '}',
    ),

    match_arm: $ => seq(
      'case',
      field('pattern', $.expr),
      '=>',
      '{',
      repeat($._statement),
      '}',
    ),

    default_arm: $ => seq(
      'default',
      '=>',
      '{',
      repeat($._statement),
      '}',
    ),

    loop_stmt: $ => seq(
      'loop',
      field('name', $.identifier),
      repeat($.carry_decl),
      'while',
      field('condition', $.expr),
      '{',
      repeat($._statement),
      repeat($.next_stmt),
      '}',
    ),

    carry_decl: $ => seq(
      'carry',
      field('name', $.identifier),
      ':',
      field('type', $._type),
      '=',
      field('init', $.expr),
    ),

    next_stmt: $ => seq(
      'next',
      field('name', $.identifier),
      '=',
      field('value', $.expr),
      ';',
    ),

    // ----- types -----
    _type: $ => choice(
      $.scalar_type,
      $.bitfield_type,
      $.bytes_type,
      $.invoke_type,
      $.identifier,
    ),

    scalar_type: $ => choice(
      'u8', 'u16', 'u24', 'u32', 'u48', 'u64',
      'i8', 'i16', 'i32', 'i64',
      'u16le', 'u24le', 'u32le', 'u48le', 'u64le',
      'u16be', 'u24be', 'u32be', 'u48be', 'u64be',
      'i16le', 'i32le', 'i64le',
      'i16be', 'i32be', 'i64be',
      $.endian_scalar_type,
      'bool',
      'str',
      'opaque',
    ),

    endian_scalar_type: $ => seq(
      choice('u16', 'u24', 'u32', 'u48', 'u64', 'i16', 'i32', 'i64'),
      '(',
      'endian',
      '=',
      $.expr,
      ')',
    ),

    bitfield_type: $ => seq('bitfield', '{', $.integer, '}'),

    bytes_type: $ => seq('bytes', '[', $._bytes_len, ']'),

    _bytes_len: $ => choice(
      $.expr,
      '..',
      'EOF',
      'stream',
    ),

    invoke_type: $ => seq(
      'invoke',
      '(',
      $.string,
      repeat(seq(',', $.expr)),
      ')',
    ),

    // ----- sessions / EFSM -----
    session_decl: $ => seq(
      'state_machine',
      field('name', $.identifier),
      '{',
      'key',
      '=',
      field('key', $.key_expr),
      ';',
      repeat1($.state_decl),
      '}',
    ),

    key_expr: $ => choice(
      seq('(', $.key_component, repeat(seq(',', $.key_component)), ')'),
      $.key_component,
    ),

    key_component: $ => choice(
      seq('bidir', '(', $.expr, ',', $.expr, ')'),
      seq('bidir_tuple', '(', $.endpoint, ',', $.endpoint, ')'),
      $.expr,
    ),

    endpoint: $ => seq('(', $.expr, ',', $.expr, repeat(seq(',', $.expr)), ')'),

    state_decl: $ => seq(
      'state',
      field('name', $.identifier),
      '{',
      repeat($.transition),
      '}',
    ),

    transition: $ => seq(
      'on',
      field('event', $.identifier),
      optional(seq('guard', field('guard', $.expr))),
      '->',
      field('target', $.identifier),
      '{',
      repeat($.action),
      '}',
      ';',
    ),

    action: $ => choice(
      seq('emit', $.identifier, ';'),
      seq('set', $.identifier, '=', $.expr, ';'),
      seq('start_timer', '(', $.identifier, ',', $.expr, ')', ';'),
      seq('cancel_timer', '(', $.identifier, ')', ';'),
    ),

    // ----- bind -----
    bind_decl: $ => seq(
      'bind',
      field('outer', $.identifier),
      'payload',
      'to',
      field('inner', $.identifier),
      'when',
      field('predicate', $.expr),
      ';',
    ),

    // ----- expressions (precedence nesting) -----
    expr: $ => $.ternary,

    ternary: $ => choice(
      prec.right(seq(
        field('condition', $.coalesce),
        '?',
        field('consequence', $.expr),
        ':',
        field('alternative', $.expr),
      )),
      $.coalesce,
    ),

    coalesce: $ => prec.left(seq(
      $.logic_or,
      repeat(seq('??', $.logic_or)),
    )),

    logic_or: $ => prec.left(seq(
      $.logic_and,
      repeat(seq('||', $.logic_and)),
    )),

    logic_and: $ => prec.left(seq(
      $.equality,
      repeat(seq('&&', $.equality)),
    )),

    equality: $ => prec.left(seq(
      $.bit_or,
      repeat(seq(choice('==', '!='), $.bit_or)),
    )),

    bit_or: $ => prec.left(seq(
      $.bit_xor,
      repeat(seq('|', $.bit_xor)),
    )),

    bit_xor: $ => prec.left(seq(
      $.bit_and,
      repeat(seq('^', $.bit_and)),
    )),

    bit_and: $ => prec.left(seq(
      $.relational,
      repeat(seq('&', $.relational)),
    )),

    relational: $ => prec.left(seq(
      $.shift,
      repeat(seq(choice('<', '<=', '>', '>='), $.shift)),
    )),

    shift: $ => prec.left(seq(
      $.additive,
      repeat(seq(choice('<<', '>>'), $.additive)),
    )),

    additive: $ => prec.left(seq(
      $.multiplicative,
      repeat(seq(choice('+', '-'), $.multiplicative)),
    )),

    multiplicative: $ => prec.left(seq(
      $.unary,
      repeat(seq(choice('*', '/', '%'), $.unary)),
    )),

    unary: $ => choice(
      seq(choice('!', '~', '-'), $.unary),
      $.postfix,
    ),

    postfix: $ => prec.left(seq(
      $._primary,
      repeat(seq('.', field('member', $.identifier))),
    )),

    _primary: $ => choice(
      $.integer,
      $.boolean,
      $.string,
      $.builtin,
      $.invoke_expr,
      $.identifier,
      seq('(', $.expr, ')'),
    ),

    invoke_expr: $ => seq(
      'invoke',
      '(',
      $.string,
      repeat(seq(',', $.expr)),
      ')',
    ),

    builtin: $ => choice(
      '__root_buffer',
      '__current_offset',
      '__root_offset',
      '__rem',
      '__count',
      $.session_projection,
    ),

    session_projection: $ => seq('__session', '(', $.string, ')'),

    // ----- literals / idents -----
    boolean: _ => choice('true', 'false'),

    integer: _ => token(choice(
      /0[xX][0-9a-fA-F]([0-9a-fA-F_]*[0-9a-fA-F])?/,
      /0[bB][01]([01_]*[01])?/,
      /[0-9]([0-9_]*[0-9])?/,
    )),

    // Plain double-quoted strings (escapes uncommon in examples; IDE scaffold).
    string: _ => token(seq('"', /[^"]*/, '"')),

    // Keywords are listed as anonymous strings elsewhere; word: identifier
    // lets tree-sitter treat them as reserved vs bare idents.
    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,
  },
});
