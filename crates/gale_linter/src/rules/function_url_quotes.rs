use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::source_text;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Require or disallow quotes around `url()` values.
///
/// Equivalent to Stylelint's `function-url-quotes` rule: every `url()` in
/// declaration values and at-rule params, as written, is checked; the fix
/// wraps an unquoted argument in `"` or unwraps a quoted one, leaving the
/// whitespace inside the parentheses alone.
///
/// Primary option: `"always"` (default) or `"never"`.
///
/// Secondary options:
///   - `except`: `["empty"]` — invert the primary for empty `url()` calls.
pub struct FunctionUrlQuotes;

/// Whether a reported `url()` needs quotes added or removed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Problem {
  Expected,
  Rejected,
}

impl Rule for FunctionUrlQuotes {
  fn name(&self) -> &'static str {
    "function-url-quotes"
  }

  fn description(&self) -> &'static str {
    "Require or disallow quotes around url() values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags `url()` arguments whose quoting does not match the option, in
  /// declaration values and at-rule params. With `except: ["empty"]` the
  /// primary is inverted for empty URLs.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let always = ctx.primary_option_str() != Some("never");
    let except_empty = ctx
      .secondary_options()
      .and_then(|v| v.get("except"))
      .and_then(|v| v.as_array())
      .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("empty")));

    let mut diags = Vec::new();
    if let CssNode::AtRule(at) = node
      && let Some((params, start)) = source_text::at_rule_params(ctx.source, at)
    {
      self.check_text(params, start, always, except_empty, &mut diags);
    }
    for decl in source_text::written_declarations(ctx.source, node) {
      if !contains_url_call(decl.value) || !is_standard_syntax_property(decl.prop) {
        continue;
      }
      self.check_text(
        decl.value,
        decl.value_start,
        always,
        except_empty,
        &mut diags,
      );
    }
    diags
  }
}

impl FunctionUrlQuotes {
  /// Check every `url()` in `text`, which starts at byte `start` of the
  /// source, as Stylelint's `functionArgumentsSearch` finds them.
  fn check_text(
    &self,
    text: &str,
    start: usize,
    always: bool,
    except_empty: bool,
    diags: &mut Vec<Diagnostic>,
  ) {
    let nodes = value_parser::parse(text);
    value_parser::walk(&nodes, &mut |node: &ValueNode| {
      if !node.is_function_named("url") || has_url_modifier(node) {
        return true;
      }
      let Some(args) = url_arguments(text, node) else {
        return true;
      };
      let Some(problem) = check_args(args, always, except_empty) else {
        return true;
      };
      let leading = args.len() - args.trim_start().len();
      let args_start = start + node.source_index + node.value.len() + 1;
      let span = Span::new(args_start + leading, args.len() - leading);
      let (message, description, edits) = match problem {
        Problem::Expected => (
          "Expected quotes around \"url\" function argument",
          "Add quotes",
          node
            .nodes
            .iter()
            .filter(|arg| arg.kind == NodeKind::Word)
            .map(|arg| {
              Edit::new(
                Span::from_range(start + arg.source_index, start + arg.source_end_index),
                format!("\"{}\"", arg.value),
              )
            })
            .collect::<Vec<_>>(),
        ),
        Problem::Rejected => (
          "Unexpected quotes around \"url\" function argument",
          "Remove quotes",
          node
            .nodes
            .iter()
            .filter(|arg| arg.kind == NodeKind::String)
            .map(|arg| {
              Edit::new(
                Span::from_range(start + arg.source_index, start + arg.source_end_index),
                arg.value,
              )
            })
            .collect(),
        ),
      };
      diags.push(
        Diagnostic::new(self.name(), message)
          .severity(self.default_severity())
          .span(span)
          .fix(Fix::new(description, edits)),
      );
      true
    });
  }
}

/// Stylelint's `mayIncludeRegexes.urlFunction` (`/\burl\(/i`).
fn contains_url_call(value: &str) -> bool {
  let lower = value.to_ascii_lowercase();
  lower.match_indices("url(").any(|(i, _)| {
    i == 0 || {
      let before = lower.as_bytes()[i - 1];
      !(before.is_ascii_alphanumeric() || before == b'_')
    }
  })
}

/// Stylelint's `isStandardSyntaxDeclaration`, as far as the property tells:
/// Sass variables (`$a`) and Less variables (`@a`, but not `@{a}`
/// interpolation) are not checked.
fn is_standard_syntax_property(prop: &str) -> bool {
  !(prop.starts_with('$') || (prop.starts_with('@') && !prop.starts_with("@{")))
}

/// Stylelint's `hasUrlModifier`: an argument that is not a string, space or
/// word (such as `crossorigin(anonymous)`), or a word holding unescaped
/// whitespace.
fn has_url_modifier(function: &ValueNode) -> bool {
  function.nodes.iter().any(|n| match n.kind {
    NodeKind::String | NodeKind::Space => false,
    NodeKind::Word => {
      let bytes = n.value.as_bytes();
      (0..bytes.len()).any(|i| bytes[i].is_ascii_whitespace() && (i == 0 || bytes[i - 1] != b'\\'))
    }
    _ => true,
  })
}

