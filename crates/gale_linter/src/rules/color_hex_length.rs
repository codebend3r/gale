use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::source_text;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Enforces hex color length.
///
/// With "short" (default): reports hex colors that can be shortened (e.g. #ffffff → #fff).
/// With "long": reports short hex colors that can be expanded (e.g. #fff → #ffffff).
///
/// Equivalent to Stylelint's `color-hex-length` rule.
pub struct ColorHexLength;

impl Rule for ColorHexLength {
  fn name(&self) -> &'static str {
    "color-hex-length"
  }

  fn description(&self) -> &'static str {
    "Specify short or long notation for hex colors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags hex colors that could be shortened under "short", or expanded under
  /// "long", and offers the rewritten value as a fix.
  ///
  /// Like Stylelint, it looks only at whole words of the value as written:
  /// hex digits inside strings, comments and `url()` are not colors, and the
  /// fix keeps the case of the digits it keeps.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let decls: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    // Primary option: "short" (default) or "long".
    let long = ctx.primary_option_str() == Some("long");

    let mut diags = Vec::new();
    for decl in decls {
      if decl.span.length == 0 {
        continue;
      }
      let Some((value, value_start)) = source_text::declaration_value(ctx.source, decl) else {
        continue;
      };
      if !value.contains('#') {
        continue;
      }
      let nodes = value_parser::parse(value);
      value_parser::walk(&nodes, &mut |node: &ValueNode| {
        if node.is_function_named("url") {
          return false;
        }
        if node.kind != NodeKind::Word || !is_valid_hex(node.value) {
          return true;
        }
        let hex = node.value;
        let expected = if long {
          if !can_expand(hex) {
            return true;
          }
          expand(hex)
        } else {
          if !can_shorten(hex) {
            return true;
          }
          shorten(hex)
        };
        let span = Span::new(value_start + node.source_index, hex.len());
        let verb = if long { "Expand" } else { "Shorten" };
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Expected \"{hex}\" to be \"{expected}\""),
          )
          .severity(self.default_severity())
          .span(span)
          .fix(Fix::new(
            format!("{verb} to \"{expected}\""),
            vec![Edit::new(span, &expected)],
          )),
        );
        true
      });
    }
    diags
  }
}

/// Whether `word` is a hex color as Stylelint's `isValidHex` sees it: `#`
/// followed by 3, 4, 6 or 8 hex digits.
fn is_valid_hex(word: &str) -> bool {
  word.strip_prefix('#').is_some_and(|digits| {
    matches!(digits.len(), 3 | 4 | 6 | 8) && digits.bytes().all(|b| b.is_ascii_hexdigit())
  })
}

/// Check if a 6-digit hex can be shortened to 3, or 8-digit to 4.  Digits
/// compare case-insensitively.
fn can_shorten(hex: &str) -> bool {
  let digits = hex[1..].to_ascii_lowercase();
  let digits = digits.as_bytes();
  matches!(digits.len(), 6 | 8) && digits.chunks(2).all(|pair| pair[0] == pair[1])
}

/// Collapses a 6- or 8-digit hex to its 3- or 4-digit form, keeping the
/// first digit of each pair as written (`#FfaAFF` becomes `#FaF`).
fn shorten(hex: &str) -> String {
  let mut short = String::from("#");
  short.extend(hex[1..].chars().step_by(2));
  short
}

/// Check if a 3-digit hex can be expanded to 6, or 4-digit to 8.
fn can_expand(hex: &str) -> bool {
  let digits = hex.len() - 1; // minus the '#'
  matches!(digits, 3 | 4)
}

/// Doubles each digit of a 3- or 4-digit hex to its 6- or 8-digit form,
/// keeping its case (`#Ffa` becomes `#FFffaa`).
fn expand(hex: &str) -> String {
  let mut long = String::from("#");
  for c in hex[1..].chars() {
    long.push(c);
    long.push(c);
  }
  long
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "color-hex-length";

  #[test]
  fn shortens_keeping_the_case_of_the_kept_digits() {
    let short = serde_json::json!("short");
    assert_eq!(
      fix(RULE, short.clone(), "a { color: #FFFFFF; }", Syntax::Css),
      "a { color: #FFF; }"
    );
    assert_eq!(
      fix(RULE, short.clone(), "a { color: #FfaAFF; }", Syntax::Css),
      "a { color: #FaF; }"
    );
    assert_eq!(
      fix(
        RULE,
        short.clone(),
        "a { something: #fff, #aba, #00ffAAaa; }",
        Syntax::Css
      ),
      "a { something: #fff, #aba, #0fAa; }"
    );
    let warnings = lint(RULE, short, "a { color: #FFFFFF; }", Syntax::Css);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].message, "Expected \"#FFFFFF\" to be \"#FFF\"");
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (11, 7));
  }

  #[test]
  fn expands_keeping_the_case_of_each_digit() {
    let long = serde_json::json!("long");
    assert_eq!(
      fix(RULE, long.clone(), "a { color: #Ffa; }", Syntax::Css),
      "a { color: #FFffaa; }"
    );
    assert_eq!(
      fix(RULE, long, "a { color: #0a0a; }", Syntax::Css),
      "a { color: #00aa00aa; }"
    );
  }

  #[test]
  fn ignores_hex_inside_strings_comments_and_urls() {
    for css in [
      "a::before { content: \"#ABABAB\"; }",
      "a { color: white /* #FFFFFF */; }",
      "a { background: url(somefile.swvg#abcdef)}",
      "a { color: #ffffffa; }",
      "a { color: #f0f0f0 #fffa; }",
    ] {
      assert!(
        lint(RULE, serde_json::json!("short"), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
    assert!(
      lint(
        RULE,
        serde_json::json!("long"),
        "a { b: url(x.svg#abc) }",
        Syntax::Css
      )
      .is_empty()
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!(["short", { "disableFix": true }]);
    assert_eq!(
      lint(RULE, options.clone(), "a { color: #FFFFFF; }", Syntax::Css).len(),
      1
    );
    assert_eq!(
      fix(RULE, options, "a { color: #FFFFFF; }", Syntax::Css),
      "a { color: #FFFFFF; }"
    );
  }
}
