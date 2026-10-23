use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::value_parser::{self, NodeKind, ValueNode};
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::autoprefixable;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::standard_syntax::is_standard_syntax_property;

const NAME: &str = "value-no-vendor-prefix";

declare_rule! {
  /// Reports vendor-prefixed values (e.g. `-webkit-flex`).
  ///
  /// Equivalent to Stylelint's `value-no-vendor-prefix` rule: every keyword
  /// or function name in a value that Autoprefixer would prefix is reported,
  /// and the fix strips the prefix, keeping the rest as written.
  pub struct ValueNoVendorPrefix => ValueNoVendorPrefixCheck {
    name: NAME,
    description: "Disallow vendor prefixes for values",
    default_severity: Warning,
  }
}

/// `value-no-vendor-prefix` as configured.
pub struct ValueNoVendorPrefixCheck {
  /// `ignoreValues`: values left alone, by name or `/regex/`.
  ignore_values: Matchers,
}

impl ValueNoVendorPrefixCheck {
  /// Stylelint's options: `true`, then `ignoreValues`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_values = options.matchers("ignoreValues");
    options.finish()?;
    Ok(Self { ignore_values })
  }
}

/// Stylelint's `prefixes`, which a value must mention to be looked at.
const PREFIXES: &[&str] = &[
  "-webkit-", "-moz-", "-ms-", "-o-", "-xv-", "-apple-", "-wap-", "-khtml-",
];

impl Check for ValueNoVendorPrefixCheck {
  /// Flags vendor-prefixed values that have a standard equivalent, skipping any
  /// listed in `ignoreValues`.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      let raw = decl.raw_value();
      let value: &str = &raw;
      let fixable = decl.value_fixable();
      let lower = value.to_ascii_lowercase();
      if !PREFIXES.iter().any(|prefix| lower.contains(prefix)) {
        continue;
      }
      if !is_standard_syntax_property(decl.prop()) {
        continue;
      }
      let nodes = value_parser::parse(value);
      value_parser::walk(&nodes, &mut |node: &ValueNode| {
        if !autoprefixable::property_value(node.value) || self.ignore_values.matches(node.value) {
          return true;
        }
        // Where the node's text starts: inside a string's quotes or a
        // comment's delimiters.
        let text_offset = match node.kind {
          NodeKind::String => 1,
          NodeKind::Comment => 2,
          _ => 0,
        };
        let start = decl.value_start() + node.source_index;
        let text = Span::new(start + text_offset, node.value.len());
        let diag = Diagnostic::new(
          NAME,
          format!("Unexpected vendor-prefixed value \"{}\"", node.value),
        )
        .span(Span::new(start, node.value.len()));
        diags.push(if fixable {
          diag.fix(Fix::new(
            format!("Remove the vendor prefix from \"{}\"", node.value),
            vec![Edit::new(text, autoprefixable::unprefix(node.value))],
          ))
        } else {
          diag
        });
        true
      });
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, invalid_options, lint};

  const RULE: &str = "value-no-vendor-prefix";

  #[test]
  fn strips_the_prefix_keeping_the_rest_as_written() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        ".a { display: -wEbKiT-fLeX; }",
        Syntax::Css
      ),
      ".a { display: fLeX; }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        ".a { background: -webkit-linear-gradient(bottom, #000, #fff); }",
        Syntax::Css
      ),
      ".a { background: linear-gradient(bottom, #000, #fff); }"
    );
    assert_eq!(
      fix(RULE, on.clone(), ".a { speak: -xv-digits; }", Syntax::Css),
      ".a { speak: digits; }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        ".a { -webkit-user-select: -moz-all; }",
        Syntax::Css
      ),
      ".a { -webkit-user-select: all; }"
    );
    let warnings = lint(RULE, on, ".a { display: -webkit-flex; }", Syntax::Css);
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (14, 12));
  }

  #[test]
  fn leaves_unlisted_values_and_variables_alone() {
    let on = serde_json::json!(true);
    for css in [
      ".a { display: -webkit-box; }",
      "a { white-space: -pre-wrap; }",
      "a { list-style-type: -moz-ethiopic-halehame; }",
    ] {
      assert!(lint(RULE, on.clone(), css, Syntax::Css).is_empty(), "{css}");
    }
    for css in [
      "a { $foo: -webkit-plaintext; }",
      "a { #{$foo}: -webkit-plaintext; }",
    ] {
      assert!(
        lint(RULE, on.clone(), css, Syntax::Scss).is_empty(),
        "{css}"
      );
    }
  }

  #[test]
  fn ignore_values_takes_a_string_or_a_list() {
    let single = serde_json::json!([true, { "ignoreValues": "/^-moz-hangul$/" }]);
    assert!(
      lint(
        RULE,
        single,
        "a { list-style-type: -moz-hangul; }",
        Syntax::Css
      )
      .is_empty()
    );
    let list = serde_json::json!([true, { "ignoreValues": ["-moz-hangul", "/^-webkit-linear-/"] }]);
    assert_eq!(
      fix(
        RULE,
        list.clone(),
        ".a { list-style-type: -moz-hangul-consonant; }",
        Syntax::Css
      ),
      ".a { list-style-type: hangul-consonant; }"
    );
    assert!(
      lint(
        RULE,
        list,
        "a { b: -webkit-linear-gradient(red, blue) }",
        Syntax::Css
      )
      .is_empty()
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    assert_eq!(
      lint(
        RULE,
        options.clone(),
        "a { display: -webkit-flex }",
        Syntax::Css
      )
      .len(),
      1
    );
    assert_eq!(
      fix(RULE, options, "a { display: -webkit-flex }", Syntax::Css),
      "a { display: -webkit-flex }"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(RULE, serde_json::json!(true)).is_empty());
    assert!(
      invalid_options(
        RULE,
        serde_json::json!([true, {"ignoreValues": ["/^-webkit-/", "-moz-box"]}])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(RULE, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"value-no-vendor-prefix\""]
    );
    assert_eq!(
      invalid_options(RULE, serde_json::json!([true, {"ignoreValue": ["grab"]}])),
      ["Invalid option name \"ignoreValue\" for rule \"value-no-vendor-prefix\""]
    );
  }
}
