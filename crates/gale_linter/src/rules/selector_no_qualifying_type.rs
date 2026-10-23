use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::selector::postcss::split_selectors;
use gale_diagnostics::{Diagnostic, Span};

use crate::chars::is_ident_char;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "selector-no-qualifying-type";

declare_rule! {
  /// Disallow qualifying a selector by type (e.g. `a.foo`, `div#bar`).
  ///
  /// Equivalent to Stylelint's `selector-no-qualifying-type` rule.
  pub struct SelectorNoQualifyingType => SelectorNoQualifyingTypeCheck {
    name: NAME,
    description: "Disallow qualifying a selector by type",
    default_severity: Warning,
  }
}

/// `selector-no-qualifying-type` as configured.
pub struct SelectorNoQualifyingTypeCheck {
  /// `ignore: ["class"]`.
  ignore_class: bool,
  /// `ignore: ["attribute"]`.
  ignore_attribute: bool,
  /// `ignore: ["id"]`.
  ignore_id: bool,
}

impl SelectorNoQualifyingTypeCheck {
  /// Stylelint's options: `true`, then `ignore` listing `"attribute"`,
  /// `"class"` and `"id"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.primary_true();
    let ignore = options.keywords("ignore", &["attribute", "class", "id"]);
    options.finish()?;
    Ok(Self {
      ignore_class: ignore.contains(&"class"),
      ignore_attribute: ignore.contains(&"attribute"),
      ignore_id: ignore.contains(&"id"),
    })
  }
}

impl Check for SelectorNoQualifyingTypeCheck {
  /// Flags selectors qualifying a class, attribute or ID with a type, honouring
  /// the `ignore` options.  A nested `&` stands for a type when a rule it is
  /// nested in, through any at-rules, ends in one.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    // Rules that are not standard syntax, keyframe selectors and, as
    // Stylelint resolves nesting before it checks and gives up on a
    // selector it cannot resolve, rules nested in a rule that is not
    // standard syntax (interpolation, a placeholder, a Less mixin) are
    // skipped.
    for rule in tree
      .walk_rules()
      .filter(|rule| rule.is_standard_syntax() && !rule.in_keyframes())
    {
      let ancestors = std::iter::successors(rule.parent_rule(), |rule| rule.parent_rule());
      let mut ancestor_is_type = false;
      let mut ancestor_is_standard = true;
      for ancestor in ancestors {
        ancestor_is_type |= selector_ends_with_type(ancestor.selector());
        ancestor_is_standard &= ancestor.is_standard_syntax();
      }
      if !ancestor_is_standard {
        continue;
      }
      check_selector_list(
        &rule.raw_selector(),
        rule.selector_start(),
        ancestor_is_type,
        self.ignore_class,
        self.ignore_attribute,
        self.ignore_id,
        &mut diags,
      );
    }
    diags
  }
}

/// Check a comma-separated selector list, emitting diagnostics with correct
/// byte offsets pointing to each compound selector that violates.
fn check_selector_list(
  selector_list: &str,
  base_offset: usize,
  ancestor_is_type: bool,
  ignore_class: bool,
  ignore_attribute: bool,
  ignore_id: bool,
  diags: &mut Vec<Diagnostic>,
) {
  for (byte_pos, sel_part) in split_selectors(selector_list) {
    let trimmed = sel_part.trim();
    if trimmed.is_empty() {
      continue;
    }
    let leading_ws = sel_part.len() - sel_part.trim_start().len();
    let sel_byte_offset = byte_pos + leading_ws;

    if let Some(compound_offset) = find_qualifying_type_offset(
      trimmed,
      ancestor_is_type,
      ignore_class,
      ignore_attribute,
      ignore_id,
    ) {
      let violation_offset = base_offset + sel_byte_offset + compound_offset;
      diags.push(
        Diagnostic::new(
          NAME,
          format!("Unexpected qualifying type selector in \"{trimmed}\""),
        )
        .span(Span::new(violation_offset, trimmed.len() - compound_offset)),
      );
    }
  }
}

