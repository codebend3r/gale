use std::collections::HashMap;

use gale_css_parser::postcss_tree::{Node, NodeKind, PostcssTree};
use gale_css_parser::value_parser;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::autoprefixable::vendor_prefix;
use crate::data::{CSS_WIDE_KEYWORDS, SHORTHANDS};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::stylelint_order::minimal_edit;
use crate::stylelint_version::installed_at_least_patch;

const NAME: &str = "declaration-block-no-redundant-longhand-properties";

declare_rule! {
  /// Disallow longhand properties that can be combined into one shorthand
  /// property, and longhands that repeat an earlier shorthand's value.
  ///
  /// Equivalent to Stylelint's `declaration-block-no-redundant-longhand-properties`
  /// rule, including its autofix: the first longhand becomes the shorthand
  /// (keeping its `!important`) and the others are removed; a longhand equal to
  /// its shorthand's value is removed.  Secondary options `ignoreShorthands`
  /// (strings or `/regex/`) and `ignoreLonghands` (strings).
  pub struct DeclarationBlockNoRedundantLonghandProperties => DeclarationBlockNoRedundantLonghandPropertiesCheck {
    name: NAME,
    description: "Disallow longhand properties that can be combined into one shorthand property",
    default_severity: Warning,
  }
}

/// `declaration-block-no-redundant-longhand-properties` as configured.
pub struct DeclarationBlockNoRedundantLonghandPropertiesCheck {
  /// The shorthand table, less what `ignoreShorthands` and
  /// `ignoreLonghands` leave out.
  options: Options,
}

impl DeclarationBlockNoRedundantLonghandPropertiesCheck {
  /// Stylelint's options: `true`, then `ignoreShorthands` and
  /// `ignoreLonghands`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_shorthands = options.matchers("ignoreShorthands");
    let ignore_longhands = options.strings("ignoreLonghands");
    options.finish()?;
    Ok(Self {
      options: Options::new(&ignore_shorthands, ignore_longhands),
    })
  }
}

/// Shorthands whose fix also takes their longhands out of every other
/// shorthand's tally.
const OVERLAPPING_SHORTHANDS: &[&str] = &[
  "border-width",
  "border-style",
  "border-color",
  "border-top",
  "border-right",
  "border-bottom",
  "border-left",
  "grid-column",
  "grid-row",
];

impl Check for DeclarationBlockNoRedundantLonghandPropertiesCheck {
  /// Checks every declaration block (rule, at-rule and the root), reporting
  /// what Stylelint reports and attaching, to each problem, one edit that
  /// rewrites the block the way Stylelint's fix leaves it.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let options = &self.options;
    let mut diags = Vec::new();
    // Stylelint visits blocks through rules and at-rules only, so nothing
    // under an SCSS nested property counts.
    let mut containers = vec![None];
    containers.extend(
      tree
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, NodeKind::Rule | NodeKind::AtRule))
        .filter(|(_, n)| !has_decl_ancestor(tree, n.parent))
        .map(|(i, _)| Some(i)),
    );
    for parent in containers {
      if let Some(block) = Block::read(tree, parent) {
        self.check_block(&block, options, &mut diags);
      }
    }
    diags
  }
}

impl DeclarationBlockNoRedundantLonghandPropertiesCheck {
  /// Reports the problems of one block, each carrying the block's fix.
  fn check_block(&self, block: &Block<'_>, options: &Options, diags: &mut Vec<Diagnostic>) {
    let report = simulate(block, options, false);
    if report.problems.is_empty() {
      return;
    }
    let edit = simulate(block, options, true)
      .rendered
      .and_then(|fixed| block.edit_to(&fixed));
    for problem in report.problems {
      let mut diag = Diagnostic::new(NAME, problem.message).span(problem.span);
      if let Some(edit) = &edit {
        diag = diag.fix(Fix::new("Use the shorthand property", vec![edit.clone()]));
      }
      diags.push(diag);
    }
  }
}

/// Whether any of `parent` and its ancestors is a declaration (an SCSS
/// nested property).
fn has_decl_ancestor(tree: &PostcssTree<'_>, mut parent: Option<usize>) -> bool {
  while let Some(index) = parent {
    if tree.nodes[index].kind == NodeKind::Decl {
      return true;
    }
    parent = tree.nodes[index].parent;
  }
  false
}

/// The rule's options, resolved into a longhand → shorthands table.
struct Options {
  /// For each longhand, the shorthands that cover it, in table order.
  longhand_to_shorthands: HashMap<&'static str, Vec<&'static str>>,
  /// Longhands never counted towards a shorthand.
  ignore_longhands: Vec<String>,
}

