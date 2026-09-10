use std::collections::HashMap;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports when a selector with lower specificity appears after a selector with
/// higher specificity, which may indicate a specificity ordering issue.
///
/// Equivalent to Stylelint's `no-descending-specificity` rule.
pub struct NoDescendingSpecificity;

/// Specificity as an (a, b, c) tuple where:
/// - a = number of ID selectors
/// - b = number of class selectors, attribute selectors, and pseudo-classes
/// - c = number of type selectors and pseudo-elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Specificity(u32, u32, u32);

/// Extract the content inside balanced parentheses, advancing `i` past the
/// closing `)`.  Assumes `chars[*i] == '('` on entry.
fn extract_parenthesized_content(chars: &[char], i: &mut usize) -> String {
  let mut depth = 1;
  *i += 1; // skip opening '('
  let start = *i;
  while *i < chars.len() && depth > 0 {
    if chars[*i] == '(' {
      depth += 1;
    } else if chars[*i] == ')' {
      depth -= 1;
    }
    if depth > 0 {
      *i += 1;
    }
  }
  let content: String = chars[start..*i].iter().collect();
  if *i < chars.len() {
    *i += 1; // skip closing ')'
  }
  content
}

/// Calculate the specificity of a selector string.
///
/// This is a simplified calculation that handles common cases:
/// - `#id` contributes to `a`
/// - `.class`, `[attr]`, `:pseudo-class` contribute to `b`
/// - `element`, `::pseudo-element` contribute to `c`
/// - Universal selector `*` contributes nothing
/// - Combinators (` `, `>`, `+`, `~`) contribute nothing
fn calculate_specificity(selector: &str) -> Specificity {
  let mut a: u32 = 0; // IDs
  let mut b: u32 = 0; // classes, attributes, pseudo-classes
  let mut c: u32 = 0; // elements, pseudo-elements

  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;

  while i < len {
    match chars[i] {
      '#' => {
        a += 1;
        i += 1;
        // Skip the identifier
        while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
      }
      '.' => {
        b += 1;
        i += 1;
        while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
      }
      '[' => {
        b += 1;
        // Skip to closing ]
        while i < len && chars[i] != ']' {
          i += 1;
        }
        if i < len {
          i += 1;
        }
      }
      ':' => {
        i += 1;
        if i < len && chars[i] == ':' {
          // Pseudo-element (::before, ::after, etc.)
          c += 1;
          i += 1;
          while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
            i += 1;
          }
        } else {
          // Pseudo-class (:hover, :first-child, etc.)
          // Special handling per CSS Selectors Level 4:
          // - :where() has zero specificity
          // - :is(), :not(), :has() take the specificity of their
          //   most specific argument
          let start = i;
          while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
            i += 1;
          }
          let pseudo_name: String = chars[start..i].iter().collect();
          if pseudo_name == "where" {
            // :where() has 0 specificity -- skip arguments entirely
            if i < len && chars[i] == '(' {
              let mut depth = 1;
              i += 1;
              while i < len && depth > 0 {
                if chars[i] == '(' {
                  depth += 1;
                } else if chars[i] == ')' {
                  depth -= 1;
                }
                i += 1;
              }
            }
          } else if pseudo_name == "is" || pseudo_name == "not" || pseudo_name == "has" {
            // :is(), :not(), :has() take the specificity of the
            // most specific argument (comma-separated selectors).
            if i < len && chars[i] == '(' {
              let inner = extract_parenthesized_content(&chars, &mut i);
              let mut max_arg = Specificity(0, 0, 0);
              for arg in inner.split(',') {
                let arg_spec = calculate_specificity(arg.trim());
                max_arg = max_arg.max(arg_spec);
              }
              a += max_arg.0;
              b += max_arg.1;
              c += max_arg.2;
            }
          } else {
            b += 1;
            // Skip parenthesized arguments like :nth-child(2n+1)
            if i < len && chars[i] == '(' {
              let mut depth = 1;
              i += 1;
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
        }
      }
      '*' | ' ' | '>' | '+' | '~' | ',' => {
        i += 1;
      }
      ch if ch.is_alphanumeric() || ch == '-' || ch == '_' => {
        // Type selector
        c += 1;
        while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
      }
      _ => {
        i += 1;
      }
    }
  }

  Specificity(a, b, c)
}

