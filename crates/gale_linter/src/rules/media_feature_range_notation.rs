use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify context or prefix notation for media feature ranges.
///
/// Equivalent to Stylelint's `media-feature-range-notation` rule.
///
/// Primary option:
///   - `"context"`: expect range notation (e.g. `width >= 768px`); the fix
///     rewrites `min-`/`max-` features in place (`(min-width: 1px)` becomes
///     `(width >= 1px)`).
///   - `"prefix"`: expect prefix notation (e.g. `min-width: 768px`); range
///     features are reported but cannot be fixed.
///
/// Secondary option `except: ["exact-value"]` flips the expectation for
/// exact values: `(width: 1px)` under `"prefix"` becomes `(width = 1px)`.
///
/// Media queries are read as written and parsed the way
/// `@csstools/media-query-list-parser` reads them, so invalid queries and
/// features whose value is not a plain value (preprocessor variables, math)
/// are skipped.
pub struct MediaFeatureRangeNotation;

/// Stylelint's `rangeTypeMediaFeatureNames`.
const RANGE_FEATURES: &[&str] = &[
  "aspect-ratio",
  "color",
  "color-index",
  "device-aspect-ratio",
  "device-height",
  "device-width",
  "height",
  "horizontal-viewport-segments",
  "monochrome",
  "resolution",
  "vertical-viewport-segments",
  "width",
];

impl Rule for MediaFeatureRangeNotation {
  fn name(&self) -> &'static str {
    "media-feature-range-notation"
  }

  fn description(&self) -> &'static str {
    "Specify context or prefix notation for media feature ranges"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags media features written in the notation the option forbids, in
  /// every `@media` query as written (nested ones included).
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in &ctx.scanned_rules().at_rules {
      if at.name.eq_ignore_ascii_case("media") {
        self.check_params(&at.params, at.params_offset, ctx, &mut diags);
      }
    }
    diags
  }
}

impl MediaFeatureRangeNotation {
  /// Check the params of one `@media` rule, which start at byte
  /// `params_start` of the source.
  fn check_params(
    &self,
    params: &str,
    params_start: usize,
    ctx: &RuleContext,
    diags: &mut Vec<Diagnostic>,
  ) {
    let prefix = ctx.primary_option_str() == Some("prefix");
    let except_exact_value = ctx
      .secondary_options()
      .and_then(|v| v.get("except"))
      .and_then(|v| v.as_array())
      .is_some_and(|items| items.iter().any(|v| v.as_str() == Some("exact-value")));

    // Stylelint only parses params that could hold a problem.
    if !except_exact_value {
      let could_offend = if prefix {
        params.contains(['<', '>'])
      } else {
        let lower = params.to_ascii_lowercase();
        lower.contains("min-") || lower.contains("max-")
      };
      if !could_offend {
        return;
      }
    }

    let message = |expected_prefix: bool| {
      format!(
        "Expected \"{}\" media feature range notation",
        if expected_prefix { "prefix" } else { "context" }
      )
    };

    let tokens = tokenize(params);
    for query in tokens.split(|t| t.kind == Tok::Comma) {
      let mut features = Vec::new();
      if !media_query(query, &mut features) {
        continue;
      }
      for feature in features {
        let name = feature.name(params);
        let unprefixed = strip_min_max(name);
        if !RANGE_FEATURES.contains(&unprefixed) {
          continue;
        }
        let is_plain = matches!(feature.form, Form::Plain { .. });
        let mut expect_prefix = prefix;
        if except_exact_value {
          let exact_range = matches!(feature.form, Form::Range { exact: true });
          let plain_unprefixed = is_plain && name.len() == unprefixed.len();
          if exact_range || plain_unprefixed {
            expect_prefix = !expect_prefix;
          }
        }
        if expect_prefix == is_plain {
          continue;
        }
        let span = Span::from_range(
          params_start + feature.open,
          params_start + feature.close + 1,
        );
        let mut diag = Diagnostic::new(self.name(), message(expect_prefix))
          .severity(self.default_severity())
          .span(span);
        if let Form::Plain { colon } = feature.form
          && !expect_prefix
        {
          let lower = name.to_ascii_lowercase();
          let operator = if lower.starts_with("min-") {
            ">="
          } else if lower.starts_with("max-") {
            "<="
          } else {
            "="
          };
          let after = params.get(feature.name_end..colon).unwrap_or("");
          let after = if after.is_empty() { " " } else { after };
          diag = diag.fix(Fix::new(
            format!("Write \"{unprefixed} {operator}\""),
            vec![Edit::new(
              Span::from_range(params_start + feature.name_start, params_start + colon + 1),
              format!("{unprefixed}{after}{operator}"),
            )],
          ));
        }
        diags.push(diag);
      }
    }
  }
}