/// Find the byte offset within a single (already comma-split) selector where
/// a qualifying type violation occurs. Returns `None` if no violation.
///
/// Handles:
/// - `div.class`, `div#id`, `div[attr]`
/// - `div:hover#thing` (type + pseudo-class + qualifier)
/// - Nesting `&` with ancestor type context (e.g. `a { &.class {} }`)
/// - `:is()`/`:not()`/`:has()`/`:where()` inner selectors
fn find_qualifying_type_offset(
  selector: &str,
  ancestor_is_type: bool,
  ignore_class: bool,
  ignore_attribute: bool,
  ignore_id: bool,
) -> Option<usize> {
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    // Skip whitespace and combinators
    if chars[i].is_ascii_whitespace() || chars[i] == '>' || chars[i] == '+' || chars[i] == '~' {
      i += 1;
      continue;
    }

    // Start of a compound selector
    let compound_start = i;
    let mut saw_type = false;

    // Check for type selector at start of compound
    if is_type_selector_start(&chars, i) {
      saw_type = true;
      while i < len && is_ident_char(chars[i]) {
        i += 1;
      }
    } else if chars[i] == '%' {
      // SCSS placeholder selector (%foo) — skip like a class
      i += 1;
      while i < len && is_ident_char(chars[i]) {
        i += 1;
      }
    } else if chars[i] == '&' {
      // Nesting selector — if parent is a type selector, treat as type
      i += 1;
      // Consume any trailing ident chars (BEM-style `&--modifier`, `&__element`)
      while i < len && is_ident_char(chars[i]) {
        i += 1;
      }
      if ancestor_is_type {
        saw_type = true;
      }
    }

    // Track whether compound has non-type simple selectors (class, id, attr)
    // for detecting qualifying inside :is()/:not() etc.
    let mut has_class = false;
    let mut has_id = false;
    let mut has_attribute = false;

    // Walk through the rest of the compound selector
    loop {
      if i >= len {
        break;
      }

      // Check for qualifying selector after a type
      if saw_type {
        if chars[i] == '.' && !ignore_class {
          return Some(compound_start);
        }
        if chars[i] == '#' && !ignore_id {
          return Some(compound_start);
        }
        if chars[i] == '[' && !ignore_attribute {
          return Some(compound_start);
        }
      }

      // Skip CSS comments `/* ... */`
      if chars[i] == '/' && i + 1 < len && chars[i + 1] == '*' {
        i += 2;
        while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2; // skip */
        }
        continue;
      }

      if chars[i] == '.' {
        // Class selector
        has_class = true;
        i += 1;
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
      } else if chars[i] == '#' {
        // ID selector
        has_id = true;
        i += 1;
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
      } else if chars[i] == '[' {
        // Attribute selector — skip to closing bracket
        has_attribute = true;
        i += 1;
        let mut bracket_depth = 1;
        while i < len && bracket_depth > 0 {
          if chars[i] == '[' {
            bracket_depth += 1;
          } else if chars[i] == ']' {
            bracket_depth -= 1;
          }
          i += 1;
        }
      } else if chars[i] == ':' {
        // Pseudo-class or pseudo-element
        i += 1;
        let is_pseudo_element = i < len && chars[i] == ':';
        if is_pseudo_element {
          i += 1;
        }
        // Consume pseudo name
        let pseudo_start = i;
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
        let pseudo_name: String = chars[pseudo_start..i].iter().collect();
        let pseudo_lower = pseudo_name.to_ascii_lowercase();

        if i < len && chars[i] == '(' {
          // Functional pseudo-class
          let is_selector_pseudo = !is_pseudo_element
            && matches!(
              pseudo_lower.as_str(),
              "is" | "not" | "has" | "where" | "matches" | "any"
            );

          if is_selector_pseudo {
            // Parse the args
            let paren_idx = i;
            i += 1;
            let args_start = i;
            let mut depth = 1;
            while i < len && depth > 0 {
              if chars[i] == '(' {
                depth += 1;
              } else if chars[i] == ')' {
                depth -= 1;
              }
              i += 1;
            }
            let args_end = if i > 0 { i - 1 } else { i };
            let inner: String = chars[args_start..args_end].iter().collect();

            // 1) Check inner selector list for standalone qualifying types
            for (inner_pos, inner_part) in split_selectors(&inner) {
              let inner_trimmed = inner_part.trim();
              if inner_trimmed.is_empty() {
                continue;
              }
              if let Some(inner_off) = find_qualifying_type_offset(
                inner_trimmed,
                false,
                ignore_class,
                ignore_attribute,
                ignore_id,
              ) {
                let leading = inner_part.len() - inner_part.trim_start().len();
                let byte_off = char_offset_to_byte_offset(selector, paren_idx + 1)
                  + inner_pos
                  + leading
                  + inner_off;
                return Some(byte_off);
              }
            }

            // 2) Check if any inner arg is a bare type selector
            //    AND the compound has non-type selectors qualifying it.
            //    e.g. [attribute]:is(a) or .class:is(a)
            let has_qualifier = (has_class && !ignore_class)
              || (has_id && !ignore_id)
              || (has_attribute && !ignore_attribute);
            if has_qualifier {
              for (inner_pos, inner_part) in split_selectors(&inner) {
                let inner_trimmed = inner_part.trim();
                if inner_trimmed.is_empty() {
                  continue;
                }
                if is_bare_type_selector(inner_trimmed) {
                  let leading = inner_part.len() - inner_part.trim_start().len();
                  let byte_off =
                    char_offset_to_byte_offset(selector, paren_idx + 1) + inner_pos + leading;
                  return Some(byte_off);
                }
              }
            }
          } else {
            // Non-selector functional pseudo — skip args
            i += 1;
            let mut depth = 1;
            while i < len && depth > 0 {
              if chars[i] == '(' {
                depth += 1;
              } else if chars[i] == ')' {
                depth -= 1;
              }
              i += 1;
            }
          }
        }
        // After pseudo, the type still qualifies what follows
      } else if chars[i] == '*' {
        i += 1;
      } else if chars[i] == '&' {
        i += 1;
        // Consume trailing ident chars (BEM-style &--modifier)
        while i < len && is_ident_char(chars[i]) {
          i += 1;
        }
      } else {
        // Unknown char (e.g. Less `@{var}`) — break out of compound
        break;
      }
    }

    // Safety: ensure `i` always advances to avoid infinite loops
    // with unrecognised characters (Less interpolation, etc.)
    if i == compound_start {
      i += 1;
    }
  }
  None
}

