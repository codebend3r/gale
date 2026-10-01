use std::sync::Arc;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::pattern::{self, Regex};
use crate::postcss_tree::{NodeKind, PostcssTree};
use crate::rule::{Rule, RuleContext, per_run};
use crate::stylelint_order::{
  blocks, comments_after_node, comments_before_node, is_allowed_to_process,
  is_standard_syntax_property, js_sort, reorder_edit, split_array_settings,
};

/// Specify the order of content within declaration blocks.
///
/// Equivalent to stylelint-order's `order/order` rule, autofix included.
/// The primary option is an array of keywords (`custom-properties`,
/// `dollar-variables`, `at-variables`, `declarations`, `rules`, `at-rules`,
/// `less-mixins`) and patterns (`{ type: "rule", selector, name }`, `{ type:
/// "at-rule", name, parameter, hasBlock }`), written as `[[...]]`, `[[...],
/// { unspecified }]` or as a bare `[...]`.  `unspecified` is `top`,
/// `bottom` or `ignore` (the default).
///
/// Every rule, at-rule and SCSS nested property with children is checked
/// over the [`PostcssTree`] of the source.  The fix sorts the block as
/// postcss-sorting does: comments travel with the node they belong to,
/// anything the order does not mention goes to the bottom, and the last
/// declaration ends with a `;`.
pub struct OrderOrder;

/// A `rule` pattern.
#[derive(Debug, Clone)]
struct RulePattern {
  position: usize,
  description: String,
  selector: Option<Arc<Regex>>,
}

/// An `at-rule` pattern.
#[derive(Debug, Clone)]
struct AtRulePattern {
  position: usize,
  description: String,
  name: Option<String>,
  parameter: Option<Arc<Regex>>,
  has_block: Option<bool>,
}

/// Where nodes missing from the order go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unspecified {
  Top,
  Bottom,
  Ignore,
}

/// The rule's options, read once per file: stylelint-order's
/// `createOrderInfo` and postcss-sorting's `createExpectedOrder`.
struct Config {
  /// Keyword to its position and description.
  keywords: Vec<(String, usize, &'static str)>,
  rules: Vec<RulePattern>,
  at_rules: Vec<AtRulePattern>,
  unspecified: Unspecified,
}

/// What the order says about one node.
#[derive(Debug, Clone, Copy)]
struct OrderData<'c> {
  /// The expected position, `None` for an unspecified node.
  position: Option<usize>,
  /// The pattern's description, or `None` to describe the node itself.
  description: Option<&'c str>,
}

impl Config {
  /// Read the options, or `None` when they are not valid (Stylelint then
  /// reports the options and skips the rule).
  fn parse(options: Option<&serde_json::Value>) -> Option<Self> {
    let (primary, secondary) = split_array_settings(options);
    let items = primary?.as_array()?;
    if !items.iter().all(is_valid_item) {
      return None;
    }
    let unspecified = match secondary.and_then(|s| s.get("unspecified")) {
      None => Unspecified::Ignore,
      Some(value) => match value.as_str()? {
        "top" => Unspecified::Top,
        "bottom" => Unspecified::Bottom,
        "ignore" => Unspecified::Ignore,
        _ => return None,
      },
    };
    let mut config = Self {
      keywords: Vec::new(),
      rules: Vec::new(),
      at_rules: Vec::new(),
      unspecified,
    };
    for (index, item) in items.iter().enumerate() {
      let position = index + 1;
      let kind = match item {
        serde_json::Value::String(keyword) if keyword == "rules" => "rule",
        serde_json::Value::String(keyword) if keyword == "at-rules" => "at-rule",
        serde_json::Value::String(keyword) => {
          config.keywords.retain(|(k, _, _)| k != keyword);
          config
            .keywords
            .push((keyword.clone(), position, keyword_description(keyword)));
          continue;
        }
        other => other.get("type").and_then(|t| t.as_str()).unwrap_or(""),
      };
      let string = |key: &str| {
        item
          .get(key)
          .and_then(|v| v.as_str())
          .filter(|s| !s.is_empty())
      };
      match kind {
        "rule" => config.rules.push(RulePattern {
          position,
          description: rule_description(item),
          selector: string("selector").and_then(compile),
        }),
        "at-rule" => config.at_rules.push(AtRulePattern {
          position,
          description: at_rule_description(item),
          name: string("name").map(str::to_string),
          parameter: string("parameter").and_then(compile),
          has_block: item.get("hasBlock").and_then(|v| v.as_bool()),
        }),
        _ => {}
      }
    }
    Some(config)
  }

