use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::pattern;
use crate::rule::{Rule, RuleContext};
use crate::value_parser::{self, NodeKind, ValueNode};

/// Stylelint's `DEPRECATED_PROPS_REMAP`: deprecated properties and their
/// replacements, sorted for binary search.  `None` means there is no
/// replacement to fix to.
static DEPRECATED_PROPERTIES: &[(&str, Option<&str>)] = &[
  ("-khtml-box-align", Some("align-items")),
  ("-khtml-box-direction", None),
  ("-khtml-box-flex", Some("flex-grow")),
  ("-khtml-box-lines", None),
  ("-khtml-box-ordinal-group", Some("order")),
  ("-khtml-box-orient", None),
  ("-khtml-box-pack", None),
  ("-khtml-user-modify", None),
  ("-moz-box-align", Some("align-items")),
  ("-moz-box-direction", None),
  ("-moz-box-flex", Some("flex-grow")),
  ("-moz-box-lines", None),
  ("-moz-box-ordinal-group", Some("order")),
  ("-moz-box-orient", None),
  ("-moz-box-pack", None),
  ("-moz-user-modify", None),
  ("-ms-box-align", Some("align-items")),
  ("-ms-box-direction", None),
  ("-ms-box-flex", Some("flex-grow")),
  ("-ms-box-lines", None),
  ("-ms-box-ordinal-group", Some("order")),
  ("-ms-box-orient", None),
  ("-ms-box-pack", None),
  ("-webkit-box-align", Some("align-items")),
  ("-webkit-box-direction", None),
  ("-webkit-box-flex", Some("flex-grow")),
  ("-webkit-box-lines", None),
  ("-webkit-box-ordinal-group", Some("order")),
  ("-webkit-box-orient", None),
  ("-webkit-box-pack", None),
  ("-webkit-user-modify", None),
  ("clip", None),
  ("grid-column-gap", Some("column-gap")),
  ("grid-gap", Some("gap")),
  ("grid-row-gap", Some("row-gap")),
  ("ime-mode", None),
  ("page-break-after", Some("break-after")),
  ("page-break-before", Some("break-before")),
  ("page-break-inside", Some("break-inside")),
  ("position-try-options", Some("position-try-fallbacks")),
  ("scroll-snap-coordinate", None),
  ("scroll-snap-destination", None),
  ("scroll-snap-margin", Some("scroll-margin")),
  ("scroll-snap-margin-bottom", Some("scroll-margin-bottom")),
  ("scroll-snap-margin-left", Some("scroll-margin-left")),
  ("scroll-snap-margin-right", Some("scroll-margin-right")),
  ("scroll-snap-margin-top", Some("scroll-margin-top")),
  ("scroll-snap-points-x", None),
  ("scroll-snap-points-y", None),
  ("scroll-snap-type-x", None),
  ("scroll-snap-type-y", None),
  ("word-wrap", Some("overflow-wrap")),
];

/// Stylelint's `VALUE_REMAP`: values that change along with the property.
fn remapped_value(property: &str, value: &str) -> Option<&'static str> {
  match (property, value) {
    ("page-break-before" | "page-break-after", "always") => Some("page"),
    _ => None,
  }
}

/// Looks `name` (lower-case) up in the deprecated table, yielding its
/// replacement if it has one.
fn find_deprecated_property(name: &str) -> Option<Option<&'static str>> {
  DEPRECATED_PROPERTIES
    .binary_search_by_key(&name, |(k, _)| k)
    .ok()
    .map(|idx| DEPRECATED_PROPERTIES[idx].1)
}

/// Disallow deprecated properties.
///
/// Equivalent to Stylelint's `property-no-deprecated` rule.  A property
/// with a standard replacement is fixed to it (`page-break-before: always`
/// becomes `break-before: page`); the others are reported without a fix.
pub struct PropertyNoDeprecated;