impl Options {
  /// Builds the table from `ignoreShorthands` and `ignoreLonghands`.
  fn new(ignore_shorthands: &Matchers, ignore_longhands: Vec<String>) -> Self {
    let mut longhand_to_shorthands: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
    for (shorthand, longhands) in SHORTHANDS {
      if ignore_shorthands.matches(shorthand) {
        continue;
      }
      for longhand in *longhands {
        if !ignore_longhands.iter().any(|l| l == longhand) {
          longhand_to_shorthands
            .entry(longhand)
            .or_default()
            .push(shorthand);
        }
      }
    }
    Self {
      longhand_to_shorthands,
      ignore_longhands,
    }
  }
}

/// The longhands of `shorthand`, if it is one.
fn longhands_of(shorthand: &str) -> Option<&'static [&'static str]> {
  SHORTHANDS
    .iter()
    .find(|(name, _)| *name == shorthand)
    .map(|(_, longhands)| *longhands)
}

/// One child of a block, as PostCSS would print it.
struct Slot<'a> {
  /// What the child is.
  kind: NodeKind,
  /// The whitespace (and stray `;`) before it.
  before: &'a str,
  /// Its own text, without a terminating `;`.
  text: &'a str,
  /// Whether PostCSS prints a `;` after it when another node follows.
  takes_semicolon: bool,
  /// Where the child starts in the source.
  start: usize,
  /// Where it ends, after its `;` if it has one.
  end_with_semicolon: usize,
  /// For a declaration, its details.
  decl: Option<DeclInfo<'a>>,
}

/// A declaration of a block.
struct DeclInfo<'a> {
  /// The property as written.
  prop: &'a str,
  /// The property in lowercase.
  prop_lower: String,
  /// PostCSS's `decl.value`: the raw value without comments next to
  /// whitespace and without trailing whitespace.
  value: String,
  /// Whether the declaration is `!important`.
  important: bool,
  /// Where the property starts and ends.
  prop_span: Span,
  /// The text before the property (an IE hack like `*`).
  hack: &'a str,
  /// The text between the property and the value.
  between: &'a str,
  /// The text between the value and the end (`!important`).
  important_raw: &'a str,
}

/// A declaration block: the children of a rule, at-rule or the root.
struct Block<'a> {
  /// The children, in order.
  slots: Vec<Slot<'a>>,
  /// The whitespace after the last child (PostCSS `raws.after`).
  after: &'a str,
  /// The source range the children and `after` cover.
  start: usize,
  /// The end of that range.
  end: usize,
  /// The source text of that range.
  text: &'a str,
  /// PostCSS `raws.semicolon`.
  semicolon: bool,
  /// Whether this is the root, whose removals pass a first node's
  /// `before` on to the next.
  is_root: bool,
}

impl<'a> Block<'a> {
  /// The block of `parent` (the root for `None`), if it holds a declaration
  /// and its text can be rebuilt from its children.
  fn read(tree: &PostcssTree<'a>, parent: Option<usize>) -> Option<Self> {
    let source = tree.source();
    let children = tree.children_of(parent);
    if !children
      .iter()
      .any(|&i| tree.nodes[i].kind == NodeKind::Decl)
    {
      return None;
    }
    let (start, end, semicolon) = match parent {
      Some(index) => {
        let node = &tree.nodes[index];
        let open = node.block_open?;
        (
          open + 1,
          block_close(source, node, open + 1),
          node.semicolon,
        )
      }
      None => (0, source.len(), tree.root_semicolon),
    };
    let mut slots = Vec::with_capacity(children.len());
    for &index in children {
      let node = &tree.nodes[index];
      let takes_semicolon =
        node.children.is_none() && matches!(node.kind, NodeKind::Decl | NodeKind::AtRule);
      // The tree's `end` takes in a terminating `;`; PostCSS prints that
      // separately.
      let has_semicolon =
        takes_semicolon && source.as_bytes().get(node.end.wrapping_sub(1)) == Some(&b';');
      let text_end = if has_semicolon {
        node.end - 1
      } else {
        node.end
      };
      let text = source.get(node.start..text_end)?;
      let decl = if node.kind == NodeKind::Decl {
        let prop = source.get(node.name_span.start..node.name_span.end)?;
        Some(DeclInfo {
          prop,
          prop_lower: prop.to_ascii_lowercase(),
          value: node.value.clone(),
          important: node.important,
          prop_span: Span::from_range(node.name_span.start, node.name_span.end),
          hack: source.get(node.start..node.name_span.start)?,
          between: source.get(node.name_span.end..node.value_span.start)?,
          important_raw: source.get(node.value_span.end..text_end)?,
        })
      } else {
        None
      };
      slots.push(Slot {
        kind: node.kind,
        // The tree lets a `*`/`_` hack overlap `before`; it belongs to the
        // node's text here.
        before: source.get(node.before.start..node.start)?,
        text,
        takes_semicolon,
        start: node.start,
        end_with_semicolon: node.end,
        decl,
      });
    }
    let after_start = slots.last().map_or(start, |s| s.end_with_semicolon);
    let block = Block {
      after: source.get(after_start..end)?,
      start: children
        .first()
        .map_or(start, |&i| tree.nodes[i].before.start),
      end,
      text: source.get(start..end)?,
      slots,
      semicolon,
      is_root: parent.is_none(),
    };
    // Only fix what the model reproduces exactly.
    let unchanged = block.render(
      &vec![SlotState::Present; block.slots.len()],
      &block.befores(),
    );
    (block.start == start && unchanged == block.text).then_some(block)
  }

