use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::css_tokenizer::{self, ComponentValue, Token, TokenType};
use crate::rule::{Rule, RuleContext};

/// Disallow invalid unspaced operators within math functions.
///
/// Equivalent to Stylelint's `function-calc-no-unspaced-operator` rule, and
/// a port of its algorithm: declaration values are tokenized as CSS, each
/// math function (and parenthesised group inside one) is split into
/// `operand operator operand` operations, and a `+` or `-` operator must
/// have exactly one space (or a newline) on each side.  Operators hidden in
/// a token (`2px+1px` reads as `2px` and `+1px`, `1px-` as a unit `px-`)
/// are found too.  The fix inserts or normalises the whitespace, exactly as
/// Stylelint's does.
pub struct FunctionCalcNoUnspacedOperator;

/// Stylelint's `mathFunctions`.
const MATH_FUNCTIONS: &[&str] = &[
  "abs",
  "acos",
  "asin",
  "atan",
  "calc",
  "cos",
  "exp",
  "sign",
  "sin",
  "sqrt",
  "tan",
  "atan2",
  "calc-size",
  "clamp",
  "hypot",
  "log",
  "max",
  "min",
  "mod",
  "pow",
  "rem",
  "round",
];

impl Rule for FunctionCalcNoUnspacedOperator {
  fn name(&self) -> &'static str {
    "function-calc-no-unspaced-operator"
  }

  fn description(&self) -> &'static str {
    "Disallow invalid unspaced operator within calc functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags `+` and `-` operators in math functions that lack a single space
  /// on either side.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for &decl in ctx.written_declarations(node).iter() {
      if !decl.value.contains(['+', '-']) || !mentions_math_function(decl.value) {
        continue;
      }
      let values = tokenize_declaration_value(decl.value);
      let mut found = Vec::new();
      walk(decl.value, &values, false, &mut found);
      for report in found {
        let message = match report.side {
          Side::Before => format!(
            "Expected single space before \"{}\" operator",
            report.operator
          ),
          Side::After => format!(
            "Expected single space after \"{}\" operator",
            report.operator
          ),
        };
        let mut diag = Diagnostic::new(self.name(), message)
          .severity(self.default_severity())
          .span(Span::new(
            decl.value_start + report.position,
            report.operator.len_utf8(),
          ));
        if !report.edits.is_empty() {
          let edits = report
            .edits
            .into_iter()
            .map(|(start, end, text)| {
              Edit::new(
                Span::from_range(decl.value_start + start, decl.value_start + end),
                text,
              )
            })
            .collect();
          diag = diag.fix(Fix::new("Space the operator", edits));
        }
        diags.push(diag);
      }
    }
    diags
  }
}

/// Stylelint's `mayIncludeRegexes.mathFunction`: a math function name, in
/// any case, at a word boundary, followed by `(`.
///
/// Runs on every declaration value holding a `+` or `-`, so it looks for
/// the names before each `(` in place rather than lower-casing the value
/// and building a pattern per name.
fn mentions_math_function(value: &str) -> bool {
  let bytes = value.as_bytes();
  let at_word_start = |start: usize| {
    start == 0 || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_')
  };
  (0..bytes.len())
    .filter(|&open| bytes[open] == b'(')
    .any(|open| {
      MATH_FUNCTIONS.iter().any(|name| {
        open.checked_sub(name.len()).is_some_and(|start| {
          bytes[start..open].eq_ignore_ascii_case(name.as_bytes()) && at_word_start(start)
        })
      })
    })
}

