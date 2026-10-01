use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::autoprefixable;
use crate::pattern;
use crate::rule::{Rule, RuleContext};
use crate::source_text;
use crate::standard_syntax::is_standard_syntax_property;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Reports vendor-prefixed values (e.g. `-webkit-flex`).
///
/// Equivalent to Stylelint's `value-no-vendor-prefix` rule: every keyword
/// or function name in a value that Autoprefixer would prefix is reported,
/// and the fix strips the prefix, keeping the rest as written.
pub struct ValueNoVendorPrefix;

/// Stylelint's `prefixes`, which a value must mention to be looked at.
const PREFIXES: &[&str] = &[
  "-webkit-", "-moz-", "-ms-", "-o-", "-xv-", "-apple-", "-wap-", "-khtml-",
];

impl Rule for ValueNoVendorPrefix {
  fn name(&self) -> &'static str {
    "value-no-vendor-prefix"
  }

  fn description(&self) -> &'static str {
    "Disallow vendor prefixes for values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags vendor-prefixed values that have a standard equivalent, skipping any
  /// listed in `ignoreValues`.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore_values = ctx.secondary_options().and_then(|v| v.get("ignoreValues"));
    let mut diags = Vec::new();
    for decl in source_text::written_declarations(ctx.source, node, ctx.syntax) {
      let lower = decl.value.to_ascii_lowercase();
      if !PREFIXES.iter().any(|prefix| lower.contains(prefix)) {
        continue;
      }
      if !is_standard_syntax_property(decl.prop) {
        continue;
      }
      let nodes = value_parser::parse(decl.value);
      value_parser::walk(&nodes, &mut |node: &ValueNode| {
        if !autoprefixable::property_value(node.value)
          || pattern::option_matches(ignore_values, node.value)
        {
          return true;
        }
        // Where the node's text starts: inside a string's quotes or a
        // comment's delimiters.
        let text_offset = match node.kind {
          NodeKind::String => 1,
          NodeKind::Comment => 2,
          _ => 0,
        };
        let start = decl.value_start + node.source_index;
        let text = Span::new(start + text_offset, node.value.len());
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected vendor-prefixed value \"{}\"", node.value),
          )
          .severity(self.default_severity())
          .span(Span::new(start, node.value.len()))
          .fix(Fix::new(
            format!("Remove the vendor prefix from \"{}\"", node.value),
            vec![Edit::new(text, autoprefixable::unprefix(node.value))],
          )),
        );
        true
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

  /// Lint `css` as `syntax` with only this rule enabled, configured with
  /// `options`.
  fn lint_as(
    css: &str,
    syntax: Syntax,
    options: serde_json::Value,
  ) -> Vec<gale_diagnostics::Diagnostic> {
    let rule = "value-no-vendor-prefix".to_string();
    let runner = LintRunner::with_options(
      RuleRegistry::default(),
      vec![rule.clone()],
      HashMap::from([(rule, options)]),
    );
    runner.lint_source(css, "test.css", syntax).diagnostics
  }

  /// `css` after applying the rule's fixes until nothing changes, the way
  /// `gale --fix` does.
  fn fix(css: &str, options: serde_json::Value) -> String {
    let mut current = css.to_string();
    for _ in 0..10 {
      let diags = lint_as(&current, Syntax::Css, options.clone());
      let (next, applied) = apply_fixes(&current, &diags);
      if applied == 0 || next == current {
        break;
      }
      current = next;
    }
    current
  }

  #[test]
  fn strips_the_prefix_keeping_the_rest_as_written() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(".a { display: -wEbKiT-fLeX; }", on.clone()),
      ".a { display: fLeX; }"
    );
    assert_eq!(
      fix(
        ".a { background: -webkit-linear-gradient(bottom, #000, #fff); }",
        on.clone()
      ),
      ".a { background: linear-gradient(bottom, #000, #fff); }"
    );
    assert_eq!(
      fix(".a { speak: -xv-digits; }", on.clone()),
      ".a { speak: digits; }"
    );
    assert_eq!(
      fix(".a { -webkit-user-select: -moz-all; }", on.clone()),
      ".a { -webkit-user-select: all; }"
    );
    let warnings = lint_as(".a { display: -webkit-flex; }", Syntax::Css, on);
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
      assert!(lint_as(css, Syntax::Css, on.clone()).is_empty(), "{css}");
    }
    for css in [
      "a { $foo: -webkit-plaintext; }",
      "a { #{$foo}: -webkit-plaintext; }",
    ] {
      assert!(lint_as(css, Syntax::Scss, on.clone()).is_empty(), "{css}");
    }
  }

  #[test]
  fn ignore_values_takes_a_string_or_a_list() {
    let single = serde_json::json!([true, { "ignoreValues": "/^-moz-hangul$/" }]);
    assert!(lint_as("a { list-style-type: -moz-hangul; }", Syntax::Css, single).is_empty());
    let list = serde_json::json!([true, { "ignoreValues": ["-moz-hangul", "/^-webkit-linear-/"] }]);
    assert_eq!(
      fix(
        ".a { list-style-type: -moz-hangul-consonant; }",
        list.clone()
      ),
      ".a { list-style-type: hangul-consonant; }"
    );
    assert!(
      lint_as(
        "a { b: -webkit-linear-gradient(red, blue) }",
        Syntax::Css,
        list
      )
      .is_empty()
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    assert_eq!(
      lint_as("a { display: -webkit-flex }", Syntax::Css, options.clone()).len(),
      1
    );
    assert_eq!(
      fix("a { display: -webkit-flex }", options),
      "a { display: -webkit-flex }"
    );
  }
}
