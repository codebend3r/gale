use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::stylelint_version::stylelint_major_version;

const DEFAULT_PATTERN: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

/// Returns true if the string has a COMPLETE SCSS interpolation (`#{...}`).
/// Mirrors Stylelint's `hasScssInterpolation` which uses `/#\{.+?\}/s`.
fn has_complete_scss_interpolation(s: &str) -> bool {
  let bytes = s.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  while i + 1 < len {
    if bytes[i] == b'#' && bytes[i + 1] == b'{' {
      // Found `#{` — check if there's a closing `}`
      if s[i + 2..].contains('}') {
        return true;
      }
    }
    i += 1;
  }
  false
}

/// Build the diagnostic message for a property name.
///
/// Stylelint 16+ uses `Expected "${name}" to match pattern "${pattern}"`.
/// Stylelint <=15 uses `Expected custom property name to match pattern "${pattern}"`.
/// Custom message templates (from JS arrow function conversion) override both.
fn build_message(
  prop_name: &str,
  custom_message: Option<&str>,
  pattern_str: &str,
  stylelint_major: u32,
) -> String {
  if let Some(template) = custom_message {
    // Replace `${name}` placeholder with the actual property name.
    // This handles messages converted from JS arrow functions:
    // `(name) => `...${name}...`` → `"...${name}..."`
    template.replace("${name}", prop_name)
  } else if stylelint_major >= 14 {
    format!("Expected \"{prop_name}\" to match pattern \"{pattern_str}\"")
  } else {
    format!("Expected custom property name to match pattern \"{pattern_str}\"")
  }
}

/// Extract the first token of a `var()` call argument from the given value string.
/// Returns `(token, offset_in_value)` where `offset_in_value` is the byte position
/// of the token within `value`.
///
/// Mimics postcss-value-parser's word tokenization: reads characters until a
/// quote (`'` or `"`), comma (`,`), close-paren (`)`), open-paren (`(`), or
/// whitespace is encountered. This matches how Stylelint identifies the custom
/// property name inside `var()`.
fn extract_var_first_token(value: &str) -> Option<(&str, usize)> {
  // Find `var(` case-insensitively, outside strings: `"var(--a)"` is a
  // string to the value parser, not a function.
  let mut quote = None;
  let var_start = value.as_bytes().windows(4).enumerate().position(|(i, w)| {
    let byte = w[0];
    match quote {
      Some(q) if byte == q && (i == 0 || value.as_bytes()[i - 1] != b'\\') => quote = None,
      Some(_) => {}
      None if byte == b'"' || byte == b'\'' => quote = Some(byte),
      None => return w.eq_ignore_ascii_case(b"var("),
    }
    false
  })?;
  let arg_start = var_start + 4;

  let bytes = value.as_bytes();
  let len = bytes.len();

  // Skip leading whitespace
  let mut pos = arg_start;
  while pos < len && (bytes[pos] == b' ' || bytes[pos] == b'\t' || bytes[pos] == b'\n') {
    pos += 1;
  }

  if pos >= len {
    return None;
  }

  // The argument must start with `--` to be a custom property reference
  if !value[pos..].starts_with("--") {
    return None;
  }

  // Read until: single quote, double quote, comma, close-paren, open-paren, or whitespace
  let token_start = pos;
  while pos < len {
    match bytes[pos] {
      b'\'' | b'"' | b',' | b')' | b'(' | b' ' | b'\t' | b'\n' => break,
      _ => pos += 1,
    }
  }

  if pos == token_start {
    return None;
  }

  Some((&value[token_start..pos], token_start))
}

const NAME: &str = "custom-property-pattern";

declare_rule! {
  /// Enforce a naming pattern for custom properties (CSS variables).
  ///
  /// Equivalent to Stylelint's `custom-property-pattern` rule.
  /// Accepts a regex string as the primary option.
  /// Default pattern: kebab-case (`^([a-z][a-z0-9]*)(-[a-z0-9]+)*$`).
  pub struct CustomPropertyPattern => CustomPropertyPatternCheck {
    name: NAME,
    description: "Specify a pattern for custom properties",
    default_severity: Warning,
  }
}

/// `custom-property-pattern` as configured.
pub struct CustomPropertyPatternCheck {
  /// The pattern every custom property name must match.
  pattern: Pattern,
  /// The `message` every rule accepts, which this rule fills in with the
  /// property's name itself.
  custom_message: Option<String>,
}

impl CustomPropertyPatternCheck {
  /// Stylelint's options: a regular expression, as a string or `/regex/`.
  /// Gale reads no option as kebab-case, and a pattern that does not
  /// compile is an invalid option.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern();
    options.finish()?;
    let pattern = match pattern {
      Some(pattern) => pattern,
      None => Pattern::compile(DEFAULT_PATTERN).expect("the default pattern compiles"),
    };
    let custom_message = setting
      .secondary()
      .and_then(|secondary| secondary.get("message"))
      .and_then(|message| message.as_str())
      .map(String::from);
    Ok(Self {
      pattern,
      custom_message,
    })
  }
}

impl Check for CustomPropertyPatternCheck {
  /// Flags custom property names that do not match the configured regex,
  /// defaulting to kebab-case and honouring a custom `message`.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let pattern_str = self.pattern.source.as_str();
    let custom_message = self.custom_message.as_deref();

