use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::empty_lines::newline_of;
use crate::postcss_tree::{NodeKind, PostcssTree};
use crate::rule::{Rule, RuleContext, per_file};
use crate::stylelint_order::{
  blocks, comments_after_declaration, comments_before_declaration, is_allowed_to_process,
  is_property, is_shorthand, js_sort, normalized_property, reorder_edit, split_array_settings,
  vendor_prefix,
};

/// Enforce a specific ordering of properties within declaration blocks.
///
/// Equivalent to stylelint-order's `order/properties-order` rule, autofix
/// included.  The primary option lists property names and groups
/// (`{ properties, groupName, emptyLineBefore, noEmptyLineBetween, order:
/// "flexible" }`); secondary options are `unspecified` (`top`, `bottom`,
/// `ignore`, `bottomAlphabetical`), `emptyLineBeforeUnspecified` and
/// `emptyLineMinimumPropertyThreshold`.
///
/// Every rule, at-rule and SCSS nested property with children is checked
/// over the [`PostcssTree`] of the source.  The fix sorts the block as
/// postcss-sorting does, comments travelling with the declaration they
/// belong to, then the empty lines between groups are fixed on the sorted
/// block in a later fix pass, as Stylelint fixes them after sorting.
pub struct OrderPropertiesOrder;

/// stylelint-order's `createOrderInfo` entry for one property.
#[derive(Debug, Clone)]
struct OrderData {
  /// Counts groups with `emptyLineBefore`, starting at 1.
  separated_group: usize,
  /// Which group (or flexible group) the property is in.
  group_position: i64,
  /// The position the property must not come before.
  expected_position: usize,
  /// The group's `groupName`, for messages.
  group_name: Option<String>,
  /// The group's `noEmptyLineBetween`.
  no_empty_line_before_inside_group: bool,
}

/// Where properties missing from the order go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unspecified {
  Top,
  Bottom,
  Ignore,
  BottomAlphabetical,
}

/// The rule's options, read once per file.
struct Config {
  /// `createOrderInfo`: property name (as configured) to its order data.
  order: HashMap<String, OrderData>,
  /// `createFlatOrder` + `createExpectedPropertiesOrder`: the property
  /// index the fixer sorts by (the last listing of a name wins).
  flat_order: HashMap<String, usize>,
  /// The `emptyLineBefore` of each group in the primary option, in order.
  group_empty_lines: Vec<Option<String>>,
  unspecified: Unspecified,
  empty_line_before_unspecified: Option<String>,
  empty_line_minimum_property_threshold: f64,
}

impl Config {
  /// Read the options, or `None` when they are not valid (Stylelint then
  /// reports the options and skips the rule).
  fn parse(options: Option<&serde_json::Value>) -> Option<Self> {
    let (primary, secondary) = split_array_settings(options);
    let items = primary?.as_array()?;
    let valid = items.iter().all(|item| match item {
      serde_json::Value::String(_) => true,
      serde_json::Value::Object(group) => {
        group
          .get("properties")
          .and_then(|p| p.as_array())
          .is_some_and(|p| p.iter().all(serde_json::Value::is_string))
          && group
            .get("emptyLineBefore")
            .is_none_or(|e| matches!(e.as_str(), Some("always" | "never" | "threshold")))
          && group
            .get("noEmptyLineBetween")
            .is_none_or(serde_json::Value::is_boolean)
      }
      _ => false,
    });
    if !valid {
      return None;
    }

    let unspecified = match secondary.and_then(|s| s.get("unspecified")) {
      None => Unspecified::Ignore,
      Some(value) => match value.as_str()? {
        "top" => Unspecified::Top,
        "bottom" => Unspecified::Bottom,
        "ignore" => Unspecified::Ignore,
        "bottomAlphabetical" => Unspecified::BottomAlphabetical,
        _ => return None,
      },
    };
    let empty_line_before_unspecified =
      match secondary.and_then(|s| s.get("emptyLineBeforeUnspecified")) {
        None => None,
        Some(value) => match value.as_str()? {
          name @ ("always" | "never" | "threshold") => Some(name.to_string()),
          _ => return None,
        },
      };
    let threshold = match secondary.and_then(|s| s.get("emptyLineMinimumPropertyThreshold")) {
      None => 0.0,
      Some(value) => value.as_f64()?,
    };

    // createOrderInfo.
    let mut order = HashMap::new();
    let mut expected_position = 0;
    let mut separated_group = 1;
    let mut group_position: i64 = 0;
    for item in items {
      match item {
        serde_json::Value::String(name) => {
          expected_position += 1;
          order.insert(
            name.clone(),
            OrderData {
              separated_group,
              group_position,
              expected_position,
              group_name: None,
              no_empty_line_before_inside_group: false,
            },
          );
        }
        serde_json::Value::Object(group) => {
          if group
            .get("emptyLineBefore")
            .is_some_and(|e| e.as_str().is_some_and(|s| !s.is_empty()))
          {
            separated_group += 1;
          }
          let flexible = group.get("order").and_then(|o| o.as_str()) == Some("flexible");
          group_position += 1;
          if flexible {
            expected_position += 1;
          }
          let group_name = group
            .get("groupName")
            .and_then(|n| n.as_str())
            .map(str::to_string);
          let no_empty_line =
            group.get("noEmptyLineBetween").and_then(|n| n.as_bool()) == Some(true);
          for name in group
            .get("properties")
            .and_then(|p| p.as_array())
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str())
          {
            if !flexible {
              expected_position += 1;
            }
            order.insert(
              name.to_string(),
              OrderData {
                separated_group,
                group_position,
                expected_position,
                group_name: group_name.clone(),
                no_empty_line_before_inside_group: no_empty_line,
              },
            );
          }
        }
        _ => {}
      }
    }