/// `name` without a leading `min-` or `max-` (any case).
fn strip_min_max(name: &str) -> &str {
  let lower = name.to_ascii_lowercase();
  if lower.starts_with("min-") || lower.starts_with("max-") {
    &name[4..]
  } else {
    name
  }
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

/// A token kind, as far as media queries care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
  /// Whitespace or a comment.
  Trivia,
  Ident,
  /// A number, percentage or dimension.
  Number,
  /// `(...)`, with its contents in `children`.
  Block,
  /// `name(...)`.
  Function,
  Colon,
  Comma,
  /// Any other single character.
  Delim(u8),
  /// A string, an unmatched `)`, or anything else that is never valid.
  Bad,
}

/// A token, its text and its byte range in the params.
#[derive(Debug, Clone)]
struct Token<'a> {
  kind: Tok,
  text: &'a str,
  start: usize,
  end: usize,
  children: Vec<Token<'a>>,
}

/// Whether `b` can start an identifier.
fn is_name_start(b: u8) -> bool {
  b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

/// Whether `b` can continue an identifier.
fn is_name_char(b: u8) -> bool {
  is_name_start(b) || b.is_ascii_digit() || b == b'-'
}

/// Tokenize `text` into media-query tokens; parenthesised groups nest.
fn tokenize(text: &str) -> Vec<Token<'_>> {
  let (tokens, _) = tokenize_from(text, 0, false);
  tokens
}

/// Tokenize from `pos`; inside a block, stop at its `)` and return the
/// position after it.
fn tokenize_from(text: &str, mut pos: usize, in_block: bool) -> (Vec<Token<'_>>, usize) {
  let bytes = text.as_bytes();
  let slice = |start: usize, end: usize| text.get(start..end).unwrap_or("");
  let mut tokens = Vec::new();
  while pos < bytes.len() {
    let start = pos;
    let b = bytes[pos];
    let kind = if b.is_ascii_whitespace() {
      while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
      }
      Tok::Trivia
    } else if b == b'/' && bytes.get(pos + 1) == Some(&b'*') {
      pos = find_comment_end(bytes, pos);
      Tok::Trivia
    } else if b == b'(' {
      let (children, after) = tokenize_from(text, pos + 1, true);
      tokens.push(Token {
        kind: Tok::Block,
        text: slice(start, after),
        start,
        end: after,
        children,
      });
      pos = after;
      continue;
    } else if b == b')' {
      if in_block {
        return (tokens, pos + 1);
      }
      pos += 1;
      Tok::Bad
    } else if b == b'"' || b == b'\'' {
      pos += 1;
      while pos < bytes.len() && bytes[pos] != b {
        pos += if bytes[pos] == b'\\' { 2 } else { 1 };
      }
      pos = (pos + 1).min(bytes.len());
      Tok::Bad
    } else if b == b':' {
      pos += 1;
      Tok::Colon
    } else if b == b',' {
      pos += 1;
      Tok::Comma
    } else if starts_number(bytes, pos) {
      pos = consume_number(bytes, pos);
      Tok::Number
    } else if starts_ident(bytes, pos) {
      pos = consume_name(bytes, pos);
      if bytes.get(pos) == Some(&b'(') {
        let (children, after) = tokenize_from(text, pos + 1, true);
        tokens.push(Token {
          kind: Tok::Function,
          text: slice(start, after),
          start,
          end: after,
          children,
        });
        pos = after;
        continue;
      }
      Tok::Ident
    } else {
      pos += 1;
      Tok::Delim(b)
    };
    tokens.push(Token {
      kind,
      text: slice(start, pos),
      start,
      end: pos,
      children: Vec::new(),
    });
  }
  // An unclosed block is never valid.
  if in_block {
    tokens.push(Token {
      kind: Tok::Bad,
      text: "",
      start: pos,
      end: pos,
      children: Vec::new(),
    });
  }
  (tokens, pos)
}

