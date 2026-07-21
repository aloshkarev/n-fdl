/**
 * tree-sitter-adgl — IDE-track grammar for ADGL.
 *
 * Dual-track (ADR-013): this CST MUST NOT feed verify/runtime.
 * Scaffold covers surface syntax for docs/idea/examples (PMTUD minimum bar).
 */

/// <reference types="tree-sitter-cli/dsl" />

module.exports = grammar({
  name: 'adgl',

  extras: $ => [
    /\s/,
    $.line_comment,
    $.block_comment,
  ],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($.ruleset_decl),

    line_comment: _ => token(seq('//', /[^\n]*/)),
    block_comment: _ => token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),

    // ----- top -----
    ruleset_decl: $ => seq(
      'ruleset',
      field('name', $.string),
      '{',
      $.ruleset_header,
      repeat($.rule),
      '}',
    ),

    ruleset_header: $ => seq(
      $.version_decl,
      repeat($.header_decl),
    ),

    version_decl: $ => seq('version', '=', field('value', $.string)),

    header_decl: $ => choice(
      $.requires_decl,
      $.mutually_exclusive_decl,
    ),

    requires_decl: $ => seq(
      'requires',
      '=',
      '[',
      $.string,
      repeat(seq(',', $.string)),
      ']',
    ),

    mutually_exclusive_decl: $ => seq(
      'mutually_exclusive',
      '(',
      $.ident_list,
      ')',
    ),

    ident_list: $ => seq($.identifier, repeat(seq(',', $.identifier))),

    rule: $ => choice($.evidence_rule, $.decision_rule),

    // ----- evidence -----
    evidence_rule: $ => seq(
      'evidence',
      field('name', $.identifier),
      '{',
      'scope',
      ':',
      field('scope', $.scope_type),
      $.anchor_block,
      repeat($.correlate_block),
      optional($.if_else_block),
      repeat(choice($.infer_stmt, $.action_stmt)),
      '}',
    ),

    // ----- decision -----
    decision_rule: $ => seq(
      'decision',
      field('name', $.identifier),
      '{',
      'scope',
      ':',
      field('scope', $.scope_type),
      $.decision_anchor,
      repeat($.correlate_block),
      optional($.if_else_block),
      repeat(choice($.emit_stmt, $.action_stmt)),
      '}',
    ),

    decision_anchor: $ => seq(
      'anchor',
      field('binding', $.identifier),
      ':',
      choice($.cause_anchor, $.problem_anchor),
    ),

    cause_anchor: $ => seq(
      'Cause',
      '(',
      field('kind', $.identifier),
      ')',
      '{',
      field('predicate', $.expr),
      '}',
    ),

    problem_anchor: $ => seq(
      'Problem',
      '(',
      field('kind', $.identifier),
      ')',
      optional(seq('{', field('predicate', $.expr), '}')),
    ),

    // ----- anchor / correlate -----
    anchor_block: $ => seq(
      'anchor',
      field('binding', $.identifier),
      ':',
      'event',
      '(',
      field('event', $.kind_ident),
      ')',
      optional(seq('{', optional(field('predicate', $.expr)), '}')),
    ),

    correlate_block: $ => seq(
      'correlate',
      field('binding', $.identifier),
      ':',
      field('source', $.correlate_source),
      '{',
      'topo',
      ':',
      field('topo', $.topo_predicate),
      'time',
      ':',
      field('time', $.time_window),
      optional(seq(
        'having',
        ':',
        'count',
        '>=',
        field('min_count', $.integer),
      )),
      '}',
    ),

    correlate_source: $ => choice(
      seq('event', '(', $.kind_ident, ')'),
      seq('Problem', '(', $.identifier, ')'),
      seq('Cause', '(', $.identifier, ')'),
    ),

    topo_predicate: $ => seq(
      field('name', $.identifier),
      '(',
      optional($.expr_list),
      ')',
    ),

    time_window: $ => seq(
      field('left', $.expr),
      'in',
      '[',
      field('start', $.expr),
      ',',
      field('end', $.expr),
      ']',
    ),

    // ----- infer / emit / action -----
    infer_stmt: $ => seq(
      'infer',
      'Cause',
      '(',
      field('kind', $.identifier),
      ')',
      '{',
      $.infer_field,
      repeat(seq(',', $.infer_field)),
      '}',
    ),

    infer_field: $ => choice(
      seq('target', ':', field('value', $.expr)),
      seq('weight', ':', field('value', $.signed_integer)),
      seq('evidence', ':', '[', $.ref_list, ']'),
    ),

    emit_stmt: $ => seq(
      'emit',
      'Problem',
      '(',
      field('kind', $.identifier),
      ')',
      '{',
      $.emit_field,
      repeat(seq(',', $.emit_field)),
      '}',
    ),

    emit_field: $ => choice(
      seq('target', ':', field('value', $.expr)),
      seq('severity', ':', field('value', $.severity)),
      seq('evidence', ':', '[', $.ref_list, ']'),
      seq('sarif_id', ':', field('value', $.string)),
    ),

    action_stmt: $ => seq(
      'action',
      field('name', $.identifier),
      optional(seq('(', field('arg', $.kind_ident), ')')),
      '{',
      optional(seq($.action_field, repeat(seq(',', $.action_field)))),
      '}',
    ),

    action_field: $ => choice(
      seq('target', ':', field('value', $.expr)),
      seq('reason', ':', field('value', $.string)),
      seq('evidence', ':', '[', $.ref_list, ']'),
    ),

    severity: _ => choice(
      'Critical',
      'High',
      'Medium',
      'Low',
      'Recommended',
      'Optional',
    ),

    ref_list: $ => seq($.identifier, repeat(seq(',', $.identifier))),

    // ----- control flow -----
    if_else_block: $ => seq(
      'if',
      field('condition', $.expr),
      '{',
      repeat(choice($.infer_stmt, $.emit_stmt, $.action_stmt)),
      '}',
      optional(seq(
        'else',
        '{',
        repeat(choice($.infer_stmt, $.emit_stmt, $.action_stmt)),
        '}',
      )),
    ),

    // ----- scope -----
    scope_type: _ => choice(
      'Session',
      'Port',
      'ClientMac',
      'Vlan',
      'AccessPoint',
      'Global',
    ),

    // ----- expressions (precedence per 01-lexical §6.1) -----
    expr: $ => $.logic_or,

    logic_or: $ => prec.left(seq(
      $.logic_and,
      repeat(seq(choice('||', 'or'), $.logic_and)),
    )),

    logic_and: $ => prec.left(seq(
      $.equality,
      repeat(seq(choice('&&', 'and'), $.equality)),
    )),

    equality: $ => prec.left(seq(
      $.additive,
      repeat(seq(
        choice('==', '!=', '<', '<=', '>', '>=', 'in'),
        $.additive,
      )),
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
      seq(choice('!', 'not'), $.unary),
      $.postfix,
    ),

    postfix: $ => prec.left(seq(
      $._primary,
      repeat(choice(
        seq('.', field('member', $.identifier)),
        seq('(', optional($.expr_list), ')'),
        seq('[', $.expr, ']'),
      )),
    )),

    _primary: $ => choice(
      $.integer,
      $.duration,
      $.string,
      $.boolean,
      $.present_expr,
      $.absent_expr,
      $.identifier,
      seq('(', $.expr, ')'),
    ),

    present_expr: $ => seq('present', '(', $.identifier, ')'),
    absent_expr: $ => seq('absent', '(', $.identifier, ')'),

    expr_list: $ => seq($.expr, repeat(seq(',', $.expr))),

    // ----- catalog / literals -----
    kind_ident: $ => seq($.identifier, repeat(seq('.', $.identifier))),

    signed_integer: $ => seq(choice('+', '-'), $.integer),

    boolean: _ => choice('true', 'false'),

    // Duration before integer so `500ms` / `1s` / `2min` munch as one token.
    duration: _ => token(/[0-9]([0-9_]*[0-9])?(ms|s|min)/),

    integer: _ => token(choice(
      /0[xX][0-9a-fA-F]([0-9a-fA-F_]*[0-9a-fA-F])?/,
      /0[bB][01]([01_]*[01])?/,
      /[0-9]([0-9_]*[0-9])?/,
    )),

    // Plain double-quoted strings (escapes uncommon in examples; IDE scaffold).
    string: _ => token(seq('"', /[^"\\]*(\\.[^"\\]*)*/, '"')),

    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,
  },
});
