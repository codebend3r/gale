use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after combinators of selectors.
///
/// Equivalent to `@stylistic/selector-combinator-space-after`.
pub struct StylisticSelectorCombinatorSpaceAfter;

const COMBINATORS: &[u8] = b">~+";

impl Rule for StylisticSelectorCombinatorSpaceAfter {
  fn name(&self) -> &'static str {
    "@stylistic/selector-combinator-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after combinators of selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let option = context.primary_option_str().unwrap_or("always");
    let source = context.source;
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut diagnostics = Vec::new();
    let mut i = 0;
    let mut in_selector = true;
    let mut depth = 0_i32;
    let mut paren_depth = 0_i32;

    while i < len {
      // Skip strings
      if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        i += 1;
        while i < len && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      // Skip block comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        continue;
      }

      // Skip SCSS line comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }

      // Skip SCSS interpolation #{...}
      if bytes[i] == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
        i += 2;
        let mut interp_depth = 1;
        while i < len && interp_depth > 0 {
          if bytes[i] == b'{' {
            interp_depth += 1;
          } else if bytes[i] == b'}' {
            interp_depth -= 1;
          }
          if interp_depth > 0 {
            i += 1;
          }
        }
        if i < len {
          i += 1;
        }
        continue;
      }

      if bytes[i] == b'{' {
        in_selector = false;
        depth += 1;
        i += 1;
        continue;
      }
      if bytes[i] == b'}' {
        depth -= 1;
        if depth < 0 {
          depth = 0;
        }
        // After a closing brace inside a parent block, next content
        // is a selector.  At the top level, next content is NOT a selector
        // (it could be an at-rule or the start of a new ruleset).
        in_selector = depth > 0;
        i += 1;
        continue;
      }

      // Semicolons end declarations/at-rules.  After `;` inside a block,
      // the next content could be a selector (for nested rules in SCSS).
      if bytes[i] == b';' {
        in_selector = depth > 0;
        i += 1;
        continue;
      }

      // Track parentheses — combinators inside parens are either
      // :has()/:not() relative selectors or function-call arguments,
      // neither of which Stylelint flags.
      if bytes[i] == b'(' {
        paren_depth += 1;
        i += 1;
        continue;
      }
      if bytes[i] == b')' {
        paren_depth -= 1;
        if paren_depth < 0 {
          paren_depth = 0;
        }
        i += 1;
        continue;
      }

      // Only check combinators in selector context and outside parens.
      // Skip SCSS comparison operators (>=, <=) and `>` inside
      // @if/@else conditions.
      if in_selector && paren_depth == 0 && COMBINATORS.contains(&bytes[i]) {
        let comb_pos = i;
        // `>=` and `<=` are SCSS comparison operators, not combinators
        if bytes[i] == b'>' && comb_pos + 1 < len && bytes[comb_pos + 1] == b'=' {
          i += 2;
          continue;
        }
        let has_space_after = comb_pos + 1 < len
          && (bytes[comb_pos + 1] == b' '
            || bytes[comb_pos + 1] == b'\t'
            || bytes[comb_pos + 1] == b'\n');

        match option {
          "always" => {
            if !has_space_after {
              diagnostics.push(
                Diagnostic::new(
                  self.name(),
                  format!("Expected a space after \"{}\"", char::from(bytes[comb_pos])),
                )
                .severity(self.default_severity())
                .span(Span::new(comb_pos, 1)),
              );
            }
          }
          "never" => {
            if has_space_after {
              diagnostics.push(
                Diagnostic::new(
                  self.name(),
                  format!("Unexpected space after \"{}\"", char::from(bytes[comb_pos])),
                )
                .severity(self.default_severity())
                .span(Span::new(comb_pos, 1)),
              );
            }
          }
          _ => {}
        }
      }

      i += 1;
    }

    diagnostics
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  fn ctx_with_option<'a>(source: &'a str, opt: &'a serde_json::Value) -> RuleContext<'a> {
    RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(opt),
    }
  }

  #[test]
  fn always_allows_space_after_combinator() {
    let opt = serde_json::json!("always");
    let source = "a > b { }";
    let d = StylisticSelectorCombinatorSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(d.is_empty());
  }

  #[test]
  fn always_reports_missing_space_after() {
    let opt = serde_json::json!("always");
    let source = "a >b { }";
    let d = StylisticSelectorCombinatorSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Expected a space after"));
  }

  #[test]
  fn never_reports_space_after() {
    let opt = serde_json::json!("never");
    let source = "a > b { }";
    let d = StylisticSelectorCombinatorSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert!(!d.is_empty());
    assert!(d[0].message.contains("Unexpected space after"));
  }

  #[test]
  fn checks_tilde_combinator() {
    let opt = serde_json::json!("always");
    let source = "a ~b { }";
    let d = StylisticSelectorCombinatorSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn checks_plus_combinator() {
    let opt = serde_json::json!("always");
    let source = "a +b { }";
    let d = StylisticSelectorCombinatorSpaceAfter.check_root(&[], &ctx_with_option(source, &opt));
    assert_eq!(d.len(), 1);
  }
}