/// Returns `true` if the selector contains SCSS/Less constructs that make
/// specificity analysis unreliable:
/// - `#{` — SCSS interpolation (dynamic specificity)
/// - `%`  — placeholder selectors (extended, not used directly)
/// - Leading `&` — parent-referencing selectors (compose with parent)
fn has_preprocessor_constructs(selector: &str) -> bool {
  let trimmed = selector.trim();
  trimmed.contains("#{") || trimmed.starts_with('%') || trimmed.starts_with('&')
}

/// Returns `true` if `s` still contains unreliable SCSS constructs AFTER
/// `&` substitution has been applied.  The ampersand itself is already
/// resolved at this point, so we only check for interpolation and
/// placeholder selectors.
fn has_remaining_preprocessor_constructs(s: &str) -> bool {
  s.contains("#{") || s.trim_start().starts_with('%')
}

/// Split a CSS selector list by commas, respecting parentheses depth so that
/// commas inside `:is()`, `:not()`, etc. are not treated as list separators.
///
/// Parts are **NOT trimmed** — callers that need clean text for display or
/// key computation must call `.trim()` themselves.  Preserving leading
/// whitespace (e.g. `"\n    > .rs-input"` from a multi-line source selector)
/// is important so that `expand_scss_selector` can produce the same internal
/// newlines that `postcss-resolve-nested-selector` produces, matching
/// Stylelint's stored selector strings.
fn split_selector_list(selector: &str) -> Vec<&str> {
  let mut parts = Vec::new();
  let mut depth: usize = 0;
  let mut start = 0;
  let bytes = selector.as_bytes();
  for (i, &b) in bytes.iter().enumerate() {
    match b {
      b'(' => depth += 1,
      b')' => depth = depth.saturating_sub(1),
      b',' if depth == 0 => {
        parts.push(&selector[start..i]); // NOT trimmed
        start = i + 1;
      }
      _ => {}
    }
  }
  let last = &selector[start..]; // NOT trimmed
  if !last.trim().is_empty() {
    parts.push(last);
  }
  parts
}

/// Expand a SCSS child selector for CHILD RECURSION (parent_selectors for nested
/// rules).  Trims all child parts to match PostCSS `list.comma` behaviour
/// (`split(string, [','], true)` trims all elements).  This ensures that when
/// the expanded selector is used as a parent in `postcss-resolve-nested-selector`,
/// it matches what `parent.selectors` (which uses `list.comma`) produces.
fn expand_scss_selector(parents: &[String], child: &str) -> Vec<String> {
  let child_parts = split_selector_list(child);
  let mut results = Vec::new();

  for child_part in &child_parts {
    // PostCSS `list.comma` trims ALL elements — do the same here.
    let child_trimmed = child_part.trim();
    if child_trimmed.is_empty() {
      continue;
    }
    if child_trimmed.contains('&') {
      for parent in parents {
        let expanded = child_trimmed.replace('&', parent.trim());
        results.push(expanded);
      }
    } else {
      for parent in parents {
        results.push(format!("{} {}", parent.trim(), child_trimmed));
      }
    }
  }
  results
}

