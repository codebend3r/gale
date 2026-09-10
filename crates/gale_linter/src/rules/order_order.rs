use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify the order of content within declaration blocks.
///
/// Equivalent to stylelint-order's `order/order` rule.
///
/// The primary option is an array of content type keywords (or objects with
/// a `"type"` key). Supported keywords:
///
/// - `"custom-properties"` — CSS custom properties (`--*`)
/// - `"dollar-variables"` — SCSS variables (`$*`)
/// - `"declarations"` — standard CSS declarations
/// - `"at-rules"` — at-rules nested inside a block (not yet supported due to
///   AST limitations; reserved for future use)
/// - `"rules"` — nested style rules
///
/// The rule checks that within each style rule, content items appear in the
/// order specified. Items whose type is not listed in the config are ignored.
pub struct OrderOrder;

/// The kind of content item found inside a style rule block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentKind {
  CustomProperty,
  DollarVariable,
  Declaration,
  Rule,
}

/// A content item with its kind and source position.
struct ContentItem {
  kind: ContentKind,
  /// Byte offset for ordering.
  offset: usize,
  /// Span for diagnostic reporting.
  span: Span,
}

impl OrderOrder {
  /// Parse the expected content-type order from options.
  ///
  /// Returns a vec of `ContentKind` values in the expected order.
  /// Returns `None` if options are missing or invalid.
  fn parse_order(options: Option<&serde_json::Value>) -> Option<Vec<ContentKind>> {
    let arr = options?.as_array()?;
    let mut order = Vec::new();

    for item in arr {
      let keyword = match item {
        serde_json::Value::String(s) => s.as_str(),
        serde_json::Value::Object(obj) => obj.get("type").and_then(|v| v.as_str()).unwrap_or(""),
        _ => continue,
      };

      match keyword {
        "custom-properties" => order.push(ContentKind::CustomProperty),
        "dollar-variables" => order.push(ContentKind::DollarVariable),
        "declarations" => order.push(ContentKind::Declaration),
        "rules" => order.push(ContentKind::Rule),
        // "at-rules" is recognized but not enforceable due to AST
        // structure — at-rules nested in style blocks are not exposed
        // as children in the current parser. Silently skip.
        _ => {}
      }
    }

    if order.is_empty() { None } else { Some(order) }
  }

  /// Collect all content items from a style rule, sorted by source offset.
  fn collect_items(rule: &gale_css_parser::StyleRule) -> Vec<ContentItem> {
    let mut items = Vec::new();

    for decl in &rule.declarations {
      let kind = if decl.property.starts_with("--") {
        ContentKind::CustomProperty
      } else if decl.property.starts_with('$') {
        ContentKind::DollarVariable
      } else {
        ContentKind::Declaration
      };

      items.push(ContentItem {
        kind,
        offset: decl.span.offset,
        span: Span::new(decl.span.offset, decl.span.length),
      });
    }

    for child in &rule.children {
      items.push(ContentItem {
        kind: ContentKind::Rule,
        offset: child.span.offset,
        span: Span::new(child.span.offset, child.span.length),
      });
    }

    items.sort_by_key(|item| item.offset);
    items
  }

  /// Map a `ContentKind` to its position in the expected order.
  /// Returns `None` if the kind is not in the order (and should be ignored).
  fn kind_position(kind: ContentKind, order: &[ContentKind]) -> Option<usize> {
    order.iter().position(|k| *k == kind)
  }

  fn kind_label(kind: ContentKind) -> &'static str {
    match kind {
      ContentKind::CustomProperty => "custom properties",
      ContentKind::DollarVariable => "dollar variables",
      ContentKind::Declaration => "declarations",
      ContentKind::Rule => "rules",
    }
  }
}

