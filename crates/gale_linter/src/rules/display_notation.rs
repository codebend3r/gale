use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::standard_syntax::is_standard_syntax_value;

/// Specify short or full notation for the `display` property.
///
/// Equivalent to Stylelint's `display-notation` rule, including its
/// autofix.  Primary option `"short"` wants the single-keyword forms
/// (`block`, `inline-flex`), `"full"` the multi-keyword ones (`block flow`,
/// `inline flex`).  Only values made purely of keywords are checked; the fix
/// swaps the keywords and keeps any comments between them.
pub struct DisplayNotation;

/// `display-outside` keywords, which sort first.
const OUTSIDE: &[&str] = &["block", "inline", "run-in"];

/// `display-inside` keywords, which sort second.
const INSIDE: &[&str] = &["flow", "flow-root", "table", "flex", "grid", "ruby"];

/// Keywords whose presence makes a value worth tokenizing (Stylelint's
/// `displayKeyword` regex: list-item, outside, inside and legacy keywords).
const DISPLAY_KEYWORDS: &[&str] = &[
  "list-item",
  "block",
  "inline",
  "run-in",
  "flow",
  "flow-root",
  "table",
  "flex",
  "grid",
  "ruby",
  "inline-block",
  "inline-table",
  "inline-flex",
  "inline-grid",
];

/// Stylelint's `SHORT_TO_LONG`: normalized value → full notation.
const SHORT_TO_LONG: &[(&str, &[&str])] = &[
  ("block list-item", &["block", "flow", "list-item"]),
  ("block", &["block", "flow"]),
  ("flex", &["block", "flex"]),
  ("flow list-item", &["block", "flow", "list-item"]),
  ("flow", &["block", "flow"]),
  ("flow-root", &["block", "flow-root"]),
  ("grid", &["block", "grid"]),
  ("inline list-item", &["inline", "flow", "list-item"]),
  ("inline", &["inline", "flow"]),
  ("inline-block", &["inline", "flow-root"]),
  ("inline-flex", &["inline", "flex"]),
  ("inline-grid", &["inline", "grid"]),
  ("inline-table", &["inline", "table"]),
  ("list-item", &["block", "flow", "list-item"]),
  ("ruby", &["inline", "ruby"]),
  ("run-in", &["run-in", "flow"]),
  ("table", &["block", "table"]),
];

/// Stylelint's `LONG_TO_SHORT`: normalized value → short notation.
const LONG_TO_SHORT: &[(&str, &[&str])] = &[
  ("block flex", &["flex"]),
  ("block flow list-item", &["list-item"]),
  ("block flow", &["block"]),
  ("block flow-root", &["flow-root"]),
  ("block grid", &["grid"]),
  ("block list-item", &["list-item"]),
  ("block table", &["table"]),
  ("flow list-item", &["list-item"]),
  ("inline flex", &["inline-flex"]),
  ("inline flow list-item", &["inline", "list-item"]),
  ("inline flow", &["inline"]),
  ("inline flow-root", &["inline-block"]),
  ("inline grid", &["inline-grid"]),
  ("inline ruby", &["ruby"]),
  ("inline table", &["inline-table"]),
  ("run-in flow", &["run-in"]),
];