/// Expand a SCSS child selector for COMPARISON CONTEXT (what gets stored as the
/// prior selector in the specificity comparison map).
///
/// Differs from `expand_scss_selector` in the non-`&` branch: child_part is
/// used **without trimming** so that leading whitespace like `"\n  "` or
/// `"\n    "` on the last comma element is preserved.
///
/// This matches how Stylelint's `flattenNestedSelectorsForRule` works:
/// - It parses the rule selector with `parseSelector` (postcss-selector-parser)
/// - For each node, calls `selectorAST.toString()` which **preserves** the
///   node's `spaces.before` (e.g. `"\n  "` from the second item in a multiline
///   comma list like `.a,\n  .b`).
/// - Then calls `resolveNestedSelector(selectorASTString, rule)` which does
///   `[parentSelector, selectorASTString].join(' ')` for non-`&` selectors,
///   keeping the `"\n  "` internal in the joined string.
/// - The final stored selector is `resolvedSelectorNode.toString().trim()` which
///   removes only leading/trailing whitespace (internal `"\n  "` is preserved).
///
/// For the `&` branch, leading whitespace IS stripped (trim_start) because
/// `postcss-resolve-nested-selector`'s `split(selector, '&', true).join(parent)`
/// keeps the whitespace BEFORE the `&`, and then `resolvedSelectorNode.toString()
/// .trim()` strips the leading `"\n  "` from the result.
fn expand_scss_selector_for_check(parents: &[String], child: &str) -> Vec<String> {
  let child_parts = split_selector_list(child);
  let mut results = Vec::new();

  for child_part in &child_parts {
    if child_part.trim().is_empty() {
      continue;
    }
    if child_part.trim_start().contains('&') {
      // `&` branch: strip leading whitespace so `"\n  &-foo"` → `"&-foo"`
      // before replacing `&` with parent.  The final result has no leading
      // whitespace (Stylelint trims via `.toString().trim()`).
      let child_no_leading = child_part.trim_start();
      for parent in parents {
        let expanded = child_no_leading.replace('&', parent.trim());
        results.push(expanded);
      }
    } else {
      // Non-`&` branch: preserve child_part INCLUDING leading whitespace
      // (e.g. `"\n    > .rs-input"` or `"\n  .rs-dropdown-item..."`).
      // Stylelint joins as `[parent, selectorASTString].join(' ')` which
      // keeps the leading `"\n  "` internal in the result.
      for parent in parents {
        results.push(format!("{} {}", parent.trim(), child_part));
      }
    }
  }
  results
}