/// The offset just past the comment starting at `pos`.
fn find_comment_end(bytes: &[u8], pos: usize) -> usize {
  let mut i = pos + 2;
  while i + 1 < bytes.len() {
    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
      return i + 2;
    }
    i += 1;
  }
  bytes.len()
}

/// Whether an identifier starts at `pos`.
fn starts_ident(bytes: &[u8], pos: usize) -> bool {
  match bytes[pos] {
    b'-' => match bytes.get(pos + 1) {
      Some(b'-') => true,
      Some(&next) => is_name_start(next) || next == b'\\',
      None => false,
    },
    b'\\' => true,
    b => is_name_start(b),
  }
}

/// The offset just past the identifier starting at `pos`.
fn consume_name(bytes: &[u8], mut pos: usize) -> usize {
  while pos < bytes.len() {
    if bytes[pos] == b'\\' {
      pos += 2;
    } else if is_name_char(bytes[pos]) {
      pos += 1;
    } else {
      break;
    }
  }
  pos.min(bytes.len())
}

/// Whether a number starts at `pos`.
fn starts_number(bytes: &[u8], pos: usize) -> bool {
  let digit_at = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
  match bytes[pos] {
    b'+' | b'-' => digit_at(pos + 1) || (bytes.get(pos + 1) == Some(&b'.') && digit_at(pos + 2)),
    b'.' => digit_at(pos + 1),
    b => b.is_ascii_digit(),
  }
}

/// The offset just past the number, percentage or dimension at `pos`.
fn consume_number(bytes: &[u8], mut pos: usize) -> usize {
  if matches!(bytes[pos], b'+' | b'-') {
    pos += 1;
  }
  while pos < bytes.len() && bytes[pos].is_ascii_digit() {
    pos += 1;
  }
  if bytes.get(pos) == Some(&b'.') && bytes.get(pos + 1).is_some_and(u8::is_ascii_digit) {
    pos += 1;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
      pos += 1;
    }
  }
  if matches!(bytes.get(pos), Some(b'e' | b'E')) {
    let mut i = pos + 1;
    if matches!(bytes.get(i), Some(b'+' | b'-')) {
      i += 1;
    }
    if bytes.get(i).is_some_and(u8::is_ascii_digit) {
      pos = i;
      while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
      }
    }
  }
  if bytes.get(pos) == Some(&b'%') {
    pos + 1
  } else if pos < bytes.len() && starts_ident(bytes, pos) {
    consume_name(bytes, pos)
  } else {
    pos
  }
}

// ---------------------------------------------------------------------------
// Media query grammar
// ---------------------------------------------------------------------------

/// How a media feature is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
  /// `name: value`; `colon` is the colon's offset.
  Plain { colon: usize },
  /// A range; `exact` when it is `name = value` or `value = name`.
  Range { exact: bool },
}

/// A plain or range media feature found in a query.
#[derive(Debug, Clone, Copy)]
struct Feature {
  /// Offsets of the parentheses around it.
  open: usize,
  close: usize,
  /// Byte range of its name.
  name_start: usize,
  name_end: usize,
  form: Form,
}

