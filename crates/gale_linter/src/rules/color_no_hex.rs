use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::value_parser;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

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
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    for decl in tree.walk_decls() {
      let value = &*decl.raw_value();
      let value_start = decl.value_start();
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
                .span(Span::new(value_start + start, 1 + hex_len)),
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

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn check(rule: &dyn Check, value: &str) -> Vec<Diagnostic> {
    let source = format!("a {{ color: {value}; }}");
    rule.check_source(&ctx_with_source(&source))
  }

  #[test]
  fn flags_three_digit_hex() {
    let d = check(&*configured(&ColorNoHex, None), "#fff");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#fff"));
    assert_eq!(d[0].span, Span::new(11, 4));
  }

  #[test]
  fn flags_six_digit_hex() {
    let d = check(&*configured(&ColorNoHex, None), "#ff0000");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("#ff0000"));
  }

  #[test]
  fn allows_named_colors() {
    let rule = configured(&ColorNoHex, None);
    assert!(check(&*rule, "red").is_empty());
    // The CSS parser used to write `white` and `transparent` as hex.
    assert!(check(&*rule, "white").is_empty());
    assert!(check(&*rule, "transparent").is_empty());
  }

  #[test]
  fn allows_rgb_function() {
    let rule = configured(&ColorNoHex, None);
    assert!(check(&*rule, "rgb(255, 0, 0)").is_empty());
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
    assert!(check(&*rule, "theme(#fff)").is_empty());
    assert_eq!(check(&*rule, "mix(#fff, theme(#000))").len(), 1);
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