/// Recursively walk `CssNode` items for SCSS/Less, expanding `&` in nested
/// selectors and checking specificity order of the fully-resolved selectors.
///
/// `parent_selectors` holds the expanded CSS selectors of the enclosing rule
/// block.  At the top level this is empty.  `AtRule` children are walked with
/// a fresh comparison context (so `@media` blocks scope comparisons).
///
/// `source` is the full file source text, used to locate individual selector
/// parts within multi-selector rules (for accurate span reporting).
fn walk_scss_nodes(
  nodes: &[CssNode],
  parent_selectors: &[String],
  comparison_ctx: &mut HashMap<String, Vec<(Specificity, String)>>,
  diagnostics: &mut Vec<Diagnostic>,
  rule_impl: &NoDescendingSpecificity,
  source: &str,
) {
  for node in nodes {
    match node {
      CssNode::Style(style) => {
        let selector = &style.selector;
        let orig_parts = split_selector_list(selector);

        // For child recursion we use UNFILTERED expansions so that when
        // the parent selector contains `#{...}` interpolation, children
        // also expand to non-standard selectors (which are then filtered
        // out at check time).  This mirrors Stylelint's behaviour where
        // `flattenNestedSelectorsForRule` returns `[]` for any rule whose
        // resolved selector is non-standard.
        let mut all_expanded_for_children: Vec<String> = Vec::new();

        for orig_part in &orig_parts {
          // orig_part may have leading whitespace (not trimmed by split_selector_list);
          // use .trim() for display, checks, and key computations.
          if orig_part.trim().is_empty() {
            continue;
          }

          // Expansions used for CHECKING (stored in comparison_ctx as prior selectors).
          // Uses `expand_scss_selector_for_check` which preserves leading whitespace
          // on non-`&` child parts, matching Stylelint's `selectorAST.toString()`
          // behaviour (postcss-selector-parser preserves `spaces.before` on each node).
          // Skip all expansions if ANY is non-standard (Stylelint's all-or-nothing rule).
          let expanded_for_check: Vec<String> = if parent_selectors.is_empty() {
            if has_preprocessor_constructs(orig_part) {
              vec![]
            } else {
              vec![orig_part.trim().to_string()]
            }
          } else {
            let expanded = expand_scss_selector_for_check(parent_selectors, orig_part);
            if expanded
              .iter()
              .any(|s| has_remaining_preprocessor_constructs(s))
            {
              // Any non-standard expansion → skip the whole rule (Stylelint behaviour)
              vec![]
            } else {
              expanded
            }
          };

          // Expansions used for CHILD recursion: no filter, so children
          // also see the interpolated parent and get filtered themselves.
          // We use the trimmed form here since children only need the
          // selector text for further expansion (their own whitespace will
          // be appended at the next level).
          let expanded_for_children: Vec<String> = if parent_selectors.is_empty() {
            // Always pass the original selector to children even if it
            // has preprocessor constructs — children need a non-empty
            // parent to inherit the non-standard context.
            vec![orig_part.trim().to_string()]
          } else {
            expand_scss_selector(parent_selectors, orig_part)
            // intentionally NOT filtered
          };
          all_expanded_for_children.extend(expanded_for_children);

          // Only check selectors for rules that have actual CSS declarations.
          // Rules with only @include/@mixin calls (no direct properties) are
          // skipped for specificity comparison — matching Stylelint's behaviour.
          if !style.declarations.is_empty() {
            for exp in &expanded_for_check {
              // Locate the individual selector part in the source for an
              // accurate span (Stylelint reports the position of the
              // specific selector, not the beginning of the rule).
              let part_trimmed = orig_part.trim();
              let leading_len = orig_part.len() - orig_part.trim_start().len();
              let part_offset = if leading_len > 0 {
                // Use the leading whitespace as a disambiguation prefix so
                // that e.g. "\n  &-header + &-body" finds the correct
                // occurrence rather than the earlier "&-header + &-body-collapse"
                // substring match.
                source
                  .get(style.span.offset..)
                  .and_then(|s| {
                    s.find(orig_part)
                      .map(|o| style.span.offset + o + leading_len)
                      .or_else(|| s.find(part_trimmed).map(|o| style.span.offset + o))
                  })
                  .unwrap_or(style.span.offset)
              } else {
                source
                  .get(style.span.offset..)
                  .and_then(|s| s.find(part_trimmed).map(|o| style.span.offset + o))
                  .unwrap_or(style.span.offset)
              };
              let span = gale_diagnostics::Span::new(part_offset, part_trimmed.len());

              check_one_selector_with_src(
                exp,
                part_trimmed,
                span,
                comparison_ctx,
                diagnostics,
                rule_impl,
              );
            }
          }
        }

        // Recurse into children with the UNFILTERED expansions so that
        // children inherit non-standard (interpolated) parent context.
        let children_nodes: Vec<CssNode> = style
          .children
          .iter()
          .map(|c| CssNode::Style(c.clone()))
          .collect();
        walk_scss_nodes(
          &children_nodes,
          &all_expanded_for_children,
          comparison_ctx,
          diagnostics,
          rule_impl,
          source,
        );
        walk_scss_nodes(
          &style.nested_at_rules,
          &all_expanded_for_children,
          comparison_ctx,
          diagnostics,
          rule_impl,
          source,
        );
      }
      CssNode::AtRule(at_rule) => {
        // Each at-rule establishes a new comparison context — matching
        // Stylelint's `selectorContextLookup` which scopes comparisons
        // to the nearest ancestor at-rule.
        let mut at_ctx: HashMap<String, Vec<(Specificity, String)>> = HashMap::new();
        walk_scss_nodes(
          &at_rule.children,
          parent_selectors,
          &mut at_ctx,
          diagnostics,
          rule_impl,
          source,
        );
      }
      _ => {}
    }
  }
}

