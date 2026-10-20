use std::sync::LazyLock;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};
use regex::Regex;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::postcss_tree::NodeKind;
use crate::rule::{Check, RuleContext, declare_rule};
use crate::sass_operators::find_operators;

const NAME: &str = "scss/operator-no-newline-before";

declare_rule! {
  /// Disallow a line break right before a Sass operator (`10px\n  + 1`).
  ///
  /// Equivalent to stylelint-scss's `scss/operator-no-newline-before`, which
  /// runs `operator-no-unspaced`'s `calculationOperatorSpaceChecker` over the
  /// stylesheet and reports each operator with only whitespace, a line break
  /// among it, before it.  Operators are told from signs and hyphens by
  /// [`find_operators`].
  ///
  /// The checker also looks inside the `#{...}` interpolation of properties,
  /// selectors and comments, but it finds that with `/#\{(.*?)\}/`, which
  /// never spans a line, so no operator there can follow a line break; only
  /// declaration values and at-rule params are scanned.
  pub struct ScssOperatorNoNewlineBefore => ScssOperatorNoNewlineBeforeCheck {
    name: NAME,
    description: "Disallow newlines before Sass operators",
    default_severity: Warning,
  }
}

/// `scss/operator-no-newline-before`, which takes no options.
pub struct ScssOperatorNoNewlineBeforeCheck;

impl ScssOperatorNoNewlineBeforeCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

/// The at-rules upstream leaves alone: Sass module loading, `@at-root`,
/// `@scope` and Tailwind's directives.
const SKIPPED_AT_RULES: &[&str] = &[
  "forward",
  "use",
  "at-root",
  "scope",
  "apply",
  "custom-variant",
  "reference",
  "source",
  "theme",
  "utility",
  "variant",
];

/// A value holding a `data:` URI is skipped whole.
static DATA_URI: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"url\(\s*['"]?data:"#).expect("valid regex"));

impl Check for ScssOperatorNoNewlineBeforeCheck {
  /// Reports each operator in a declaration value or at-rule params that
  /// starts a line.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for node in &tree.nodes {
      match node.kind {
        NodeKind::Decl => {
          if node.name == "unicode-range" || DATA_URI.is_match(&node.value) {
            continue;
          }
          let Some(index) = tree.declaration_value_index(node) else {
            continue;
          };
          // Sass variable values are where `+` and `/` most often do math.
          let is_after_colon = node.name.starts_with('$');
          self.check_string(&node.value, node.start + index, is_after_colon, &mut diags);
        }
        NodeKind::AtRule => {
          if SKIPPED_AT_RULES.contains(&node.name.as_str()) {
            continue;
          }
          let params_start = node.start + tree.at_rule_param_index(node);
          if node.name == "media" || node.name == "import" {
            for (offset, value) in media_query_values(&node.params) {
              self.check_string(value, params_start + offset, true, &mut diags);
            }
          } else {
            self.check_string(&node.params, params_start, true, &mut diags);
          }
        }
        NodeKind::Rule | NodeKind::Comment => {}
      }
    }
    diags
  }
}

impl ScssOperatorNoNewlineBeforeCheck {
  /// Reports each operator in `string`, which starts at `start` in the
  /// source, that has a line break in the whitespace before it
  /// (`checkNewlineBefore`).
  fn check_string(
    &self,
    string: &str,
    start: usize,
    is_after_colon: bool,
    diags: &mut Vec<Diagnostic>,
  ) {
    let bytes = string.as_bytes();
    for operator in find_operators(string, is_after_colon) {
      // Upstream's scan stops before index 0, so a value never counts its
      // own first character.
      let newline_before = bytes[1.min(operator.start)..operator.start]
        .iter()
        .rev()
        .take_while(|&&b| matches!(b, b' ' | b'\n' | b'\t' | b'\r' | b'\x0c'))
        .any(|&b| b == b'\n');
      if !newline_before {
        continue;
      }
      let symbol = &string[operator.start..operator.end];
      diags.push(
        Diagnostic::new(NAME, format!("Unexpected newline before \"{symbol}\""))
          .message_args([symbol])
          .span(Span::new(start + operator.start, symbol.len())),
      );
    }
  }
}

/// The parts of `@media` or `@import` params upstream scans for operators,
/// with their offsets in the params: the value of each media feature
/// (`(min-width: $a + 1)`), and the argument of a `url()` that is quoted or
/// holds whitespace.
///
/// Upstream gets these from `postcss-media-query-parser`; its other nodes
/// (media types, keywords, feature names) are only searched for single-line
/// interpolation, which cannot hold a line break.
fn media_query_values(params: &str) -> Vec<(usize, &str)> {
  let bytes = params.as_bytes();
  let mut values = Vec::new();
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      quote @ (b'"' | b'\'') => i = skip_string(bytes, i, quote),
      b'(' => {
        let close = matching_paren(bytes, i);
        let inner = &params[i + 1..close];
        if params[..i].to_ascii_lowercase().ends_with("url") {
          let trimmed = inner.trim();
          if trimmed.starts_with(['"', '\'']) || trimmed.contains(char::is_whitespace) {
            values.push((i + 1 + inner.find(trimmed).unwrap_or(0), trimmed));
          }
        } else if let Some(colon) = feature_colon(inner) {
          let rest = &inner[colon + 1..];
          let value = rest.trim();
          let leading = rest.len() - rest.trim_start().len();
          values.push((i + 1 + colon + 1 + leading, value));
        }
        i = close + 1;
      }
      _ => i += 1,
    }
  }
  values
}

