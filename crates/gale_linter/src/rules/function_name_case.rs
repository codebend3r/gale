use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "function-name-case";

declare_rule! {
  /// Enforces consistent case for function names.
  ///
  /// Equivalent to Stylelint's `function-name-case` rule.
  /// Supports `"lower"` (default) and `"upper"` primary options.
  ///
  /// Secondary options:
  /// - `ignoreFunctions`: array of function name strings or regex patterns
  ///   (e.g. `"/^get.*$/"`) to ignore.
  pub struct FunctionNameCase => FunctionNameCaseCheck {
    name: NAME,
    description: "Specify lowercase or uppercase for function names",
    default_severity: Warning,
  }
}

/// `function-name-case` as configured.
pub struct FunctionNameCaseCheck {
  /// `"lower"` or `"upper"`.
  mode: &'static str,
  /// `ignoreFunctions`: names (any case) and `/regex/` entries.
  ignore_functions: Matchers,
}

impl FunctionNameCaseCheck {
  /// Stylelint's options: `"lower"` or `"upper"`, then `ignoreFunctions`.
  /// Gale reads no option as `"lower"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let mode = options.primary_one_of(&["lower", "upper"]);
    let ignore_functions = options.strings("ignoreFunctions");
    options.finish()?;
    Ok(Self {
      mode: mode.unwrap_or("lower"),
      ignore_functions: Matchers::new(&ignore_functions),
    })
  }
}

/// CSS functions whose canonical name contains uppercase letters (camelCase).
/// When enforcing lowercase, these are expected in their canonical form,
/// not lowercased. When enforcing uppercase, they should be fully uppercased.
const CAMEL_CASE_FUNCTIONS: &[&str] = &[
  "translateX",
  "translateY",
  "translateZ",
  "translate3d",
  "scaleX",
  "scaleY",
  "scaleZ",
  "scale3d",
  "rotateX",
  "rotateY",
  "rotateZ",
  "rotate3d",
  "skewX",
  "skewY",
  "perspective",
];

/// Returns the canonical form of a camelCase CSS function if the name matches
/// (case-insensitive), or `None` if it's not a known camelCase function.
fn camel_case_canonical(name: &str) -> Option<&'static str> {
  CAMEL_CASE_FUNCTIONS
    .iter()
    .find(|&&f| f.eq_ignore_ascii_case(name))
    .copied()
}

/// Returns the expected function name given the case mode.
/// For camelCase CSS functions, lowercase mode uses the canonical form,
/// and uppercase mode uses the fully uppercased form.
fn expected_name(name: &str, mode: &str) -> String {
  if let Some(canonical) = camel_case_canonical(name) {
    match mode {
      "upper" => canonical.to_ascii_uppercase(),
      _ => canonical.to_string(),
    }
  } else {
    match mode {
      "upper" => name.to_ascii_uppercase(),
      _ => name.to_ascii_lowercase(),
    }
  }
}

/// A function name split after its first `-...-` piece, as
/// `("-webkit-", "radial-gradient")`, or `("", name)`.  Stylelint cases the
/// whole name; gale cases the two parts apart, so the camelCase names apply
/// after a prefix too.
fn split_vendor_prefix(name: &str) -> (&str, &str) {
  if name.starts_with('-') {
    // Vendor prefixes: -webkit-, -moz-, -ms-, -o-
    // Find the second hyphen which ends the prefix
    if let Some(pos) = name[1..].find('-') {
      let prefix_end = pos + 2; // +1 for the skipped first char, +1 to include the hyphen
      return (&name[..prefix_end], &name[prefix_end..]);
    }
  }
  ("", name)
}

