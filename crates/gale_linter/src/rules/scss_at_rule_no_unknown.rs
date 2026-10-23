use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::data::{is_known_at_rule_for_syntax, is_stylelint_at_keyword};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "scss/at-rule-no-unknown";

declare_rule! {
  /// SCSS replacement for `at-rule-no-unknown`.
  ///
  /// Like the core rule but recognises SCSS-specific at-rules (`@mixin`,
  /// `@include`, `@if`, `@each`, `@extend`, `@use`, `@forward`, etc.).
  /// Which at-rules are known depends on the file's syntax.
  ///
  /// `ignoreAtRules` lists at-rule names (or `/regex/`es) to leave alone.
  pub struct ScssAtRuleNoUnknown => ScssAtRuleNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown at-rules (SCSS-aware)",
    default_severity: Warning,
  }
}

/// `scss/at-rule-no-unknown` as configured.
pub struct ScssAtRuleNoUnknownCheck {
  /// `ignoreAtRules`: at-rule names (or `/regex/`es) left alone.
  ignore_at_rules: Matchers,
}

impl ScssAtRuleNoUnknownCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules` (names or
  /// `/regex/`es, matched against the name as written, without the `@`).
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_at_rules = options.matchers("ignoreAtRules");
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }
}

impl Check for ScssAtRuleNoUnknownCheck {
  /// Flags at-rules that are neither standard CSS nor SCSS, at the `@name`
  /// word.  Runs on every syntax, as Stylelint does, since configs enable it
  /// project-wide, and leaves the Sass at-rules alone in all of them, as
  /// stylelint-scss does; like the core rule it checks against, it skips
  /// what `isStandardSyntaxAtRule` rejects (Less mixins and variables among
  /// them).
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      let name = at.name();
      if !at.is_standard_syntax()
        || name.starts_with('-')
        || self.ignore_at_rules.matches(name)
        || is_stylelint_at_keyword(name, ctx.syntax)
        || is_known_at_rule_for_syntax(name, Syntax::Scss)
      {
        continue;
      }
      diags.push(
        Diagnostic::new(NAME, format!("Unexpected unknown at-rule \"@{name}\""))
          .span(Span::new(at.start, name.len() + 1)),
      );
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{invalid_options, lint};

  /// The reports for `@name x {}` in `syntax`.
  fn check(name: &str, syntax: Syntax) -> Vec<Diagnostic> {
    lint(NAME, json!(true), &format!("@{name} x {{}}"), syntax)
  }

  #[test]
  fn reports_unknown_in_css() {
    // Stylelint runs scss/at-rule-no-unknown on CSS files too when enabled
    // (typically via stylelint-config-standard-scss), Sass at-rules known.
    assert_eq!(check("tailwind", Syntax::Css).len(), 1);
    assert!(check("mixin", Syntax::Css).is_empty());
    assert!(check("top-center", Syntax::Css).is_empty());
  }

  #[test]
  fn allows_scss_at_rules() {
    for name in [
      "mixin", "include", "if", "each", "extend", "use", "forward", "media",
    ] {
      assert!(check(name, Syntax::Scss).is_empty(), "{name}");
    }
  }

  #[test]
  fn reports_unknown_in_scss_at_the_name() {
    let d = lint(NAME, json!(true), "a { @tailwind base; }", Syntax::Scss);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@tailwind"));
    assert_eq!((d[0].span.offset, d[0].span.length), (4, 9));
  }

  #[test]
  fn skips_vendor_prefixed_and_less_mixins() {
    assert!(check("-webkit-keyframes", Syntax::Scss).is_empty());
    let less = "@var: 1px;\na { .m(); }";
    assert!(lint(NAME, json!(true), less, Syntax::Less).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreAtRules": ["tailwind", "/^my-/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"scss/at-rule-no-unknown\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreAtRules": [1] }])),
      ["Invalid value \"1\" for option \"ignoreAtRules\" of rule \"scss/at-rule-no-unknown\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFunctions": ["x"] }])
      ),
      ["Invalid option name \"ignoreFunctions\" for rule \"scss/at-rule-no-unknown\""]
    );
  }

  #[test]
  fn ignore_at_rules_leaves_listed_at_rules_alone() {
    let source = "@tailwind base;\n@my-thing x;\n@other y;\n";
    let all = lint(NAME, json!(true), source, Syntax::Scss);
    assert_eq!(all.len(), 3);
    let options = json!([true, { "ignoreAtRules": ["tailwind", "/^my-/"] }]);
    let left = lint(NAME, options, source, Syntax::Scss);
    assert_eq!(left.len(), 1);
    assert!(left[0].message.contains("@other"));
  }
}