  /// The position and description the order gives `node`.  `for_fixer`
  /// follows postcss-sorting, which knows no `less-mixins` and so sorts a
  /// Less mixin call as an at-rule.
  fn order_data(&self, tree: &PostcssTree, node: usize, for_fixer: bool) -> OrderData<'_> {
    let n = &tree.nodes[node];
    let keyword = |name: &str| {
      let entry = self.keywords.iter().find(|(k, _, _)| k == name);
      OrderData {
        position: entry.map(|e| e.1),
        description: entry.map(|e| e.2).or(Some(keyword_description(name))),
      }
    };
    match n.kind {
      NodeKind::AtRule if n.variable => keyword("at-variables"),
      NodeKind::AtRule if n.mixin && !for_fixer => keyword("less-mixins"),
      NodeKind::Decl => {
        if n.name.starts_with("--") {
          keyword("custom-properties")
        } else if n.name.starts_with('$') {
          keyword("dollar-variables")
        } else if is_standard_syntax_property(&n.name) {
          keyword("declarations")
        } else {
          OrderData {
            position: None,
            description: Some("undefined"),
          }
        }
      }
      NodeKind::Rule => {
        let mut best: Option<&RulePattern> = None;
        let mut max = 0;
        for rule in &self.rules {
          let priority = match &rule.selector {
            None => 1,
            Some(selector) if pattern::is_match(selector, &n.name) => 2,
            Some(_) => 0,
          };
          if priority > max {
            max = priority;
            best = Some(rule);
          }
        }
        match best {
          Some(rule) => OrderData {
            position: Some(rule.position),
            description: Some(&rule.description),
          },
          None => OrderData {
            position: None,
            description: None,
          },
        }
      }
      NodeKind::AtRule => {
        let has_block = n.children.as_ref().is_some_and(|c| !c.is_empty());
        let mut best: Option<&AtRulePattern> = None;
        let mut max = 0;
        for at_rule in &self.at_rules {
          let priority = at_rule_priority(at_rule, &n.name, &n.params, has_block);
          if priority > max {
            max = priority;
            best = Some(at_rule);
          }
        }
        match best {
          Some(at_rule) => OrderData {
            position: Some(at_rule.position),
            description: Some(&at_rule.description),
          },
          None => OrderData {
            position: None,
            description: None,
          },
        }
      }
      NodeKind::Comment => OrderData {
        position: None,
        description: None,
      },
    }
  }
}

/// stylelint-order's `validatePrimaryOption` for one item.
fn is_valid_item(item: &serde_json::Value) -> bool {
  match item {
    serde_json::Value::String(keyword) => matches!(
      keyword.as_str(),
      "custom-properties"
        | "dollar-variables"
        | "at-variables"
        | "declarations"
        | "rules"
        | "at-rules"
        | "less-mixins"
    ),
    serde_json::Value::Object(object) => {
      let non_empty_string = |key: &str| {
        object
          .get(key)
          .and_then(|v| v.as_str())
          .is_some_and(|s| !s.is_empty())
      };
      match object.get("type").and_then(|t| t.as_str()) {
        Some("at-rule") => {
          if object.contains_key("parameter") && !object.contains_key("name") {
            return false;
          }
          // Like stylelint-order, the last option checked decides.
          let mut valid = true;
          if let Some(has_block) = object.get("hasBlock") {
            valid = has_block.is_boolean();
          }
          if object.contains_key("name") {
            valid = non_empty_string("name");
          }
          if object.contains_key("parameter") {
            valid = non_empty_string("parameter");
          }
          valid
        }
        Some("rule") => {
          let mut valid = true;
          if object.contains_key("selector") {
            valid = non_empty_string("selector");
          }
          if valid && object.contains_key("name") {
            valid = non_empty_string("name");
          }
          valid
        }
        _ => false,
      }
    }
    _ => false,
  }
}

/// A `selector` or `parameter` option as a regex: a `/regex/flags` string
/// (how a JavaScript `RegExp` reaches Gale), or a string `new RegExp()` is
/// given.
fn compile(source: &str) -> Option<Arc<Regex>> {
  pattern::regex_entry(source).or_else(|| pattern::compile(source).ok())
}