impl Check for FunctionNameCaseCheck {
  /// Flags function names not in the configured case, skipping any matched by
  /// `ignoreFunctions`.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let is_scss = matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass | gale_css_parser::Syntax::Less
    );

    // Primary option: "lower" (default) or "upper"
    let mode = self.mode;
    let ignore_fns = &self.ignore_functions;

    let mut diags = Vec::new();
    // The declarations of style rules, read whole: property through `;`.
    for decl in tree.walk_decls() {
      if !decl.in_style_rule() {
        continue;
      }
      let decl_start = decl.span().offset;
      let raw_value_area = decl.source_text();

      // In SCSS/Less, skip values with interpolation or SCSS variables,
      // since the actual function name may be dynamic.
      if is_scss && (raw_value_area.contains("#{") || raw_value_area.contains('$')) {
        continue;
      }

      // Find the value part: after the first colon
      let value_start_in_area = match raw_value_area.find(':') {
        Some(pos) => pos + 1,
        None => continue,
      };
      let raw_value = &raw_value_area[value_start_in_area..];
      let value_abs_start = decl_start + value_start_in_area;

      // Extract function calls from the raw source value
      for (func_name, rel_offset) in extract_function_names_with_offsets(raw_value) {
        // Strip vendor prefix for the purpose of case checking
        let (vendor_prefix, base_name) = split_vendor_prefix(&func_name);

        // Compute the expected form
        let expected_base = expected_name(base_name, mode);
        let expected_vendor = match mode {
          "upper" => vendor_prefix.to_ascii_uppercase(),
          _ => vendor_prefix.to_ascii_lowercase(),
        };
        let expected_full = format!("{expected_vendor}{expected_base}");

        if func_name == expected_full {
          // Already in the expected case
          continue;
        }

        // Check ignoreFunctions against the original function name
        if ignore_fns.matches(&func_name) {
          continue;
        }

        // The absolute byte offset for the function name in the source
        let abs_offset = value_abs_start + rel_offset;

        let fix = Fix::new(
          format!("Change function name to \"{expected_full}\""),
          vec![Edit::new(
            Span::new(abs_offset, func_name.len()),
            &expected_full,
          )],
        );

        let diag = Diagnostic::new(
          NAME,
          format!("Expected \"{func_name}\" to be \"{expected_full}\""),
        )
        .span(Span::new(abs_offset, func_name.len()))
        .fix(fix);

        diags.push(diag);
      }
    }
    diags
  }
}

