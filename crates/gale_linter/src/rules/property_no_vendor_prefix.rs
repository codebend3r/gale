use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::autoprefixable;
use crate::data::CSS_WIDE_KEYWORDS;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::source_text;

const NAME: &str = "property-no-vendor-prefix";

declare_rule! {
  /// Reports vendor-prefixed properties (e.g. `-webkit-transform`).
  ///
  /// Equivalent to Stylelint's `property-no-vendor-prefix` rule.  Only
  /// prefixes Autoprefixer would add back are reported, and the fix strips the
  /// prefix from the property as written, keeping its case.
  pub struct PropertyNoVendorPrefix => PropertyNoVendorPrefixCheck {
    name: NAME,
    description: "Disallow vendor prefixes for properties",
    default_severity: Warning,
  }
}

/// `property-no-vendor-prefix` as configured.
pub struct PropertyNoVendorPrefixCheck {
  /// `ignoreProperties`: names or `/regex/` entries.
  ignore_properties: Matchers,
}

impl PropertyNoVendorPrefixCheck {
  /// Stylelint's options: `true`, then `ignoreProperties`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_properties = options.matchers("ignoreProperties");
    options.finish()?;
    Ok(Self { ignore_properties })
  }
}

impl Check for PropertyNoVendorPrefixCheck {
  /// Flags vendor-prefixed properties that have an unprefixed equivalent,
  /// skipping any listed in `ignoreProperties`.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
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
      if self.ignore_properties.matches(prop) {
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
      diags.push(Diagnostic::new(NAME, message).span(span).fix(Fix::new(
        format!("Remove vendor prefix from \"{prop}\""),
        vec![Edit::new(span, unprefixed)],
      )));
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
      [first] => *first == "auto" || CSS_WIDE_KEYWORDS.contains(first),
      _ => false,
    }
  })
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "property-no-vendor-prefix";

  #[test]
  fn strips_the_prefix_keeping_the_property_case() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { -webkit-transform: scale(1); }",
        Syntax::Css
      ),
      "a { transform: scale(1); }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { -wEbKiT-tRaNsFoRm: scale(1); }",
        Syntax::Css
      ),
      "a { tRaNsFoRm: scale(1); }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { -WEBKIT-TRANSFORM: scale(1); }",
        Syntax::Css
      ),
      "a { TRANSFORM: scale(1); }"
    );
    let warnings = lint(RULE, on, "a { -moz-columns: 2; }", Syntax::Css);
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
      assert!(lint(RULE, on.clone(), css, Syntax::Css).is_empty(), "{css}");
    }
    assert_eq!(
      fix(
        RULE,
        on,
        "a { -webkit-background-size: 1px 2px, 2px 1px; }",
        Syntax::Css
      ),
      "a { background-size: 1px 2px, 2px 1px; }"
    );
  }

  #[test]
  fn ignore_properties_compares_strings_exactly_and_regexes_as_written() {
    let options = serde_json::json!([true, {
      "ignoreProperties": ["-webkit-transform", "/^-webkit-animation-/i"]
    }]);
    assert!(
      lint(
        RULE,
        options.clone(),
        "a { -webkit-transform: none; }",
        Syntax::Css
      )
      .is_empty()
    );
    assert!(
      lint(
        RULE,
        options.clone(),
        "a { -webkit-ANIMATION-DeLaY: 0.5s; }",
        Syntax::Css
      )
      .is_empty()
    );
    assert_eq!(
      fix(
        RULE,
        options,
        "a { -WEBKIT-tranSFoRM: translateY(-50%); }",
        Syntax::Css
      ),
      "a { tranSFoRM: translateY(-50%); }"
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    assert_eq!(
      lint(RULE, options.clone(), "a { -o-columns: 2; }", Syntax::Css).len(),
      1
    );
    assert_eq!(
      fix(RULE, options, "a { -o-columns: 2; }", Syntax::Css),
      "a { -o-columns: 2; }"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        RULE,
        serde_json::json!([true, { "ignoreProperties": "/^-webkit-/" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(RULE, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"property-no-vendor-prefix\""]
    );
    assert_eq!(
      invalid_options(RULE, serde_json::json!([true, { "ignore": ["a"] }])),
      ["Invalid option name \"ignore\" for rule \"property-no-vendor-prefix\""]
    );
  }
}