/// Extract the last compound selector from a complex selector, stripping
/// pseudo-classes.  This is used to group selectors for comparison — only
/// selectors that share the same "last compound" can be meaningfully compared
/// for specificity ordering (matching Stylelint behaviour).
///
/// For example:
/// - `.foo .bar:hover` → `.bar`
/// - `#id > .cls` → `.cls`
/// - `a` → `a`
/// - `a, b` → last individual selector's last compound
fn last_compound_selector_without_pseudo_classes(selector: &str) -> String {
  // For comma-separated selector lists, take the last individual selector.
  let individual = selector.rsplit(',').next().unwrap_or(selector).trim();

  // Split by combinators (` `, `>`, `+`, `~`) to get the last compound.
  // We walk backwards to find the last combinator.
  let chars: Vec<char> = individual.chars().collect();
  let mut last_compound_start = 0;
  let mut i = chars.len();
  let mut in_parens = 0i32;
  while i > 0 {
    i -= 1;
    match chars[i] {
      ')' => in_parens += 1,
      '(' => in_parens -= 1,
      ' ' | '>' | '+' | '~' if in_parens == 0 => {
        // Found a combinator; the last compound starts after it
        // (skip any whitespace/combinators).
        let mut j = i + 1;
        while j < chars.len() && matches!(chars[j], ' ' | '>' | '+' | '~') {
          j += 1;
        }
        last_compound_start = j;
        break;
      }
      _ => {}
    }
  }

  let compound: String = chars[last_compound_start..].iter().collect();

  // Strip pseudo-classes (`:name` but not `::pseudo-element`)
  let mut result = String::new();
  let compound_chars: Vec<char> = compound.chars().collect();
  let clen = compound_chars.len();
  let mut ci = 0;
  while ci < clen {
    if compound_chars[ci] == ':' {
      // Check for double-colon `::pseudo-element` — keep it as-is.
      if ci + 1 < clen && compound_chars[ci + 1] == ':' {
        // Pseudo-element: push `::` and the element name.
        result.push(':');
        result.push(':');
        ci += 2;
        while ci < clen
          && (compound_chars[ci].is_alphanumeric()
            || compound_chars[ci] == '-'
            || compound_chars[ci] == '_')
        {
          result.push(compound_chars[ci]);
          ci += 1;
        }
      } else {
        // Single-colon — this is a pseudo-class; skip it.
        ci += 1; // skip ':'
        // Skip the pseudo-class name
        if ci < clen && compound_chars[ci] == '(' {
          // functional pseudo-class like :not(...)
          let mut depth = 1;
          ci += 1;
          while ci < clen && depth > 0 {
            if compound_chars[ci] == '(' {
              depth += 1;
            } else if compound_chars[ci] == ')' {
              depth -= 1;
            }
            ci += 1;
          }
        } else {
          while ci < clen
            && (compound_chars[ci].is_alphanumeric()
              || compound_chars[ci] == '-'
              || compound_chars[ci] == '_')
          {
            ci += 1;
          }
          // Handle functional pseudo-class: :nth-child(...)
          if ci < clen && compound_chars[ci] == '(' {
            let mut depth = 1;
            ci += 1;
            while ci < clen && depth > 0 {
              if compound_chars[ci] == '(' {
                depth += 1;
              } else if compound_chars[ci] == ')' {
                depth -= 1;
              }
              ci += 1;
            }
          }
        }
      }
    } else {
      result.push(compound_chars[ci]);
      ci += 1;
    }
  }

  result.to_lowercase()
}

/// Walk nodes and check specificity, grouping selectors by their last
/// compound selector (without pseudo-classes) so that only selectors
/// targeting the same element are compared.  This matches Stylelint's
/// comparison strategy.
///
/// When `top_level_only` is true, only top-level style rules are compared
/// (no recursion into nested children).  This is used for SCSS/Sass/Less
/// where nested selectors compose with their parent so their written
/// specificity is incomplete.
fn check_specificity_walk(
  nodes: &[CssNode],
  comparison_ctx: &mut HashMap<String, Vec<(Specificity, String)>>,
  diagnostics: &mut Vec<Diagnostic>,
  rule: &NoDescendingSpecificity,
  top_level_only: bool,
  skip_preprocessor: bool,
) {
  for node in nodes {
    match node {
      CssNode::Style(style) => {
        if !skip_preprocessor || !has_preprocessor_constructs(&style.selector) {
          check_one_selector(
            &style.selector,
            gale_diagnostics::Span::new(style.span.offset, style.span.length),
            comparison_ctx,
            diagnostics,
            rule,
          );
        }

        // Recurse into nested children only for plain CSS.
        if !top_level_only {
          for child in &style.children {
            check_one_selector(
              &child.selector,
              gale_diagnostics::Span::new(child.span.offset, child.span.length),
              comparison_ctx,
              diagnostics,
              rule,
            );
          }
        }
      }
      CssNode::AtRule(at_rule) => {
        // Each at-rule establishes a new comparison context — matching
        // Stylelint's `selectorContextLookup` which scopes comparisons
        // to the nearest ancestor at-rule.
        let mut at_ctx: HashMap<String, Vec<(Specificity, String)>> = HashMap::new();
        check_specificity_walk(
          &at_rule.children,
          &mut at_ctx,
          diagnostics,
          rule,
          top_level_only,
          skip_preprocessor,
        );
      }
      _ => {}
    }
  }
}