    // createFlatOrder.
    let mut flat_order = HashMap::new();
    let mut index = 0;
    for item in items {
      let names: Vec<&str> = match item {
        serde_json::Value::String(name) => vec![name.as_str()],
        serde_json::Value::Object(group) => group
          .get("properties")
          .and_then(|p| p.as_array())
          .into_iter()
          .flatten()
          .filter_map(|p| p.as_str())
          .collect(),
        _ => Vec::new(),
      };
      for name in names {
        flat_order.insert(name.to_string(), index);
        index += 1;
      }
    }

    // A list that names no property is what Gale's static reading of a
    // JavaScript config leaves of an order it cannot evaluate (shared
    // configs build theirs with `.concat()` calls), so the rule stays off
    // rather than sorting every block alphabetically against a list the
    // author never wrote.
    if flat_order.is_empty() {
      return None;
    }

    let group_empty_lines = items
      .iter()
      .filter(|item| !item.is_string())
      .map(|group| {
        group
          .get("emptyLineBefore")
          .and_then(|e| e.as_str())
          .map(str::to_string)
      })
      .collect();

    Some(Self {
      order,
      flat_order,
      group_empty_lines,
      unspecified,
      empty_line_before_unspecified,
      empty_line_minimum_property_threshold: threshold,
    })
  }

  /// `groups[separatedGroup - 2].emptyLineBefore`.
  fn group_empty_line(&self, separated_group: usize) -> Option<&str> {
    separated_group
      .checked_sub(2)
      .and_then(|i| self.group_empty_lines.get(i))
      .and_then(|e| e.as_deref())
  }
}

/// stylelint-order's `getNodeData` for a property.
struct PropertyData<'c> {
  node: usize,
  name: String,
  unprefixed_name: String,
  order: Option<&'c OrderData>,
}

impl Rule for OrderPropertiesOrder {
  fn name(&self) -> &'static str {
    "order/properties-order"
  }

  fn description(&self) -> &'static str {
    "Enforce a specific ordering of properties within declaration blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Checks the property order and the empty lines between groups in every
  /// block of the document.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    // The order list can run to hundreds of properties; build the lookup
    // tables once per file.
    let parsed = per_file(self.name(), ctx.options, || Config::parse(ctx.options));
    let Some(config) = parsed.as_ref() else {
      return vec![];
    };
    let tree = ctx.postcss_tree();
    let newline = newline_of(ctx.source);
    let mut diags = Vec::new();
    for (block, children) in blocks(&tree) {
      let sorting = self.check_order(&tree, config, block, children, &mut diags);
      self.check_empty_lines(&tree, config, children, newline, !sorting, &mut diags);
    }
    diags
  }
}

