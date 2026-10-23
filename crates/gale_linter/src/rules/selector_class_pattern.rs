use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::selector::postcss::{blank_comments, split_selectors};
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const DEFAULT_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

const NAME: &str = "selector-class-pattern";

declare_rule! {
  /// Enforces a pattern for class selectors.
  ///
  /// Accepts a regex string as the primary option (e.g. `"^[a-z][a-zA-Z0-9]+$"`).
  /// Defaults to kebab-case pattern if no option is provided.
  ///
  /// Equivalent to Stylelint's `selector-class-pattern` rule.
  pub struct SelectorClassPattern => SelectorClassPatternCheck {
    name: NAME,
    description: "Specify a pattern for class selectors",
    default_severity: Warning,
  }
}

/// `selector-class-pattern` as configured.
pub struct SelectorClassPatternCheck {
  /// The pattern every class name must match.
  pattern: Pattern,
}

impl SelectorClassPatternCheck {
  /// Stylelint's options: a regular expression, as a string or `/regex/`.
  /// Gale reads no option as kebab-case, and a pattern that does not
  /// compile is an invalid option, as Stylelint reports it, rather than the
  /// rule quietly switching off.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern();
    options.finish()?;
    let pattern = match pattern {
      Some(pattern) => pattern,
      None => Pattern::compile(DEFAULT_PATTERN).expect("the default pattern compiles"),
    };
    Ok(Self { pattern })
  }
}

impl Check for SelectorClassPatternCheck {
  /// Flags class names that do not match the configured regex, defaulting to
  /// kebab-case.
  ///
  /// Rules that are not standard syntax (interpolation, placeholders, Less
  /// mixins) are skipped, as Stylelint skips them.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let pattern_str = self.pattern.source.as_str();

    let mut diags = Vec::new();
    for rule in tree.walk_rules().filter(|rule| rule.is_standard_syntax()) {
      // The selector as written, its comments blanked out so that a class
      // named in one is not read and offsets still line up with the source.
      let selector = blank_comments(&rule.raw_selector());
      self.check_selector(&selector, rule.selector_start(), pattern_str, &mut diags);
    }
    diags
  }
}

impl SelectorClassPatternCheck {
  /// Reports the classes of `selector`, which starts at `selector_start` in
  /// the source, that do not match the pattern.
  fn check_selector(
    &self,
    selector: &str,
    selector_start: usize,
    pattern_str: &str,
    diags: &mut Vec<Diagnostic>,
  ) {
    // Stylelint checks each individual selector in a comma-separated
    // list and reports per-class violations with the position of the
    // class within the source.  We split by comma, extract classes from
    // each part, and compute byte offsets for accurate reporting.
    let parts: Vec<&str> = split_selectors(selector)
      .into_iter()
      .map(|(_, part)| part)
      .collect();
    let mut cursor: usize = 0; // byte position within the selector string

    for (idx, part) in parts.iter().enumerate() {
      if idx > 0 {
        // Skip past the comma separator in the selector string.
        cursor += 1; // the comma
      }
      // Skip whitespace between comma and start of this part.
      let sel_bytes = selector.as_bytes();
      while cursor < sel_bytes.len() && matches!(sel_bytes[cursor], b' ' | b'\t' | b'\n' | b'\r') {
        cursor += 1;
      }
      let part_start = cursor;
      let trimmed_part = part.trim();
      // Advance cursor past this part.
      cursor = part_start + trimmed_part.len();

      for (class, class_byte_offset) in extract_class_names_with_offsets(trimmed_part) {
        if !self.pattern.is_match(&class) {
          let offset = selector_start + part_start + class_byte_offset;
          let msg = if is_kebab_case_pattern(pattern_str) {
            format!("Expected class selector \".{class}\" to be kebab-case")
          } else {
            format!("Expected \".{class}\" to match pattern \"{pattern_str}\"")
          };
          diags.push(
            Diagnostic::new(NAME, msg)
              .span(Span::new(offset, class.len() + 1)) // +1 for the dot
              .message_args([format!(".{class}"), pattern_str.to_string()]),
          );
        }
      }
    }
  }
}

/// Returns true if the pattern is the default kebab-case pattern.
/// Stylelint uses "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$" as the default,
/// and produces a human-readable "to be kebab-case" message for it.
fn is_kebab_case_pattern(pattern: &str) -> bool {
  pattern == DEFAULT_PATTERN || pattern == "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$"
}

