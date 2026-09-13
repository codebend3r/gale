use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports vendor-prefixed properties (e.g. `-webkit-transform`).
///
/// Equivalent to Stylelint's `property-no-vendor-prefix` rule.
pub struct PropertyNoVendorPrefix;

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

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    // Read ignoreProperties from options (secondary option object).
    let ignore_props: Vec<String> = ctx
      .secondary_options()
      .or(ctx.options)
      .and_then(|v| v.get("ignoreProperties"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|item| item.as_str().map(|s| s.to_ascii_lowercase()))
          .collect()
      })
      .unwrap_or_default();

    // Collect declarations to check from both Style rules and standalone Declaration nodes
    let decls: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let mut diags = Vec::new();
    for decl in decls {
      if is_vendor_prefixed(&decl.property) {
        // Skip properties in the ignore list
        let prop_lower = decl.property.to_ascii_lowercase();
        if ignore_props.iter().any(|p| p == &prop_lower) {
          continue;
        }
        let unprefixed = strip_vendor_prefix(&decl.property);

        // Try to find the property in the source to build a fix
        let decl_start = decl.span.offset;
        let decl_end = decl_start + decl.span.length;
        let fix = if decl_end <= ctx.source.len() && decl_start < decl_end {
          let search_area = &ctx.source[decl_start..decl_end];
          let lower_search = search_area.to_ascii_lowercase();
          let lower_prop = decl.property.to_ascii_lowercase();
          lower_search.find(&lower_prop).map(|rel_offset| {
            let abs_offset = decl_start + rel_offset;
            Fix::new(
              format!("Remove vendor prefix from \"{}\"", decl.property),
              vec![Edit::new(
                Span::new(abs_offset, decl.property.len()),
                &unprefixed,
              )],
            )
          })
        } else {
          None
        };

        let mut diag = Diagnostic::new(
          self.name(),
          format!("Unexpected vendor-prefixed property \"{}\"", decl.property),
        )
        .severity(self.default_severity())
        .span(Span::new(decl.span.offset, decl.span.length));

        if let Some(f) = fix {
          diag = diag.fix(f);
        }

        diags.push(diag);
      }
    }
    diags
  }
}

fn strip_vendor_prefix(property: &str) -> String {
  let p = property.to_ascii_lowercase();
  for prefix in &["-webkit-", "-moz-", "-ms-", "-o-"] {
    if p.starts_with(prefix) {
      return property[prefix.len()..].to_string();
    }
  }
  property.to_string()
}

/// Known CSS properties that have standard unprefixed equivalents.
/// Only these should be flagged when vendor-prefixed. Properties like
/// `-webkit-tap-highlight-color` or `-webkit-overflow-scrolling` that
/// have NO standard equivalent are not autoprefixable and should not
/// be flagged (matching Stylelint's behavior).
const KNOWN_PREFIXABLE_PROPERTIES: &[&str] = &[
  "align-content",
  "align-items",
  "align-self",
  "animation",
  "animation-delay",
  "animation-direction",
  "animation-duration",
  "animation-fill-mode",
  "animation-iteration-count",
  "animation-name",
  "animation-play-state",
  "animation-timing-function",
  "appearance",
  "backdrop-filter",
  "backface-visibility",
  "background-clip",
  "background-origin",
  "background-size",
  "border-image",
  "border-radius",
  "border-top-left-radius",
  "border-top-right-radius",
  "border-bottom-left-radius",
  "border-bottom-right-radius",
  "box-decoration-break",
  "box-shadow",
  "box-sizing",
  "clip-path",
  "column-count",
  "column-fill",
  "column-gap",
  "column-rule",
  "column-rule-color",
  "column-rule-style",
  "column-rule-width",
  "column-span",
  "column-width",
  "columns",
  "filter",
  "flex",
  "flex-basis",
  "flex-direction",
  "flex-flow",
  "flex-grow",
  "flex-shrink",
  "flex-wrap",
  "font-feature-settings",
  "font-kerning",
  "font-variant-ligatures",
  "grid",
  "grid-area",
  "grid-auto-columns",
  "grid-auto-flow",
  "grid-auto-rows",
  "grid-column",
  "grid-column-end",
  "grid-column-gap",
  "grid-column-start",
  "grid-gap",
  "grid-row",
  "grid-row-end",
  "grid-row-gap",
  "grid-row-start",
  "grid-template",
  "grid-template-areas",
  "grid-template-columns",
  "grid-template-rows",
  "hyphens",
  "image-rendering",
  "justify-content",
  "mask",
  "mask-image",
  "object-fit",
  "object-position",
  "opacity",
  "order",
  "overscroll-behavior",
  "perspective",
  "perspective-origin",
  "scroll-snap-type",
  "shape-image-threshold",
  "shape-margin",
  "shape-outside",
  "tab-size",
  "text-decoration",
  "text-decoration-color",
  "text-decoration-line",
  "text-decoration-skip",
  "text-decoration-style",
  "text-emphasis",
  "text-emphasis-color",
  "text-emphasis-position",
  "text-emphasis-style",
  "text-orientation",
  "text-overflow",
  "touch-action",
  "transform",
  "transform-origin",
  "transform-style",
  "transition",
  "transition-delay",
  "transition-duration",
  "transition-property",
  "transition-timing-function",
  "user-select",
  "will-change",
  "writing-mode",
];

