use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;
use std::sync::LazyLock;

use crate::rule::{Rule, RuleContext};

/// Reports when `+` or `-` operators inside CSS math function expressions are
/// not surrounded by single spaces.
///
/// Per the CSS spec, `+` and `-` inside `calc()` and other math functions must
/// have a single space on both sides. `*` and `/` do not require spaces.
///
/// This rule operates on raw source text rather than the parsed AST because
/// the CSS parser (lightningcss) normalizes calc expressions, losing the
/// original spacing information.
///
/// Equivalent to Stylelint's `function-calc-no-unspaced-operator` rule.
pub struct FunctionCalcNoUnspacedOperator;

/// All CSS math function names (case-insensitive matching done at check time).
const MATH_FUNCTIONS: &[&str] = &[
  "calc",
  "min",
  "max",
  "clamp",
  "abs",
  "sign",
  "round",
  "mod",
  "rem",
  "sin",
  "cos",
  "tan",
  "asin",
  "acos",
  "atan",
  "atan2",
  "pow",
  "sqrt",
  "hypot",
  "log",
  "exp",
  "calc-size",
];

/// Regex to find math function calls in raw source text.
static MATH_FUNC_START: LazyLock<Regex> = LazyLock::new(|| {
  let names = MATH_FUNCTIONS.join("|");
  Regex::new(&format!(r"(?i)({names})\(")).unwrap()
});

/// Regex to detect CSS comments.
static COMMENT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/\*[\s\S]*?\*/").unwrap());

/// Functions whose arguments should be skipped entirely.
fn is_opaque_function(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  matches!(lower.as_str(), "var" | "env" | "constant" | "attr")
}

/// Extract the body of a parenthesized expression starting right after the `(`.
/// Returns `(body_str, end_exclusive)` where end_exclusive points past the `)`.
fn extract_paren_body(s: &str, start: usize) -> Option<(&str, usize)> {
  let mut depth = 1;
  let bytes = s.as_bytes();
  let mut i = start;
  while i < bytes.len() {
    match bytes[i] {
      b'(' => depth += 1,
      b')' => {
        depth -= 1;
        if depth == 0 {
          return Some((&s[start..i], i + 1));
        }
      }
      _ => {}
    }
    i += 1;
  }
  Some((&s[start..], s.len()))
}

/// Describes which side of an operator is missing a space.
#[derive(Debug, Clone, Copy, PartialEq)]
enum UnspacedSide {
  Before,
  After,
}

/// Skip a preprocessor token (SCSS $var-name, Less @var-name, SCSS #{...}).
/// Returns the end index (exclusive) if found.
fn skip_preprocessor_token(bytes: &[u8], pos: usize) -> Option<usize> {
  let len = bytes.len();
  if pos >= len {
    return None;
  }

  // SCSS interpolation: #{...}
  if bytes[pos] == b'#' && pos + 1 < len && bytes[pos + 1] == b'{' {
    let mut depth = 1;
    let mut j = pos + 2;
    while j < len {
      if bytes[j] == b'{' {
        depth += 1;
      } else if bytes[j] == b'}' {
        depth -= 1;
        if depth == 0 {
          return Some(j + 1);
        }
      }
      j += 1;
    }
    return Some(len);
  }

  // SCSS variable: $name-with-hyphens
  if bytes[pos] == b'$' {
    let mut i = pos + 1;
    while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
      i += 1;
    }
    if i > pos + 1 {
      if i < len
        && bytes[i] == b'('
        && let Some((_, end)) = extract_paren_body(std::str::from_utf8(bytes).unwrap_or(""), i + 1)
      {
        return Some(end);
      }
      return Some(i);
    }
  }

  // Less variable: @name-with-hyphens
  if bytes[pos] == b'@' {
    let mut i = pos + 1;
    while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
      i += 1;
    }
    if i > pos + 1 {
      return Some(i);
    }
  }

  None
}

/// Validate whitespace after an operator. Returns true if valid.
/// Valid: exactly one space (0x20) followed by non-whitespace, or newline
/// (with optional \r before \n) directly after operator.
fn valid_whitespace_after(bytes: &[u8], op_pos: usize, len: usize) -> bool {
  let start = op_pos + 1;
  if start >= len {
    return false;
  }

  // Single space followed by non-whitespace = valid
  if bytes[start] == b' ' && start + 1 < len && !bytes[start + 1].is_ascii_whitespace() {
    return true;
  }

  // Newline directly after operator = valid
  if bytes[start] == b'\n' {
    return true;
  }
  // CRLF directly after operator = valid
  if bytes[start] == b'\r' && start + 1 < len && bytes[start + 1] == b'\n' {
    return true;
  }

  false
}