/// Convert a char offset to a byte offset in the given string.
fn char_offset_to_byte_offset(s: &str, char_offset: usize) -> usize {
  s.char_indices()
    .nth(char_offset)
    .map(|(byte_idx, _)| byte_idx)
    .unwrap_or(s.len())
}

/// Check if the last compound selector in a selector string ends with (or is)
/// a type selector. Used to determine if `&` in nested rules resolves to a type.
fn selector_ends_with_type(selector: &str) -> bool {
  let trimmed = selector.trim();
  if trimmed.is_empty() {
    return false;
  }
  // Find the last compound selector (after last combinator at top level)
  let parts = split_selectors(trimmed);
  // Use the last comma-separated selector
  if let Some((_pos, last_sel)) = parts.last() {
    let last_sel = last_sel.trim();
    if last_sel.is_empty() {
      return false;
    }
    // Find the last compound selector within this (after last space/combinator)
    let chars: Vec<char> = last_sel.chars().collect();
    let len = chars.len();
    let mut last_compound_start = 0;
    let mut i = 0;
    let mut depth = 0;
    while i < len {
      match chars[i] {
        '(' => depth += 1,
        ')' if depth > 0 => depth -= 1,
        ' ' | '>' | '+' | '~' if depth == 0 => {
          // Skip whitespace/combinators
          while i < len
            && (chars[i].is_ascii_whitespace()
              || chars[i] == '>'
              || chars[i] == '+'
              || chars[i] == '~')
          {
            i += 1;
          }
          if i < len {
            last_compound_start = i;
          }
          continue;
        }
        _ => {}
      }
      i += 1;
    }
    // Check if the last compound starts with a type selector
    if last_compound_start < len {
      return is_type_selector_start(&chars, last_compound_start);
    }
  }
  false
}

/// Check if a selector string is a bare type selector (e.g. `a`, `div`).
/// Used to detect when `:is(a)` contains a type that is qualified by outer selectors.
fn is_bare_type_selector(selector: &str) -> bool {
  let trimmed = selector.trim();
  if trimmed.is_empty() {
    return false;
  }
  let chars: Vec<char> = trimmed.chars().collect();
  if !is_type_selector_start(&chars, 0) {
    return false;
  }
  // All characters should be ident chars
  chars.iter().all(|c| is_ident_char(*c))
}