impl Rule for DisplayNotation {
  fn name(&self) -> &'static str {
    "display-notation"
  }

  fn description(&self) -> &'static str {
    "Specify short or long form for display values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports `display` values in the other notation, with a fix that
  /// rewrites the keywords.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let table = match ctx.primary_option_str() {
      Some("short") => LONG_TO_SHORT,
      Some("full") => SHORT_TO_LONG,
      _ => return Vec::new(),
    };
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for decl in tree.decls() {
      let (Some(prop), Some(value)) = (
        ctx.source_slice(decl.name_span.start, decl.name_span.end),
        ctx.source_slice(decl.value_span.start, decl.value_span.end),
      ) else {
        continue;
      };
      if !prop.eq_ignore_ascii_case("display")
        || !mentions_display_keyword(value)
        || !is_standard_syntax_value(value)
      {
        continue;
      }
      // Only whitespace, comments and keywords.
      let Some(tokens) = tokenize(value) else {
        continue;
      };
      let keywords: Vec<&Token> = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Ident)
        .collect();
      let (Some(first), Some(last)) = (keywords.first(), keywords.last()) else {
        continue;
      };
      let mut names: Vec<&str> = keywords.iter().map(|t| t.value.as_str()).collect();
      names.sort_by_key(|name| keyword_order(name));
      let normalized = names.join(" ").to_lowercase();
      let Some((_, replacement)) = table.iter().find(|(from, _)| *from == normalized) else {
        continue;
      };
      let replacement = replacement.join(" ");
      let comments: String = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Comment && t.start > first.start && t.end <= last.start)
        .map(|t| &value[t.start..t.end])
        .collect();
      let span = Span::from_range(
        decl.value_span.start + first.start,
        decl.value_span.start + last.end,
      );
      diags.push(
        Diagnostic::new(
          self.name(),
          format!("Expected \"{normalized}\" to be \"{replacement}\""),
        )
        .severity(self.default_severity())
        .span(span)
        .fix(Fix::new(
          format!("Replace with \"{replacement}\""),
          vec![Edit::new(span, format!("{replacement}{comments}"))],
        )),
      );
    }
    diags
  }
}

/// Whether `value` holds a display keyword between word boundaries.
fn mentions_display_keyword(value: &str) -> bool {
  let lower = value.to_ascii_lowercase();
  let bytes = lower.as_bytes();
  let is_word = |i: usize| {
    bytes
      .get(i)
      .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
  };
  DISPLAY_KEYWORDS.iter().any(|word| {
    lower
      .match_indices(word)
      .any(|(start, _)| (start == 0 || !is_word(start - 1)) && !is_word(start + word.len()))
  })
}

/// Sort key of a keyword: outside, then inside, then `list-item`, with
/// anything else first.
fn keyword_order(keyword: &str) -> u8 {
  let is = |list: &[&str]| list.iter().any(|k| k.eq_ignore_ascii_case(keyword));
  if is(OUTSIDE) {
    1
  } else if is(INSIDE) {
    2
  } else if keyword.eq_ignore_ascii_case("list-item") {
    3
  } else {
    0
  }
}

/// The kinds of CSS token a keyword-only value can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
  Whitespace,
  Comment,
  Ident,
}

/// One token of a `display` value.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
  kind: TokenKind,
  /// Byte offset of the token in the value.
  start: usize,
  /// Byte offset just past the token.
  end: usize,
  /// An ident's name with escapes resolved.
  value: String,
}

/// Split `css` into whitespace, comment and ident tokens following CSS
/// Syntax 3, or `None` as soon as any other token (a number, string,
/// function, delimiter, ...) appears.
fn tokenize(css: &str) -> Option<Vec<Token>> {
  let bytes = css.as_bytes();
  let mut tokens = Vec::new();
  let mut pos = 0;
  while pos < bytes.len() {
    let start = pos;
    let b = bytes[pos];
    if is_whitespace(b) {
      while pos < bytes.len() && is_whitespace(bytes[pos]) {
        pos += 1;
      }
      tokens.push(Token {
        kind: TokenKind::Whitespace,
        start,
        end: pos,
        value: String::new(),
      });
    } else if css[pos..].starts_with("/*") {
      pos = css[pos + 2..]
        .find("*/")
        .map_or(bytes.len(), |i| pos + 2 + i + 2);
      tokens.push(Token {
        kind: TokenKind::Comment,
        start,
        end: pos,
        value: String::new(),
      });
    } else if starts_ident(&css[pos..]) {
      let (value, end) = consume_ident(css, pos);
      // `name(` is a function (or `url(`), not a keyword.
      if bytes.get(end) == Some(&b'(') {
        return None;
      }
      pos = end;
      tokens.push(Token {
        kind: TokenKind::Ident,
        start,
        end,
        value,
      });
    } else {
      return None;
    }
  }
  Some(tokens)
}