  /// Every child's `before`, as owned strings a simulation may change.
  fn befores(&self) -> Vec<String> {
    self.slots.iter().map(|s| s.before.to_string()).collect()
  }

  /// The block's text with the children in `states`, printed as PostCSS
  /// prints a container body.
  fn render(&self, states: &[SlotState], befores: &[String]) -> String {
    let live: Vec<usize> = (0..self.slots.len())
      .filter(|&i| states[i] != SlotState::Removed)
      .collect();
    // The last node that is not a comment (or the first node).
    let mut last = live.len().saturating_sub(1);
    while last > 0 && self.slots[live[last]].kind == NodeKind::Comment {
      last -= 1;
    }
    let mut out = String::with_capacity(self.text.len());
    for (position, &i) in live.iter().enumerate() {
      let slot = &self.slots[i];
      out.push_str(&befores[i]);
      match &states[i] {
        SlotState::Replaced(text) => out.push_str(text),
        _ => out.push_str(slot.text),
      }
      if slot.takes_semicolon && (position != last || self.semicolon) {
        out.push(';');
      }
    }
    out.push_str(self.after);
    out
  }

  /// The smallest edit turning this block's text into `fixed`, if they
  /// differ.
  fn edit_to(&self, fixed: &str) -> Option<Edit> {
    minimal_edit(self.start, self.end, self.text, fixed)
  }
}

/// Where the `}` closing the block of `node` sits (its body ends there),
/// or the node's end when the block runs to the end of the input.  A rule
/// that took a stray `;` ends after it, so step back over that first.
fn block_close(source: &str, node: &Node, body_start: usize) -> usize {
  let bytes = source.as_bytes();
  let mut close = node.end.min(bytes.len());
  while close > body_start && matches!(bytes[close - 1], b';' | b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
  {
    close -= 1;
  }
  if close > body_start && bytes[close - 1] == b'}' {
    close - 1
  } else {
    node.end
  }
}

/// What a simulated fix has done to a child.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SlotState {
  /// Untouched.
  Present,
  /// Replaced by a new declaration with this text.
  Replaced(String),
  /// Removed.
  Removed,
}

/// A problem found by the simulation.
struct Problem {
  message: String,
  span: Span,
}

/// What a simulation produced.
struct Simulation {
  /// The problems reported (without fixing, the warnings).
  problems: Vec<Problem>,
  /// With fixing, the block text afterwards, if anything changed.
  rendered: Option<String>,
}