impl OrderPropertiesOrder {
  /// stylelint-order's `checkNodeForOrder`: report every property that
  /// comes too early, each with the fix that sorts the whole block.
  /// Returns whether that fix changes the block.
  fn check_order(
    &self,
    tree: &PostcssTree,
    config: &Config,
    block: usize,
    children: &[usize],
    diags: &mut Vec<Diagnostic>,
  ) -> bool {
    let properties: Vec<PropertyData> = children
      .iter()
      .filter(|&&c| is_property(tree, c))
      .map(|&c| node_data(tree, config, c))
      .collect();
    let mut problems = Vec::new();
    for index in 1..properties.len() {
      let (correct, first, second) = check_order(
        &properties[index - 1],
        &properties[index],
        &properties[..index],
        config.unspecified,
      );
      if !correct {
        problems.push((first, second));
      }
    }
    if problems.is_empty() {
      return false;
    }
    let fix = is_allowed_to_process(tree, block)
      .then(|| sort_edit(tree, config, block, children))
      .flatten();
    for (first, second) in problems {
      let second = &properties[second];
      let first = &properties[first];
      let mut message = format!(
        "Expected \"{}\" to come before \"{}\"",
        second.name, first.name
      );
      if let Some(group) = second.order.and_then(|o| o.group_name.as_deref()) {
        message.push_str(&format!(" in group \"{group}\""));
      }
      let mut diag = Diagnostic::new(self.name(), message)
        .severity(self.default_severity())
        .span(node_span(tree, second.node));
      if let Some(edit) = &fix {
        diag = diag.fix(Fix::new("Sort the properties", vec![edit.clone()]));
      }
      diags.push(diag);
    }
    fix.is_some()
  }

  /// stylelint-order's `checkNodeForEmptyLines`.  The fixes are left off
  /// while the block is still to be sorted: Stylelint fixes empty lines on
  /// the sorted block, which the next fix pass sees.
  fn check_empty_lines(
    &self,
    tree: &PostcssTree,
    config: &Config,
    children: &[usize],
    newline: &str,
    fixable: bool,
    diags: &mut Vec<Diagnostic>,
  ) {
    let props_count = children.iter().filter(|&&c| is_property(tree, c)).count() as f64;
    let data: Vec<Option<PropertyData>> = children
      .iter()
      .map(|&c| is_property(tree, c).then(|| node_data(tree, config, c)))
      .collect();
    let mut report = |node: usize, name: &str, add: bool| {
      let (message, new_before) = if add {
        (
          format!("Expected an empty line before property \"{name}\""),
          add_empty_line_before(tree.before(node), newline),
        )
      } else {
        (
          format!("Unexpected empty line before property \"{name}\""),
          remove_empty_lines_before(tree.before(node), newline),
        )
      };
      let mut diag = Diagnostic::new(self.name(), message)
        .severity(self.default_severity())
        .span(node_span(tree, node));
      if fixable {
        let range = &tree.nodes[node].before;
        diag = diag.fix(Fix::new(
          if add {
            "Add an empty line"
          } else {
            "Remove the empty lines"
          },
          vec![Edit::new(
            Span::from_range(range.start, range.end),
            new_before,
          )],
        ));
      }
      diags.push(diag);
    };

    for index in 0..children.len() {
      let mut previous = index.checked_sub(1);
      // A comment sharing the previous line steps back one more node.
      if let Some(p) = previous
        && tree.nodes[children[p]].kind == NodeKind::Comment
        && !tree.before(children[p]).contains('\n')
      {
        previous = index.checked_sub(2);
      }
      let Some(previous) = previous else {
        continue;
      };
      let (Some(first), Some(second)) = (&data[previous], &data[index]) else {
        continue;
      };
      self.check_empty_line_before(tree, config, first, second, props_count, &mut report);
    }

    // checkEmptyLineBeforeFirstProp.
    if let Some(Some(first)) = data.first() {
      let empty_line_before = match first.order {
        Some(order) => config.group_empty_line(order.separated_group).is_some(),
        None => config.empty_line_before_unspecified.is_some(),
      };
      if empty_line_before && has_empty_line_before(tree, first.node) {
        report(first.node, &first.name, false);
      }
    }
  }