/// Offset just past the string opened by `quote` at `start`.
fn skip_string(bytes: &[u8], start: usize, quote: u8) -> usize {
  let mut i = start + 1;
  while i < bytes.len() && bytes[i] != quote {
    if bytes[i] == b'\\' {
      i += 1;
    }
    i += 1;
  }
  (i + 1).min(bytes.len())
}

/// Offset of the `)` closing the `(` at `open`, or the end of the text.
fn matching_paren(bytes: &[u8], open: usize) -> usize {
  let mut depth = 0usize;
  let mut i = open;
  while i < bytes.len() {
    match bytes[i] {
      quote @ (b'"' | b'\'') => {
        i = skip_string(bytes, i, quote);
        continue;
      }
      b'(' => depth += 1,
      b')' => {
        depth -= 1;
        if depth == 0 {
          return i;
        }
      }
      _ => {}
    }
    i += 1;
  }
  bytes.len()
}

/// Offset of the `:` separating a media feature from its value: the first
/// one outside strings and `{...}`.
fn feature_colon(feature: &str) -> Option<usize> {
  let bytes = feature.as_bytes();
  let mut braces = 0usize;
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      quote @ (b'"' | b'\'') => {
        i = skip_string(bytes, i, quote);
        continue;
      }
      b'{' => braces += 1,
      b'}' => braces = braces.saturating_sub(1),
      b':' if braces == 0 => return Some(i),
      _ => {}
    }
    i += 1;
  }
  None
}

#[cfg(test)]
mod tests {
  use super::NAME;
  use gale_css_parser::Syntax;
  use gale_diagnostics::SourceLineIndex;
  use serde_json::json;

  use crate::testing::{invalid_options, lint};

  const RULE: &str = "scss/operator-no-newline-before";

  /// The `(line, column, endLine, endColumn, message)` of each warning for
  /// `source`.
  fn warnings(source: &str, syntax: Syntax) -> Vec<(usize, usize, usize, usize, String)> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, syntax)
      .into_iter()
      .map(|d| {
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (line, column, end_line, end_column, d.message)
      })
      .collect()
  }

  /// `(line, column, endLine, endColumn, message)` for an operator.
  fn at(
    line: usize,
    column: usize,
    end_column: usize,
    operator: &str,
  ) -> (usize, usize, usize, usize, String) {
    let message = format!("Unexpected newline before \"{operator}\"");
    (line, column, line, end_column, message)
  }

  #[test]
  fn accepts_upstream_cases() {
    for source in [
      "a { width: 1 +1; }",
      "a { width: s1- +1; }",
      "\n      a {\n        width: 10px +\n          1;\n      }\n    ",
      "\n      a {\n        width: 10px\n          -1;\n      }\n    ",
      "\n      a {\n        width: 10px +\n          1 + 1;\n      }\n    ",
      "\n        :root {\n          --foo: '{{}}';\n        }\n      ",
      "\n      @font-face {\n        font-family: 'Ampersand';\n        src: local('Times New Roman');\n        unicode-range: U+26;\n      }\n      ",
    ] {
      assert_eq!(warnings(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn reports_an_indented_operator_starting_a_line() {
    assert_eq!(
      warnings(
        "\n      a {\n        width: 10px\n        + 1;\n      }\n    ",
        Syntax::Scss
      ),
      vec![at(4, 9, 10, "+")]
    );
  }

  #[test]
  fn reports_an_operator_at_the_start_of_a_line() {
    assert_eq!(
      warnings(
        "\n      a {\n        width: 10px\n+ 1;\n      }\n    ",
        Syntax::Scss
      ),
      vec![at(4, 1, 2, "+")]
    );
  }

  #[test]
  fn reports_in_variables_and_at_rule_params() {
    assert_eq!(
      warnings("$a: 1\n  * 2;\n", Syntax::Scss),
      vec![at(2, 3, 4, "*")]
    );
    assert_eq!(
      warnings("@if $a\n  == 1 { }\n", Syntax::Scss),
      vec![at(2, 3, 5, "==")]
    );
    assert_eq!(
      warnings("@function f($a: 1\n  + 2) { }\n", Syntax::Scss),
      vec![at(2, 3, 4, "+")]
    );
  }

  #[test]
  fn reports_in_media_feature_values_only() {
    assert_eq!(
      warnings("@media (min-width: $a\n  + 1) { }\n", Syntax::Scss),
      vec![at(2, 3, 4, "+")]
    );
    assert_eq!(
      warnings("@media screen\n  and (color) { }\n", Syntax::Scss),
      vec![]
    );
  }

  #[test]
  fn skips_strings_data_uris_and_module_rules() {
    for source in [
      "a { content: 'a\n  + b'; }",
      "a { background: url(data:image/png;base64,iVBOR\n  +w0KGgo) $a\n  + 1; }",
      "@use 'a' with ($b: 1\n  + 2);",
    ] {
      assert_eq!(warnings(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn reports_in_plain_css_too() {
    assert_eq!(
      warnings("a { width: 10px\n  + 1; }", Syntax::Css),
      vec![at(2, 3, 4, "+")]
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/operator-no-newline-before\""]
    );
  }
}