fn is_vendor_prefixed(property: &str) -> bool {
  let p = property.to_ascii_lowercase();
  // Custom properties (--) are not vendor prefixes
  if p.starts_with("--") {
    return false;
  }
  if !p.starts_with("-webkit-")
    && !p.starts_with("-moz-")
    && !p.starts_with("-ms-")
    && !p.starts_with("-o-")
  {
    return false;
  }

  // Only flag if the unprefixed property is a known standard property.
  let unprefixed = strip_vendor_prefix_lower(&p);
  KNOWN_PREFIXABLE_PROPERTIES
    .binary_search(&unprefixed.as_str())
    .is_ok()
}

fn strip_vendor_prefix_lower(property: &str) -> String {
  for prefix in &["-webkit-", "-moz-", "-ms-", "-o-"] {
    if let Some(stripped) = property.strip_prefix(prefix) {
      return stripped.to_string();
    }
  }
  property.to_string()
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_decl(prop: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: "none".to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_webkit_prefix() {
    let d = PropertyNoVendorPrefix.check(&style_decl("-webkit-transform"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("-webkit-transform"));
  }

  #[test]
  fn allows_standard_property() {
    assert!(
      PropertyNoVendorPrefix
        .check(&style_decl("transform"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn allows_custom_property() {
    assert!(
      PropertyNoVendorPrefix
        .check(&style_decl("--my-var"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn emits_fix_for_vendor_prefixed_property() {
    let source = "a { -webkit-transform: none; }";
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "-webkit-transform".to_string(),
        value: "none".to_string(),
        span: ParserSpan::new(4, 24),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let d = PropertyNoVendorPrefix.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].fix.is_some());
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].new_text, "transform");
  }

  #[test]
  fn ignore_properties_checks_as_is_not_unprefixed() {
    // v17: ignoreProperties: ["transform"] should NOT match -webkit-transform
    let opts = serde_json::json!(["true", {"ignoreProperties": ["transform"]}]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let d = PropertyNoVendorPrefix.check(&style_decl("-webkit-transform"), &ctx);
    assert_eq!(
      d.len(),
      1,
      "ignoreProperties: [\"transform\"] should NOT ignore -webkit-transform"
    );
  }

  #[test]
  fn ignore_properties_matches_full_prefixed_name() {
    // v17: ignoreProperties: ["-webkit-transform"] SHOULD match -webkit-transform
    let opts = serde_json::json!(["true", {"ignoreProperties": ["-webkit-transform"]}]);
    let ctx = RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let d = PropertyNoVendorPrefix.check(&style_decl("-webkit-transform"), &ctx);
    assert!(
      d.is_empty(),
      "ignoreProperties: [\"-webkit-transform\"] SHOULD ignore -webkit-transform"
    );
  }
}
