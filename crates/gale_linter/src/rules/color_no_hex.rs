use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::value_parser;

const NAME: &str = "color-no-hex";

declare_rule! {
  /// Disallow hex colors entirely.
  ///
  /// Any hex color (`#` followed by hex digits) in a declaration value is flagged.
  ///
  /// Equivalent to Stylelint's `color-no-hex` rule.
  pub struct ColorNoHex => ColorNoHexCheck {
    name: NAME,
    description: "Disallow hex colors",
    default_severity: Warning,
  }
}

/// `color-no-hex` as configured.
pub struct ColorNoHexCheck {
  /// `ignoreFunctions`: hex colors inside a matching function are allowed.
  ignore_functions: Matchers,
}

impl ColorNoHexCheck {
  /// Stylelint's options: `true`, then `ignoreFunctions`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_functions = options.matchers("ignoreFunctions");
    options.finish()?;
    Ok(Self { ignore_functions })
  }

  /// The byte ranges of `value` inside functions listed in
  /// `ignoreFunctions`, whose hex colors are allowed.
  fn ignored_ranges(&self, value: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    if self.ignore_functions.is_empty() || !value.contains('(') {
      return ranges;
    }
    value_parser::walk(&value_parser::parse(value), &mut |node| {
      if node.is_function() && self.ignore_functions.matches(node.value) {
        ranges.push((node.source_index, node.source_end_index));
        return false;
      }
      true
    });
    ranges
  }
}

impl Check for ColorNoHexCheck {
  /// Flags every valid hex color (3, 4, 6 or 8 digits) in a declaration value.
  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let declarations: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mut diags = Vec::new();

    for decl in declarations {
      let value = &decl.value;
      let ignored = self.ignored_ranges(value);
      let bytes = value.as_bytes();
      let len = bytes.len();
      let mut i = 0;

      while i < len {
        if bytes[i] == b'#' {
          let start = i;
          i += 1;
          let hex_start = i;
          while i < len && bytes[i].is_ascii_hexdigit() {
            i += 1;
          }
          let hex_len = i - hex_start;

          // Only flag valid hex color lengths: 3, 4, 6, 8
          if matches!(hex_len, 3 | 4 | 6 | 8)
            && !ignored
              .iter()
              .any(|&(from, to)| (from..to).contains(&start))
          {
            let hex_str = &value[start..start + 1 + hex_len];
            diags.push(
              Diagnostic::new(NAME, format!("Unexpected hex color \"{hex_str}\""))
                .span(Span::new(decl.span.offset + start, 1 + hex_len)),
            );
          }
        } else {
          i += 1;
        }
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, ctx, invalid_options};

  fn style_with_value(val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, val.len()),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn flags_three_digit_hex() {
    let rule = configured(&ColorNoHex, None);
    let d = rule.check(&style_with_value("#fff"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#fff"));
  }

  #[test]
  fn flags_six_digit_hex() {
    let rule = configured(&ColorNoHex, None);
    let d = rule.check(&style_with_value("#ff0000"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ff0000"));
  }

  #[test]
  fn allows_named_colors() {
    let rule = configured(&ColorNoHex, None);
    let d = rule.check(&style_with_value("red"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_rgb_function() {
    let rule = configured(&ColorNoHex, None);
    let d = rule.check(&style_with_value("rgb(255, 0, 0)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(ColorNoHex::META.name, "color-no-hex");
  }

  #[test]
  fn ignore_functions_allows_hex_colors_inside_them() {
    let rule = configured(
      &ColorNoHex,
      serde_json::json!([true, { "ignoreFunctions": ["/^theme$/"] }]),
    );
    assert!(
      rule
        .check(&style_with_value("theme(#fff)"), &ctx())
        .is_empty()
    );
    assert_eq!(
      rule
        .check(&style_with_value("mix(#fff, theme(#000))"), &ctx())
        .len(),
      1
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFunctions": "theme" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("never")),
      ["Unexpected option value \"never\" for rule \"color-no-hex\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": ["theme"] }])),
      ["Invalid option name \"ignore\" for rule \"color-no-hex\""]
    );
  }
}
