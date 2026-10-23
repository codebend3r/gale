use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::selector::postcss::{self, Kind};
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::autoprefixable;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-no-vendor-prefix";

declare_rule! {
  /// Disallow vendor prefixes in selectors.
  ///
  /// Equivalent to Stylelint's `selector-no-vendor-prefix` rule.
  /// Only flags vendor-prefixed pseudo-classes and pseudo-elements that have
  /// standard (unprefixed) equivalents — i.e., ones Autoprefixer can handle.
  /// Browser-specific pseudo-elements/classes (like `::-webkit-slider-thumb`)
  /// that have no standard equivalent are NOT flagged.  The fix strips the
  /// prefix and keeps the rest of the name as written.
  pub struct SelectorNoVendorPrefix => SelectorNoVendorPrefixCheck {
    name: NAME,
    description: "Disallow vendor prefixes for selectors",
    default_severity: Warning,
  }
}

/// `selector-no-vendor-prefix` as configured.
pub struct SelectorNoVendorPrefixCheck {
  /// The `ignoreSelectors` option.
  ignore: Matchers,
}

impl SelectorNoVendorPrefixCheck {
  /// Stylelint's options: nothing but `true`, then `ignoreSelectors`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore = options.matchers("ignoreSelectors");
    options.finish()?;
    Ok(Self { ignore })
  }
}

/// Stylelint's `prefixes`, which a selector must mention to be looked at.
const PREFIXES: &[&str] = &[
  "-webkit-", "-moz-", "-ms-", "-o-", "-xv-", "-apple-", "-wap-", "-khtml-",
];

impl Check for SelectorNoVendorPrefixCheck {
  /// Flags vendor-prefixed pseudos that have a standard equivalent, in every
  /// style rule's selector as written, skipping `ignoreSelectors`.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for rule in tree
      .walk_rules()
      .filter(|rule| rule.is_standard_syntax() && !rule.in_keyframes())
    {
      let prelude = rule.raw_selector();
      let first = diags.len();
      let lower = prelude.to_ascii_lowercase();
      if !PREFIXES.iter().any(|prefix| lower.contains(prefix)) {
        continue;
      }
      let Some(selectors) = postcss::parse(&prelude, rule.selector_start()) else {
        continue;
      };
      postcss::walk(&selectors, &mut |visit| {
        let pseudo = visit.node;
        if pseudo.kind != Kind::Pseudo
          || !autoprefixable::selector(&pseudo.value)
          || self.ignore.matches(&pseudo.value)
        {
          return;
        }
        let span = Span::new(pseudo.start, pseudo.value.len());
        let unprefixed = autoprefixable::unprefix(&pseudo.value);
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Unexpected vendor-prefixed selector \"{}\"", pseudo.value),
          )
          .span(span)
          .fix(Fix::new(
            format!("Change \"{}\" to \"{unprefixed}\"", pseudo.value),
            vec![Edit::new(span, unprefixed)],
          )),
        );
      });
      if !rule.selector_fixable() {
        diags[first..].iter_mut().for_each(|diag| diag.fix = None);
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, invalid_options, lint};

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

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        RULE,
        serde_json::json!([true, { "ignoreSelectors": ["::-moz-placeholder", "/^:-webkit-/"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(RULE, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"selector-no-vendor-prefix\""]
    );
    assert_eq!(
      invalid_options(RULE, serde_json::json!([true, { "ignore": ["x"] }])),
      ["Invalid option name \"ignore\" for rule \"selector-no-vendor-prefix\""]
    );
  }
}
