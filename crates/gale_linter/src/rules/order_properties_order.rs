use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Enforce a specific ordering of properties within declaration blocks.
///
/// Equivalent to stylelint-order's `order/properties-order` rule.
pub struct OrderPropertiesOrder;

/// Info about a property's position in the expected order.
#[derive(Debug, Clone, Copy)]
struct PropertyInfo {
  /// Global position index (for strict ordering comparison).
  order_index: usize,
  /// Group index (for inter-group checks like emptyLineBefore).
  group_index: usize,
}

/// How to handle properties not mentioned in the order spec.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Unspecified {
  Ignore,
  Top,
  Bottom,
  BottomAlphabetical,
}

/// When to require an empty line before a group or unspecified properties.
#[derive(Debug, Clone, Copy, PartialEq)]
enum EmptyLineBefore {
  Always,
  Never,
  Threshold,
}

/// Per-group settings.
#[derive(Debug, Clone)]
struct GroupInfo {
  empty_line_before: Option<EmptyLineBefore>,
  no_empty_line_between: bool,
  flexible: bool,
}

/// A compiled regex pattern entry from the property order config.
#[derive(Debug, Clone)]
struct RegexEntry {
  pattern: regex::Regex,
  info: PropertyInfo,
}

/// Parsed configuration for the rule.
struct Config {
  /// Map from lowercase property name to its ordering info.
  property_map: HashMap<String, PropertyInfo>,
  /// Regex patterns for property matching (tried when exact match fails).
  regex_patterns: Vec<RegexEntry>,
  /// Per-group settings (indexed by group_index).
  groups: Vec<GroupInfo>,
  /// How to handle unspecified properties.
  unspecified: Unspecified,
  /// Empty line requirement before unspecified properties.
  empty_line_before_unspecified: Option<EmptyLineBefore>,
  /// Minimum number of properties in a block before threshold-based empty line
  /// rules switch from "never" to "always".
  empty_line_min_threshold: Option<usize>,
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

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let config = match parse_config(ctx) {
      Some(c) => c,
      None => return vec![],
    };

    let mut diagnostics = Vec::new();

    // Collect relevant declarations (skip SCSS vars and custom props).
    let decls: Vec<(usize, &str, usize, usize)> = rule
      .declarations
      .iter()
      .enumerate()
      .filter(|(_, d)| !d.property.starts_with('$') && !d.property.starts_with("--"))
      .map(|(i, d)| (i, d.property.as_str(), d.span.offset, d.span.length))
      .collect();

    let total_props = decls.len();

    // Check if there are skipped declarations (custom props / SCSS vars)
    // before the first relevant declaration. This affects emptyLineBefore
    // for the first real property.
    let has_skipped_before_first = decls.first().map(|&(di, _, _, _)| di > 0).unwrap_or(false);

    // Determine if threshold is met: used for "threshold" emptyLineBefore.
    let threshold_met = config
      .empty_line_min_threshold
      .map(|t| total_props >= t)
      .unwrap_or(true);

    // Tracking state
    let mut last_order_index: Option<usize> = None;
    let mut last_group_index: Option<usize> = None;
    let mut last_property_name: Option<String> = None;
    let mut last_was_specified: Option<bool> = None;
    let mut last_unspecified_name: Option<String> = None;
    let mut is_first_prop = true;
    // Track unprefixed properties that have been seen, so vendor-prefixed
    // versions appearing after them can be flagged.
    let mut seen_unprefixed: std::collections::HashSet<String> = std::collections::HashSet::new();