/// The text between a `url(`'s parentheses, whitespace included, read the
/// way `functionArgumentsSearch` reads it: the parenthesised group from `(`
/// re-parsed as an ordinary value, so quotes and nested parentheses balance.
fn url_arguments<'a>(text: &'a str, function: &ValueNode) -> Option<&'a str> {
  let open = function.source_index + function.value.len();
  let expression = open + 1;
  let group = value_parser::parse(text.get(open..)?).into_iter().next()?;
  let group_end = open + group.source_end_index;
  let end = if group_end > expression && text.as_bytes().get(group_end - 1) == Some(&b')') {
    group_end - 1
  } else {
    group_end
  };
  text.get(expression..end.max(expression))
}

/// Whether the argument breaks the option, and how.  `None` for one that
/// complies, uses preprocessor syntax, or needs its quotes to be valid CSS.
fn check_args(args: &str, always: bool, except_empty: bool) -> Option<Problem> {
  let trimmed = args.trim_start();
  if !is_standard_syntax_url(trimmed) {
    return None;
  }
  let mut expect_quotes = always;
  if except_empty && matches!(args.trim(), "" | "''" | "\"\"") {
    expect_quotes = !expect_quotes;
  }
  let has_quotes = trimmed.starts_with('\'') || trimmed.starts_with('"');
  if has_quotes && requires_quotes(trimmed) {
    return None;
  }
  match (expect_quotes, has_quotes) {
    (true, false) => Some(Problem::Expected),
    (false, true) => Some(Problem::Rejected),
    _ => None,
  }
}

/// Stylelint's `isStandardSyntaxUrl`.
fn is_standard_syntax_url(url: &str) -> bool {
  if url.is_empty() {
    return true;
  }
  // Sass, template and PostCSS-simple-vars interpolation work anywhere.
  if has_braced(url, b'{', b'}', true) || has_after(url, "$(", b')') {
    return false;
  }
  let quoted =
    (url.starts_with('\'') && url.ends_with('\'')) || (url.starts_with('"') && url.ends_with('"'));
  if quoted {
    // Only Less interpolation works inside quotes.
    return !has_after(url, "@{", b'}');
  }
  // A Less variable works only at the beginning.
  let is_less_variable = url
    .strip_prefix("@@")
    .or_else(|| url.strip_prefix('@'))
    .is_some_and(|name| {
      !name.is_empty()
        && name
          .bytes()
          .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    });
  if is_less_variable {
    return false;
  }
  // A Sass variable can sit anywhere in an unquoted url, among a limited
  // set of characters, unless the url ends with `/`.
  let scss_chars = url.bytes().all(|b| {
    b.is_ascii_alphanumeric() || b.is_ascii_whitespace() || b"$_+-,./*'\"@#?".contains(&b)
  });
  !(url.contains('$') && scss_chars && !url.ends_with('/'))
}

/// Whether `text` holds `open`, at least one character, then `close`
/// (`/\{.+?\}/s` when `multiline`).
fn has_braced(text: &str, open: u8, close: u8, multiline: bool) -> bool {
  let bytes = text.as_bytes();
  bytes.iter().enumerate().any(|(i, &b)| {
    b == open
      && bytes[i + 1..]
        .iter()
        .take_while(|&&c| multiline || c != b'\n')
        .skip(1)
        .any(|&c| c == close)
  })
}

/// Whether `text` holds `opener`, at least one character on the same line,
/// then `close` (`/@\{.+?\}/`, `/\$\(.+?\)/`).
fn has_after(text: &str, opener: &str, close: u8) -> bool {
  text.match_indices(opener).any(|(i, _)| {
    text.as_bytes()[i + opener.len()..]
      .iter()
      .take_while(|&&c| c != b'\n')
      .skip(1)
      .any(|&c| c == close)
  })
}

/// Stylelint's `requiresQuotes`: whether the quoted argument's content
/// would not survive as an unquoted `url(...)` token, because it holds
/// whitespace, quotes, parentheses, control characters or a bad escape.
fn requires_quotes(url: &str) -> bool {
  let content = match url.chars().next() {
    Some(quote) => match url.rfind(quote) {
      Some(end) if end > 0 => &url[quote.len_utf8()..end],
      _ => url,
    },
    None => url,
  };
  !is_plain_url_token(content)
}