/// stylelint-order's `calcAtRulePatternPriority`.
fn at_rule_priority(pattern: &AtRulePattern, name: &str, params: &str, has_block: bool) -> u32 {
  let mut priority = 0;
  if pattern.has_block == Some(has_block) {
    priority += 10_010;
  }
  if pattern.name.as_deref() == Some(name) {
    priority += 10_100;
  }
  if let Some(parameter) = &pattern.parameter {
    // A blockless at-rule without params tests `undefined`.
    let text = if params.is_empty() {
      "undefined"
    } else {
      params
    };
    if pattern::is_match(parameter, text) {
      priority += 11_100;
    }
  }
  // stylelint-order checks for `paremeter` here, so a pattern with only a
  // parameter still counts as one without name and hasBlock.
  if pattern.has_block.is_none() && pattern.name.is_none() {
    priority = 1;
  }
  if pattern.has_block.is_some() && pattern.name.is_some() && priority < 20_000 {
    priority = 0;
  }
  if pattern.name.is_some() && pattern.parameter.is_some() && priority < 21_100 {
    priority = 0;
  }
  if pattern.name.is_some()
    && pattern.parameter.is_some()
    && pattern.has_block.is_some()
    && priority < 30_000
  {
    priority = 0;
  }
  priority
}

/// stylelint-order's description of a keyword.
fn keyword_description(keyword: &str) -> &'static str {
  match keyword {
    "custom-properties" => "custom property",
    "dollar-variables" => "$-variable",
    "at-variables" => "@-variable",
    "less-mixins" => "Less mixin",
    "declarations" => "declaration",
    _ => "undefined",
  }
}

/// stylelint-order's description of a `rule` pattern.
fn rule_description(item: &serde_json::Value) -> String {
  let mut text = "rule".to_string();
  if let Some(name) = item
    .get("name")
    .and_then(|n| n.as_str())
    .filter(|n| !n.is_empty())
  {
    text.push_str(&format!(" \"{name}\""));
  } else if let Some(selector) = item
    .get("selector")
    .and_then(|s| s.as_str())
    .filter(|s| !s.is_empty())
  {
    text.push_str(&format!(" with selector matching \"{selector}\""));
  }
  text
}

/// stylelint-order's description of an `at-rule` pattern.
fn at_rule_description(item: &serde_json::Value) -> String {
  let mut text = match item
    .get("name")
    .and_then(|n| n.as_str())
    .filter(|n| !n.is_empty())
  {
    Some(name) => format!("@{name}"),
    None => "at-rule".to_string(),
  };
  if let Some(parameter) = item
    .get("parameter")
    .and_then(|p| p.as_str())
    .filter(|p| !p.is_empty())
  {
    text.push_str(&format!(" \"{parameter}\""));
  }
  if let Some(has_block) = item.get("hasBlock") {
    if has_block.as_bool() == Some(true) {
      text.push_str(" with a block");
    } else {
      text = format!("blockless {text}");
    }
  }
  text
}

/// The description stylelint-order gives a node no pattern matches.
fn node_description(tree: &PostcssTree, node: usize) -> String {
  let n = &tree.nodes[node];
  match n.kind {
    NodeKind::Rule if n.name.is_empty() => "rule".to_string(),
    NodeKind::Rule => format!("rule with selector matching \"{}\"", n.name),
    NodeKind::AtRule => {
      let mut text = format!("@{}", n.name);
      if !n.params.is_empty() {
        text.push_str(&format!(" \"{}\"", n.params));
      }
      if n.children.as_ref().is_some_and(|c| !c.is_empty()) {
        text.push_str(" with a block");
      } else {
        text = format!("blockless {text}");
      }
      text
    }
    _ => "undefined".to_string(),
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

  /// Checks the order of the nodes in every block of the document.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let parsed = per_run(self.name(), ctx.options, || Config::parse(ctx.options));
    let Some(config) = parsed.as_ref() else {
      return vec![];
    };
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();
    for (block, children) in blocks(&tree) {
      self.check_block(&tree, config, block, children, &mut diags);
    }
    diags
  }
}