  /// stylelint-order's `checkEmptyLineBefore` for two neighbouring
  /// properties.
  fn check_empty_line_before(
    &self,
    tree: &PostcssTree,
    config: &Config,
    first: &PropertyData,
    second: &PropertyData,
    props_count: f64,
    report: &mut impl FnMut(usize, &str, bool),
  ) {
    // `lastKnownSeparatedGroup` never moves from 1 in stylelint-order.
    let last_known_separated_group = 1;
    let first_group = first
      .order
      .map_or(last_known_separated_group, |o| o.separated_group);
    let second_group = second
      .order
      .map_or(last_known_separated_group, |o| o.separated_group);
    let start_of_specified_group = second.order.is_some() && first_group != second_group;
    let start_of_unspecified_group = first.order.is_some() && second.order.is_none();

    if start_of_specified_group || start_of_unspecified_group {
      let mut empty_line_before = config.group_empty_line(second_group);
      if start_of_unspecified_group {
        empty_line_before = config.empty_line_before_unspecified.as_deref();
      }
      let below_threshold = props_count < config.empty_line_minimum_property_threshold;
      let threshold = empty_line_before == Some("threshold");
      let has_empty_line = has_empty_line_before(tree, second.node);
      if (empty_line_before == Some("always") || (threshold && !below_threshold)) && !has_empty_line
      {
        report(second.node, &second.name, true);
      } else if (empty_line_before == Some("never") || (threshold && below_threshold))
        && has_empty_line
      {
        report(second.node, &second.name, false);
      }
    }

    if let (Some(a), Some(b)) = (first.order, second.order)
      && a.group_position == b.group_position
      && b.no_empty_line_before_inside_group
      && has_empty_line_before(tree, second.node)
    {
      report(second.node, &second.name, false);
    }
  }
}

/// The span a report points at: the node's first character to its end.
fn node_span(tree: &PostcssTree, node: usize) -> Span {
  let n = &tree.nodes[node];
  Span::from_range(n.start, n.end.max(n.start))
}

/// stylelint-order's `getNodeData` for the property `node`.
fn node_data<'c>(tree: &PostcssTree, config: &'c Config, node: usize) -> PropertyData<'c> {
  let name = tree.nodes[node].name.clone();
  let unprefixed_name = normalized_property(&name);
  let order = config.order.get(&unprefixed_name);
  PropertyData {
    node,
    name,
    unprefixed_name,
    order,
  }
}

/// stylelint-order's `checkOrder` for two neighbouring properties, given
/// the properties before the second.  Returns whether the order is correct
/// and the indexes (into `all` plus the second) of the pair to report.
fn check_order(
  first: &PropertyData,
  second: &PropertyData,
  all: &[PropertyData],
  unspecified: Unspecified,
) -> (bool, usize, usize) {
  let second_index = all.len();
  let first_index = second_index - 1;
  let report = |correct: bool| (correct, first_index, second_index);
  let first_name = first.name.to_lowercase();
  let second_name = second.name.to_lowercase();

  if first.unprefixed_name == second.unprefixed_name {
    let wrong = vendor_prefix(&first_name).is_empty() && !vendor_prefix(&second_name).is_empty();
    return report(!wrong);
  }

  let first_specified = first.order.is_some();
  let second_specified = second.order.is_some();
  if let (Some(a), Some(b)) = (first.order, second.order) {
    return report(a.expected_position <= b.expected_position);
  }

  if !first_specified
    && let Some(b) = second.order
    && let Some(prior) = all[..first_index].iter().rposition(|p| p.order.is_some())
    && all[prior]
      .order
      .is_some_and(|a| a.expected_position > b.expected_position)
  {
    return (false, prior, second_index);
  }

  use Unspecified::*;
  let correct = match unspecified {
    BottomAlphabetical if first_specified && !second_specified => true,
    BottomAlphabetical if !first_specified && !second_specified => {
      is_alphabetical_order(first, second)
    }
    BottomAlphabetical if !first_specified => false,
    _ if !first_specified && !second_specified => true,
    Ignore => true,
    Top => !first_specified,
    Bottom => !second_specified,
    BottomAlphabetical => true,
  };
  report(correct)
}

