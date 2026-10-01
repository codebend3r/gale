use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::autoprefixable;
use crate::pattern;
use crate::rule::{Rule, RuleContext};
use crate::source_text;

/// Reports vendor-prefixed properties (e.g. `-webkit-transform`).
///
/// Equivalent to Stylelint's `property-no-vendor-prefix` rule.  Only
/// prefixes Autoprefixer would add back are reported, and the fix strips the
/// prefix from the property as written, keeping its case.
pub struct PropertyNoVendorPrefix;

/// Stylelint's `basicKeywords`.
const BASIC_KEYWORDS: &[&str] = &["initial", "inherit", "revert", "revert-layer", "unset"];

impl Rule for PropertyNoVendorPrefix {
  fn name(&self) -> &'static str {
    "property-no-vendor-prefix"
  }

  fn description(&self) -> &'static str {
    "Disallow vendor prefixes for properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags vendor-prefixed properties that have an unprefixed equivalent,
  /// skipping any listed in `ignoreProperties`.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore_props = ctx
      .secondary_options()
      .and_then(|v| v.get("ignoreProperties"));

    let decls: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mut diags = Vec::new();
    for decl in decls {
      if decl.span.length == 0 {
        continue;
      }
      let Some((prop, prop_start)) = source_text::declaration_property(ctx.source, decl) else {
        continue;
      };
      if pattern::option_matches(ignore_props, prop) {
        continue;
      }
      // A vendor prefix, but not a custom property.
      if !prop.starts_with('-') || prop.starts_with("--") {
        continue;
      }
      if !autoprefixable::property(prop) {
        continue;
      }
      // whatwg/compat#28: the prefixed form treats one length differently.
      if prop == "-webkit-background-size" {
        let value = source_text::declaration_value(ctx.source, decl).map_or("", |(v, _)| v);
        if !is_safe_background_size(value) {
          continue;
        }
      }

      let unprefixed = autoprefixable::unprefix(prop);
      let span = Span::new(prop_start, prop.len());
      // Stylelint 15 and older word the message differently.
      let message = if crate::stylelint_version::stylelint_major_version() >= 16 {
        format!("Unexpected vendor-prefixed property \"{prop}\"")
      } else {
        format!("Unexpected vendor-prefix \"{prop}\"")
      };
      diags.push(
        Diagnostic::new(self.name(), message)
          .severity(self.default_severity())
          .span(span)
          .fix(Fix::new(
            format!("Remove vendor prefix from \"{prop}\""),
            vec![Edit::new(span, unprefixed)],
          )),
      );
    }
    diags
  }
}

/// Whether a `-webkit-background-size` value means the same unprefixed:
/// every comma-separated layer has two sizes, or is `auto` or a basic
/// keyword.  Other single sizes are read differently by the prefixed form.
fn is_safe_background_size(value: &str) -> bool {
  value.split(',').all(|layer| {
    let words: Vec<&str> = layer.split_whitespace().collect();
    match words.as_slice() {
      [_, _] => true,
      [first] => *first == "auto" || BASIC_KEYWORDS.contains(first),
      _ => false,
    }
  })
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use gale_css_parser::Syntax;
  use gale_diagnostics::apply_fixes;

  use crate::{LintRunner, RuleRegistry};

  /// Lint `css` with only this rule enabled, configured with `options`.
  fn lint(css: &str, options: serde_json::Value) -> Vec<gale_diagnostics::Diagnostic> {
    let rule = "property-no-vendor-prefix".to_string();
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
  fn strips_the_prefix_keeping_the_property_case() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix("a { -webkit-transform: scale(1); }", on.clone()),
      "a { transform: scale(1); }"
    );
    assert_eq!(
      fix("a { -wEbKiT-tRaNsFoRm: scale(1); }", on.clone()),
      "a { tRaNsFoRm: scale(1); }"
    );
    assert_eq!(
      fix("a { -WEBKIT-TRANSFORM: scale(1); }", on.clone()),
      "a { TRANSFORM: scale(1); }"
    );
    let warnings = lint("a { -moz-columns: 2; }", on);
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (4, 12));
  }

  #[test]
  fn leaves_properties_without_a_standard_form_alone() {
    let on = serde_json::json!(true);
    for css in [
      "a { -webkit-touch-callout: none; }",
      "a { -WEBKIT-TOUCH-CALLOUT: none; }",
      "a { --webkit-transform: 1px; }",
      "a { -webkit-background-size: 1px; }",
      "a { -webkit-background-size: 1px   2px ,   1px; }",
    ] {
      assert!(lint(css, on.clone()).is_empty(), "{css}");
    }
    assert_eq!(
      fix("a { -webkit-background-size: 1px 2px, 2px 1px; }", on),
      "a { background-size: 1px 2px, 2px 1px; }"
    );
  }

  #[test]
  fn ignore_properties_compares_strings_exactly_and_regexes_as_written() {
    let options = serde_json::json!([true, {
      "ignoreProperties": ["-webkit-transform", "/^-webkit-animation-/i"]
    }]);
    assert!(lint("a { -webkit-transform: none; }", options.clone()).is_empty());
    assert!(lint("a { -webkit-ANIMATION-DeLaY: 0.5s; }", options.clone()).is_empty());
    assert_eq!(
      fix("a { -WEBKIT-tranSFoRM: translateY(-50%); }", options),
      "a { tranSFoRM: translateY(-50%); }"
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    assert_eq!(lint("a { -o-columns: 2; }", options.clone()).len(), 1);
    assert_eq!(fix("a { -o-columns: 2; }", options), "a { -o-columns: 2; }");
  }
}