/// Whether `url(<content>)` tokenizes as exactly one `<url-token>`, per CSS
/// Syntax Level 3 "consume a url token".
fn is_plain_url_token(content: &str) -> bool {
  let bytes = content.as_bytes();
  let mut i = 0;
  while i < bytes.len() && is_css_whitespace(bytes[i]) {
    i += 1;
  }
  if matches!(bytes.get(i), Some(b'"' | b'\'')) {
    // `url("...` is a function token, not a url token.
    return false;
  }
  while i < bytes.len() {
    match bytes[i] {
      b')' => return false,
      b if is_css_whitespace(b) => {
        while i < bytes.len() && is_css_whitespace(bytes[i]) {
          i += 1;
        }
        return i == bytes.len();
      }
      b'"' | b'\'' | b'(' => return false,
      0x00..=0x08 | 0x0B | 0x0E..=0x1F | 0x7F => return false,
      b'\\' => {
        match bytes.get(i + 1) {
          // A backslash before a newline is not an escape; before the end,
          // it escapes the closing parenthesis.
          Some(b'\n' | b'\r' | b'\x0c') => return false,
          None => return true,
          Some(b) if b.is_ascii_hexdigit() => {
            i += 1;
            let mut digits = 0;
            while digits < 6 && i < bytes.len() && bytes[i].is_ascii_hexdigit() {
              i += 1;
              digits += 1;
            }
            if i < bytes.len() && is_css_whitespace(bytes[i]) {
              i += 1;
            }
            continue;
          }
          Some(_) => {
            i += 1 + content[i + 1..].chars().next().map_or(1, char::len_utf8);
            continue;
          }
        }
      }
      _ => {}
    }
    i += 1;
  }
  true
}

/// Whitespace as CSS tokenization defines it.
fn is_css_whitespace(b: u8) -> bool {
  matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use gale_css_parser::Syntax;
  use gale_diagnostics::apply_fixes;

  use crate::{LintRunner, RuleRegistry};

  /// Lint `css` with only this rule enabled, configured with `options`.
  fn lint(css: &str, options: serde_json::Value) -> Vec<gale_diagnostics::Diagnostic> {
    let rule = "function-url-quotes".to_string();
    let runner = LintRunner::with_options(
      RuleRegistry::default(),
      vec![rule.clone()],
      HashMap::from([(rule, options)]),
    );
    runner.lint_source(css, "test.css", Syntax::Css).diagnostics
  }

  /// `css` after applying the rule's fixes until nothing changes, the way
  /// `gale --fix` does.
  fn fix(css: &str, options: serde_json::Value) -> String {
    let mut current = css.to_string();
    for _ in 0..10 {
      let (next, applied) = apply_fixes(&current, &lint(&current, options.clone()));
      if applied == 0 || next == current {
        break;
      }
      current = next;
    }
    current
  }

  #[test]
  fn always_adds_double_quotes_inside_the_parentheses() {
    let always = serde_json::json!("always");
    assert_eq!(
      fix("@import url( foo.css );", always.clone()),
      "@import url( \"foo.css\" );"
    );
    assert_eq!(
      fix("@import url(foo\\ .css);", always.clone()),
      "@import url(\"foo\\ .css\");"
    );
    assert_eq!(
      fix(
        "@font-face { font-family: 'foo'; src: url(foo.ttf); }",
        always.clone()
      ),
      "@font-face { font-family: 'foo'; src: url(\"foo.ttf\"); }"
    );
    assert_eq!(
      fix(
        "a { b: url(data:image/png;base64,abc), image-set(url(a.png) 1x) }",
        always.clone()
      ),
      "a { b: url(\"data:image/png;base64,abc\"), image-set(url(\"a.png\") 1x) }"
    );
    let warnings = lint("a { cursor: url( foo.png ); }", always);
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (17, 8));
  }

  #[test]
  fn never_removes_quotes_unless_the_url_needs_them() {
    let never = serde_json::json!("never");
    assert_eq!(
      fix("@import URL( 'foo.css' );", never.clone()),
      "@import URL( foo.css );"
    );
    assert_eq!(
      fix("@document url(\"http://www.w3.org/\");", never.clone()),
      "@document url(http://www.w3.org/);"
    );
    assert_eq!(
      fix("a { b: url( \"a\\ b\" ) }", never.clone()),
      "a { b: url( a\\ b ) }"
    );
    for css in [
      "a { b: url(\"image file.png\") }",
      "a { b: url(\"foo(bar).png\") }",
      "a { b: url(\"foo)bogus\") }",
      "a { b: url(\"'foo'\") }",
      "a { b: url('foo.svg' crossorigin(anonymous)) }",
    ] {
      assert!(lint(css, never.clone()).is_empty(), "{css}");
    }
  }

  #[test]
  fn except_empty_inverts_the_option_for_empty_urls() {
    let options = serde_json::json!(["never", { "except": ["empty"] }]);
    assert!(lint("a { b: url(\"\") }", options.clone()).is_empty());
    assert_eq!(lint("a { b: url() }", options).len(), 1);
  }

  #[test]
  fn skips_preprocessor_urls() {
    let always = serde_json::json!("always");
    for css in [
      "@import url(@variable);",
      "@import url($variable + 'foo.css');",
      "a { b: url(#{$a}/b.png) }",
    ] {
      assert!(lint(css, always.clone()).is_empty(), "{css}");
    }
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!(["always", { "disableFix": true }]);
    assert_eq!(lint("a { b: url(x) }", options.clone()).len(), 1);
    assert_eq!(fix("a { b: url(x) }", options), "a { b: url(x) }");
  }
}