/// CSS whitespace: space, tab and newlines.
fn is_whitespace(b: u8) -> bool {
  matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// Whether `c` may start an identifier.
fn is_ident_start(c: char) -> bool {
  c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}

/// Whether `c` may continue an identifier.
fn is_name_char(c: char) -> bool {
  is_ident_start(c) || c.is_ascii_digit() || c == '-'
}

/// Whether `text` starts with a valid escape: `\` not followed by a newline.
fn starts_escape(text: &str) -> bool {
  let mut chars = text.chars();
  chars.next() == Some('\\') && !matches!(chars.next(), Some('\n' | '\r' | '\x0c'))
}

/// Whether `text` starts an ident sequence (CSS Syntax 3 §4.3.9).
fn starts_ident(text: &str) -> bool {
  let mut chars = text.chars();
  match chars.next() {
    Some('-') => {
      let rest = &text[1..];
      match rest.chars().next() {
        Some(c) if is_ident_start(c) || c == '-' => true,
        _ => starts_escape(rest),
      }
    }
    Some('\\') => starts_escape(text),
    Some(c) => is_ident_start(c),
    None => false,
  }
}

/// Consume the ident sequence at `pos`, returning its name with escapes
/// resolved and the offset just past it.
fn consume_ident(css: &str, mut pos: usize) -> (String, usize) {
  let mut name = String::new();
  while let Some(c) = css[pos..].chars().next() {
    if is_name_char(c) {
      name.push(c);
      pos += c.len_utf8();
    } else if starts_escape(&css[pos..]) {
      let (escaped, end) = consume_escape(css, pos + 1);
      name.push(escaped);
      pos = end;
    } else {
      break;
    }
  }
  (name, pos)
}

/// Consume the escape whose `\` sits just before `pos`: up to six hex
/// digits and one optional whitespace, or any single character.
fn consume_escape(css: &str, pos: usize) -> (char, usize) {
  let hex: String = css[pos..]
    .chars()
    .take_while(char::is_ascii_hexdigit)
    .take(6)
    .collect();
  if hex.is_empty() {
    return match css[pos..].chars().next() {
      Some(c) => (c, pos + c.len_utf8()),
      None => ('\u{FFFD}', pos),
    };
  }
  let mut end = pos + hex.len();
  if css[end..].starts_with("\r\n") {
    end += 2;
  } else if css.as_bytes().get(end).is_some_and(|b| is_whitespace(*b)) {
    end += 1;
  }
  let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
  let c = match code {
    0 => '\u{FFFD}',
    _ => char::from_u32(code).unwrap_or('\u{FFFD}'),
  };
  (c, end)
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{fix, lint};

  const RULE: &str = "display-notation";

  #[test]
  fn fixes_to_short_notation() {
    let short = || json!(["short"]);
    assert_eq!(
      fix(RULE, short(), "a { display: block flow; }", Syntax::Css),
      "a { display: block; }"
    );
    assert_eq!(
      fix(
        RULE,
        short(),
        "a { display: flow-root INLINE; }",
        Syntax::Css
      ),
      "a { display: inline-block; }"
    );
    assert_eq!(
      fix(
        RULE,
        short(),
        "a { display: inline /* x */ flow /* y */ list-item; }",
        Syntax::Css
      ),
      "a { display: inline list-item/* x *//* y */; }"
    );
  }

  #[test]
  fn fixes_to_full_notation() {
    assert_eq!(
      fix(
        RULE,
        json!(["full"]),
        "a { display: inline-flex !important; }",
        Syntax::Css
      ),
      "a { display: inline flex !important; }"
    );
  }

  #[test]
  fn skips_values_that_are_not_just_keywords() {
    let short = || json!(["short"]);
    assert!(
      lint(
        RULE,
        short(),
        "a { display: block var(--foo, flow); }",
        Syntax::Css
      )
      .is_empty()
    );
    assert!(lint(RULE, short(), "a { display: $block flow; }", Syntax::Scss).is_empty());
    assert!(lint(RULE, short(), "a { display: none; }", Syntax::Css).is_empty());
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = json!(["short", { "disableFix": true }]);
    let source = "a { display: block flow; }";
    assert_eq!(fix(RULE, options.clone(), source, Syntax::Css), source);
    assert_eq!(lint(RULE, options, source, Syntax::Css).len(), 1);
  }
}