impl OrderOrder {
  /// stylelint-order's `checkNode`: report every node that comes before
  /// one the order puts ahead of it, each with the fix that sorts the
  /// whole block.
  fn check_block(
    &self,
    tree: &PostcssTree,
    config: &Config,
    block: usize,
    children: &[usize],
    diags: &mut Vec<Diagnostic>,
  ) {
    let mut seen: Vec<(usize, OrderData)> = Vec::new();
    let mut problems = Vec::new();
    for &child in children {
      if tree.nodes[child].kind == NodeKind::Comment {
        continue;
      }
      let data = config.order_data(tree, child, false);
      seen.push((child, data));
      let Some(&(previous, previous_data)) = seen.len().checked_sub(2).map(|i| &seen[i]) else {
        continue;
      };
      let mut prior = (previous, previous_data);
      if previous_data.position.is_none()
        && let Some(&found) = seen[..seen.len() - 1]
          .iter()
          .rev()
          .find(|(_, d)| d.position.is_some())
      {
        prior = found;
      }
      if !is_correct_order(prior.1.position, data.position, config.unspecified) {
        problems.push((child, data, prior.0, prior.1));
      }
    }
    if problems.is_empty() {
      return;
    }
    let edit = if is_allowed_to_process(tree, block) {
      sort_edit(tree, config, children)
    } else {
      None
    };
    for (child, data, prior, prior_data) in problems {
      let describe = |node: usize, data: OrderData| {
        data
          .description
          .map_or_else(|| node_description(tree, node), str::to_string)
      };
      let message = format!(
        "Expected {} to come before {}",
        describe(child, data),
        describe(prior, prior_data)
      );
      let n = &tree.nodes[child];
      let mut diag = Diagnostic::new(self.name(), message)
        .severity(self.default_severity())
        .span(Span::from_range(n.start, n.end.max(n.start)));
      if let Some(edit) = &edit {
        diag = diag.fix(Fix::new("Sort the block", vec![edit.clone()]));
      }
      diags.push(diag);
    }
  }
}

/// stylelint-order's `checkOrder` for `order/order`.
fn is_correct_order(first: Option<usize>, second: Option<usize>, unspecified: Unspecified) -> bool {
  match (first, second) {
    (Some(a), Some(b)) => a <= b,
    (None, None) => true,
    _ => match unspecified {
      Unspecified::Ignore => true,
      Unspecified::Top => first.is_none(),
      Unspecified::Bottom => second.is_none(),
    },
  }
}

/// One node of postcss-sorting's list to sort.
struct SortItem {
  node: usize,
  position: f64,
  initial_index: f64,
}