    let is_scss = matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
    );

    let stylelint_major = stylelint_major_version();

    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      // --- Check 1: custom property DEFINITION (left side) ---
      if let Some(name) = decl.prop().strip_prefix("--") {
        // Skip names with COMPLETE SCSS interpolation `#{...}`.
        // Mirrors Stylelint's isStandardSyntaxProperty check.
        if is_scss && has_complete_scss_interpolation(name) {
          // skip — non-standard syntax
        } else if !self.pattern.is_match(name) {
          let full_name = decl.prop();
          let message = build_message(full_name, custom_message, pattern_str, stylelint_major);
          // Stylelint reports the property, not the whole declaration.
          diags.push(
            Diagnostic::new(NAME, message)
              .span(Span::new(decl.prop_start(), full_name.len()))
              .message_args([full_name, pattern_str]),
          );
        }
      }

      // --- Check 2: custom property REFERENCES inside var() in values ---
      // Stylelint 14+ checks var() arguments; older versions (<=13) only
      // check property definitions (left side of declarations).
      if stylelint_major >= 14 {
        // The value as written, so that offsets into it are offsets into
        // the source.
        let value_start = decl.value_start();
        let value = decl.written_value();
        if value.contains("var(") || value.contains("VAR(") {
          if let Some((prop_name, token_offset_in_value)) = extract_var_first_token(value) {
            if let Some(name) = prop_name.strip_prefix("--") {
              // Same SCSS interpolation skip as above
              let skip = is_scss && has_complete_scss_interpolation(name);
              if !skip && !self.pattern.is_match(name) {
                let message =
                  build_message(prop_name, custom_message, pattern_str, stylelint_major);
                diags.push(
                  Diagnostic::new(NAME, message)
                    .span(Span::new(
                      value_start + token_offset_in_value,
                      prop_name.len(),
                    ))
                    .message_args([prop_name, pattern_str]),
                );
              }
            }
          }
        }
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  /// What `rule` reports for `:root { <prop>: #fff; }`.
  fn check_property(rule: &dyn Check, prop: &str) -> Vec<Diagnostic> {
    let source = format!(":root {{ {prop}: #fff; }}");
    rule.check_source(&ctx_with_source(&source))
  }

  #[test]
  fn reports_non_kebab_custom_property() {
    let rule = configured(&CustomPropertyPattern, None);
    let d = check_property(rule.as_ref(), "--myColor");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myColor"));
  }

  #[test]
  fn allows_kebab_case_custom_property() {
    let rule = configured(&CustomPropertyPattern, None);
    let d = check_property(rule.as_ref(), "--my-color");
    assert!(d.is_empty());
  }

  #[test]
  fn ignores_regular_properties() {
    let rule = configured(&CustomPropertyPattern, None);
    let d = check_property(rule.as_ref(), "color");
    assert!(d.is_empty());
  }

  #[test]
  fn custom_pattern() {
    let opts = serde_json::json!("^pf-(v[56]|t-)-(color|global|chart|[lcud])-.+$");
    let rule = configured(&CustomPropertyPattern, opts.clone());
    // Should pass: matches the custom pattern
    assert!(check_property(rule.as_ref(), "--pf-v5-color-primary").is_empty());
    // Should fail: doesn't match the custom pattern
    let d = check_property(rule.as_ref(), "--my-color");
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn custom_pattern_in_message() {
    let opts = serde_json::json!("^pf-");
    let rule = configured(&CustomPropertyPattern, opts.clone());
    let d = check_property(rule.as_ref(), "--my-color");
    assert!(d[0].message.contains("^pf-"));
  }

  #[test]
  fn array_format_options_with_secondary() {
    // Config like: ["^sds|slds-c|kx-...", { "message": "..." }]
    let opts = serde_json::json!(["^sds|slds-c|kx-([a-z][a-z0-9]*)(-[a-z0-9]+)*$", { "message": "Custom msg" }]);
    let rule = configured(&CustomPropertyPattern, opts.clone());
    // _slds-c-... should match via the `slds-c` alternative
    assert!(
      check_property(rule.as_ref(), "--_slds-c-accordion-spacing").is_empty(),
      "Should not flag --_slds-c-... with the SLDS pattern"
    );
    // my-color should be flagged (doesn't match the pattern)
    let d = check_property(rule.as_ref(), "--my-color");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Custom msg");
  }

  #[test]
  fn reports_the_property_and_the_var_argument_where_written() {
    let rule = configured(&CustomPropertyPattern, None);
    let source = "a {\n  --myColor: red;\n  color: /* c */ var( --otherColor);\n}\n";
    let found: Vec<(usize, usize)> = rule
      .check_source(&ctx_with_source(source))
      .iter()
      .map(|d| (d.span.offset, d.span.length))
      .collect();
    assert_eq!(found, [(6, 9), (44, 12)]);
    assert_eq!(&source[44..56], "--otherColor");
  }

  #[test]
  fn checks_every_declaration_but_not_strings() {
    let rule = configured(&CustomPropertyPattern, None);
    let source = "$map: (a: var(--Bad), b: \"var(--Quoted)\");\n@mixin m { --Def: 1; }";
    let names: Vec<String> = rule
      .check_source(&crate::testing::scss_ctx_with_source(source))
      .iter()
      .map(|d| source[d.span.offset..d.span.end()].to_string())
      .collect();
    assert_eq!(names, ["--Bad", "--Def"]);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    // The rule takes no secondary options of its own, so Stylelint does not
    // look at them.
    assert!(invalid_options(NAME, serde_json::json!(["^a", { "anything": 1 }])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(["a", "b"])),
      Vec::<String>::new(),
      "the second entry is the secondary options"
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"custom-property-pattern\""]
    );
  }
}