/// stylelint-order's `checkAlphabeticalOrder`.
fn is_alphabetical_order(first: &PropertyData, second: &PropertyData) -> bool {
  let (a, b) = (&first.unprefixed_name, &second.unprefixed_name);
  if is_shorthand(a, b) {
    return true;
  }
  if is_shorthand(b, a) {
    return false;
  }
  if a == b {
    let first_name = first.name.to_lowercase();
    let second_name = second.name.to_lowercase();
    return !(vendor_prefix(&first_name).is_empty() && !vendor_prefix(&second_name).is_empty());
  }
  a < b
}

/// stylelint-order's `hasEmptyLineBefore`: an empty line in the node's
/// `raws.before`, or in that of a comment right before it.
fn has_empty_line_before(tree: &PostcssTree, node: usize) -> bool {
  if has_order_empty_line(tree.before(node)) {
    return true;
  }
  tree.prev(node).is_some_and(|prev| {
    tree.nodes[prev].kind == NodeKind::Comment && has_order_empty_line(tree.before(prev))
  })
}

/// `/\r?\n\s*\r?\n/`: two line breaks with only whitespace between them.
fn has_order_empty_line(text: &str) -> bool {
  let bytes = text.as_bytes();
  bytes.iter().enumerate().any(|(i, &b)| {
    b == b'\n' && {
      let rest = &text[i + 1..];
      let blank = rest.len() - rest.trim_start().len();
      rest[..blank].contains('\n')
    }
  })
}

/// stylelint-order's `addEmptyLineBefore` on a `raws.before`.
fn add_empty_line_before(before: &str, newline: &str) -> String {
  if !before.contains('\n') {
    format!("{newline}{newline}{before}")
  } else if before.starts_with('\n') || before.starts_with("\r\n") {
    format!("{newline}{before}")
  } else if before.ends_with('\n') {
    format!("{before}{newline}")
  } else {
    // Insert before the first `\r?\n`.
    let at = before.find('\n').unwrap_or(0);
    let at = if at > 0 && before.as_bytes()[at - 1] == b'\r' {
      at - 1
    } else {
      at
    };
    format!("{}{newline}{}", &before[..at], &before[at..])
  }
}

/// stylelint-order's `removeEmptyLinesBefore`: every run matching
/// `/(\r?\n\s*\r?\n)+/` becomes one line break.
fn remove_empty_lines_before(before: &str, newline: &str) -> String {
  use std::sync::OnceLock;
  static EMPTY_LINES: OnceLock<regex::Regex> = OnceLock::new();
  let re = EMPTY_LINES.get_or_init(|| regex::Regex::new(r"(\r?\n\s*\r?\n)+").expect("valid regex"));
  re.replace_all(before, regex::NoExpand(newline))
    .into_owned()
}

/// One entry of postcss-sorting's list of declarations to sort.
struct SortItem<'n> {
  node: usize,
  /// The declaration's property; `None` for a comment.
  name: Option<&'n str>,
  unprefixed_name: String,
  order: Option<usize>,
  initial_index: f64,
}

/// postcss-sorting's `sortNodeProperties` for `block`, whose children are
/// `children`: the edit that sorts it, if it changes anything.
fn sort_edit(
  tree: &PostcssTree,
  config: &Config,
  block: usize,
  children: &[usize],
) -> Option<Edit> {
  let position = match config.unspecified {
    Unspecified::Ignore => Unspecified::Bottom,
    other => other,
  };
  let mut items: Vec<SortItem> = Vec::new();
  let mut processed = vec![false; children.len()];
  for (index, &child) in children.iter().enumerate() {
    if !is_property(tree, child) {
      continue;
    }
    let name = tree.nodes[child].name.as_str();
    let unprefixed_name = normalized_property(name);
    let order = config.flat_order.get(&unprefixed_name).copied();
    processed[index] = true;
    let comment = |node: usize, initial_index: f64| SortItem {
      node,
      name: None,
      unprefixed_name: unprefixed_name.clone(),
      order,
      initial_index,
    };
    let before = comments_before_declaration(tree, child);
    let after = comments_after_declaration(tree, child);
    for k in 1..=before.len() {
      processed[index - k] = true;
    }
    for k in 1..=after.len() {
      processed[index + k] = true;
    }
    let mut initial = index as f64;
    let mut before_items: Vec<SortItem> = before
      .iter()
      .rev()
      .map(|&c| {
        initial -= 0.0001;
        comment(c, initial)
      })
      .collect();
    before_items.reverse();
    let mut initial = index as f64;
    let after_items: Vec<SortItem> = after
      .iter()
      .map(|&c| {
        initial += 0.0001;
        comment(c, initial)
      })
      .collect();
    items.extend(before_items);
    items.push(SortItem {
      node: child,
      name: Some(name),
      unprefixed_name,
      order,
      initial_index: index as f64,
    });
    items.extend(after_items);
  }

  js_sort(&mut items, |a, b| sort_declarations(a, b, position));

  let mut order = Vec::with_capacity(children.len());
  let mut inserted = false;
  for (index, &child) in children.iter().enumerate() {
    if processed[index] {
      if !inserted {
        inserted = true;
        order.extend(items.iter().map(|item| item.node));
      }
    } else {
      order.push(child);
    }
  }
  reorder_edit(tree, children, &order, tree.nodes[block].semicolon)
}