/// Stylelint's `tokenizeDeclarationValue`: tokenize, split dimensions whose
/// unit holds a dash (`10px-20px` is a typo for `10px - 20px`), and glue
/// Sass `#{$...}` interpolation into one curly block, then group the tokens.
fn tokenize_declaration_value(value: &str) -> Vec<ComponentValue> {
  let mut tokens = css_tokenizer::tokenize(value);

  // Step 1.1: re-tokenize dimensions whose unit holds a dash.
  let mut i = 0;
  while i < tokens.len() {
    let token = &tokens[i];
    if token.kind == TokenType::Dimension && !token.unit.starts_with("--") {
      if let Some(dash) = token.unit.find('-') {
        let remainder = token.unit[dash..].to_string();
        if remainder.len() > 1 {
          let unit = token.unit[..dash].to_string();
          // Where the remainder starts in the source.  The rewritten token
          // may print shorter (`1.50px` becomes `1.5px`, as in Stylelint)
          // but still stands for all the text before it.
          let base = if token.raw.ends_with(&remainder) {
            token.end - remainder.len()
          } else {
            token.start + token.raw.len().saturating_sub(remainder.len())
          };
          let token = &mut tokens[i];
          token.mutate_unit(&unit);
          token.end = base;
          let mut rest = css_tokenizer::tokenize(&remainder);
          for t in &mut rest {
            t.start += base;
            t.end += base;
          }
          tokens.splice(i + 1..i + 1, rest);
        }
      }
    }
    i += 1;
  }

  // Step 1.2: `#` `{` `$` is Sass interpolation; make `#{` one token.
  let mut i = 0;
  while i + 2 < tokens.len() {
    if tokens[i].is_delim('#')
      && tokens[i + 1].kind == TokenType::OpenCurly
      && tokens[i + 2].is_delim('$')
    {
      let hash_start = tokens[i].start;
      tokens[i + 1].raw = "#{".to_string();
      tokens[i + 1].start = hash_start;
      tokens.remove(i);
    }
    i += 1;
  }

  css_tokenizer::parse_list_of_component_values(tokens)
}

/// Which side of an operator lacks a single space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
  Before,
  After,
}

/// A problem found, with the edits (offsets into the value) that fix its
/// container; only the first problem of a container carries them.
struct Report {
  side: Side,
  operator: char,
  /// Offset of the operator in the value.
  position: usize,
  edits: Vec<(usize, usize, String)>,
}

/// Walk component values the way `@csstools/css-parser-algorithms`'s
/// `walk` does, tracking whether we are inside a math function: a math
/// function or a `(` block inside one is checked; any other function or
/// block leaves math context for its contents.
fn walk(source: &str, values: &[ComponentValue], in_math: bool, found: &mut Vec<Report>) {
  for value in values {
    match value {
      ComponentValue::Function {
        name,
        value: children,
        end,
      } => {
        let math = MATH_FUNCTIONS.contains(&name.value.to_ascii_lowercase().as_str());
        if math {
          check_container(
            source,
            children,
            content_end(children, end, name.end),
            found,
          );
        }
        walk(source, children, math, found);
      }
      ComponentValue::SimpleBlock {
        open,
        value: children,
        end,
      } if open.kind == TokenType::OpenParen => {
        if in_math {
          check_container(
            source,
            children,
            content_end(children, end, open.end),
            found,
          );
        }
        walk(source, children, in_math, found);
      }
      ComponentValue::SimpleBlock {
        value: children, ..
      } => walk(source, children, false, found),
      _ => {}
    }
  }
}

/// Where a container's contents end: at its closing token, or after its
/// last child when it is unclosed.
fn content_end(children: &[ComponentValue], end: &Option<Token>, open_end: usize) -> usize {
  match end {
    Some(close) => close.start,
    None => children.last().map_or(open_end, ComponentValue::end),
  }
}

// ---------------------------------------------------------------------------
// One container, simulated the way Stylelint mutates it while fixing
// ---------------------------------------------------------------------------

/// What an item of a container is.
#[derive(Debug, Clone, PartialEq)]
enum ItemKind {
  Token(Token),
  Whitespace,
  Comment,
  /// A nested function or block, opaque here; `operand` tells whether
  /// Stylelint's `isOperandNode` accepts it.
  Container {
    operand: bool,
  },
}

/// One child of the container being checked.
#[derive(Debug, Clone)]
struct Item {
  /// Stable identity, as JavaScript object identity is in Stylelint.
  id: usize,
  kind: ItemKind,
  /// The item's text now.
  text: String,
  /// Its span in the value, `None` for items the fixes inserted.
  original: Option<(usize, usize)>,
  /// Whether `text` differs from what is in the source.
  changed: bool,
}

impl Item {
  /// Whether this is whitespace or a comment.
  fn is_trivia(&self) -> bool {
    matches!(self.kind, ItemKind::Whitespace | ItemKind::Comment)
  }

