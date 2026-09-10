use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow a space after the commas of functions.
///
/// Primary: "always" | "never" | "always-single-line"
pub struct StylisticFunctionCommaSpaceAfter;

impl Rule for StylisticFunctionCommaSpaceAfter {
  fn name(&self) -> &'static str {
    "@stylistic/function-comma-space-after"
  }

  fn description(&self) -> &'static str {
    "Require or disallow a space after the commas of functions"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = ctx.primary_option_str().unwrap_or("always");
    let mut diagnostics = Vec::new();
    let bytes = ctx.source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
      // Skip comments
      if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
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

      // Skip strings
      if bytes[i] == b'\'' || bytes[i] == b'"' {
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

      // Detect function call: identifier followed by `(`
      // Skip pseudo-class/pseudo-element functions and SCSS at-rule parens
      if bytes[i] == b'('
        && i > 0
        && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-' || bytes[i - 1] == b'_')
      {
        // Check if this is a pseudo-class/element function like :not(), :is(), :where(), :has()
        // by looking for a colon before the function name
        let mut is_pseudo_fn = false;
        // Check if this is url() — commas inside url() are not function argument separators
        let mut is_url_fn = false;
        {
          let mut p = i - 1;
          let fn_end = p;
          while p > 0 && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
          {
            p -= 1;
          }
          let fn_start = if bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_'
          {
            p
          } else {
            p + 1
          };
          let fn_name = &ctx.source[fn_start..=fn_end];
          if fn_name.eq_ignore_ascii_case("url") {
            is_url_fn = true;
          }
          if bytes[p] == b':' {
            is_pseudo_fn = true;
          }
        }

        // Check if this is an SCSS at-rule like @include mixin(), @if(), @each, @for, @while
        // The pattern is: @keyword [optional-name](...) so we may need to walk back
        // over the function name, then whitespace, then the at-rule keyword, then @.
        let mut is_at_rule_paren = false;
        {
          let mut p = i - 1;
          // Walk back over the function/mixin name
          while p > 0 && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
          {
            p -= 1;
          }
          // Check if directly preceded by @
          if bytes[p] == b'@' {
            is_at_rule_paren = true;
          } else {
            // Skip whitespace (for `@include mixin(...)` pattern)
            while p > 0 && (bytes[p] == b' ' || bytes[p] == b'\t') {
              p -= 1;
            }
            // Walk back over the at-rule keyword (e.g., "include")
            while p > 0
              && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'-' || bytes[p] == b'_')
            {
              p -= 1;
            }
            if bytes[p] == b'@' {
              is_at_rule_paren = true;
            }
          }
        }

        let paren_start = i;
        // Find matching closing paren
        let mut depth = 1;
        let mut j = i + 1;
        while j < len && depth > 0 {
          if bytes[j] == b'(' {
            depth += 1;
          } else if bytes[j] == b')' {
            depth -= 1;
          } else if bytes[j] == b'\'' || bytes[j] == b'"' {
            let q = bytes[j];
            j += 1;
            while j < len && bytes[j] != q {
              if bytes[j] == b'\\' {
                j += 1;
              }
              j += 1;
            }
          } else if bytes[j] == b'#' && j + 1 < len && bytes[j + 1] == b'{' {
            // Skip SCSS interpolation inside function args
            j += 2;
            let mut id = 1;
            while j < len && id > 0 {
              if bytes[j] == b'{' {
                id += 1;
              } else if bytes[j] == b'}' {
                id -= 1;
              }
              if id > 0 {
                j += 1;
              }
            }
          } else if bytes[j] == b'/' && j + 1 < len && bytes[j + 1] == b'/' {
            // Skip SCSS line comments inside function args
            while j < len && bytes[j] != b'\n' {
              j += 1;
            }
          }
          if depth > 0 {
            j += 1;
          }
        }
        let paren_end = j;

        // Skip pseudo-class functions, SCSS at-rule parens, and url() functions
        if is_pseudo_fn || is_at_rule_paren || is_url_fn {
          i = paren_end + 1;
          continue;
        }

        let func_content = &ctx.source[paren_start..paren_end.min(len)];
        let is_single_line = !func_content.contains('\n');

        // Scan for commas inside this function call (depth 0 only —
        // nested function calls will be picked up by the outer loop
        // when we advance `i` to just past the opening paren).
        let mut k = paren_start + 1;
        let mut inner_depth = 0;
        while k < paren_end {
          // Skip SCSS interpolation inside function args
          if bytes[k] == b'#' && k + 1 < paren_end && bytes[k + 1] == b'{' {
            k += 2;
            let mut id = 1;
            while k < paren_end && id > 0 {
              if bytes[k] == b'{' {
                id += 1;
              } else if bytes[k] == b'}' {
                id -= 1;
              }
              if id > 0 {
                k += 1;
              }
            }
            if k < paren_end {
              k += 1;
            }
            continue;
          }
          // Skip SCSS line comments inside function args
          if bytes[k] == b'/' && k + 1 < paren_end && bytes[k + 1] == b'/' {
            while k < paren_end && bytes[k] != b'\n' {
              k += 1;
            }
            continue;
          }
          // Skip block comments inside function args
          if bytes[k] == b'/' && k + 1 < paren_end && bytes[k + 1] == b'*' {
            k += 2;
            while k + 1 < paren_end && !(bytes[k] == b'*' && bytes[k + 1] == b'/') {
              k += 1;
            }
            if k + 1 < paren_end {
              k += 2;
            }
            continue;
          }
          if bytes[k] == b'(' {
            inner_depth += 1;
          } else if bytes[k] == b')' {
            if inner_depth > 0 {
              inner_depth -= 1;
            }
          } else if bytes[k] == b'\'' || bytes[k] == b'"' {
            let q = bytes[k];
            k += 1;
            while k < paren_end && bytes[k] != q {
              if bytes[k] == b'\\' {
                k += 1;
              }
              k += 1;
            }
          } else if bytes[k] == b',' && inner_depth == 0 {
            let comma_pos = k;
            let after = comma_pos + 1;
            let has_space = after < len && bytes[after] == b' ';

            let violation = match option {
              "always" => !has_space,
              "never" => has_space,
              "always-single-line" => is_single_line && !has_space,
              _ => false,
            };

            if violation {
              let msg = match option {
                "always" => "Expected single space after \",\"",
                "always-single-line" => {
                  "Expected single space after \",\" in a single-line function"
                }
                "never" => "Unexpected space after \",\"",
                _ => "Expected single space after \",\"",
              };
              diagnostics.push(
                Diagnostic::new(self.name(), msg)
                  .severity(self.default_severity())
                  .span(Span::new(comma_pos, 1)),
              );
            }
          }
          k += 1;
        }
        // Advance past the opening paren only — do NOT skip to paren_end.
        // This lets nested function calls like `var(a, var(b,c))` be
        // individually detected and checked by the outer loop.
        i = paren_start + 1;
        continue;
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

  fn check(source: &str, option: &str) -> Vec<Diagnostic> {
    let rule = StylisticFunctionCommaSpaceAfter;
    let opts = serde_json::json!(option);
    let ctx = RuleContext {
      file_path: "test.css",
      source,
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    rule.check_root(&[], &ctx)
  }

  #[test]
  fn always_accepts_space_after_comma() {
    let d = check("a { transform: translate(1px, 2px); }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn always_rejects_no_space_after_comma() {
    let d = check("a { transform: translate(1px,2px); }", "always");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn never_accepts_no_space() {
    let d = check("a { transform: translate(1px,2px); }", "never");
    assert!(d.is_empty());
  }

  #[test]
  fn never_rejects_space() {
    let d = check("a { transform: translate(1px, 2px); }", "never");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn ignores_scss_interpolation_inside_function() {
    let d = check("a { background: url(#{$var, $other}); }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_pseudo_class_functions() {
    let d = check("a:not(.foo,.bar) { color: red; }", "always");
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_at_include_parens() {
    let d = check("a { @include mixin($a,$b); }", "always");
    assert!(d.is_empty());
  }
}