/// Extract function names from a CSS value string, along with their byte
/// offsets within the string.
fn extract_function_names_with_offsets(value: &str) -> Vec<(String, usize)> {
  let mut results = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Look for the start of an identifier (letter, hyphen for vendor prefix, or underscore)
    if bytes[i].is_ascii_alphabetic() || bytes[i] == b'-' || bytes[i] == b'_' {
      let start = i;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }
      if i < len && bytes[i] == b'(' {
        let name = &value[start..i];
        results.push((name.to_string(), start));
      }
    } else {
      i += 1;
    }
  }

  results
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::testing::{CheckSource, configured, invalid_options};
  use gale_css_parser::Syntax;

  fn make_ctx(source: &str) -> RuleContext<'_> {
    RuleContext {
      env: Default::default(),
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      cache: None,
    }
  }

  #[test]
  fn reports_uppercase_function() {
    let source = "a { color: RGB(0, 0, 0); }";
    let ctx = make_ctx(source);
    let d = configured(&FunctionNameCase, None).check_source(&ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("rgb"));
  }

  #[test]
  fn allows_lowercase_function() {
    let source = "a { color: rgb(0, 0, 0); }";
    let ctx = make_ctx(source);
    assert!(
      configured(&FunctionNameCase, None)
        .check_source(&ctx)
        .is_empty()
    );
  }

  #[test]
  fn reports_mixed_case_calc() {
    let source = "a { margin: Calc(100% - 20px); }";
    let ctx = make_ctx(source);
    let d = configured(&FunctionNameCase, None).check_source(&ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("calc"));
  }

  #[test]
  fn allows_camel_case_function_in_canonical_form() {
    let source = "a { transform: translateX(0); }";
    let ctx = make_ctx(source);
    assert!(
      configured(&FunctionNameCase, None)
        .check_source(&ctx)
        .is_empty()
    );
  }

  #[test]
  fn reports_wrong_case_camel_case_function() {
    let source = "a { transform: TranslateX(0); }";
    let ctx = make_ctx(source);
    let d = configured(&FunctionNameCase, None).check_source(&ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("translateX"));
  }

  #[test]
  fn upper_mode_reports_lowercase() {
    use serde_json::json;
    let opts: serde_json::Value = json!(["upper"]);
    let source = "a { margin: calc(5%); }";
    let ctx = make_ctx(source);
    let d = configured(&FunctionNameCase, opts.clone()).check_source(&ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("CALC"));
  }

  #[test]
  fn upper_mode_allows_uppercase() {
    use serde_json::json;
    let opts: serde_json::Value = json!(["upper"]);
    let source = "a { margin: CALC(5%); }";
    let ctx = make_ctx(source);
    assert!(
      configured(&FunctionNameCase, opts.clone())
        .check_source(&ctx)
        .is_empty()
    );
  }

  #[test]
  fn upper_mode_allows_translatex_uppercase() {
    use serde_json::json;
    let opts: serde_json::Value = json!(["upper"]);
    let source = "a { transform: TRANSLATEX(0); }";
    let ctx = make_ctx(source);
    assert!(
      configured(&FunctionNameCase, opts.clone())
        .check_source(&ctx)
        .is_empty()
    );
  }

  #[test]
  fn ignore_functions_exact_match() {
    use serde_json::json;
    let opts: serde_json::Value = json!(["lower", {"ignoreFunctions": ["someFunction"]}]);
    let source = "a { color: someFunction(); }";
    let ctx = make_ctx(source);
    assert!(
      configured(&FunctionNameCase, opts.clone())
        .check_source(&ctx)
        .is_empty()
    );
  }

  #[test]
  fn ignore_functions_regex_pattern() {
    use serde_json::json;
    let opts: serde_json::Value = json!(["lower", {"ignoreFunctions": ["/^get.*$/"]}]);
    let source = "a { color: getDefaultColor(); }";
    let ctx = make_ctx(source);
    assert!(
      configured(&FunctionNameCase, opts.clone())
        .check_source(&ctx)
        .is_empty()
    );
  }

  #[test]
  fn vendor_prefix_lowercase_mode() {
    let source = "a { background: -webkit-radial-gradient(red, green, blue); }";
    let ctx = make_ctx(source);
    assert!(
      configured(&FunctionNameCase, None)
        .check_source(&ctx)
        .is_empty()
    );
  }

  #[test]
  fn vendor_prefix_uppercase_detected() {
    let source = "a { background: -WEBKIT-radial-gradient(red, green, blue); }";
    let ctx = make_ctx(source);
    let d = configured(&FunctionNameCase, None).check_source(&ctx);
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn correct_column_for_function_name() {
    let source = "a { padding: Some-function(5px); }";
    let ctx = make_ctx(source);
    let d = configured(&FunctionNameCase, None).check_source(&ctx);
    assert_eq!(d.len(), 1);
    // "Some-function" starts at column 14 (1-indexed), byte offset 13
    assert_eq!(d[0].span.offset, 13);
  }

  #[test]
  fn emits_fix_for_uppercase_function() {
    let source = "a { color: RGB(0, 0, 0); }";
    let ctx = make_ctx(source);
    let d = configured(&FunctionNameCase, None).check_source(&ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].fix.is_some());
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].new_text, "rgb");
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["upper", { "ignoreFunctions": "someFunction" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("camel")),
      ["Invalid option value \"camel\" for rule \"function-name-case\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(["lower", { "ignore": ["x"] }])),
      ["Invalid option name \"ignore\" for rule \"function-name-case\""]
    );
  }
}
