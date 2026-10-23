use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "syntax-string-no-invalid";

declare_rule! {
  /// Disallow invalid syntax strings in `@property` descriptors.
  ///
  /// The `syntax` descriptor in a `@property` rule must be a quoted string
  /// containing a valid CSS syntax definition. This rule checks that a quoted
  /// value is non-empty and uses only known CSS type names; like Stylelint's,
  /// it has nothing to say about a value that is not a string.
  ///
  /// Equivalent to Stylelint's `syntax-string-no-invalid` rule.
  pub struct SyntaxStringNoInvalid => SyntaxStringNoInvalidCheck {
    name: NAME,
    description: "Disallow invalid syntax strings in @property descriptors",
    default_severity: Error,
  }
}

/// `syntax-string-no-invalid`, which takes no options.
pub struct SyntaxStringNoInvalidCheck;

impl SyntaxStringNoInvalidCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

/// Known CSS syntax component types that can appear inside `< >`.
const KNOWN_SYNTAX_TYPES: &[&str] = &[
  "angle",
  "color",
  "custom-ident",
  "flex",
  "image",
  "integer",
  "length",
  "length-percentage",
  "number",
  "percentage",
  "resolution",
  "time",
  "transform-function",
  "transform-list",
  "url",
];

/// Validate a CSS syntax string (the unquoted content between quotes).
///
/// Returns `None` if valid, or `Some(reason)` if invalid.
fn validate_syntax_string(value: &str) -> Option<String> {
  if value.is_empty() {
    return Some("Empty syntax string".to_string());
  }

  // Universal syntax
  if value == "*" {
    return None;
  }

  // Split on '|' for alternatives (pipe-separated)
  for component in value.split('|') {
    let component = component.trim();
    if component.is_empty() {
      return Some("Empty component in syntax string".to_string());
    }

    // Strip trailing multiplier (+, #)
    let component = component.trim_end_matches('+').trim_end_matches('#').trim();

    if component.is_empty() {
      return Some("Empty component in syntax string".to_string());
    }

    // Must be a data type like <length> or a keyword (custom-ident literal)
    if component.starts_with('<') {
      if !component.ends_with('>') {
        return Some(format!("Unclosed angle bracket in \"{component}\""));
      }
      let type_name = &component[1..component.len() - 1];
      if !KNOWN_SYNTAX_TYPES.contains(&type_name) {
        return Some(format!("Unknown syntax type \"<{type_name}>\""));
      }
    }
    // Otherwise it's a keyword ident, which is valid
  }

  None
}

impl Check for SyntaxStringNoInvalidCheck {
  /// Flags a `@property` `syntax` descriptor string that is empty or names a
  /// CSS type this rule does not know.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // The `syntax` descriptors of every `@property`.
    let descriptors = tree
      .walk_at_rules()
      .filter(|at| at.name() == "property")
      .flat_map(|at| at.child_decls())
      .filter(|decl| decl.prop() == "syntax");
    for decl in descriptors {
      let value = decl.value().trim();

      // Stylelint checks the strings of the value and nothing else.
      let quoted = value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
          || (value.starts_with('\'') && value.ends_with('\'')));
      if !quoted {
        continue;
      }
      let inner = &value[1..value.len() - 1];
      if let Some(reason) = validate_syntax_string(inner) {
        diags.push(
          Diagnostic::new(NAME, format!("Invalid syntax string: {reason}")).span(decl.span()),
        );
      }
    }

    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_diagnostics::Span;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  fn property_rule(syntax_value: &str) -> String {
    format!("@property --my-color {{ syntax: {syntax_value}; inherits: false; }}")
  }

  #[test]
  fn allows_valid_length() {
    assert!(
      configured(&SyntaxStringNoInvalid, None)
        .check_source(&ctx_with_source(&property_rule("\"<length>\"")))
        .is_empty()
    );
  }

  #[test]
  fn allows_valid_color() {
    assert!(
      configured(&SyntaxStringNoInvalid, None)
        .check_source(&ctx_with_source(&property_rule("\"<color>\"")))
        .is_empty()
    );
  }

  #[test]
  fn allows_universal() {
    assert!(
      configured(&SyntaxStringNoInvalid, None)
        .check_source(&ctx_with_source(&property_rule("\"*\"")))
        .is_empty()
    );
  }

  #[test]
  fn allows_pipe_separated() {
    assert!(
      configured(&SyntaxStringNoInvalid, None)
        .check_source(&ctx_with_source(&property_rule("\"<length> | <color>\"")))
        .is_empty()
    );
  }

  #[test]
  fn allows_plus_suffix() {
    assert!(
      configured(&SyntaxStringNoInvalid, None)
        .check_source(&ctx_with_source(&property_rule("\"<length>+\"")))
        .is_empty()
    );
  }

  #[test]
  fn allows_hash_suffix() {
    assert!(
      configured(&SyntaxStringNoInvalid, None)
        .check_source(&ctx_with_source(&property_rule("\"<length>#\"")))
        .is_empty()
    );
  }

  #[test]
  fn reports_unknown_type() {
    let d = configured(&SyntaxStringNoInvalid, None)
      .check_source(&ctx_with_source(&property_rule("\"<foo>\"")));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unknown syntax type"));
    assert!(d[0].message.contains("<foo>"));
  }

  #[test]
  fn ignores_unquoted_value() {
    // Stylelint's rule reads only the strings of the value; a value that is
    // not one is `at-rule-descriptor-value-no-unknown`'s business.
    let d = configured(&SyntaxStringNoInvalid, None)
      .check_source(&ctx_with_source(&property_rule("<length>")));
    assert!(d.is_empty());
  }

  #[test]
  fn reports_empty_string() {
    let d = configured(&SyntaxStringNoInvalid, None)
      .check_source(&ctx_with_source(&property_rule("\"\"")));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Empty syntax string"));
  }

  #[test]
  fn ignores_non_property_at_rules() {
    let source = "@media screen { syntax: \"<bogus>\"; }";
    assert!(
      configured(&SyntaxStringNoInvalid, None)
        .check_source(&ctx_with_source(source))
        .is_empty()
    );
  }

  #[test]
  fn reads_property_rules_in_css() {
    // The CSS parser used to keep no descriptors for `@property`.
    let source = "@property --x {\n  syntax: \"<foo>\";\n}";
    let d = configured(&SyntaxStringNoInvalid, None).check_source(&ctx_with_source(source));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].span, Span::new(18, 16));
  }

  #[test]
  fn allows_keyword_idents() {
    // Plain keywords like "auto" are valid in syntax descriptors
    assert!(
      configured(&SyntaxStringNoInvalid, None)
        .check_source(&ctx_with_source(&property_rule("\"auto\"")))
        .is_empty()
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"syntax-string-no-invalid\""]
    );
  }
}
