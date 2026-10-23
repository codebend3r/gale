use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::value_parser::{self, ValueNode};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/function-color-channel";

declare_rule! {
  /// Require `color.channel()` in place of the Sass functions that read one
  /// channel of a colour (`color.alpha()`, `red()`, `opacity()` and the
  /// rest).
  ///
  /// Equivalent to stylelint-scss's `scss/function-color-channel`.  Names are
  /// matched exactly, and nothing in a `filter` declaration is reported, since
  /// `opacity()` there is the CSS filter function.
  pub struct ScssFunctionColorChannel => ScssFunctionColorChannelCheck {
    name: NAME,
    description: "Require the color.channel function",
    default_severity: Warning,
  }
}

/// `scss/function-color-channel`, which takes no options.
pub struct ScssFunctionColorChannelCheck;

impl ScssFunctionColorChannelCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

/// The functions upstream reports.
const FUNCTION_NAMES: &[&str] = &[
  "color.alpha",
  "color.blackness",
  "color.blue",
  "color.green",
  "color.hue",
  "color.lightness",
  "color.red",
  "color.saturation",
  "color.whiteness",
  "alpha",
  "blackness",
  "blue",
  "green",
  "hue",
  "lightness",
  "opacity",
  "red",
  "saturation",
];

impl Check for ScssFunctionColorChannelCheck {
  /// Reports the name of every channel function called in a declaration
  /// value, outside `filter`.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return Vec::new();
    }
    let mut diags = Vec::new();

    for decl in tree.walk_decls() {
      if decl.name == "filter" {
        continue;
      }
      let Some(value_index) = decl.value_index() else {
        continue;
      };
      let parsed = value_parser::parse(&decl.value);
      value_parser::walk(&parsed, &mut |node: &ValueNode<'_>| {
        if node.is_function() && FUNCTION_NAMES.contains(&node.value) {
          diags.push(
            Diagnostic::new(NAME, "Expected the color.channel function to be used").span(
              Span::new(
                decl.start + value_index + node.source_index,
                node.value.len(),
              ),
            ),
          );
        }
        true
      });
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::NAME;
  use gale_css_parser::Syntax;
  use gale_diagnostics::SourceLineIndex;
  use serde_json::json;

  use crate::testing::{invalid_options, lint};

  const RULE: &str = "scss/function-color-channel";

  /// The `(line, column, endLine, endColumn)` of each warning for `source`.
  fn ranges(source: &str, syntax: Syntax) -> Vec<(usize, usize, usize, usize)> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, syntax)
      .iter()
      .map(|d| {
        assert_eq!(d.message, "Expected the color.channel function to be used");
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (line, column, end_line, end_column)
      })
      .collect()
  }

  #[test]
  fn accepts_color_channel_scale_color_and_filters() {
    for source in [
      "\n        @use 'sass:color';\n        p {\n          opacity: color.channel(hsl(80deg 30% 50%), \"opacity\");\n        }\n      ",
      "\n        p {\n          color: scale-color(blue, $alpha: -40%);\n        }\n      ",
      "\n        p {\n          filter: opacity(50%);\n        }\n      ",
      "\n        p {\n          filter: drop-shadow(0 0 5px black) opacity(50%);\n        }\n      ",
    ] {
      assert_eq!(ranges(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn reports_module_channel_functions() {
    for name in [
      "alpha",
      "blackness",
      "blue",
      "green",
      "hue",
      "lightness",
      "red",
      "saturation",
      "whiteness",
    ] {
      let source = format!(
        "\n        @use 'sass:color';\n        p {{\n          opacity: color.{name}(#e1d7d2);\n        }}\n      "
      );
      let end = 20 + "color.".len() + name.len();
      assert_eq!(
        ranges(&source, Syntax::Scss),
        vec![(4, 20, 4, end)],
        "{source}"
      );
    }
  }

  #[test]
  fn reports_global_channel_functions() {
    for name in [
      "alpha",
      "opacity",
      "blackness",
      "blue",
      "green",
      "hue",
      "lightness",
      "red",
      "saturation",
    ] {
      let source =
        format!("\n        p {{\n          opacity: {name}(#e1d7d2);\n        }}\n      ");
      let end = 20 + name.len();
      assert_eq!(
        ranges(&source, Syntax::Scss),
        vec![(3, 20, 3, end)],
        "{source}"
      );
    }
  }

  #[test]
  fn reports_nested_calls_and_variables() {
    assert_eq!(
      ranges("$a: rgba(red(#fff), 0, 0, alpha($c));", Syntax::Scss),
      vec![(1, 10, 1, 13), (1, 27, 1, 32)]
    );
  }

  #[test]
  fn matches_names_exactly() {
    for source in [
      "a { b: Alpha(#fff); }",
      "a { b: color.whiteness-x(#fff); }",
      "a { b: whiteness(#fff); }",
      "a { b: (1 + 2); }",
    ] {
      assert_eq!(ranges(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn ignores_plain_css() {
    assert_eq!(ranges("a { opacity: alpha(#fff); }", Syntax::Css), vec![]);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    // Upstream validates no secondary options, so it ignores them all.
    assert!(invalid_options(NAME, serde_json::json!([true, { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/function-color-channel\""]
    );
  }
}