impl Feature {
  /// The feature's name as written.
  fn name<'a>(&self, params: &'a str) -> &'a str {
    params.get(self.name_start..self.name_end).unwrap_or("")
  }
}

/// The tokens that are not whitespace or comments.
fn significant<'t, 'a>(tokens: &'t [Token<'a>]) -> Vec<&'t Token<'a>> {
  tokens.iter().filter(|t| t.kind != Tok::Trivia).collect()
}

/// Whether `token` is the keyword `word`, ignoring case.
fn is_keyword(token: &Token, word: &str) -> bool {
  token.kind == Tok::Ident && token.text.eq_ignore_ascii_case(word)
}

/// Validate one media query (the tokens between two commas) and collect its
/// plain and range features.  `false` for a query the parser rejects, whose
/// features Stylelint never sees.
fn media_query(tokens: &[Token], features: &mut Vec<Feature>) -> bool {
  let tokens = significant(tokens);
  let Some(first) = tokens.first() else {
    return false;
  };
  if first.kind == Tok::Block
    || (is_keyword(first, "not") && tokens.get(1).is_some_and(|t| t.kind == Tok::Block))
  {
    return condition(&tokens, true, features);
  }
  // `[not | only]? <media-type> [and <condition-without-or>]?`
  let mut i = 0;
  if is_keyword(tokens[0], "not") || is_keyword(tokens[0], "only") {
    i = 1;
  }
  let Some(media_type) = tokens.get(i) else {
    return false;
  };
  if media_type.kind != Tok::Ident
    || ["and", "or", "not", "only", "layer"]
      .iter()
      .any(|k| is_keyword(media_type, k))
  {
    return false;
  }
  match tokens.get(i + 1) {
    None => true,
    Some(and) if is_keyword(and, "and") => condition(&tokens[i + 2..], false, features),
    Some(_) => false,
  }
}

/// A `<media-condition>` (or, without `allow_or`, a
/// `<media-condition-without-or>`): `not <in-parens>`, or in-parens joined
/// by only `and` or only `or`.
fn condition(tokens: &[&Token], allow_or: bool, features: &mut Vec<Feature>) -> bool {
  let Some(first) = tokens.first() else {
    return false;
  };
  if is_keyword(first, "not") {
    return tokens.len() == 2 && in_parens(tokens[1], features);
  }
  let mut joiner: Option<&str> = None;
  let mut expect_parens = true;
  for token in tokens {
    if expect_parens {
      if !in_parens(token, features) {
        return false;
      }
    } else {
      let word = ["and", "or"].into_iter().find(|w| is_keyword(token, w));
      match (word, joiner) {
        (Some("or"), _) if !allow_or => return false,
        (Some(w), None) => joiner = Some(w),
        (Some(w), Some(j)) if w == j => {}
        _ => return false,
      }
    }
    expect_parens = !expect_parens;
  }
  !expect_parens
}

/// A `<media-in-parens>`: a nested condition, a media feature, or anything
/// else in parentheses (general enclosed), which is valid but no feature.
/// Functions count as general enclosed too.
fn in_parens(token: &Token, features: &mut Vec<Feature>) -> bool {
  match token.kind {
    Tok::Function => return true,
    Tok::Block => {}
    _ => return false,
  }
  // An unclosed block ends in an empty `Bad` token.
  if token
    .children
    .last()
    .is_some_and(|t| t.kind == Tok::Bad && t.start == t.end)
  {
    return false;
  }
  let inner = significant(&token.children);
  let nested = inner.first().is_some_and(|t| {
    t.kind == Tok::Block
      || (is_keyword(t, "not") && inner.get(1).is_some_and(|n| n.kind == Tok::Block))
  });
  if nested {
    let mut found = Vec::new();
    if condition(&inner, true, &mut found) {
      features.extend(found);
    }
    return true;
  }
  if let Some(feature) = media_feature(token, &inner) {
    features.push(feature);
  }
  true
}