    for &(_di, prop, offset, length) in &decls {
      let prop_lower = prop.to_ascii_lowercase();
      let lookup = strip_vendor_prefix(&prop_lower);
      let is_vendor_prefixed = prop.starts_with('-');

      let info = config.property_map.get(lookup).or_else(|| {
        // Try regex patterns if exact match fails.
        config
          .regex_patterns
          .iter()
          .find(|re| re.pattern.is_match(lookup))
          .map(|re| &re.info)
      });

      if let Some(info) = info {
        // === SPECIFIED PROPERTY ===

        // Check if vendor-prefixed property appears after its unprefixed
        // counterpart. In stylelint-order, vendor-prefixed properties
        // must come BEFORE the unprefixed version.
        if is_vendor_prefixed && seen_unprefixed.contains(lookup) {
          let prev_name = last_property_name.as_deref().unwrap_or("unknown");
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Expected \"{prop}\" to come before \"{prev_name}\""),
            )
            .severity(self.default_severity())
            .span(Span::new(offset, length)),
          );
        }

        // 1. Order check
        self.check_order(
          &config,
          info,
          is_vendor_prefixed,
          last_order_index,
          last_group_index,
          &last_property_name,
          prop,
          offset,
          length,
          &mut diagnostics,
        );

        // 2. Empty line before group check
        self.check_empty_line_before_specified(
          ctx,
          &config,
          info,
          is_first_prop,
          has_skipped_before_first,
          last_was_specified,
          last_group_index,
          threshold_met,
          prop,
          offset,
          length,
          &mut diagnostics,
        );

        // 3. noEmptyLineBetween check (within same group)
        if let Some(prev_group) = last_group_index
          && info.group_index == prev_group
          && last_was_specified == Some(true)
          && let Some(group_info) = config.groups.get(info.group_index)
          && group_info.no_empty_line_between
        {
          let has_empty = has_empty_line_before(ctx.source, offset);
          if has_empty {
            diagnostics.push(
              Diagnostic::new(
                self.name(),
                format!("Unexpected empty line before \"{prop}\""),
              )
              .severity(self.default_severity())
              .span(Span::new(offset, length)),
            );
          }
        }

        // Update tracking
        if !is_vendor_prefixed {
          last_order_index = Some(info.order_index);
          last_group_index = Some(info.group_index);
          last_property_name = Some(prop.to_string());
          last_was_specified = Some(true);
          last_unspecified_name = None;
          is_first_prop = false;
          seen_unprefixed.insert(lookup.to_string());
        } else {
          if last_order_index
            .map(|li| info.order_index >= li)
            .unwrap_or(true)
          {
            last_order_index = Some(info.order_index);
            last_group_index = Some(info.group_index);
          }
          last_property_name = Some(prop.to_string());
          last_was_specified = Some(true);
          last_unspecified_name = None;
          is_first_prop = false;
        }
      } else {
        // === UNSPECIFIED PROPERTY ===
        let prev_was_specified = last_was_specified;

        // 1. emptyLineBeforeUnspecified check (BEFORE updating tracking)
        if let Some(elbu) = config.empty_line_before_unspecified {
          let effective_elb = resolve_threshold(elbu, threshold_met);
          if is_first_prop {
            let has_empty = has_empty_line_before(ctx.source, offset);
            if has_empty {
              diagnostics.push(
                Diagnostic::new(
                  self.name(),
                  format!("Unexpected empty line before \"{prop}\""),
                )
                .severity(self.default_severity())
                .span(Span::new(offset, length)),
              );
            }
          } else if prev_was_specified == Some(true) {
            let has_empty = has_empty_line_before(ctx.source, offset);
            let has_non_inline_comment =
              has_standalone_comment_or_atrule_before(ctx.source, offset);
            match effective_elb {
              EmptyLineBefore::Always => {
                if !has_empty && !has_non_inline_comment {
                  diagnostics.push(
                    Diagnostic::new(
                      self.name(),
                      format!("Expected an empty line before \"{prop}\""),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(offset, length)),
                  );
                }
              }
              EmptyLineBefore::Never => {
                if has_empty {
                  diagnostics.push(
                    Diagnostic::new(
                      self.name(),
                      format!("Unexpected empty line before \"{prop}\""),
                    )
                    .severity(self.default_severity())
                    .span(Span::new(offset, length)),
                  );
                }
              }
              EmptyLineBefore::Threshold => unreachable!(),
            }
          }
        }

        // 2. Ordering check for unspecified
        match config.unspecified {
          Unspecified::Ignore => {}
          Unspecified::Top => {
            if prev_was_specified == Some(true) {
              let prev_name = last_property_name.as_deref().unwrap_or("unknown");
              diagnostics.push(
                Diagnostic::new(
                  self.name(),
                  format!("Expected \"{prop}\" to come before \"{prev_name}\""),
                )
                .severity(self.default_severity())
                .span(Span::new(offset, length)),
              );
            }
            last_was_specified = Some(false);
            last_property_name = Some(prop.to_string());
          }
          Unspecified::Bottom => {
            last_order_index = Some(usize::MAX);
            last_property_name = Some(prop.to_string());
            last_was_specified = Some(false);
          }
          Unspecified::BottomAlphabetical => {
            if let Some(ref last_unspec) = last_unspecified_name
              && prop_lower < *last_unspec
            {
              diagnostics.push(
                Diagnostic::new(
                  self.name(),
                  format!("Expected \"{prop}\" to come before \"{}\"", last_unspec),
                )
                .severity(self.default_severity())
                .span(Span::new(offset, length)),
              );
            }
            last_order_index = Some(usize::MAX);
            last_property_name = Some(prop.to_string());
            last_was_specified = Some(false);
            last_unspecified_name = Some(prop_lower.clone());
          }
        }

        is_first_prop = false;
      }
    }

    diagnostics
  }
}

