use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::data::is_stylelint_at_keyword;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::stylelint_version::stylelint_major_version;

const NAME: &str = "at-rule-no-unknown";

declare_rule! {
  /// Disallow unknown at-rules.
  ///
  /// Equivalent to Stylelint's `at-rule-no-unknown` rule, including its
  /// `ignoreAtRules` option (names or `/regex/` entries, matched against the
  /// name as written, without the `@`).
  pub struct AtRuleNoUnknown => AtRuleNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown at-rules",
    default_severity: Warning,
  }
}

/// `at-rule-no-unknown` as configured.
pub struct AtRuleNoUnknownCheck {
  /// `ignoreAtRules`.
  ignore_at_rules: Matchers,
}

impl AtRuleNoUnknownCheck {
  /// Stylelint's options: `true`, then `ignoreAtRules`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_at_rules = options.matchers("ignoreAtRules");
    options.finish()?;
    Ok(Self { ignore_at_rules })
  }
}

impl Check for AtRuleNoUnknownCheck {
  /// Flags at-rules that are not standard for the file's syntax, at the
  /// `@name` word. Vendor-prefixed names, those in `ignoreAtRules` and what
  /// Stylelint's `isStandardSyntaxAtRule` rejects (Less mixins and
  /// variables, `@charset`) are left alone.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      let name = at.name();
      if !at.is_standard_syntax()
        || name.starts_with('-')
        || self.ignore_at_rules.matches(name)
        || is_stylelint_at_keyword(name, ctx.syntax)
      {
        continue;
      }
      let at_rule = format!("@{name}");
      let message = if stylelint_major_version() >= 17 {
        format!("Unknown at-rule \"{at_rule}\"")
      } else {
        format!("Unexpected unknown at-rule \"{at_rule}\"")
      };
      diags.push(
        Diagnostic::new(NAME, message)
          .span(Span::new(at.start, at_rule.len()))
          .message_args([at_rule]),
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
  fn check(name: &str, options: serde_json::Value, syntax: Syntax) -> Vec<Diagnostic> {
    lint(NAME, options, &format!("@{name} x {{}}"), syntax)
  }

  #[test]
  fn reports_unknown_at_the_name() {
    let d = check("tailwind", json!(true), Syntax::Css);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("@tailwind"));
    assert_eq!((d[0].span.offset, d[0].span.length), (0, 9));
    let d = lint(NAME, json!(true), "a { @foo bar; }", Syntax::Css);
    assert_eq!((d[0].span.offset, d[0].span.length), (4, 4));
    // Stylelint's `isStandardSyntaxAtRule` skips a blockless at-rule
    // without params.
    assert!(lint(NAME, json!(true), "@foo;", Syntax::Css).is_empty());
  }

  #[test]
  fn ignore_at_rules_skips_listed_names() {
    // material-ui's config, for Tailwind's at-rules.
    let options = json!([true, { "ignoreAtRules": ["theme", "/^conf/"] }]);
    assert!(check("theme", options.clone(), Syntax::Css).is_empty());
    assert!(check("config", options.clone(), Syntax::Css).is_empty());
    assert_eq!(check("tailwind", options, Syntax::Css).len(), 1);
  }

  #[test]
  fn allows_known() {
    for name in ["media", "keyframes", "MEDIA"] {
      assert!(check(name, json!(true), Syntax::Css).is_empty(), "{name}");
    }
  }

  #[test]
  fn allows_the_rest_of_stylelints_at_keywords() {
    let css = "@page { @top-center { content: 'x'; } }\n\
      @font-feature-values f { @styleset { a: 1; } }\n\
      a { @apply --b; }\n@custom-media --c (width > 1px);";
    assert!(lint(NAME, json!(true), css, Syntax::Css).is_empty());
  }

  #[test]
  fn skips_vendor_prefixed() {
    assert!(check("-webkit-keyframes", json!(true), Syntax::Css).is_empty());
  }

  #[test]
  fn allows_scss_at_rules_in_scss() {
    for name in [
      "mixin", "include", "if", "each", "extend", "use", "forward", "at-root",
    ] {
      assert!(check(name, json!(true), Syntax::Scss).is_empty(), "{name}");
    }
    let scss = "@if $a { b: c } @else if $d { e: f } @else { g: h }";
    assert!(lint(NAME, json!(true), scss, Syntax::Scss).is_empty());
  }

  #[test]
  fn reports_scss_at_rules_in_css() {
    for name in ["mixin", "include", "if"] {
      assert_eq!(check(name, json!(true), Syntax::Css).len(), 1, "{name}");
    }
  }

  #[test]
  fn allows_less_at_rules_and_mixins_in_less() {
    assert!(check("plugin", json!(true), Syntax::Less).is_empty());
    assert!(check("detached-ruleset", json!(true), Syntax::Less).is_empty());
    let less = "@var: 1px;\n.m() { a: b }\na { .m(); @r(); }";
    assert!(lint(NAME, json!(true), less, Syntax::Less).is_empty());
  }

  #[test]
  fn reports_less_at_rules_in_css() {
    assert_eq!(check("plugin", json!(true), Syntax::Css).len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreAtRules": "tailwind" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("never")),
      ["Unexpected option value \"never\" for rule \"at-rule-no-unknown\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreAtRule": ["x"] }])),
      ["Invalid option name \"ignoreAtRule\" for rule \"at-rule-no-unknown\""]
    );
  }
}