  /// Stylelint's `isOperandNode`.
  fn is_operand(&self) -> bool {
    match &self.kind {
      ItemKind::Container { operand } => *operand,
      ItemKind::Token(t) => matches!(
        t.kind,
        TokenType::Number | TokenType::Dimension | TokenType::Percentage | TokenType::Ident
      ),
      _ => false,
    }
  }

  /// The `+` or `-` this item is, if it is that delim.
  fn operator(&self) -> Option<char> {
    match &self.kind {
      ItemKind::Token(t) if t.is_delim('+') => Some('+'),
      ItemKind::Token(t) if t.is_delim('-') => Some('-'),
      _ => None,
    }
  }

  /// Whether this is the `*` or `/` delim, or a `,` or `;`, where a broken
  /// operation stops.
  fn is_boundary(&self) -> bool {
    match &self.kind {
      ItemKind::Token(t) => {
        matches!(t.kind, TokenType::Comma | TokenType::Semicolon)
          || t.is_delim('*')
          || t.is_delim('/')
      }
      _ => false,
    }
  }
}

/// The container's items and the reports and fixes made on them.
struct Container {
  items: Vec<Item>,
  next_id: usize,
}

/// An operation: `first before operator after second`.
struct Operation {
  first: usize,
  before: Vec<usize>,
  /// The operator's item id once it is in the list, its character and its
  /// position in the value.
  operator: Option<(Option<usize>, char, usize)>,
  after: Vec<usize>,
  second: usize,
}

impl Container {
  /// The current index of the item with `id`.
  fn index_of(&self, id: usize) -> usize {
    self
      .items
      .iter()
      .position(|item| item.id == id)
      .expect("items are never removed")
  }

  /// A new whitespace item holding one space.
  fn new_space(&mut self) -> Item {
    self.next_id += 1;
    Item {
      id: self.next_id,
      kind: ItemKind::Whitespace,
      text: " ".to_string(),
      original: None,
      changed: false,
    }
  }

  /// Stylelint's `parseOperation` from `cursor`: the operation there, and
  /// where parsing stopped.
  fn parse_operation(&self, mut cursor: usize) -> (Option<Operation>, usize) {
    let items = &self.items;
    let at = |i: usize| items.get(i);
    while at(cursor).is_some_and(Item::is_trivia) {
      cursor += 1;
    }
    let mut first = None;
    if at(cursor).is_some_and(Item::is_operand) {
      first = Some(items[cursor].id);
      cursor += 1;
    }
    let mut before = Vec::new();
    while at(cursor).is_some_and(Item::is_trivia) {
      before.push(items[cursor].id);
      cursor += 1;
    }
    let mut operator = None;
    if let Some(c) = at(cursor).and_then(Item::operator) {
      let position = items[cursor].original.map_or(0, |(start, _)| start);
      operator = Some((Some(items[cursor].id), c, position));
      cursor += 1;
    }
    let mut after = Vec::new();
    while at(cursor).is_some_and(Item::is_trivia) {
      after.push(items[cursor].id);
      cursor += 1;
    }
    let mut second = None;
    if at(cursor).is_some_and(Item::is_operand) {
      second = Some(items[cursor].id);
      cursor += 1;
    }
    while at(cursor).is_some_and(Item::is_trivia) {
      cursor += 1;
    }
    match (first, second) {
      (Some(first), Some(second)) => (
        Some(Operation {
          first,
          before,
          operator,
          after,
          second,
        }),
        cursor,
      ),
      _ => {
        while let Some(item) = at(cursor) {
          if item.is_boundary() {
            return (None, cursor);
          }
          cursor += 1;
        }
        (None, items.len())
      }
    }
  }

  /// Insert `new` items before index `index`.
  fn insert(&mut self, index: usize, new: Vec<Item>) {
    self.items.splice(index..index, new);
  }

  /// The id of the item at `id`'s token, mutably.
  fn token_mut(&mut self, id: usize) -> Option<&mut Token> {
    let index = self.index_of(id);
    match &mut self.items[index].kind {
      ItemKind::Token(t) => Some(t),
      _ => None,
    }
  }

  /// Refresh an item's text from its (mutated) token.
  fn sync_token_text(&mut self, id: usize) {
    let index = self.index_of(id);
    if let ItemKind::Token(t) = &self.items[index].kind {
      let raw = t.raw.clone();
      let item = &mut self.items[index];
      if item.text != raw {
        item.text = raw;
        item.changed = true;
      }
    }
  }

