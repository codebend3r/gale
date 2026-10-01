use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::autoprefixable;
use crate::pattern;
use crate::rule::{Rule, RuleContext};
use crate::selector::postcss::{self, Kind};
use crate::standard_syntax::is_standard_syntax_selector;
use crate::style_rules::scan_style_rules;

/// Disallow vendor prefixes in selectors.
///
/// Equivalent to Stylelint's `selector-no-vendor-prefix` rule.
/// Only flags vendor-prefixed pseudo-classes and pseudo-elements that have
/// standard (unprefixed) equivalents — i.e., ones Autoprefixer can handle.
/// Browser-specific pseudo-elements/classes (like `::-webkit-slider-thumb`)
/// that have no standard equivalent are NOT flagged.  The fix strips the
/// prefix and keeps the rest of the name as written.
pub struct SelectorNoVendorPrefix;

/// Stylelint's `prefixes`, which a selector must mention to be looked at.
const PREFIXES: &[&str] = &[
  "-webkit-", "-moz-", "-ms-", "-o-", "-xv-", "-apple-", "-wap-", "-khtml-",
];

impl Rule for SelectorNoVendorPrefix {
  fn name(&self) -> &'static str {
    "selector-no-vendor-prefix"
  }

  fn description(&self) -> &'static str {
    "Disallow vendor prefixes for selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags vendor-prefixed pseudos that have a standard equivalent, in every
  /// style rule's selector as written, skipping `ignoreSelectors`.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore = ctx
      .secondary_options()
      .and_then(|v| v.get("ignoreSelectors"));
    let mut diags = Vec::new();
    for rule in scan_style_rules(ctx.source, ctx.syntax) {
      let lower = rule.prelude.to_ascii_lowercase();
      if !PREFIXES.iter().any(|prefix| lower.contains(prefix))
        || !is_standard_syntax_selector(&rule.prelude)
      {
        continue;
      }
      let Some(selectors) = postcss::parse(&rule.prelude, rule.offset) else {
        continue;
      };
      postcss::walk(&selectors, &mut |visit| {
        let pseudo = visit.node;
        if pseudo.kind != Kind::Pseudo
          || !autoprefixable::selector(&pseudo.value)
          || pattern::option_matches(ignore, &pseudo.value)
        {
          return;
        }
        let span = Span::new(pseudo.start, pseudo.value.len());
        let unprefixed = autoprefixable::unprefix(&pseudo.value);
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected vendor-prefixed selector \"{}\"", pseudo.value),
          )
          .severity(self.default_severity())
          .span(span)
          .fix(Fix::new(
            format!("Change \"{}\" to \"{unprefixed}\"", pseudo.value),
            vec![Edit::new(span, unprefixed)],
          )),
        );
      });
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use gale_css_parser::Syntax;
  use gale_diagnostics::apply_fixes;

  use crate::{LintRunner, RuleRegistry};

  /// Lint `css` with only this rule enabled, configured with `options`.
  fn lint(css: &str, options: serde_json::Value) -> Vec<gale_diagnostics::Diagnostic> {
    let rule = "selector-no-vendor-prefix".to_string();
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
  fn strips_the_prefix_keeping_the_name_as_written() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(":-wEbKiT-fUlL-sCrEeN a {}", on.clone()),
      ":fUlL-sCrEeN a {}"
    );
    assert_eq!(
      fix("input::-ms-clear + input::-moz-placeholder {}", on.clone()),
      "input::-ms-clear + input::placeholder {}"
    );
    let warnings = lint("body, :-ms-fullscreen a {}", on);
    assert_eq!(warnings.len(), 1);
    assert_eq!(
      warnings[0].message,
      "Unexpected vendor-prefixed selector \":-ms-fullscreen\""
    );
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (6, 15));
  }

  #[test]
  fn leaves_attribute_values_and_unlisted_pseudos_alone() {
    let on = serde_json::json!(true);
    for css in [
      "a[data-foo=\":-webkit-full-screen\"] {}",
      "input::-webkit-slider-thumb {}",
      ":fullscreen a {}",
    ] {
      assert!(lint(css, on.clone()).is_empty(), "{css}");
    }
  }

  #[test]
  fn ignore_selectors_matches_strings_and_regexes() {
    let options = serde_json::json!([true, {
      "ignoreSelectors": ["::-webkit-input-placeholder", "/-moz-.*/", "/-screen$/"]
    }]);
    for css in [
      "input::-webkit-input-placeholder {}",
      "input::-moz-placeholder {}",
      ":-webkit-full-screen a {}",
    ] {
      assert!(lint(css, options.clone()).is_empty(), "{css}");
    }
    assert_eq!(
      fix("input::-ms-input-placeholder {}", options),
      "input::input-placeholder {}"
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    assert_eq!(lint(":-ms-fullscreen {}", options.clone()).len(), 1);
    assert_eq!(fix(":-ms-fullscreen {}", options), ":-ms-fullscreen {}");
  }
}