/// postcss-sorting's `sortDeclarations` comparator.
fn sort_declarations(a: &SortItem, b: &SortItem, position: Unspecified) -> f64 {
  if let (Some(a_name), Some(b_name)) = (a.name, b.name)
    && a.unprefixed_name == b.unprefixed_name
  {
    let a_prefixed = !vendor_prefix(a_name).is_empty();
    let b_prefixed = !vendor_prefix(b_name).is_empty();
    if !a_prefixed && b_prefixed {
      return 1.0;
    }
    if a_prefixed && !b_prefixed {
      return -1.0;
    }
  }
  if let (Some(a_order), Some(b_order)) = (a.order, b.order)
    && a_order != b_order
  {
    return a_order as f64 - b_order as f64;
  }
  if matches!(
    position,
    Unspecified::Bottom | Unspecified::BottomAlphabetical
  ) {
    if a.order.is_some() && b.order.is_none() {
      return -1.0;
    }
    if a.order.is_none() && b.order.is_some() {
      return 1.0;
    }
  }
  if position == Unspecified::Top {
    if a.order.is_some() && b.order.is_none() {
      return 1.0;
    }
    if a.order.is_none() && b.order.is_some() {
      return -1.0;
    }
  }
  if position == Unspecified::BottomAlphabetical && a.order.is_none() && b.order.is_none() {
    return sort_declarations_alphabetically(a, b);
  }
  a.initial_index - b.initial_index
}