  /// A delim item for an operator the fixes insert.
  fn operator_item(&mut self, c: char) -> Item {
    self.next_id += 1;
    let mut token = css_tokenizer::tokenize(&c.to_string()).remove(0);
    token.start = 0;
    token.end = 0;
    Item {
      id: self.next_id,
      kind: ItemKind::Token(token),
      text: c.to_string(),
      original: None,
      changed: false,
    }
  }
}

/// Check one math container's children, recording reports and applying
/// Stylelint's fixes to a simulated copy so later operations see them, as
/// they do in Stylelint's fix mode.
fn check_container(source: &str, children: &[ComponentValue], end: usize, found: &mut Vec<Report>) {
  let mut container = Container {
    items: Vec::new(),
    next_id: 0,
  };
  for child in children {
    container.next_id += 1;
    let (kind, text) = match child {
      ComponentValue::Token(t) => (ItemKind::Token(t.clone()), t.raw.clone()),
      ComponentValue::Whitespace(_) => (ItemKind::Whitespace, child.to_css()),
      ComponentValue::Comment(t) => (ItemKind::Comment, t.raw.clone()),
      ComponentValue::Function { name, .. } => {
        let lower = name.value.to_ascii_lowercase();
        let operand = MATH_FUNCTIONS.contains(&lower.as_str()) || lower == "var";
        (ItemKind::Container { operand }, child.to_css())
      }
      ComponentValue::SimpleBlock { .. } => (ItemKind::Container { operand: true }, child.to_css()),
    };
    // A token the tokenizing step rewrote prints differently already.
    let changed = source.get(child.start()..child.end()) != Some(text.as_str());
    container.items.push(Item {
      id: container.next_id,
      kind,
      text,
      original: Some((child.start(), child.end())),
      changed,
    });
  }

  let first_report = found.len();
  let mut cursor = 0;
  while cursor < container.items.len() {
    let (operation, next) = container.parse_operation(cursor);
    let Some(mut operation) = operation else {
      cursor = next + 1;
      continue;
    };
    if operation.operator.is_none() {
      complete_missing_operator(&mut container, &mut operation, found);
    }
    if operation.operator.is_some() {
      check_complete(&mut container, &operation, Side::Before, found);
      check_complete(&mut container, &operation, Side::After, found);
      check_operand_whitespace(&mut container, &operation, Side::Before, found);
      check_operand_whitespace(&mut container, &operation, Side::After, found);
    }
    cursor = container.index_of(operation.second);
  }

  if found.len() > first_report {
    found[first_report].edits = edits_of(&container, end);
  }
}

/// Stylelint's `checkOperationWithoutOperator`: an operator hidden at the
/// end of the first operand (`1px-`, `g-`) or as the sign of the second
/// (`+1px`) is reported and, in the simulation, split out.
fn complete_missing_operator(
  container: &mut Container,
  op: &mut Operation,
  found: &mut Vec<Report>,
) {
  let first_index = container.index_of(op.first);
  if let ItemKind::Token(token) = &container.items[first_index].kind {
    let trailing = match token.kind {
      TokenType::Dimension => Some((token.unit.clone(), true)),
      TokenType::Ident => Some((token.value.clone(), false)),
      _ => None,
    };
    if let Some((text, is_unit)) = trailing
      && let Some(c @ ('+' | '-')) = text.chars().last()
    {
      let position = token.end - 1;
      op.operator = Some((None, c, position));
      op.after = std::mem::take(&mut op.before);
      let space = container.new_space();
      op.before = vec![space.id];
      found.push(Report {
        side: Side::Before,
        operator: c,
        position,
        edits: Vec::new(),
      });
      // insertOperatorAfterFirstOperand, then shorten the first operand.
      let operator = container.operator_item(c);
      op.operator = Some((Some(operator.id), c, position));
      container.insert(first_index + 1, vec![space, operator]);
      let shortened = &text[..text.len() - 1];
      if let Some(token) = container.token_mut(op.first) {
        if is_unit {
          token.mutate_unit(shortened);
        } else {
          token.mutate_ident(shortened);
        }
      }
      container.sync_token_text(op.first);
      return;
    }
  }

  let second_index = container.index_of(op.second);
  if let ItemKind::Token(token) = &container.items[second_index].kind
    && token.is_numeric()
    && let Some(c @ ('+' | '-')) = token.sign
  {
    let position = token.start;
    let space = container.new_space();
    op.after = vec![space.id];
    found.push(Report {
      side: Side::After,
      operator: c,
      position,
      edits: Vec::new(),
    });
    // insertOperatorBeforeSecondOperand, then drop the sign.
    let operator = container.operator_item(c);
    op.operator = Some((Some(operator.id), c, position));
    container.insert(second_index, vec![operator, space]);
    if let Some(token) = container.token_mut(op.second) {
      token.sign = None;
      token.raw = token.raw[c.len_utf8()..].to_string();
    }
    container.sync_token_text(op.second);
  }
}

