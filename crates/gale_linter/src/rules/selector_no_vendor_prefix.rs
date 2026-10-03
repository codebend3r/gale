use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::autoprefixable;
use crate::pattern;
use crate::rule::{Rule, RuleContext};
use crate::selector::postcss::{self, Kind};
use crate::standard_syntax::is_standard_syntax_selector;

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
    for rule in &ctx.scanned_rules().style_rules {
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
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "selector-no-vendor-prefix";

  #[test]
  fn strips_the_prefix_keeping_the_name_as_written() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(RULE, on.clone(), ":-wEbKiT-fUlL-sCrEeN a {}", Syntax::Css),
      ":fUlL-sCrEeN a {}"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "input::-ms-clear + input::-moz-placeholder {}",
        Syntax::Css
      ),
      "input::-ms-clear + input::placeholder {}"
    );
    let warnings = lint(RULE, on, "body, :-ms-fullscreen a {}", Syntax::Css);
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
      assert!(lint(RULE, on.clone(), css, Syntax::Css).is_empty(), "{css}");
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
      assert!(
        lint(RULE, options.clone(), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
    assert_eq!(
      fix(
        RULE,
        options,
        "input::-ms-input-placeholder {}",
        Syntax::Css
      ),
      "input::input-placeholder {}"
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    assert_eq!(
      lint(RULE, options.clone(), ":-ms-fullscreen {}", Syntax::Css).len(),
      1
    );
    assert_eq!(
      fix(RULE, options, ":-ms-fullscreen {}", Syntax::Css),
      ":-ms-fullscreen {}"
    );
  }
}