impl OrderPropertiesOrder {
  /// Check property ordering.
  #[allow(clippy::too_many_arguments)]
  fn check_order(
    &self,
    config: &Config,
    info: &PropertyInfo,
    is_vendor_prefixed: bool,
    last_order_index: Option<usize>,
    last_group_index: Option<usize>,
    last_property_name: &Option<String>,
    prop: &str,
    offset: usize,
    length: usize,
    diagnostics: &mut Vec<Diagnostic>,
  ) {
    let Some(prev_idx) = last_order_index else {
      return;
    };
    if info.order_index >= prev_idx {
      return;
    }

    if is_vendor_prefixed {
      // Vendor-prefixed property: only report if the previous property
      // is NOT the same unprefixed base (vendor after unprefixed is OK).
      let prop_lower = prop.to_ascii_lowercase();
      let lookup = strip_vendor_prefix(&prop_lower);
      let prev_is_same_base = last_property_name
        .as_ref()
        .map(|p| {
          let p_lower = p.to_ascii_lowercase();
          strip_vendor_prefix(&p_lower) == lookup
        })
        .unwrap_or(false);
      if prev_is_same_base {
        return;
      }
    } else {
      // Non-vendor-prefixed: check flexible group
      if let Some(prev_group) = last_group_index {
        let in_same_group = info.group_index == prev_group;
        let is_flexible = config
          .groups
          .get(info.group_index)
          .map(|g| g.flexible)
          .unwrap_or(false);
        if in_same_group && is_flexible {
          return;
        }
      }
    }

    let prev_name = last_property_name.as_deref().unwrap_or("unknown");
    diagnostics.push(
      Diagnostic::new(
        self.name(),
        format!("Expected \"{prop}\" to come before \"{prev_name}\""),
      )
      .severity(self.default_severity())
      .span(Span::new(offset, length)),
    );
  }

  /// Check emptyLineBefore for a specified property.
  #[allow(clippy::too_many_arguments)]
  fn check_empty_line_before_specified(
    &self,
    ctx: &RuleContext,
    config: &Config,
    info: &PropertyInfo,
    is_first_prop: bool,
    has_skipped_before_first: bool,
    last_was_specified: Option<bool>,
    last_group_index: Option<usize>,
    threshold_met: bool,
    prop: &str,
    offset: usize,
    length: usize,
    diagnostics: &mut Vec<Diagnostic>,
  ) {
    let group_info = match config.groups.get(info.group_index) {
      Some(gi) => gi,
      None => return,
    };

    let elb = match group_info.empty_line_before {
      Some(e) => e,
      None => return,
    };

    let effective = resolve_threshold(elb, threshold_met);

    if is_first_prop {
      // If there are skipped declarations (custom props / SCSS vars)
      // before this first real property, don't check empty lines at all.
      // The empty line between the skipped decl and this property is
      // irrelevant to the ordering rule.
      if has_skipped_before_first {
        return;
      }
      // First property in the block: empty line after `{` is always wrong
      let has_empty = has_empty_line_before(ctx.source, offset);
      if has_empty {
        diagnostics.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected empty line before \"{prop}\""),
          )
          .severity(self.default_severity())
          .span(Span::new(offset, length)),
        );
      }
      return;
    }

    // Only check when moving to a different group (or from unspecified)
    let different_group = last_group_index
      .map(|lg| lg != info.group_index)
      .unwrap_or(true);
    let from_unspecified = last_was_specified == Some(false);

    if !different_group && !from_unspecified {
      return;
    }

    let has_empty = has_empty_line_before(ctx.source, offset);
    let has_separator = has_standalone_comment_or_atrule_before(ctx.source, offset);

    match effective {
      EmptyLineBefore::Always => {
        if !has_empty && !has_separator {
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Expected an empty line before \"{prop}\""),
            )
            .severity(self.default_severity())
            .span(Span::new(offset, length)),
          );
        }
      }
      EmptyLineBefore::Never => {
        // If there's a standalone comment or at-rule between the
        // properties, the empty line is acceptable even in "never" mode
        // (the at-rule/comment acts as a natural separator).
        if has_empty && !has_separator {
          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Unexpected empty line before \"{prop}\""),
            )
            .severity(self.default_severity())
            .span(Span::new(offset, length)),
          );
        }
      }
      EmptyLineBefore::Threshold => unreachable!(),
    }
  }
}