/// Stylelint's `checkCompleteOperation`: whitespace must be on `side` of
/// the operator; the fix inserts a space.
fn check_complete(container: &mut Container, op: &Operation, side: Side, found: &mut Vec<Report>) {
  let ids = match side {
    Side::Before => &op.before,
    Side::After => &op.after,
  };
  let has_space = ids.iter().any(|id| {
    container
      .items
      .iter()
      .any(|item| item.id == *id && item.kind == ItemKind::Whitespace)
  });
  if has_space {
    return;
  }
  let Some((Some(operator_id), c, position)) = op.operator else {
    return;
  };
  found.push(Report {
    side,
    operator: c,
    position,
    edits: Vec::new(),
  });
  let index = container.index_of(operator_id) + usize::from(side == Side::After);
  let space = container.new_space();
  container.insert(index, vec![space]);
}

/// Stylelint's `checkOperandWhitespace`: whitespace beside the operator
/// must be a single space, or start with a newline; the fix keeps a single
/// space, or the whitespace from its first newline on.
fn check_operand_whitespace(
  container: &mut Container,
  op: &Operation,
  side: Side,
  found: &mut Vec<Report>,
) {
  let Some((_, c, position)) = op.operator else {
    return;
  };
  let ids = match side {
    Side::Before => op.before.clone(),
    Side::After => op.after.clone(),
  };
  for id in ids {
    let index = container.index_of(id);
    let item = &container.items[index];
    if item.kind != ItemKind::Whitespace || item.text == " " {
      continue;
    }
    let first_newline = item
      .text
      .char_indices()
      .find(|&(i, ch)| ch == '\n' || (ch == '\r' && item.text[i + 1..].starts_with('\n')))
      .map(|(i, _)| i);
    if first_newline == Some(0) {
      continue;
    }
    found.push(Report {
      side,
      operator: c,
      position,
      edits: Vec::new(),
    });
    let replacement = match first_newline {
      Some(i) => item.text[i..].to_string(),
      None => " ".to_string(),
    };
    let item = &mut container.items[index];
    if item.text != replacement {
      item.text = replacement;
      item.changed = item.original.is_some();
    }
  }
}