/// Whether the tokens are a valid `<mf-value>` to
/// `@csstools/media-query-list-parser`: a number (a math function counts),
/// dimension, identifier or `env()`, or a ratio of two numbers.  Any other
/// function, such as a Sass one, makes the feature general enclosed.
fn is_mf_value(tokens: &[&Token]) -> bool {
  let number = |t: &Token| t.kind == Tok::Number || is_function_in(t, MATH_FUNCTIONS);
  let single = |t: &Token| number(t) || t.kind == Tok::Ident || is_function_in(t, &["env"]);
  match tokens {
    [only] => single(only),
    [a, slash, b] => slash.kind == Tok::Delim(b'/') && number(a) && number(b),
    _ => false,
  }
}

/// The math functions `@csstools/media-query-list-parser` reads as numbers.
const MATH_FUNCTIONS: &[&str] = &[
  "abs", "acos", "asin", "atan", "atan2", "calc", "clamp", "cos", "exp", "hypot", "log", "max",
  "min", "mod", "pow", "rem", "round", "sign", "sin", "sqrt", "tan",
];

/// Whether `token` is a function whose name (ignoring case) is in `names`.
fn is_function_in(token: &Token, names: &[&str]) -> bool {
  token.kind == Tok::Function
    && token
      .text
      .split('(')
      .next()
      .is_some_and(|name| names.iter().any(|n| n.eq_ignore_ascii_case(name)))
}

/// The comparison operator starting at `tokens[i]`, as the number of tokens
/// it spans and whether it is a lone `=`.
fn operator_at(tokens: &[&Token], i: usize) -> Option<(usize, bool)> {
  let first = tokens.get(i)?;
  match first.kind {
    Tok::Delim(b'=') => Some((1, true)),
    Tok::Delim(b'<' | b'>') => {
      let eq = tokens
        .get(i + 1)
        .is_some_and(|t| t.kind == Tok::Delim(b'=') && t.start == first.end);
      Some((if eq { 2 } else { 1 }, false))
    }
    _ => None,
  }
}