/// Resolve "threshold" to either "always" or "never" based on whether the
/// threshold is met.
fn resolve_threshold(elb: EmptyLineBefore, threshold_met: bool) -> EmptyLineBefore {
  match elb {
    EmptyLineBefore::Threshold => {
      if threshold_met {
        EmptyLineBefore::Always
      } else {
        EmptyLineBefore::Never
      }
    }
    other => other,
  }
}

/// Strip vendor prefix from a property name.
fn strip_vendor_prefix(prop: &str) -> &str {
  if !prop.starts_with('-') {
    return prop;
  }
  if let Some(rest) = prop
    .strip_prefix("-webkit-")
    .or_else(|| prop.strip_prefix("-moz-"))
    .or_else(|| prop.strip_prefix("-ms-"))
    .or_else(|| prop.strip_prefix("-o-"))
  {
    rest
  } else {
    prop
  }
}

/// Check if there is an empty line (two consecutive newlines with only whitespace
/// between them) before the given byte offset in the source.
fn has_empty_line_before(source: &str, offset: usize) -> bool {
  if offset == 0 || offset > source.len() {
    return false;
  }

  let before = &source[..offset];
  let mut newline_count = 0;

  for ch in before.chars().rev() {
    if ch == '\n' {
      newline_count += 1;
      if newline_count >= 2 {
        return true;
      }
    } else if ch == '\r' || ch == '\t' || ch == ' ' {
      // whitespace, continue
    } else {
      // Hit non-whitespace
      return false;
    }
  }

  newline_count >= 2
}

/// Check if there is a standalone comment (on its own line, not inline after a
/// declaration) or an at-rule between the previous declaration and this offset.
/// This is used to determine if a comment/at-rule acts as a group separator.
fn has_standalone_comment_or_atrule_before(source: &str, offset: usize) -> bool {
  if offset == 0 || offset > source.len() {
    return false;
  }

  let before = &source[..offset];

  // Get the lines before this offset
  let lines: Vec<&str> = before.lines().collect();
  if lines.len() < 2 {
    return false;
  }

  // Check lines between the previous declaration and this one (skip the current
  // line which contains this property).
  // Walk backwards from the line before the current one.
  for line in lines.iter().rev().skip(1) {
    let trimmed = line.trim();

    if trimmed.is_empty() {
      continue;
    }

    // A line that is entirely a comment (standalone comment)
    if trimmed.starts_with("/*") && trimmed.ends_with("*/") {
      return true;
    }
    // A line starting with `//` (SCSS comment)
    if trimmed.starts_with("//") {
      return true;
    }
    // Start of a multi-line comment
    if trimmed.starts_with("/*") {
      return true;
    }
    // End of a multi-line comment (on its own line)
    if trimmed == "*/" {
      return true;
    }
    // An at-rule line (e.g., @media)
    if trimmed.starts_with('@') {
      return true;
    }

    // If we hit a non-empty, non-comment, non-at-rule line, it's the
    // previous declaration. Check if it has an inline comment.
    // An inline comment (e.g., `display: none; /* comment */`) does NOT
    // count as a standalone separator.
    break;
  }

  false
}

/// Insert a property name or regex pattern (strings delimited by `/`) into the
/// appropriate collection.
fn insert_property_or_regex(
  s: &str,
  order_index: usize,
  group_index: usize,
  property_map: &mut HashMap<String, PropertyInfo>,
  regex_patterns: &mut Vec<RegexEntry>,
) {
  let info = PropertyInfo {
    order_index,
    group_index,
  };

  // Detect regex patterns: strings starting and ending with `/`.
  if s.starts_with('/') && s.ends_with('/') && s.len() > 2 {
    let pattern_str = &s[1..s.len() - 1];
    if let Ok(re) = regex::Regex::new(pattern_str) {
      regex_patterns.push(RegexEntry { pattern: re, info });
      return;
    }
    // If regex compilation fails, fall through and treat as a literal name.
  }

  property_map.insert(s.to_ascii_lowercase(), info);
}