impl Rule for PropertyNoDeprecated {
  fn name(&self) -> &'static str {
    "property-no-deprecated"
  }

  fn description(&self) -> &'static str {
    "Disallow deprecated properties"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags deprecated properties, naming the replacement when one exists,
  /// and skipping `ignoreProperties`.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let ignore = ctx
      .secondary_options()
      .and_then(|v| v.get("ignoreProperties"));
    let mut diags = Vec::new();
    for &decl in ctx.written_declarations(node).iter() {
      let prop = decl.prop;
      // Sass and Less variables are not properties.
      if prop.starts_with('$') || (prop.starts_with('@') && !prop.starts_with("@{")) {
        continue;
      }
      if pattern::option_matches(ignore, prop) {
        continue;
      }
      let lower = prop.to_ascii_lowercase();
      let Some(replacement) = find_deprecated_property(&lower) else {
        continue;
      };
      if lower == "-webkit-box-orient" && decl.value.eq_ignore_ascii_case("vertical") {
        continue;
      }
      let span = Span::new(decl.prop_start, prop.len());
      let mut diag = Diagnostic::new(
        self.name(),
        match replacement {
          Some(repl) => format!("Expected \"{prop}\" to be \"{repl}\""),
          None => format!("Unexpected deprecated property \"{prop}\""),
        },
      )
      .severity(self.default_severity())
      .span(span);
      if let Some(repl) = replacement {
        let mut edits = vec![Edit::new(span, repl)];
        value_parser::walk(&value_parser::parse(decl.value), &mut |node: &ValueNode| {
          if node.kind == NodeKind::Word
            && let Some(value) = remapped_value(&lower, &node.value.to_ascii_lowercase())
          {
            edits.push(Edit::new(
              Span::new(decl.value_start + node.source_index, node.value.len()),
              value,
            ));
          }
          true
        });
        diag = diag.fix(Fix::new(format!("Replace with \"{repl}\""), edits));
      }
      diags.push(diag);
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "property-no-deprecated";

  #[test]
  fn table_is_sorted_for_binary_search() {
    assert!(
      super::DEPRECATED_PROPERTIES
        .windows(2)
        .all(|w| w[0].0 < w[1].0)
    );
  }

  #[test]
  fn fixes_properties_with_a_replacement() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { page-break-before: always /* foo */; }",
        Syntax::Css
      ),
      "a { break-before: page /* foo */; }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a {\n\t-moz-box-flex: revert;\n\t-moz-box-pack: start;\n}",
        Syntax::Css
      ),
      "a {\n\tflex-grow: revert;\n\t-moz-box-pack: start;\n}"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { WORD-WRAP: break-word; }",
        Syntax::Css
      ),
      "a { overflow-wrap: break-word; }"
    );
    let warnings = lint(RULE, on, "a { grid-gap: 1px; }", Syntax::Css);
    assert_eq!(warnings[0].message, "Expected \"grid-gap\" to be \"gap\"");
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (4, 8));
  }

  #[test]
  fn reports_properties_without_a_replacement_unfixed() {
    let on = serde_json::json!(true);
    for css in [
      "a { clip: rect(0, 0, 0, 0); }",
      "a { ime-mode: active; }",
      "a { -webkit-user-modify: read-only; }",
    ] {
      let warnings = lint(RULE, on.clone(), css, Syntax::Css);
      assert_eq!(warnings.len(), 1, "{css}");
      assert!(warnings[0].fix.is_none(), "{css}");
    }
    assert!(
      lint(
        RULE,
        on.clone(),
        "a { -webkit-box-orient: vertical; }",
        Syntax::Css
      )
      .is_empty()
    );
    assert!(lint(RULE, on, "a { clip-path: rect(0 0 0 0); }", Syntax::Css).is_empty());
  }

  #[test]
  fn ignore_properties_matches_names_and_regexes() {
    let options = serde_json::json!([true, { "ignoreProperties": ["/^grid-/", "word-wrap"] }]);
    assert!(
      lint(
        RULE,
        options.clone(),
        "a { grid-row-gap: 1px; word-wrap: break-word; }",
        Syntax::Css
      )
      .is_empty()
    );
    assert_eq!(
      lint(
        RULE,
        options,
        "a { -webkit-user-modify: read-only; }",
        Syntax::Css
      )
      .len(),
      1
    );
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!([true, { "disableFix": true }]);
    let css = "a { grid-gap: 1px; }";
    assert_eq!(lint(RULE, options.clone(), css, Syntax::Css).len(), 1);
    assert_eq!(fix(RULE, options, css, Syntax::Css), css);
  }
}