/// Returns `true` if the selector contains a vendor-prefixed pseudo-class
/// (e.g. `:-moz-ui-invalid`, `:-webkit-autofill`). Selectors with vendor-
/// prefixed pseudo-classes have non-standard specificity behaviour and
/// Stylelint skips them from descending specificity comparison.
fn has_vendor_prefixed_pseudo_class(selector: &str) -> bool {
  // Look for `:-` followed by a vendor prefix pattern.
  let bytes = selector.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  while i + 1 < len {
    if bytes[i] == b':' && bytes[i + 1] == b'-' {
      // Make sure this isn't `::` (pseudo-element).
      if i > 0 && bytes[i - 1] == b':' {
        i += 2;
        continue;
      }
      return true;
    }
    i += 1;
  }
  false
}

/// Check a single (already expanded) selector string for descending specificity.
/// `display_selector` is what appears in the diagnostic message — for plain CSS
/// it equals `selector`; for SCSS it is the original unexpanded selector string.
///
/// Mirrors Stylelint's algorithm exactly:
/// - Stores ALL previously seen selectors per key (not just the high-water mark).
/// - Reports a violation against the FIRST prior entry with strictly higher
///   specificity, then stops checking further priors (`break`).
/// - Always appends the new entry to the list (regardless of violation).
fn check_one_selector_with_src(
  selector: &str,         // expanded CSS selector (for specificity + key computation)
  display_selector: &str, // original text shown in the message
  span: Span,
  comparison_ctx: &mut HashMap<String, Vec<(Specificity, String)>>,
  diagnostics: &mut Vec<Diagnostic>,
  rule: &NoDescendingSpecificity,
) {
  for individual in split_selector_list(selector) {
    if individual.is_empty() {
      continue;
    }
    if has_vendor_prefixed_pseudo_class(individual) {
      continue;
    }

    let spec = calculate_specificity(individual);
    let key = last_compound_selector_without_pseudo_classes(individual);
    if key.is_empty() {
      continue;
    }

    if let Some(priors) = comparison_ctx.get(&key) {
      for (prior_spec, prior_selector) in priors {
        if spec < *prior_spec {
          let msg = format!(
            "Expected selector \"{}\" to come before selector \"{}\"",
            display_selector.trim(),
            prior_selector
          );
          diagnostics.push(
            Diagnostic::new(rule.name(), msg)
              .severity(rule.default_severity())
              .span(span),
          );
          break; // report against FIRST prior with higher spec, then stop
        }
      }
    }

    // Always push — Stylelint always appends the new entry.
    comparison_ctx
      .entry(key)
      .or_default()
      .push((spec, individual.to_string()));
  }
}

fn check_one_selector(
  selector: &str,
  span: Span,
  comparison_ctx: &mut HashMap<String, Vec<(Specificity, String)>>,
  diagnostics: &mut Vec<Diagnostic>,
  rule: &NoDescendingSpecificity,
) {
  check_one_selector_with_src(selector, selector, span, comparison_ctx, diagnostics, rule);
}