/// Validate whitespace before an operator. Returns true if valid.
/// Valid: exactly one space preceded by non-whitespace, or a newline somewhere
/// in the whitespace sequence before the operator (multiline formatting).
fn valid_whitespace_before(bytes: &[u8], op_pos: usize) -> bool {
  if op_pos == 0 {
    return false;
  }

  if !bytes[op_pos - 1].is_ascii_whitespace() {
    return false;
  }

  // Walk back through all whitespace before the operator
  let mut j = op_pos - 1;
  let mut found_newline = false;
  loop {
    if bytes[j] == b'\n' {
      found_newline = true;
    }
    if j == 0 || !bytes[j - 1].is_ascii_whitespace() {
      break;
    }
    j -= 1;
  }

  // If there's a newline in the whitespace, it's valid (multiline formatting)
  if found_newline {
    return true;
  }

  // Otherwise, must be exactly one space
  // j is now at the first whitespace character
  op_pos - 1 == j && bytes[j] == b' '
}

/// Recursively find unspaced +/- operators within a math function body.
/// `body` is the text inside the math function's parentheses.
/// `base_offset` is the byte offset of `body[0]` within the full source.
fn find_unspaced_operators(
  body: &str,
  base_offset: usize,
  is_scss: bool,
) -> Vec<(usize, char, UnspacedSide)> {
  let bytes = body.as_bytes();
  let len = bytes.len();
  let mut results = Vec::new();
  let mut i = 0;
  // Track the start of the last SCSS interpolation #{...} so we can report
  // the interpolation start position instead of the operator position,
  // matching Stylelint's PostCSS-SCSS behaviour.
  let mut last_interp_start: Option<usize> = None;

  while i < len {
    let ch = bytes[i];

    // Skip SCSS interpolation #{...}
    if is_scss && ch == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
      last_interp_start = Some(i);
      let mut depth = 1;
      let mut j = i + 2;
      while j < len {
        if bytes[j] == b'{' {
          depth += 1;
        } else if bytes[j] == b'}' {
          depth -= 1;
          if depth == 0 {
            j += 1;
            break;
          }
        }
        j += 1;
      }
      i = j;
      continue;
    }

    // Skip square brackets [...]
    if ch == b'[' {
      let mut depth = 1;
      let mut j = i + 1;
      while j < len {
        if bytes[j] == b'[' {
          depth += 1;
        } else if bytes[j] == b']' {
          depth -= 1;
          if depth == 0 {
            j += 1;
            break;
          }
        }
        j += 1;
      }
      i = j;
      continue;
    }

    // Skip curly braces {...}
    if ch == b'{' {
      let mut depth = 1;
      let mut j = i + 1;
      while j < len {
        if bytes[j] == b'{' {
          depth += 1;
        } else if bytes[j] == b'}' {
          depth -= 1;
          if depth == 0 {
            j += 1;
            break;
          }
        }
        j += 1;
      }
      i = j;
      continue;
    }

    // Handle identifiers and function calls
    if ch.is_ascii_alphabetic() || ch == b'_' {
      let name_start = i;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }
      if i < len && bytes[i] == b'(' {
        let func_name = &body[name_start..i];
        let inner_start = i + 1;
        if let Some((inner_body, end)) = extract_paren_body(body, inner_start) {
          if is_opaque_function(func_name) {
            // Skip opaque functions entirely (var, env, etc.)
            i = end;
            continue;
          }

          // For any function (math or not), recurse into its body
          // to find nested math functions and check their operators.
          let inner_results =
            find_unspaced_operators(inner_body, base_offset + inner_start, is_scss);
          results.extend(inner_results);
          i = end;
          continue;
        }
      }
      // Not a function call, just an identifier - already advanced i
      continue;
    }

    // Handle SCSS/Less variables ($var-name, @var-name)
    if is_scss
      && (ch == b'$' || ch == b'@')
      && let Some(end) = skip_preprocessor_token(bytes, i)
    {
      i = end;
      continue;
    }

    // Handle parenthesized sub-expressions: (...)
    if ch == b'(' {
      let inner_start = i + 1;
      if let Some((inner_body, end)) = extract_paren_body(body, inner_start) {
        // Recurse into parenthesized expression
        let inner_results = find_unspaced_operators(inner_body, base_offset + inner_start, is_scss);
        results.extend(inner_results);
        i = end;
        continue;
      }
    }

    // Check `+` and `-` operators
    if ch == b'+' || ch == b'-' {
      // Skip scientific notation: e.g., 1e+2, 1E-3
      if i > 0
        && (bytes[i - 1] == b'e' || bytes[i - 1] == b'E')
        && i > 1
        && bytes[i - 2].is_ascii_digit()
      {
        i += 1;
        continue;
      }

      // Skip if at start of body (unary) or after only whitespace
      if i == 0 {
        i += 1;
        continue;
      }

      // Skip unary after `(`
      if bytes[i - 1] == b'(' {
        i += 1;
        continue;
      }

      // Skip unary when everything before is whitespace (e.g., `calc(   +1px)`)
      {
        let all_ws_before = bytes[..i].iter().all(|b| b.is_ascii_whitespace());
        if all_ws_before {
          i += 1;
          continue;
        }
      }

      // Skip `--` double-dash ident tokens
      if ch == b'-' && i + 1 < len && bytes[i + 1] == b'-' {
        let mut j = i + 2;
        while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b'_')
        {
          j += 1;
        }
        if j < len
          && bytes[j] == b'('
          && let Some((_, end)) = extract_paren_body(body, j + 1)
        {
          j = end;
        }
        i = j;
        continue;
      }

      // Skip trailing `-` at end of token (e.g., `2px-`, `2-`)
      if ch == b'-' {
        let after = i + 1;
        if (after >= len
          || bytes[after] == b')'
          || (bytes[after] == b' ' && (after + 1 >= len || bytes[after + 1] == b')')))
          && i > 0
          && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'%')
        {
          i += 1;
          continue;
        }
      }

      // Skip unary `+`/`-` after another operator or comma
      {
        let mut j = i;
        while j > 0 && bytes[j - 1].is_ascii_whitespace() {
          j -= 1;
        }
        if j > 0
          && (bytes[j - 1] == b'*'
            || bytes[j - 1] == b'/'
            || bytes[j - 1] == b'+'
            || bytes[j - 1] == b'-'
            || bytes[j - 1] == b','
            || bytes[j - 1] == b'(')
        {
          i += 1;
          continue;
        }
      }

      // This is a binary operator. Check spacing.
      let op_char = ch as char;

      let has_valid_before = valid_whitespace_before(bytes, i);
      let has_valid_after = valid_whitespace_after(bytes, i, len);

      // When SCSS interpolation precedes the operator on the same
      // "operand" (i.e. `#{$var} + x`), Stylelint's PostCSS-SCSS
      // parser collapses the interpolation into a single word token
      // and reports the position of that token (the `#`) rather than
      // the operator. Match that behaviour.
      let report_offset = if is_scss {
        if let Some(interp) = last_interp_start {
          // Only use the interpolation start if it's the operand
          // immediately before this operator (separated only by
          // optional `}` and whitespace).
          let mut p = i;
          while p > 0 && bytes[p - 1].is_ascii_whitespace() {
            p -= 1;
          }
          // Check if the token before whitespace ends with `}`
          // from the interpolation
          if p > 0 && bytes[p - 1] == b'}' {
            base_offset + interp
          } else {
            base_offset + i
          }
        } else {
          base_offset + i
        }
      } else {
        base_offset + i
      };

      if !has_valid_before {
        results.push((report_offset, op_char, UnspacedSide::Before));
      }
      if !has_valid_after {
        results.push((report_offset, op_char, UnspacedSide::After));
      }
    }

    i += 1;
  }

  results
}