/// Whether the character at `i` begins a type selector rather than a class,
/// ID or pseudo name.
fn is_type_selector_start(chars: &[char], i: usize) -> bool {
  let ch = chars[i];
  if !ch.is_ascii_alphabetic() && ch.is_ascii() {
    return false;
  }
  // Must not be preceded by '.', '#', or ':' (which would mean it's part of a class/id/pseudo)
  if i > 0 {
    let prev = chars[i - 1];
    if prev == '.' || prev == '#' || prev == ':' {
      return false;
    }
  }
  true
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, invalid_options};

  /// A rule with the selector `sel`.
  fn style_with_selector(sel: &str) -> String {
    format!("{sel} {{ color: red; }}")
  }

  #[test]
  fn reports_type_with_class() {
    let rule = configured(&SelectorNoQualifyingType, None);
    let node = style_with_selector("a.foo");
    let d = rule.check_source(&context(&node, Syntax::Css));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("a.foo"));
  }

  #[test]
  fn reports_type_with_id() {
    let rule = configured(&SelectorNoQualifyingType, None);
    let node = style_with_selector("div#bar");
    let d = rule.check_source(&context(&node, Syntax::Css));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_class_only() {
    let rule = configured(&SelectorNoQualifyingType, None);
    let node = style_with_selector(".foo");
    let d = rule.check_source(&context(&node, Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn allows_type_only() {
    let rule = configured(&SelectorNoQualifyingType, None);
    let node = style_with_selector("div");
    let d = rule.check_source(&context(&node, Syntax::Css));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_type_with_pseudo_then_id() {
    let rule = configured(&SelectorNoQualifyingType, None);
    // div:hover#thing should be reported
    let node = style_with_selector("div:hover#thing");
    let d = rule.check_source(&context(&node, Syntax::Css));
    assert_eq!(d.len(), 1, "should report div:hover#thing as qualifying");
  }

  #[test]
  fn ignore_option_id() {
    let rule = configured(
      &SelectorNoQualifyingType,
      serde_json::json!({"ignore": ["id"]}),
    );
    let node = style_with_selector("div#bar");
    let d = rule.check_source(&context(&node, Syntax::Css));
    assert!(
      d.is_empty(),
      "should ignore id qualifying when ignore: [\"id\"]"
    );
  }

  #[test]
  fn ignore_option_class() {
    let rule = configured(
      &SelectorNoQualifyingType,
      serde_json::json!({"ignore": ["class"]}),
    );
    let node = style_with_selector("ul.list");
    let d = rule.check_source(&context(&node, Syntax::Css));
    assert!(
      d.is_empty(),
      "should ignore class qualifying when ignore: [\"class\"]"
    );
  }

  #[test]
  fn reports_nesting_ampersand_with_class() {
    let rule = configured(&SelectorNoQualifyingType, None);
    // a { &.class {} } — the & resolves to the parent type `a`
    let parent = "a { &.class { color: red; } }".to_string();
    let d = rule.check_source(&context(&parent, Syntax::Css));
    assert_eq!(
      d.len(),
      1,
      "should report &.class when parent is type selector"
    );
  }

  #[test]
  fn no_infinite_loop_on_is() {
    let rule = configured(&SelectorNoQualifyingType, None);
    // Ensure :is(a, b) does not hang
    let node = style_with_selector(":is(a, b):is(c, d)");
    let d = rule.check_source(&context(&node, Syntax::Css));
    // No qualifying type here
    assert!(d.is_empty());
  }

  #[test]
  fn split_respects_parens() {
    let parts = split_selectors("a, :is(b, c), d");
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0].1, "a");
    assert_eq!(parts[1].1, " :is(b, c)");
    assert_eq!(parts[2].1, " d");
  }

  #[test]
  fn skips_nested_in_scss_interpolation_parent() {
    let rule = configured(&SelectorNoQualifyingType, None);
    // a.foo nested inside .#{$var} should be skipped in SCSS mode
    let parent = ".#{$var} { a.foo { color: red; } }".to_string();
    let d = rule.check_source(&context(&parent, Syntax::Scss));
    assert!(
      d.is_empty(),
      "should skip rules nested inside SCSS interpolation parent"
    );
  }

  #[test]
  fn reports_where_the_selector_is_written() {
    let rule = configured(&SelectorNoQualifyingType, None);
    let source = ".x,\n  .y>a.foo {}\nb { @media print { &.c {} } }";
    let d = rule.check_source(&context(source, Syntax::Css));
    let found: Vec<_> = d
      .iter()
      .map(|d| &source[d.span.offset..d.span.end()])
      .collect();
    // The `&` under `@media` stands for `b`, through the at-rule.
    assert_eq!(found, ["a.foo", "&.c"]);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignore": ["attribute", "class", "id"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"selector-no-qualifying-type\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignore": ["type"] }])),
      ["Invalid value \"type\" for option \"ignore\" of rule \"selector-no-qualifying-type\""]
    );
  }
}