impl Rule for NoDescendingSpecificity {
  fn name(&self) -> &'static str {
    "no-descending-specificity"
  }

  fn description(&self) -> &'static str {
    "Disallow selectors of lower specificity from coming after overriding selectors of higher specificity"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut comparison_ctx: HashMap<String, Vec<(Specificity, String)>> = HashMap::new();

    let is_preprocessor = matches!(context.syntax, Syntax::Scss | Syntax::Sass | Syntax::Less);

    if is_preprocessor {
      // For SCSS/Sass/Less: expand `&` references by walking the nested AST
      // with parent-selector context, then check the fully-expanded selectors.
      // At-rule children (e.g. @media) are walked without extra parent context.
      walk_scss_nodes(
        nodes,
        &[],
        &mut comparison_ctx,
        &mut diagnostics,
        self,
        context.source,
      );
    } else {
      // For plain CSS: full behavior — compare all selectors at all levels.
      check_specificity_walk(
        nodes,
        &mut comparison_ctx,
        &mut diagnostics,
        self,
        false, // top_level_only
        false, // skip_preprocessor
      );
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn make_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  #[test]
  fn reports_descending_specificity() {
    let rule = NoDescendingSpecificity;
    // `.foo .bar` then `.bar` => same last compound (`.bar`), descending
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: ".foo .bar".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(6, 10),
          important: false,
        }],
        span: ParserSpan::new(0, 22),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".bar".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(26, 11),
          important: false,
        }],
        span: ParserSpan::new(23, 16),
        ..Default::default()
      }),
    ];
    let diags = rule.check_root(&nodes, &make_context());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("to come before selector"));
  }

  #[test]
  fn no_report_for_different_last_compound() {
    let rule = NoDescendingSpecificity;
    // `#id` then `a` => different last compound selectors, not comparable
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "#id".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 18),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![],
        span: ParserSpan::new(19, 16),
        ..Default::default()
      }),
    ];
    let diags = rule.check_root(&nodes, &make_context());
    assert!(
      diags.is_empty(),
      "different last compound selectors should not be compared"
    );
  }

  #[test]
  fn ignores_ascending_specificity() {
    let rule = NoDescendingSpecificity;
    // a { } then .class { } then #id { }
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "a".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 5),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".cls".to_string(),
        declarations: vec![],
        span: ParserSpan::new(6, 8),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: "#id".to_string(),
        declarations: vec![],
        span: ParserSpan::new(15, 7),
        ..Default::default()
      }),
    ];
    let diags = rule.check_root(&nodes, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn test_specificity_calculation() {
    assert_eq!(calculate_specificity("a"), Specificity(0, 0, 1));
    assert_eq!(calculate_specificity(".class"), Specificity(0, 1, 0));
    assert_eq!(calculate_specificity("#id"), Specificity(1, 0, 0));
    assert_eq!(calculate_specificity("a.class"), Specificity(0, 1, 1));
    assert_eq!(calculate_specificity("#id .class a"), Specificity(1, 1, 1));
  }

  #[test]
  fn test_where_has_zero_specificity() {
    // :where() contributes nothing
    assert_eq!(calculate_specificity(":where(.a)"), Specificity(0, 0, 0));
    assert_eq!(calculate_specificity(":where(#id)"), Specificity(0, 0, 0));
    assert_eq!(calculate_specificity("a:where(.b)"), Specificity(0, 0, 1));
  }

  #[test]
  fn test_is_takes_max_argument_specificity() {
    // :is(.a) has specificity of .a => (0,1,0)
    assert_eq!(calculate_specificity(":is(.a)"), Specificity(0, 1, 0));
    // :is(#id, .a) takes the max => (1,0,0)
    assert_eq!(calculate_specificity(":is(#id, .a)"), Specificity(1, 0, 0));
    // a:is(.b) => type(a) + class(.b) = (0,1,1)
    assert_eq!(calculate_specificity("a:is(.b)"), Specificity(0, 1, 1));
  }

  #[test]
  fn test_not_takes_max_argument_specificity() {
    assert_eq!(calculate_specificity(":not(.a)"), Specificity(0, 1, 0));
    assert_eq!(calculate_specificity(":not(#id)"), Specificity(1, 0, 0));
  }

  #[test]
  fn test_has_takes_max_argument_specificity() {
    assert_eq!(calculate_specificity(":has(.a)"), Specificity(0, 1, 0));
    assert_eq!(calculate_specificity(":has(> .a)"), Specificity(0, 1, 0));
  }

  fn make_scss_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }

  #[test]
  fn scss_reports_top_level_descending_specificity() {
    let rule = NoDescendingSpecificity;
    // Top-level selectors in SCSS with same last compound — should be checked
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: ".foo .bar".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(6, 10),
          important: false,
        }],
        span: ParserSpan::new(0, 22),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".bar".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(26, 11),
          important: false,
        }],
        span: ParserSpan::new(23, 16),
        ..Default::default()
      }),
    ];
    let diags = rule.check_root(&nodes, &make_scss_context());
    assert_eq!(
      diags.len(),
      1,
      "should report descending specificity for top-level SCSS selectors"
    );
  }

  #[test]
  fn scss_skips_nested_children() {
    let rule = NoDescendingSpecificity;
    // A top-level `.foo .bar` with a nested `.bar` child — the nested `.bar`
    // should NOT be compared in SCSS because nested selectors compose with parent.
    let nodes = vec![CssNode::Style(StyleRule {
      selector: ".foo .bar".to_string(),
      declarations: vec![],
      children: vec![StyleRule {
        selector: ".bar".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(12, 11),
          important: false,
        }],
        span: ParserSpan::new(8, 17),
        ..Default::default()
      }],
      span: ParserSpan::new(0, 27),

      nested_at_rules: Vec::new(),
    })];
    let diags = rule.check_root(&nodes, &make_scss_context());
    assert!(
      diags.is_empty(),
      "should not compare nested SCSS children against parent"
    );

    // Same structure in plain CSS SHOULD compare nested children
    // (both share last compound `.bar`)
    let diags = rule.check_root(&nodes, &make_context());
    assert_eq!(diags.len(), 1, "plain CSS should compare nested children");
  }

  #[test]
  fn scss_skips_interpolation_selectors() {
    let rule = NoDescendingSpecificity;
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "#id".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 10),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".#{$var}".to_string(),
        declarations: vec![],
        span: ParserSpan::new(11, 15),
        ..Default::default()
      }),
    ];
    let diags = rule.check_root(&nodes, &make_scss_context());
    assert!(
      diags.is_empty(),
      "should skip selectors with SCSS interpolation"
    );
  }

  #[test]
  fn scss_skips_ampersand_selectors() {
    let rule = NoDescendingSpecificity;
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "#id".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 10),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: "&:hover".to_string(),
        declarations: vec![],
        span: ParserSpan::new(11, 15),
        ..Default::default()
      }),
    ];
    let diags = rule.check_root(&nodes, &make_scss_context());
    assert!(diags.is_empty(), "should skip &-prefixed selectors in SCSS");
  }

  #[test]
  fn scss_skips_placeholder_selectors() {
    let rule = NoDescendingSpecificity;
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: "#id".to_string(),
        declarations: vec![],
        span: ParserSpan::new(0, 10),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: "%placeholder".to_string(),
        declarations: vec![],
        span: ParserSpan::new(11, 20),
        ..Default::default()
      }),
    ];
    let diags = rule.check_root(&nodes, &make_scss_context());
    assert!(
      diags.is_empty(),
      "should skip placeholder selectors in SCSS"
    );
  }

  #[test]
  fn less_uses_same_selective_approach() {
    let rule = NoDescendingSpecificity;
    // Same last compound selector `.bar` — should report in Less
    let source = ".foo .bar { color: red; }\n.bar { color: blue; }";
    let nodes = vec![
      CssNode::Style(StyleRule {
        selector: ".foo .bar".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(12, 10),
          important: false,
        }],
        span: ParserSpan::new(0, 25),
        ..Default::default()
      }),
      CssNode::Style(StyleRule {
        selector: ".bar".to_string(),
        declarations: vec![Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(33, 11),
          important: false,
        }],
        span: ParserSpan::new(26, 21),
        ..Default::default()
      }),
    ];
    let less_context = RuleContext {
      file_path: "test.less",
      source,
      syntax: Syntax::Less,
      options: None,
    };
    let diags = rule.check_root(&nodes, &less_context);
    assert_eq!(
      diags.len(),
      1,
      "should report top-level descending specificity in Less"
    );
  }
}