/// postcss-sorting's `sortDeclarationsAlphabetically` comparator.
fn sort_declarations_alphabetically(a: &SortItem, b: &SortItem) -> f64 {
  if is_shorthand(&a.unprefixed_name, &b.unprefixed_name) {
    return -1.0;
  }
  if is_shorthand(&b.unprefixed_name, &a.unprefixed_name) {
    return 1.0;
  }
  if a.unprefixed_name == b.unprefixed_name {
    if let (Some(a_name), Some(b_name)) = (a.name, b.name) {
      let a_prefixed = !vendor_prefix(a_name).is_empty();
      let b_prefixed = !vendor_prefix(b_name).is_empty();
      if !a_prefixed && b_prefixed {
        return 1.0;
      }
      if a_prefixed && !b_prefixed {
        return -1.0;
      }
    }
    return a.initial_index - b.initial_index;
  }
  if a.unprefixed_name <= b.unprefixed_name {
    -1.0
  } else {
    1.0
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::empty_lines::fix_with;
  use gale_css_parser::Syntax;
  use serde_json::json;

  /// The messages for `source` with `options`.
  fn messages(source: &str, options: serde_json::Value) -> Vec<String> {
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: Some(&options),
      cache: None,
    };
    OrderPropertiesOrder
      .check_root(&[], &ctx)
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  /// `source` fixed with the rule set to `options`.
  fn fix(source: &str, options: serde_json::Value) -> String {
    fix_with("order/properties-order", options, source, Syntax::Css)
  }

  #[test]
  fn reports_properties_out_of_order() {
    let order = json!([[
      "my",
      "transform",
      "font-smoothing",
      "top",
      "transition",
      "border",
      "color"
    ]]);
    assert_eq!(
      messages("a { color: pink; top: 0; }", order.clone()),
      vec!["Expected \"top\" to come before \"color\""]
    );
    assert!(messages("a { top: 0; color: pink; }", order.clone()).is_empty());
    assert!(
      messages(
        "a { -webkit-transform: none; transform: none; }",
        order.clone()
      )
      .is_empty()
    );
    assert_eq!(
      messages("a { transform: none; -webkit-transform: none; }", order).len(),
      1
    );
    let grouped =
      json!([[{ "groupName": "font", "properties": ["font-size", "font-weight"] }, "height"]]);
    assert_eq!(
      messages("a { height: 1px; font-size: 2px; }", grouped),
      vec!["Expected \"font-size\" to come before \"height\" in group \"font\""]
    );
  }

  #[test]
  fn fix_sorts_and_keeps_comments_with_their_declarations() {
    let order = json!([["height", "width", "color"]]);
    assert_eq!(
      fix("a { color: pink; width: 1px; height: 2px }", order.clone()),
      "a { height: 2px; width: 1px; color: pink }"
    );
    assert_eq!(
      fix(
        "a {\n  /* c */\n  color: pink;\n  width: 1px; /* w */\n  top: 0;\n}",
        order.clone()
      ),
      "a {\n  width: 1px; /* w */\n  /* c */\n  color: pink;\n  top: 0;\n}"
    );
    assert_eq!(
      fix(
        "a { -moz-transform: none; transform: none; -webkit-transform: none; }",
        json!([["transform"]])
      ),
      "a { -moz-transform: none; -webkit-transform: none; transform: none; }"
    );
    // Sass control blocks are reported but never sorted.
    let ctx_source = "@if $a { color: pink; height: 1px; }";
    assert_eq!(
      fix_with("order/properties-order", order, ctx_source, Syntax::Scss),
      ctx_source
    );
  }

  #[test]
  fn fix_sorts_then_fixes_empty_lines_between_groups() {
    let groups = json!([[
      { "emptyLineBefore": "always", "properties": ["width", "height"] },
      { "emptyLineBefore": "always", "properties": ["font-size", "font-family"] }
    ]]);
    assert_eq!(
      fix(
        "a {\n  width: 1px;\n  font-size: 2px;\n  height: 3px;\n}",
        groups
      ),
      "a {\n  width: 1px;\n  height: 3px;\n\n  font-size: 2px;\n}"
    );
    let unspecified = json!([["height", "width"], { "unspecified": "bottom", "emptyLineBeforeUnspecified": "always" }]);
    assert_eq!(
      fix("a {\r\n  height: 1px;\r\n  color: red;\r\n}", unspecified),
      "a {\r\n  height: 1px;\r\n\r\n  color: red;\r\n}"
    );
  }

  #[test]
  fn unspecified_positions() {
    assert_eq!(
      fix(
        "a { height: 1px; top: 0; }",
        json!([["height"], { "unspecified": "top" }])
      ),
      "a { top: 0; height: 1px; }"
    );
    assert_eq!(
      fix(
        "a { bottom: 0; height: 1px; }",
        json!([["height"], { "unspecified": "bottom" }])
      ),
      "a { height: 1px; bottom: 0; }"
    );
    assert_eq!(
      fix(
        "a { compose: b; top: 0; bottom: 0; }",
        json!([["all", "compose"], { "unspecified": "bottomAlphabetical" }])
      ),
      "a { compose: b; bottom: 0; top: 0; }"
    );
  }

  #[test]
  fn empty_line_helpers_match_stylelint_order() {
    assert_eq!(add_empty_line_before(" ", "\n"), "\n\n ");
    assert_eq!(add_empty_line_before("\n  ", "\n"), "\n\n  ");
    assert_eq!(add_empty_line_before(";\n", "\n"), ";\n\n");
    assert_eq!(add_empty_line_before(";\n  ", "\n"), ";\n\n  ");
    assert_eq!(remove_empty_lines_before("\n\n\n  ", "\n"), "\n  ");
    assert!(has_order_empty_line("\n \n"));
    assert!(!has_order_empty_line("\n  "));
  }
}