/// Check if offset is inside a CSS comment in the source.
fn is_in_comment(source: &str, offset: usize) -> bool {
  for m in COMMENT_RE.find_iter(source) {
    if offset >= m.start() && offset < m.end() {
      return true;
    }
    if m.start() > offset {
      break;
    }
  }
  false
}

impl Rule for FunctionCalcNoUnspacedOperator {
  fn name(&self) -> &'static str {
    "function-calc-no-unspaced-operator"
  }

  fn description(&self) -> &'static str {
    "Disallow unspaced operators within calc functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let source = context.source;
    let is_scss = matches!(context.syntax, Syntax::Scss | Syntax::Less | Syntax::Sass);
    let mut diagnostics = Vec::new();

    // Find all top-level math function calls in the source
    for m in MATH_FUNC_START.find_iter(source) {
      // Check preceding character to exclude things like `rem-calc(`
      if m.start() > 0 {
        let prev = source.as_bytes()[m.start() - 1];
        if prev.is_ascii_alphanumeric() || prev == b'-' || prev == b'_' {
          continue;
        }
      }

      // Skip if inside a comment
      if is_in_comment(source, m.start()) {
        continue;
      }

      let body_start = m.end();
      if let Some((body, _end)) = extract_paren_body(source, body_start) {
        let violations = find_unspaced_operators(body, body_start, is_scss);

        for (offset, op, side) in &violations {
          let msg = match side {
            UnspacedSide::Before => {
              format!("Expected single space before \"{op}\" operator")
            }
            UnspacedSide::After => {
              format!("Expected single space after \"{op}\" operator")
            }
          };

          diagnostics.push(
            Diagnostic::new(self.name(), msg)
              .severity(self.default_severity())
              .span(Span::new(*offset, 1)),
          );
        }
      }
    }

    diagnostics
  }

  fn check(&self, _node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    // All checking is done in check_root using raw source text,
    // since the CSS parser normalizes calc expressions.
    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn make_context(source: &str) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_unspaced_plus_in_calc_both_sides() {
    let rule = FunctionCalcNoUnspacedOperator;
    let source = "a { width: calc(100%+20px); }";
    let ctx = make_context(source);
    let diags = rule.check_root(&[], &ctx);
    assert_eq!(diags.len(), 2);
    assert!(diags[0].message.contains("before"));
    assert!(diags[1].message.contains("after"));
  }

  #[test]
  fn reports_unspaced_plus_in_calc_one_side() {
    let rule = FunctionCalcNoUnspacedOperator;
    let source = "a { width: calc(100% +20px); }";
    let ctx = make_context(source);
    let diags = rule.check_root(&[], &ctx);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("after"));
  }

  #[test]
  fn ignores_properly_spaced_calc() {
    let rule = FunctionCalcNoUnspacedOperator;
    let source = "a { width: calc(100% - 20px); }";
    let ctx = make_context(source);
    let diags = rule.check_root(&[], &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn checks_min_max_clamp() {
    let rule = FunctionCalcNoUnspacedOperator;
    let source = "a { width: min(25vw+25vw); }";
    let ctx = make_context(source);
    let diags = rule.check_root(&[], &ctx);
    assert!(!diags.is_empty());
  }

  #[test]
  fn ignores_rem_calc_function() {
    let rule = FunctionCalcNoUnspacedOperator;
    let source = "a { top: rem-calc(10px+ 10px); }";
    let ctx = make_context(source);
    let diags = rule.check_root(&[], &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_unary_minus() {
    let rule = FunctionCalcNoUnspacedOperator;
    let source = "a { width: calc(-20px + 100%); }";
    let ctx = make_context(source);
    let diags = rule.check_root(&[], &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn checks_nested_parens() {
    let rule = FunctionCalcNoUnspacedOperator;
    let source = "a { top: calc((1px+ 1px)); }";
    let ctx = make_context(source);
    let diags = rule.check_root(&[], &ctx);
    assert!(!diags.is_empty());
  }

  #[test]
  fn checks_multiple_spaces_before() {
    let rule = FunctionCalcNoUnspacedOperator;
    let source = "a { top: calc(1px  + 2px); }";
    let ctx = make_context(source);
    let diags = rule.check_root(&[], &ctx);
    assert!(!diags.is_empty());
    assert!(diags[0].message.contains("before"));
  }

  #[test]
  fn ignores_comment_in_calc() {
    let rule = FunctionCalcNoUnspacedOperator;
    let source = "a { padding: 0 /* calc(1px+2px) */ 0; }";
    let ctx = make_context(source);
    let diags = rule.check_root(&[], &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn ignores_double_dash_ident() {
    let rule = FunctionCalcNoUnspacedOperator;
    let source = "a { padding: calc(1px --2px); }";
    let ctx = make_context(source);
    let diags = rule.check_root(&[], &ctx);
    assert!(diags.is_empty());
  }
}