/// postcss-sorting's `sortNode` for the block holding `children`: the edit
/// that sorts it (and ends its last declaration with `;`), if it changes
/// anything.
fn sort_edit(tree: &PostcssTree, config: &Config, children: &[usize]) -> Option<Edit> {
  let mut items: Vec<SortItem> = Vec::new();
  let mut captured = vec![false; tree.nodes.len()];
  for (index, &child) in children.iter().enumerate() {
    if tree.nodes[child].kind == NodeKind::Comment {
      continue;
    }
    let position = config
      .order_data(tree, child, true)
      .position
      .map_or(f64::INFINITY, |p| p as f64);
    let before = comments_before_node(tree, child);
    let after = comments_after_node(tree, child);
    let mut initial = index as f64;
    let mut before_items: Vec<SortItem> = before
      .iter()
      .rev()
      .map(|&c| {
        initial -= 0.0001;
        SortItem {
          node: c,
          position,
          initial_index: initial,
        }
      })
      .collect();
    before_items.reverse();
    let mut initial = index as f64;
    let after_items: Vec<SortItem> = after
      .iter()
      .map(|&c| {
        initial += 0.0001;
        SortItem {
          node: c,
          position,
          initial_index: initial,
        }
      })
      .collect();
    for item in before_items.iter().chain(&after_items) {
      captured[item.node] = true;
    }
    items.extend(before_items);
    items.push(SortItem {
      node: child,
      position,
      initial_index: index as f64,
    });
    items.extend(after_items);
  }
  // processLastComments: comments that belong to no node sort last.
  for (index, &child) in children.iter().enumerate() {
    if tree.nodes[child].kind == NodeKind::Comment && !captured[child] {
      items.push(SortItem {
        node: child,
        position: f64::INFINITY,
        initial_index: index as f64,
      });
    }
  }
  js_sort(&mut items, |a, b| {
    if a.position != b.position {
      a.position - b.position
    } else {
      a.initial_index - b.initial_index
    }
  });
  let order: Vec<usize> = items.iter().map(|item| item.node).collect();
  reorder_edit(tree, children, &order, true)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::empty_lines::fix_with;
  use gale_css_parser::Syntax;
  use serde_json::json;

  /// The messages for `source` in `syntax` with `options`.
  fn messages(source: &str, syntax: Syntax, options: serde_json::Value) -> Vec<String> {
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax,
      options: Some(&options),
      cache: None,
    };
    OrderOrder
      .check_root(&[], &ctx)
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  /// `source` fixed with the rule set to `options`.
  fn fix(source: &str, syntax: Syntax, options: serde_json::Value) -> String {
    fix_with("order/order", options, source, syntax)
  }

  #[test]
  fn reads_both_option_shapes() {
    let source = "a { display: none; --width: 10px; }";
    for options in [
      json!([["custom-properties", "declarations"]]),
      json!(["custom-properties", "declarations"]),
      json!([["custom-properties", "declarations"], { "unspecified": "ignore" }]),
    ] {
      assert_eq!(
        messages(source, Syntax::Css, options),
        vec!["Expected custom property to come before declaration"]
      );
    }
    assert!(
      messages(
        source,
        Syntax::Css,
        json!([["custom-properties", "nonsense"]])
      )
      .is_empty()
    );
  }

  #[test]
  fn reports_nodes_out_of_order() {
    let order = json!([[
      "custom-properties",
      "dollar-variables",
      "declarations",
      "rules",
      "at-rules"
    ]]);
    assert_eq!(
      messages(
        "div { a { color: blue; } color: tomato; }",
        Syntax::Scss,
        order.clone()
      ),
      vec!["Expected declaration to come before rule"]
    );
    assert!(
      messages(
        "a { --w: 1px; $h: 2px; /* c */ display: none; span {} @media (x) {} }",
        Syntax::Scss,
        order
      )
      .is_empty()
    );
    let mixins = json!([["less-mixins", "rules"]]);
    assert_eq!(
      messages("a { span {} .mixin(); }", Syntax::Less, mixins).len(),
      1
    );
  }

  #[test]
  fn at_rule_and_rule_patterns() {
    let at_rules = json!([[
      { "type": "at-rule", "name": "include", "hasBlock": true },
      { "type": "at-rule", "name": "include" },
      { "type": "at-rule", "hasBlock": true },
      { "type": "at-rule", "name": "include", "parameter": "media" },
      { "type": "at-rule", "name": "include", "parameter": "media", "hasBlock": true }
    ]]);
    assert_eq!(
      fix(
        "a {\n  @include media('palm') {\n    display: block;\n  }\n  @include media('desk');\n}",
        Syntax::Scss,
        at_rules
      ),
      "a {\n  @include media('desk');\n  @include media('palm') {\n    display: block;\n  }\n}"
    );
    let rules = json!([[{ "type": "rule", "selector": "^a" }, { "type": "rule", "selector": "/^&/" }, { "type": "rule" }]]);
    assert_eq!(
      fix("a { a {} &:hover {} abbr {} span {} }", Syntax::Scss, rules),
      "a { a {} abbr {} &:hover {} span {} }"
    );
  }

  #[test]
  fn fix_moves_comments_and_unspecified_nodes() {
    let order = json!([["custom-properties", "declarations"], { "unspecified": "bottom" }]);
    assert_eq!(
      fix(
        "a {\n  $w: 5px;\n  /* c */\n  display: none\n}",
        Syntax::Scss,
        order
      ),
      "a {\n  /* c */\n  display: none;\n  $w: 5px;\n}"
    );
    // The fixer always sends unspecified nodes to the bottom, so with
    // `unspecified: "top"` this cannot be fixed.
    let top = json!([["custom-properties", "declarations"], { "unspecified": "top" }]);
    let source = "a {\n  display: none;\n  $width: 5px;\n}";
    assert_eq!(messages(source, Syntax::Scss, top.clone()).len(), 1);
    assert_eq!(fix(source, Syntax::Scss, top), source);
  }
}