/// The plain or range feature in a `(...)` block, if its contents are one.
fn media_feature(block: &Token, inner: &[&Token]) -> Option<Feature> {
  let feature = |name: &Token, form| Feature {
    open: block.start,
    close: block.end - 1,
    name_start: name.start,
    name_end: name.end,
    form,
  };
  // `name: value`
  if let [name, colon, value @ ..] = inner
    && name.kind == Tok::Ident
    && colon.kind == Tok::Colon
  {
    return is_mf_value(value).then(|| feature(name, Form::Plain { colon: colon.start }));
  }
  // Ranges: split at the operators.
  let mut parts: Vec<Vec<&Token>> = vec![Vec::new()];
  let mut operators = Vec::new();
  let mut i = 0;
  while i < inner.len() {
    if let Some((len, exact)) = operator_at(inner, i) {
      operators.push(exact);
      parts.push(Vec::new());
      i += len;
    } else {
      parts.last_mut().expect("never empty").push(inner[i]);
      i += 1;
    }
  }
  let is_name = |part: &[&Token]| matches!(part, [t] if t.kind == Tok::Ident);
  match (parts.as_slice(), operators.as_slice()) {
    ([name, value], [exact]) if is_name(name) && is_mf_value(value) => {
      Some(feature(name[0], Form::Range { exact: *exact }))
    }
    ([value, name], [exact]) if is_mf_value(value) && is_name(name) => {
      Some(feature(name[0], Form::Range { exact: *exact }))
    }
    ([low, name, high], [_, _]) if is_mf_value(low) && is_name(name) && is_mf_value(high) => {
      Some(feature(name[0], Form::Range { exact: false }))
    }
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "media-feature-range-notation";

  #[test]
  fn context_rewrites_prefixed_features() {
    let context = serde_json::json!("context");
    assert_eq!(
      fix(
        RULE,
        context.clone(),
        "@media not print, ( min-width  : 1px ) {}",
        Syntax::Css
      ),
      "@media not print, ( width  >= 1px ) {}"
    );
    assert_eq!(
      fix(
        RULE,
        context.clone(),
        "@media (min-width: 1px)\n  and (max-width: 2px)\n  and (width: 3px) {}",
        Syntax::Css
      ),
      "@media (width >= 1px)\n  and (width <= 2px)\n  and (width = 3px) {}"
    );
    assert_eq!(
      fix(
        RULE,
        context.clone(),
        "@media (min-width: 1px) and (not (max-width: 2px)), (MIN-WIDTH: 3px) {}",
        Syntax::Css
      ),
      "@media (width >= 1px) and (not (width <= 2px)), (MIN-WIDTH: 3px) {}"
    );
    let warnings = lint(
      RULE,
      context,
      "@media screen and (min-width: 1px) {}",
      Syntax::Css,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (18, 16));
  }

  #[test]
  fn reads_media_rules_nested_in_style_rules() {
    assert_eq!(
      fix(
        RULE,
        serde_json::json!("context"),
        "a { @media (min-width: 1px) { b: c } }",
        Syntax::Css
      ),
      "a { @media (width >= 1px) { b: c } }"
    );
  }

  #[test]
  fn skips_invalid_queries_and_values_that_are_not_plain() {
    let context = serde_json::json!("context");
    for css in [
      "@media (min-width: 1px) and invalid stuff (x) {}",
      "@media (min-width: 1px) and (max-width: 3px) or (x) {}",
      "@media (min-width: \"a\") {}",
      "@media (min-width: 1px 2px) {}",
      "@media (min-color) {}",
      "@media (pointer: fine) {}",
    ] {
      assert!(
        lint(RULE, context.clone(), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
    for css in [
      "@media (min-width: $var) {}",
      "@media (min-width: (124px + 300px)) {}",
      "@media screen and (max-width: bp($md)) {}",
    ] {
      assert!(
        lint(RULE, context.clone(), css, Syntax::Scss).is_empty(),
        "{css}"
      );
    }
    assert_eq!(
      fix(
        RULE,
        context,
        "@media (min-width: calc(1px)), (min-aspect-ratio: 16/9) {}",
        Syntax::Css
      ),
      "@media (width >= calc(1px)), (aspect-ratio >= 16/9) {}"
    );
  }

  #[test]
  fn prefix_reports_ranges_without_fixing_them() {
    let prefix = serde_json::json!("prefix");
    for css in ["@media (width >= 1px) {}", "@media (1px < width <= 2px) {}"] {
      let warnings = lint(RULE, prefix.clone(), css, Syntax::Css);
      assert_eq!(warnings.len(), 1, "{css}");
      assert!(warnings[0].fix.is_none(), "{css}");
    }
    assert!(lint(RULE, prefix, "@media (min-width: 1px) {}", Syntax::Css).is_empty());
  }

  #[test]
  fn exact_values_flip_with_except() {
    let prefix = serde_json::json!(["prefix", { "except": ["exact-value"] }]);
    assert_eq!(
      fix(
        RULE,
        prefix.clone(),
        "@media (width: 1px), (width: 2px) {}",
        Syntax::Css
      ),
      "@media (width = 1px), (width = 2px) {}"
    );
    assert!(lint(RULE, prefix, "@media (width = 1px) {}", Syntax::Css).is_empty());
    let context = serde_json::json!(["context", { "except": ["exact-value"] }]);
    let warnings = lint(
      RULE,
      context.clone(),
      "@media (1px = width) {}",
      Syntax::Css,
    );
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].fix.is_none());
    assert!(lint(RULE, context, "@media (width: 1px) {}", Syntax::Css).is_empty());
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!(["context", { "disableFix": true }]);
    let css = "@media (min-width: 1px) {}";
    assert_eq!(lint(RULE, options.clone(), css, Syntax::Css).len(), 1);
    assert_eq!(fix(RULE, options, css, Syntax::Css), css);
  }
}