impl Rule for OrderOrder {
  fn name(&self) -> &'static str {
    "order/order"
  }

  fn description(&self) -> &'static str {
    "Specify the order of content within declaration blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let order = match Self::parse_order(ctx.options) {
      Some(o) => o,
      None => return vec![],
    };

    let items = Self::collect_items(rule);
    let mut diagnostics = Vec::new();

    let mut last_order_pos: Option<usize> = None;
    let mut last_kind: Option<ContentKind> = None;

    for item in &items {
      if let Some(pos) = Self::kind_position(item.kind, &order) {
        if let Some(prev_pos) = last_order_pos
          && pos < prev_pos
        {
          let prev_label = Self::kind_label(last_kind.unwrap());
          let cur_label = Self::kind_label(item.kind);
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Expected {cur_label} to come before {prev_label}"),
            )
            .severity(self.default_severity())
            .span(item.span),
          );
        }
        last_order_pos = Some(pos);
        last_kind = Some(item.kind);
      }
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx_with_options(options: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(options),
    }
  }

  fn ctx_no_options() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn make_decl(property: &str, value: &str, offset: usize, length: usize) -> Declaration {
    Declaration {
      property: property.to_string(),
      value: value.to_string(),
      span: ParserSpan::new(offset, length),
      important: false,
    }
  }

  #[test]
  fn no_options_no_diagnostics() {
    let rule = OrderOrder;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("--my-var", "10px", 19, 16),
      ],
      span: ParserSpan::new(0, 40),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_no_options());
    assert!(diags.is_empty());
  }

  #[test]
  fn correct_order_custom_props_before_declarations() {
    let rule = OrderOrder;
    let options = serde_json::json!(["custom-properties", "declarations"]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("--my-var", "10px", 4, 16),
        make_decl("display", "block", 21, 14),
      ],
      span: ParserSpan::new(0, 40),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn wrong_order_declarations_before_custom_props() {
    let rule = OrderOrder;
    let options = serde_json::json!(["custom-properties", "declarations"]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("--my-var", "10px", 19, 16),
      ],
      span: ParserSpan::new(0, 40),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("custom properties"));
    assert!(diags[0].message.contains("declarations"));
  }

  #[test]
  fn dollar_variables_before_declarations() {
    let rule = OrderOrder;
    let options = serde_json::json!(["dollar-variables", "custom-properties", "declarations"]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("$my-var", "10px", 4, 14),
        make_decl("--color", "red", 19, 13),
        make_decl("display", "block", 33, 14),
      ],
      span: ParserSpan::new(0, 50),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn dollar_variables_out_of_order() {
    let rule = OrderOrder;
    let options = serde_json::json!(["dollar-variables", "custom-properties", "declarations"]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("$my-var", "10px", 19, 14),
      ],
      span: ParserSpan::new(0, 40),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("dollar variables"));
  }

  #[test]
  fn rules_after_declarations() {
    let rule = OrderOrder;
    let options = serde_json::json!(["declarations", "rules"]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![make_decl("display", "block", 4, 14)],
      children: vec![StyleRule {
        selector: "&:hover".to_string(),
        declarations: vec![make_decl("color", "red", 30, 10)],
        span: ParserSpan::new(19, 25),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 50),

      nested_at_rules: Vec::new(),
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn rules_before_declarations_wrong() {
    let rule = OrderOrder;
    let options = serde_json::json!(["declarations", "rules"]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![make_decl("display", "block", 30, 14)],
      children: vec![StyleRule {
        selector: "&:hover".to_string(),
        declarations: vec![make_decl("color", "red", 10, 10)],
        span: ParserSpan::new(4, 25),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 50),

      nested_at_rules: Vec::new(),
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    // The declaration at offset 30 comes after the nested rule at offset 4 in source,
    // but in the expected order declarations should come before rules.
    // Actually: the nested rule is at offset 4, declaration at offset 30.
    // In the config, declarations should come before rules.
    // In the source, rule(4) comes before declaration(30).
    // So declaration(30) is fine — it comes after the rule in source but
    // wait, the rule at offset 4 has order-pos 1 ("rules"),
    // and declaration at offset 30 has order-pos 0 ("declarations").
    // Since 0 < 1 and last_order_pos was 1, this triggers a diagnostic.
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("declarations"));
    assert!(diags[0].message.contains("rules"));
  }

  #[test]
  fn object_syntax_for_content_types() {
    let rule = OrderOrder;
    let options = serde_json::json!([
        { "type": "custom-properties" },
        { "type": "declarations" },
        { "type": "rules" }
    ]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("--color", "red", 4, 13),
        make_decl("display", "block", 18, 14),
      ],
      children: vec![StyleRule {
        selector: "&:hover".to_string(),
        declarations: vec![],
        span: ParserSpan::new(33, 15),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 50),

      nested_at_rules: Vec::new(),
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn unspecified_kinds_are_ignored() {
    let rule = OrderOrder;
    // Only specifying declarations — custom properties are not in the list
    // and should be ignored regardless of position.
    let options = serde_json::json!(["declarations"]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("--my-var", "10px", 19, 16),
        make_decl("color", "red", 36, 10),
      ],
      span: ParserSpan::new(0, 50),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn full_order_correct() {
    let rule = OrderOrder;
    let options = serde_json::json!([
      "dollar-variables",
      "custom-properties",
      "declarations",
      "rules"
    ]);
    let node = CssNode::Style(StyleRule {
      selector: ".card".to_string(),
      declarations: vec![
        make_decl("$size", "10px", 10, 13),
        make_decl("--color", "blue", 24, 14),
        make_decl("display", "flex", 39, 13),
      ],
      children: vec![StyleRule {
        selector: "&:hover".to_string(),
        declarations: vec![],
        span: ParserSpan::new(53, 15),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 70),

      nested_at_rules: Vec::new(),
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }
}