/// Extract class names from a selector along with their byte offsets
/// (offset of the `.` character) within the selector string.
fn extract_class_names_with_offsets(selector: &str) -> Vec<(String, usize)> {
  let mut classes = Vec::new();
  let chars: Vec<char> = selector.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut byte_pos: usize = 0;

  while i < len {
    let char_byte_len = chars[i].len_utf8();
    if chars[i] == '.' {
      let dot_byte_pos = byte_pos;
      i += 1;
      byte_pos += char_byte_len;
      let start = i;
      // A CSS class selector must start with a letter, hyphen, underscore,
      // or non-ASCII character after the dot. If the next character is a
      // digit, this is a numeric value (e.g. `.5` in `opacity: .5`), not
      // a class selector.
      if i < len && chars[i].is_ascii_digit() {
        // Skip past the numeric value
        while i < len
          && (chars[i].is_ascii_alphanumeric()
            || chars[i] == '.'
            || chars[i] == '-'
            || chars[i] == '_')
        {
          byte_pos += chars[i].len_utf8();
          i += 1;
        }
        continue;
      }
      while i < len {
        if chars[i].is_ascii_alphanumeric()
          || chars[i] == '-'
          || chars[i] == '_'
          || !chars[i].is_ascii()
        {
          byte_pos += chars[i].len_utf8();
          i += 1;
        } else if chars[i] == '#' && i + 1 < len && chars[i + 1] == '{' {
          byte_pos += chars[i].len_utf8();
          i += 1;
          byte_pos += chars[i].len_utf8();
          i += 1;
          let mut depth = 1;
          while i < len && depth > 0 {
            if chars[i] == '{' {
              depth += 1;
            } else if chars[i] == '}' {
              depth -= 1;
            }
            byte_pos += chars[i].len_utf8();
            i += 1;
          }
        } else {
          break;
        }
      }
      if i > start {
        let name: String = chars[start..i].iter().collect();
        classes.push((name, dot_byte_pos));
      }
    } else {
      byte_pos += char_byte_len;
      i += 1;
    }
  }

  classes
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options};

  /// A rule with the selector `sel`.
  fn style_with_selector(sel: &str) -> String {
    format!("{sel} {{ color: red; }}")
  }

  #[test]
  fn reports_camel_case_class() {
    let rule = configured(&SelectorClassPattern, None);
    let d = rule.check_source(&ctx_with_source(&style_with_selector(".myClass")));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myClass"));
  }

  #[test]
  fn allows_kebab_case_class() {
    let rule = configured(&SelectorClassPattern, None);
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector(".my-class")))
        .is_empty()
    );
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector(".foo")))
        .is_empty()
    );
  }

  #[test]
  fn reports_underscore_class() {
    let rule = configured(&SelectorClassPattern, None);
    let d = rule.check_source(&ctx_with_source(&style_with_selector(".my_class")));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn custom_pattern_camel_case() {
    let opts = serde_json::json!("^[a-z][a-zA-Z0-9]+$");
    let rule = configured(&SelectorClassPattern, opts.clone());
    // camelCase should pass
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector(".myClass")))
        .is_empty()
    );
    // kebab-case should fail
    let d = rule.check_source(&ctx_with_source(&style_with_selector(".my-class")));
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn custom_pattern_in_message() {
    let opts = serde_json::json!("^[a-z][a-zA-Z0-9]+$");
    let rule = configured(&SelectorClassPattern, opts.clone());
    let d = rule.check_source(&ctx_with_source(&style_with_selector(".my-class")));
    assert!(d[0].message.contains("^[a-z][a-zA-Z0-9]+$"));
  }

  #[test]
  fn ignores_numeric_values_after_dot() {
    let rule = configured(&SelectorClassPattern, None);
    // These are numeric values (e.g. `opacity: .5`), not class selectors.
    // Gale should not report them as class pattern violations.
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector(".5")))
        .is_empty(),
      ".5 should be ignored (numeric value)"
    );
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector(".2")))
        .is_empty(),
      ".2 should be ignored (numeric value)"
    );
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector(".15")))
        .is_empty(),
      ".15 should be ignored (numeric value)"
    );
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector(".65")))
        .is_empty(),
      ".65 should be ignored (numeric value)"
    );
    assert!(
      rule
        .check_source(&ctx_with_source(&style_with_selector(".8")))
        .is_empty(),
      ".8 should be ignored (numeric value)"
    );
  }

  #[test]
  fn extract_class_names_skips_numeric_dot_values() {
    // Verify the extraction function itself skips numeric values
    let names = |selector| -> Vec<String> {
      extract_class_names_with_offsets(selector)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
    };
    assert!(names(".5").is_empty());
    assert!(names(".15").is_empty());
    assert!(names(".8").is_empty());
    // But real class names still work
    assert_eq!(names(".foo"), vec!["foo"]);
    assert_eq!(names(".foo-bar"), vec!["foo-bar"]);
  }

  #[test]
  fn reports_at_the_class_as_written_and_skips_comments() {
    use crate::testing::context;
    use gale_css_parser::Syntax;
    let rule = configured(&SelectorClassPattern, None);
    let source = "a>.fooBar,\n  .x /* .Skip */ .barBaz {}";
    let d = rule.check_source(&ctx_with_source(source));
    let found: Vec<_> = d
      .iter()
      .map(|d| &source[d.span.offset..d.span.end()])
      .collect();
    assert_eq!(found, [".fooBar", ".barBaz"]);
    let scss = ".a { // .Skip\n  .bAd {} }\n.#{$x} .Interpolated {}\n%p .Placeholder {}";
    let d = rule.check_source(&context(scss, Syntax::Scss));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains(".bAd"));
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!("/^[a-z]+$/i")).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"selector-class-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-z"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0].starts_with("Invalid option value \"^[a-z\" for rule \"selector-class-pattern\": "),
      "{broken:?}"
    );
  }
}