/// The edits that turn the container's source into its simulated text:
/// inserted items become insertions before the next original item (merged
/// into its replacement when it changed too), at the container's end if
/// none follows.
fn edits_of(container: &Container, end: usize) -> Vec<(usize, usize, String)> {
  let mut edits = Vec::new();
  let mut prefix = String::new();
  for item in &container.items {
    match item.original {
      None => prefix.push_str(&item.text),
      Some((start, stop)) => {
        if item.changed {
          edits.push((start, stop, format!("{prefix}{}", item.text)));
        } else if !prefix.is_empty() {
          edits.push((start, start, prefix.clone()));
        }
        prefix.clear();
      }
    }
  }
  if !prefix.is_empty() {
    edits.push((end, end, prefix));
  }
  edits
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use gale_css_parser::Syntax;
  use gale_diagnostics::apply_fixes;

  use super::mentions_math_function;
  use crate::{LintRunner, RuleRegistry};

  /// Lint `css` as `syntax` with only this rule enabled, configured with
  /// `options`.
  fn lint_as(
    css: &str,
    syntax: Syntax,
    options: serde_json::Value,
  ) -> Vec<gale_diagnostics::Diagnostic> {
    let rule = "function-calc-no-unspaced-operator".to_string();
    let runner = LintRunner::with_options(
      RuleRegistry::default(),
      vec![rule.clone()],
      HashMap::from([(rule, options)]),
    );
    runner.lint_source(css, "test.css", syntax).diagnostics
  }

  /// `css` after applying the rule's fixes until nothing changes, the way
  /// `gale --fix` does.
  fn fix(css: &str) -> String {
    let mut current = css.to_string();
    for _ in 0..10 {
      let diags = lint_as(&current, Syntax::Css, serde_json::json!(true));
      let (next, applied) = apply_fixes(&current, &diags);
      if applied == 0 || next == current {
        break;
      }
      current = next;
    }
    current
  }

  #[test]
  fn spaces_operators_hidden_in_tokens() {
    assert_eq!(
      fix("a { top: calc(2px+1px); }"),
      "a { top: calc(2px + 1px); }"
    );
    assert_eq!(
      fix("a { top: calc(1px- 2px); }"),
      "a { top: calc(1px - 2px); }"
    );
    assert_eq!(
      fix("a { padding: calc(1px+2px-3px-4px); }"),
      "a { padding: calc(1px + 2px - 3px - 4px); }"
    );
    assert_eq!(
      fix("a { top: rgb(from red calc(r+1) calc(g-+2) calc(clamp(10px+2px, g+2, none))); }"),
      "a { top: rgb(from red calc(r + 1) calc(g - +2) calc(clamp(10px + 2px, g + 2, none))); }"
    );
    assert_eq!(
      fix("a { top: calc(1.50px-2px); }"),
      "a { top: calc(1.5px - 2px); }"
    );
    assert_eq!(
      fix("a { padding: calc(1\\23 - 2px) calc(1\\23 a- 2px); }"),
      "a { padding: calc(1\\23  - 2px) calc(1\\23 a - 2px); }"
    );
  }

  #[test]
  fn normalises_whitespace_around_operators() {
    assert_eq!(
      fix("a { top: calc(1px +\t-1px); }"),
      "a { top: calc(1px + -1px); }"
    );
    assert_eq!(
      fix("a { top: calc(1px  + 2px); }"),
      "a { top: calc(1px + 2px); }"
    );
    assert_eq!(
      fix("a { padding: calc(1rem +  \t\r\n  1em); }"),
      "a { padding: calc(1rem +\r\n  1em); }"
    );
    let warnings = lint_as(
      "a { top: calc(1px +2px); }",
      Syntax::Css,
      serde_json::json!(true),
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(
      warnings[0].message,
      "Expected single space after \"+\" operator"
    );
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (18, 1));
  }

  /// A math function name counts in any case, after anything but a word
  /// character, and only right before `(`.
  #[test]
  fn finds_math_functions_like_stylelints_pattern() {
    for value in [
      "calc(1px+2px)",
      "CaLc(1px)",
      "-webkit-calc(1px)",
      "1px -min(2px, 3px)",
      "calc-size(auto, size)",
      "var(--a) max(1px,2px)",
      "é clamp(1px, 2px, 3px)",
      "#{$a}round(1.5)",
    ] {
      assert!(mentions_math_function(value), "{value}");
    }
    for value in [
      "xcalc(1px)",
      "my_calc(1px)",
      "calc (1px)",
      "translate(-1px)",
      "min-width",
      "(calc)",
      "size(1px)",
      "",
    ] {
      assert!(!mentions_math_function(value), "{value}");
    }
  }

  #[test]
  fn leaves_valid_and_non_math_values_alone() {
    for css in [
      "a { top: calc(1px * -0.2); }",
      "a { top: calc(-1px * -1); }",
      "a { top: calc(2rem + @fh+dsf-as); }",
      "a { top: rem-calc(10px+ 10px); }",
      "a { top: calc(5--6px-7px); }",
      "a { padding: 10px calc([10px+5px]); }",
      "a { margin-top: calc(var(--a) +\n  var(--b)); }",
      "a { padding: 0 /* calc(1px+2px) */ 0; }",
    ] {
      assert!(
        lint_as(css, Syntax::Css, serde_json::json!(true)).is_empty(),
        "{css}"
      );
    }
    assert!(
      lint_as(
        "a { top: calc(100% - #{$foo}); }",
        Syntax::Scss,
        serde_json::json!(true)
      )
      .is_empty()
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    let css = "a { top: calc(1px+ 2px); }";
    let warnings = lint_as(css, Syntax::Css, options);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].fix.is_none());
  }
}