/// Parse the rule configuration from the context.
fn parse_config(ctx: &RuleContext) -> Option<Config> {
  let options = ctx.options?;
  let secondary = ctx.secondary_options();

  // The options can be:
  // 1. A bare array of property groups: [{properties: [...]}, ...]
  // 2. An array [primary_array, secondary_object]: [[...groups...], {unspecified: "bottom"}]
  // We need the array of property groups.
  let arr = match options {
    serde_json::Value::Array(arr) => {
      // Check if first element is an array (nested format)
      if arr.first().is_some_and(|v| v.is_array()) {
        arr.first().and_then(|v| v.as_array())?
      } else {
        arr
      }
    }
    _ => return None,
  };

  let mut property_map = HashMap::new();
  let mut regex_patterns: Vec<RegexEntry> = Vec::new();
  let mut groups: Vec<GroupInfo> = Vec::new();
  let mut order_idx = 0usize;
  let mut group_idx = 0usize;

  for item in arr {
    match item {
      serde_json::Value::String(s) => {
        insert_property_or_regex(
          s,
          order_idx,
          group_idx,
          &mut property_map,
          &mut regex_patterns,
        );
        order_idx += 1;
        groups.push(GroupInfo {
          empty_line_before: None,
          no_empty_line_between: false,
          flexible: false,
        });
        group_idx += 1;
      }
      serde_json::Value::Object(obj) => {
        let elb = obj
          .get("emptyLineBefore")
          .and_then(|v| v.as_str())
          .and_then(|s| match s {
            "always" => Some(EmptyLineBefore::Always),
            "never" => Some(EmptyLineBefore::Never),
            "threshold" => Some(EmptyLineBefore::Threshold),
            _ => None,
          });

        let no_empty_between = obj
          .get("noEmptyLineBetween")
          .and_then(|v| v.as_bool())
          .unwrap_or(false);

        let flexible = obj
          .get("order")
          .and_then(|v| v.as_str())
          .map(|s| s == "flexible")
          .unwrap_or(false);

        if let Some(props) = obj.get("properties").and_then(|v| v.as_array()) {
          for prop in props {
            if let Some(s) = prop.as_str() {
              insert_property_or_regex(
                s,
                order_idx,
                group_idx,
                &mut property_map,
                &mut regex_patterns,
              );
              order_idx += 1;
            }
          }
        }
        groups.push(GroupInfo {
          empty_line_before: elb,
          no_empty_line_between: no_empty_between,
          flexible,
        });
        group_idx += 1;
      }
      _ => {}
    }
  }

  if property_map.is_empty() {
    return None;
  }

  let unspecified = secondary
    .and_then(|s| s.get("unspecified"))
    .and_then(|v| v.as_str())
    .map(|s| match s {
      "top" => Unspecified::Top,
      "bottom" => Unspecified::Bottom,
      "bottomAlphabetical" => Unspecified::BottomAlphabetical,
      _ => Unspecified::Ignore,
    })
    .unwrap_or(Unspecified::Ignore);

  let empty_line_before_unspecified = secondary
    .and_then(|s| s.get("emptyLineBeforeUnspecified"))
    .and_then(|v| v.as_str())
    .and_then(|s| match s {
      "always" => Some(EmptyLineBefore::Always),
      "never" => Some(EmptyLineBefore::Never),
      "threshold" => Some(EmptyLineBefore::Threshold),
      _ => None,
    });

  let empty_line_min_threshold = secondary
    .and_then(|s| s.get("emptyLineMinimumPropertyThreshold"))
    .and_then(|v| v.as_u64())
    .map(|n| n as usize);

  Some(Config {
    property_map,
    regex_patterns,
    groups,
    unspecified,
    empty_line_before_unspecified,
    empty_line_min_threshold,
  })
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
    let rule = OrderPropertiesOrder;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("position", "relative", 19, 19),
      ],
      span: ParserSpan::new(0, 40),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_no_options());
    assert!(diags.is_empty());
  }

  #[test]
  fn simple_array_correct_order() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([["position", "top", "right", "display", "width"]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("position", "relative", 4, 19),
        make_decl("top", "0", 24, 6),
        make_decl("display", "block", 31, 14),
        make_decl("width", "100%", 46, 12),
      ],
      span: ParserSpan::new(0, 60),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn simple_array_wrong_order() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([["position", "top", "right", "display", "width"]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("position", "relative", 19, 19),
      ],
      span: ParserSpan::new(0, 40),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("position"));
    assert!(diags[0].message.contains("display"));
  }

  #[test]
  fn grouped_objects_correct_order() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([[
        { "properties": ["position", "top", "right", "bottom", "left"] },
        { "properties": ["display", "flex-direction"] },
        { "properties": ["width", "height"] }
    ]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("position", "absolute", 4, 19),
        make_decl("top", "0", 24, 6),
        make_decl("display", "flex", 31, 13),
        make_decl("width", "100px", 45, 13),
      ],
      span: ParserSpan::new(0, 60),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn grouped_objects_wrong_order() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([[
        { "properties": ["position", "top", "right", "bottom", "left"] },
        { "properties": ["display", "flex-direction"] },
        { "properties": ["width", "height"] }
    ]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("width", "100px", 4, 13),
        make_decl("position", "absolute", 18, 19),
      ],
      span: ParserSpan::new(0, 40),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("position"));
  }

  #[test]
  fn mixed_strings_and_objects() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([[
        "position",
        { "properties": ["display", "flex"] },
        "color"
    ]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("position", "relative", 4, 19),
        make_decl("display", "flex", 24, 13),
        make_decl("color", "red", 38, 10),
      ],
      span: ParserSpan::new(0, 50),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn unknown_properties_are_ignored() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([["position", "display", "width"]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("position", "relative", 4, 19),
        make_decl("unknown-prop", "foo", 24, 18),
        make_decl("display", "block", 43, 14),
      ],
      span: ParserSpan::new(0, 60),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn skips_scss_variables() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([["position", "display", "width"]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("$my-var", "10px", 19, 14),
        make_decl("position", "relative", 34, 19),
      ],
      span: ParserSpan::new(0, 55),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn skips_custom_properties() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([["position", "display", "width"]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("position", "relative", 4, 19),
        make_decl("--my-var", "10px", 24, 16),
        make_decl("display", "block", 41, 14),
      ],
      span: ParserSpan::new(0, 60),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn vendor_prefix_maps_to_unprefixed() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([["transform", "font-smoothing", "top", "color"]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("color", "pink", 4, 11),
        make_decl("-webkit-font-smoothing", "antialiased", 16, 38),
      ],
      span: ParserSpan::new(0, 60),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn unspecified_bottom() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([["height", "color"], {"unspecified": "bottom"}]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("bottom", "0", 4, 9),
        make_decl("height", "1px", 14, 12),
      ],
      span: ParserSpan::new(0, 30),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn unspecified_top() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([["height", "color"], {"unspecified": "top"}]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("height", "1px", 4, 12),
        make_decl("top", "0", 17, 6),
      ],
      span: ParserSpan::new(0, 30),
      ..Default::default()
    });
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn has_empty_line_detection() {
    assert!(has_empty_line_before("a {\n\n\tdisplay: none;", 6));
    assert!(!has_empty_line_before("a {\n\tdisplay: none;", 5));
  }

  #[test]
  fn strip_vendor_prefix_works() {
    assert_eq!(strip_vendor_prefix("transform"), "transform");
    assert_eq!(strip_vendor_prefix("-webkit-transform"), "transform");
    assert_eq!(strip_vendor_prefix("-moz-box-sizing"), "box-sizing");
    assert_eq!(strip_vendor_prefix("-ms-flex"), "flex");
    assert_eq!(strip_vendor_prefix("-o-transition"), "transition");
    assert_eq!(strip_vendor_prefix("--custom"), "--custom");
  }

  #[test]
  fn regex_pattern_matches_properties() {
    // Config with a regex pattern `/^animation/` that should match
    // any property starting with "animation".
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([["display", "/^animation/"]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("animation-name", "fade", 19, 21),
      ],
      span: ParserSpan::new(0, 45),
      ..Default::default()
    });
    // Correct order: display then animation-name (matches /^animation/).
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert!(diags.is_empty());
  }

  #[test]
  fn regex_pattern_wrong_order() {
    let rule = OrderPropertiesOrder;
    let options = serde_json::json!([["/^animation/", "display"]]);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        make_decl("display", "block", 4, 14),
        make_decl("animation-name", "fade", 19, 21),
      ],
      span: ParserSpan::new(0, 45),
      ..Default::default()
    });
    // Wrong order: display before animation-name, but /^animation/ should come first.
    let diags = rule.check(&node, &ctx_with_options(&options));
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("animation-name"));
  }

  #[test]
  fn standalone_comment_detection() {
    // Standalone comment on its own line
    assert!(has_standalone_comment_or_atrule_before(
      "a {\n\tdisplay: none;\n\t/* comment */\n\tposition: abs;",
      36
    ));
    // Inline comment - not standalone
    assert!(!has_standalone_comment_or_atrule_before(
      "a {\n\tdisplay: none; /* comment */\n\tposition: abs;",
      34
    ));
  }
}