/// Run Stylelint's check over `block`.  Without `fix` it only reports; with
/// `fix` it applies every fix as it goes, as `stylelint --fix` does, and
/// returns the resulting text.
fn simulate(block: &Block<'_>, options: &Options, fix: bool) -> Simulation {
  let mut states = vec![SlotState::Present; block.slots.len()];
  let mut befores = block.befores();
  let mut problems = Vec::new();
  let decls: Vec<(usize, &DeclInfo<'_>)> = block
    .slots
    .iter()
    .enumerate()
    .filter_map(|(i, s)| s.decl.as_ref().map(|d| (i, d)))
    .collect();
  // Declarations (indices into `decls`) collected per prefixed shorthand.
  let mut collected: HashMap<String, Vec<usize>> = HashMap::new();
  let mut declared_shorthands: HashMap<String, String> = HashMap::new();

  for (d, &(slot, decl)) in decls.iter().enumerate() {
    let value = decl.value.trim();
    if CSS_WIDE_KEYWORDS.contains(&value) {
      continue;
    }
    let prop = decl.prop_lower.as_str();
    let prefix = vendor_prefix(prop);
    let unprefixed = &prop[prefix.len()..];
    if longhands_of(unprefixed).is_some() {
      declared_shorthands.insert(prop.to_string(), value.to_string());
    }
    let Some(shorthands) = options.longhand_to_shorthands.get(unprefixed) else {
      continue;
    };
    for shorthand in shorthands {
      let prefixed = format!("{prefix}{shorthand}");
      if declared_shorthands.get(&prefixed).map(String::as_str) == Some(value) {
        if fix {
          remove(block, &mut states, &mut befores, slot);
        } else {
          problems.push(Problem {
            message: format!(
              "Redundant longhand property \"{}\" after shorthand property \"{prefixed}\"",
              decl.prop
            ),
            span: decl.prop_span,
          });
        }
        break;
      }
      let nodes = collected.entry(prefixed.clone()).or_default();
      nodes.push(d);
      let nodes = nodes.clone();
      let mut expected: Vec<String> = longhands_of(shorthand)
        .unwrap_or_default()
        .iter()
        .filter(|l| !options.ignore_longhands.iter().any(|i| i == *l))
        .map(|l| format!("{prefix}{l}"))
        .collect();
      let mut found: Vec<String> = nodes
        .iter()
        .map(|&n| decls[n].1.prop_lower.clone())
        .collect();
      let data = expected.clone();
      expected.sort();
      found.sort();
      if expected != found {
        continue;
      }
      let important = nodes.iter().filter(|&&n| decls[n].1.important).count();
      if important != 0 && important != nodes.len() {
        continue;
      }
      let by_prop: HashMap<&str, &DeclInfo<'_>> = nodes
        .iter()
        .map(|&n| (decls[n].1.prop_lower.as_str(), decls[n].1))
        .collect();
      let resolved = resolve_shorthand_value(&prefixed, &data, &by_prop).filter(|v| !v.is_empty());
      let (first_slot, first) = decls[nodes[0]];
      let (last_slot, _) = decls[nodes[nodes.len() - 1]];
      match (&resolved, fix) {
        (Some(value), true) => {
          if states[first_slot] == SlotState::Present {
            states[first_slot] = SlotState::Replaced(format!(
              "{}{prefixed}{}{value}{}",
              first.hack, first.between, first.important_raw
            ));
          }
          for &n in &nodes {
            remove(block, &mut states, &mut befores, decls[n].0);
          }
          if OVERLAPPING_SHORTHANDS.contains(shorthand) {
            let consumed: Vec<String> = nodes
              .iter()
              .map(|&n| decls[n].1.prop_lower.clone())
              .collect();
            for list in collected.values_mut() {
              list.retain(|&n| !consumed.contains(&decls[n].1.prop_lower));
            }
          }
        }
        (None, true) => {}
        (_, false) => {
          let span = if installed_at_least_patch(17, 11, 1) {
            // Contiguous longhands are reported from the first, others from
            // the last; the problem runs to the end of the last.
            let contiguous = last_slot - first_slot + 1 == nodes.len();
            let start = block.slots[if contiguous { first_slot } else { last_slot }].start;
            Span::from_range(start, block.slots[last_slot].end_with_semicolon)
          } else {
            // Before 17.11.1 Stylelint reported the property of the
            // declaration that completes the set.
            decl.prop_span
          };
          problems.push(Problem {
            message: format!("Expected shorthand property \"{prefixed}\""),
            span,
          });
        }
      }
    }
  }

  let rendered = fix
    .then(|| block.render(&states, &befores))
    .filter(|text| text != block.text);
  Simulation { problems, rendered }
}

/// PostCSS's `node.remove()` on child `slot`: a no-op once it is gone; at
/// the root, removing the first node hands its `before` to the next one.
fn remove(block: &Block<'_>, states: &mut [SlotState], befores: &mut [String], slot: usize) {
  if states[slot] != SlotState::Present {
    // Already removed, or replaced by a new node that stays.
    return;
  }
  if block.is_root {
    let live: Vec<usize> = (0..states.len())
      .filter(|&i| states[i] != SlotState::Removed)
      .collect();
    if live.first() == Some(&slot) && live.len() > 1 {
      befores[live[1]] = befores[slot].clone();
    }
  }
  states[slot] = SlotState::Removed;
}

/// Stylelint's `resolveShorthandValue`: the shorthand value for the
/// longhands in `by_prop`, or `None` when it cannot be written safely.
fn resolve_shorthand_value(
  prefixed: &str,
  data: &[String],
  by_prop: &HashMap<&str, &DeclInfo<'_>>,
) -> Option<String> {
  let get = |prop: &str| by_prop.get(prop).map(|d| d.value.trim().to_string());
  let present = |prop: &str| get(prop).filter(|v| !v.is_empty());
  match prefixed {
    "font" => {
      let [style, variant, weight, stretch, size, line_height, family] = [
        "font-style",
        "font-variant",
        "font-weight",
        "font-stretch",
        "font-size",
        "line-height",
        "font-family",
      ]
      .map(present);
      Some(format!(
        "{} {} {} {} {}/{} {}",
        style?, variant?, weight?, stretch?, size?, line_height?, family?
      ))
    }
    "font-synthesis" => {
      let values = [
        "font-synthesis-weight",
        "font-synthesis-style",
        "font-synthesis-small-caps",
      ]
      .map(get);
      if !values
        .iter()
        .all(|v| matches!(v.as_deref(), Some("none" | "auto")))
      {
        return None;
      }
      let auto: Vec<&str> = values
        .iter()
        .zip(["weight", "style", "small-caps"])
        .filter(|(v, _)| v.as_deref() == Some("auto"))
        .map(|(_, name)| name)
        .collect();
      Some(if auto.is_empty() {
        "none".to_string()
      } else {
        auto.join(" ")
      })
    }
    "grid-column" | "grid-row" => {
      let start = present(&format!("{prefixed}-start"))?;
      let end = present(&format!("{prefixed}-end"))?;
      Some(format!("{start} / {end}"))
    }
    "grid-template" => {
      let areas = present("grid-template-areas")?;
      let columns = present("grid-template-columns")?;
      let rows = present("grid-template-rows")?;
      if columns.contains("repeat(") || rows.contains("repeat(") {
        return None;
      }
      let split_areas = quoted_strings(&areas);
      let split_rows: Vec<&str> = rows.split(' ').collect();
      if split_areas.is_empty() || split_areas.len() != split_rows.len() {
        return None;
      }
      let zipped: Vec<String> = split_areas
        .iter()
        .zip(&split_rows)
        .map(|(area, row)| format!("{area} {row}"))
        .collect();
      Some(format!("{} / {columns}", zipped.join(" ")))
    }
    "transition" => {
      let delays = comma_separated(get("transition-delay"));
      let durations = comma_separated(get("transition-duration"));
      let timings = comma_separated(get("transition-timing-function"));
      let properties = comma_separated(get("transition-property"));
      if delays.is_empty() || durations.is_empty() || timings.is_empty() || properties.is_empty() {
        return None;
      }
      let items: Vec<String> = properties
        .iter()
        .enumerate()
        .map(|(i, property)| {
          [
            property.as_str(),
            &durations[i % durations.len()],
            &timings[i % timings.len()],
            &delays[i % delays.len()],
          ]
          .join(" ")
        })
        .collect();
      Some(items.join(", "))
    }
    _ => {
      let prefix = vendor_prefix(prefixed);
      if &prefixed[prefix.len()..] == "background"
        && by_prop.contains_key(format!("{prefix}background-size").as_str())
      {
        // Stylelint cannot place `background-size` after the position yet.
        return None;
      }
      let values: Vec<String> = data.iter().filter_map(|p| present(p)).collect();
      Some(values.join(" "))
    }
  }
}

/// The `"..."` strings in `text` (`/"[^"]+"/g`).
fn quoted_strings(text: &str) -> Vec<&str> {
  let mut found = Vec::new();
  let mut rest = 0;
  while let Some(open) = text[rest..].find('"') {
    let start = rest + open;
    match text[start + 1..].find('"') {
      Some(len) if len > 0 => {
        found.push(&text[start..start + 1 + len + 1]);
        rest = start + 1 + len + 1;
      }
      Some(_) => rest = start + 1,
      None => break,
    }
  }
  found
}

/// Stylelint's `commaSeparated`: the trimmed, non-empty items of a
/// comma-separated value.
fn comma_separated(input: Option<String>) -> Vec<String> {
  let Some(trimmed) = input
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
  else {
    return Vec::new();
  };
  if !trimmed.contains(',') {
    return vec![trimmed];
  }
  let mut parts: Vec<Vec<value_parser::ValueNode<'_>>> = vec![Vec::new()];
  for node in value_parser::parse(&trimmed) {
    if node.is_comma() {
      parts.push(Vec::new());
    } else if let Some(part) = parts.last_mut() {
      part.push(node);
    }
  }
  parts
    .iter()
    .map(|nodes| value_parser::stringify(nodes).trim().to_string())
    .filter(|part| !part.is_empty())
    .collect()
}

#[cfg(test)]
mod tests;
