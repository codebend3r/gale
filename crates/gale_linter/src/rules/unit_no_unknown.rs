use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::data::is_known_unit;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::pattern;
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "unit-no-unknown";

declare_rule! {
  pub struct UnitNoUnknown => UnitNoUnknownCheck {
    name: NAME,
    description: "Disallow unknown units",
    default_severity: Warning,
  }
}

/// `unit-no-unknown` as configured.
pub struct UnitNoUnknownCheck {
  opts: Options,
}

impl UnitNoUnknownCheck {
  /// Stylelint's options: `true`, then `ignoreUnits` and `ignoreFunctions`.
  /// Which units are known also depends on the project's Stylelint, from
  /// `env`.
  fn from_setting(setting: &Setting, env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    let ignore_units = options.strings("ignoreUnits");
    let ignore_functions = options.strings("ignoreFunctions");
    options.finish()?;
    let additional_unknown_units = if env.stylelint_older_than(15) {
      UNITS_UNKNOWN_BEFORE_STYLELINT_15
    } else {
      &[]
    };
    Ok(Self {
      opts: Options {
        ignore_units,
        ignore_functions,
        additional_unknown_units,
      },
    })
  }
}

// ---------------------------------------------------------------------------
// Unit extraction with byte offsets
// ---------------------------------------------------------------------------

struct UnitOccurrence {
  unit: String,
  function: Option<String>,
  number_byte_offset: usize,
  unit_byte_len: usize,
}

/// Pulls the units out of `value`, recording each one's byte offset and the
/// function call it sits inside, if any.
fn extract_units_with_context(value: &str) -> Vec<UnitOccurrence> {
  let mut results = Vec::new();
  let chars: Vec<char> = value.chars().collect();
  let len = chars.len();
  let mut i = 0;
  let mut func_stack: Vec<String> = Vec::new();

  let mut char_to_byte: Vec<usize> = Vec::with_capacity(len + 1);
  {
    let mut bp = 0;
    for ch in value.chars() {
      char_to_byte.push(bp);
      bp += ch.len_utf8();
    }
    char_to_byte.push(bp);
  }

  while i < len {
    if chars[i] == '"' || chars[i] == '\'' {
      let quote = chars[i];
      i += 1;
      while i < len && chars[i] != quote {
        if chars[i] == '\\' {
          i += 1;
        }
        i += 1;
      }
      if i < len {
        i += 1;
      }
      continue;
    }

    if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
      i += 2;
      while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
        i += 1;
      }
      if i + 1 < len {
        i += 2;
      }
      continue;
    }

    if chars[i] == '(' {
      let fn_end = i;
      let mut fn_start = i;
      if fn_end > 0 {
        let mut j = fn_end as isize - 1;
        while j >= 0
          && (chars[j as usize].is_ascii_alphanumeric()
            || chars[j as usize] == '-'
            || chars[j as usize] == '_')
        {
          fn_start = j as usize;
          j -= 1;
        }
      }
      let func_name = if fn_start < fn_end {
        chars[fn_start..fn_end]
          .iter()
          .collect::<String>()
          .to_ascii_lowercase()
      } else {
        String::new()
      };
      func_stack.push(func_name);
      i += 1;
      continue;
    }

    if chars[i] == ')' {
      func_stack.pop();
      i += 1;
      continue;
    }

    // Skip inside url() only
    if let Some(func) = func_stack.last()
      && func == "url"
    {
      i += 1;
      continue;
    }

    if i + 1 < len && chars[i] == '-' && chars[i + 1] == '-' {
      i += 2;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    if chars[i] == '$' {
      i += 1;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    if chars[i] == '@' && i + 1 < len && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
    {
      i += 1;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      continue;
    }

    if chars[i] == '#' {
      i += 1;
      while i < len && chars[i].is_ascii_hexdigit() {
        i += 1;
      }
      continue;
    }

    if (chars[i] == 'U' || chars[i] == 'u') && i + 1 < len && chars[i + 1] == '+' {
      i += 2;
      while i < len && (chars[i].is_ascii_hexdigit() || chars[i] == '-' || chars[i] == '?') {
        i += 1;
      }
      continue;
    }

    if chars[i].is_ascii_digit()
      || (chars[i] == '.' && i + 1 < len && chars[i + 1].is_ascii_digit())
    {
      if i > 1
        && chars[i - 1] == '-'
        && (chars[i - 2].is_ascii_alphanumeric() || chars[i - 2] == '-' || chars[i - 2] == '_')
      {
        while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
          i += 1;
        }
        continue;
      }

      let number_start = i;
      while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
        i += 1;
      }
      if i < len && (chars[i] == 'e' || chars[i] == 'E') {
        let save = i;
        i += 1;
        if i < len && (chars[i] == '+' || chars[i] == '-') {
          i += 1;
        }
        if i < len && chars[i].is_ascii_digit() {
          while i < len && chars[i].is_ascii_digit() {
            i += 1;
          }
        } else {
          i = save;
        }
      }

      if i < len && (chars[i].is_ascii_alphabetic() || chars[i] == '%') {
        let unit_char_start = i;
        if chars[i] == '%' {
          i += 1;
        } else {
          while i < len && chars[i].is_ascii_alphabetic() {
            i += 1;
          }
        }
        if i < len
          && (chars[i] == '-'
            || chars[i] == '_'
            || chars[i] == '('
            || chars[i].is_ascii_alphanumeric())
        {
          while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
          {
            i += 1;
          }
          continue;
        }
        // Skip SCSS map keys: tokens like `2xl:` where the "unit" is
        // actually part of a map key (e.g., `(2xl: value)`).
        let mut peek = i;
        while peek < len && chars[peek].is_ascii_whitespace() {
          peek += 1;
        }
        if peek < len && chars[peek] == ':' {
          // Looks like a map key — skip this token entirely.
          i = peek + 1;
          continue;
        }

        let unit: String = chars[unit_char_start..i].iter().collect();
        let func = func_stack.last().cloned();
        results.push(UnitOccurrence {
          unit,
          function: func,
          number_byte_offset: char_to_byte[number_start],
          unit_byte_len: char_to_byte[i] - char_to_byte[unit_char_start],
        });
      }
    } else {
      i += 1;
    }
  }

  results
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Matches `value` against a `/…/` or `/…/i` regex pattern, else an exact name.
fn matches_pattern_ci(value: &str, pattern: &str) -> bool {
  pattern::match_regex_entry(pattern, value).unwrap_or_else(|| value.eq_ignore_ascii_case(pattern))
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// The rule's options, with what the project's Stylelint knows.
struct Options {
  ignore_units: Vec<String>,
  ignore_functions: Vec<String>,
  /// Units gale knows but the project's Stylelint does not, which it
  /// reports as unknown (see [`UNITS_UNKNOWN_BEFORE_STYLELINT_15`]).
  additional_unknown_units: &'static [&'static str],
}

/// The units Stylelint 15 added to its list of known units: dynamic
/// viewport units, container query units and a few newer font-relative
/// ones.  A project on an older Stylelint has them reported as unknown.
const UNITS_UNKNOWN_BEFORE_STYLELINT_15: &[&str] = &[
  "dvh", "dvw", "dvb", "dvi", "dvmax", "dvmin", "lvh", "lvw", "lvb", "lvi", "lvmax", "lvmin",
  "svh", "svw", "svb", "svi", "svmax", "svmin", "cqw", "cqh", "cqi", "cqb", "cqmin", "cqmax",
  "cap", "ic", "rcap", "rch", "rex", "ric", "vb", "vi",
];

/// Whether any `ignoreUnits` pattern matches this unit.
fn unit_is_ignored(unit: &str, patterns: &[String]) -> bool {
  patterns.iter().any(|p| matches_pattern_ci(unit, p))
}

/// Whether any `ignoreFunctions` pattern matches this function name.
fn function_is_ignored(func_name: &str, patterns: &[String]) -> bool {
  patterns.iter().any(|p| matches_pattern_ci(func_name, p))
}

/// `x` unit is only valid inside `image-set()` and `@media (resolution: ...)`.
fn is_x_unit_valid_in_context(func: Option<&str>, in_media_resolution: bool) -> bool {
  if in_media_resolution {
    return true;
  }
  if let Some(f) = func {
    return f == "image-set";
  }
  false
}

/// Byte offset where a declaration's value begins, past the colon and whitespace.
fn find_value_offset(source: &str, decl_offset: usize, decl_length: usize) -> usize {
  let end = (decl_offset + decl_length).min(source.len());
  if decl_offset >= source.len() || end <= decl_offset {
    return decl_offset;
  }
  let slice = &source[decl_offset..end];
  if let Some(colon) = slice.find(':') {
    let after_colon = colon + 1;
    let mut off = after_colon;
    let bytes = slice.as_bytes();
    while off < bytes.len() && bytes[off].is_ascii_whitespace() {
      off += 1;
    }
    decl_offset + off
  } else {
    decl_offset
  }
}

/// Byte offset where an at-rule's prelude begins, past its name and whitespace.
fn find_params_offset(source: &str, at_rule_offset: usize) -> usize {
  let start = at_rule_offset;
  if start >= source.len() {
    return start;
  }
  let rest = &source[start..];
  let mut off = 1;
  let bytes = rest.as_bytes();
  while off < bytes.len() && (bytes[off].is_ascii_alphanumeric() || bytes[off] == b'-') {
    off += 1;
  }
  while off < bytes.len() && bytes[off].is_ascii_whitespace() {
    off += 1;
  }
  start + off
}

// ---------------------------------------------------------------------------
// Rule implementation
// ---------------------------------------------------------------------------

impl Check for UnitNoUnknownCheck {
  /// Flags units that are not standard CSS. `content`, `unicode-range` and SCSS
  /// variable declarations are skipped.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let opts = &self.opts;
    let mut diags = Vec::new();

    match node {
      CssNode::Style(rule) => {
        for decl in &rule.declarations {
          if decl.property.eq_ignore_ascii_case("content") {
            continue;
          }
          if decl.property.eq_ignore_ascii_case("unicode-range") {
            continue;
          }
          // Skip SCSS variable declarations ($var: value)
          if decl.property.starts_with('$') {
            continue;
          }
          let value_offset = find_value_offset(ctx.source, decl.span.offset, decl.span.length);
          check_value(&decl.value, opts, false, value_offset, NAME, &mut diags);
        }
      }
      CssNode::AtRule(at_rule) => {
        if !at_rule.params.is_empty() {
          let at_name = at_rule.name.to_ascii_lowercase();
          // Skip at-rules whose params are identifiers, not CSS values
          if matches!(
            at_name.as_str(),
            "font-face"
              | "mixin"
              | "include"
              | "function"
              | "extend"
              | "import"
              | "use"
              | "forward"
              | "at-root"
              | "return"
              | "debug"
              | "warn"
              | "error"
              | "each"
              | "for"
              | "while"
              | "namespace"
              | "layer"
              | "charset"
          ) {
            // skip
          } else {
            let params_offset = find_params_offset(ctx.source, at_rule.span.offset);
            check_at_rule_params(&at_rule.params, opts, params_offset, NAME, &mut diags);
          }
        }
      }
      CssNode::Declaration(decl) => {
        if decl.property.eq_ignore_ascii_case("content")
          || decl.property.eq_ignore_ascii_case("unicode-range")
          || decl.property.starts_with('$')
        {
          return vec![];
        }
        let value_offset = find_value_offset(ctx.source, decl.span.offset, decl.span.length);
        check_value(&decl.value, opts, false, value_offset, NAME, &mut diags);
      }
      _ => {}
    }

    diags
  }
}

/// Reports each unknown unit in `value`. Resolution features accept units the
/// rest of CSS does not.
fn check_value(
  value: &str,
  opts: &Options,
  in_media_resolution: bool,
  value_base_offset: usize,
  rule_name: &str,
  diags: &mut Vec<Diagnostic>,
) {
  let occurrences = extract_units_with_context(value);
  for occ in occurrences {
    let unit_lower = occ.unit.to_ascii_lowercase();

    if is_known_unit(&unit_lower) {
      // A unit the project's Stylelint version does not know yet.
      let forced_unknown = opts
        .additional_unknown_units
        .iter()
        .any(|u| u.eq_ignore_ascii_case(&unit_lower));

      if unit_lower == "x" {
        if !forced_unknown
          && is_x_unit_valid_in_context(occ.function.as_deref(), in_media_resolution)
        {
          continue;
        }
      } else if !forced_unknown {
        continue;
      }
    }

    if unit_is_ignored(&occ.unit, &opts.ignore_units) {
      continue;
    }

    if let Some(ref func) = occ.function
      && function_is_ignored(func, &opts.ignore_functions)
    {
      continue;
    }

    let abs_offset = value_base_offset + occ.number_byte_offset;
    diags.push(
      Diagnostic::new(
        rule_name,
        format!("Unexpected unknown unit \"{}\"", occ.unit),
      )
      .span(Span::new(abs_offset, occ.unit_byte_len)),
    );
  }
}

/// Reports unknown units in an at-rule prelude, per media feature.
fn check_at_rule_params(
  params: &str,
  opts: &Options,
  params_base_offset: usize,
  rule_name: &str,
  diags: &mut Vec<Diagnostic>,
) {
  let features = extract_media_features(params);

  for (feature_name, feature_value, value_byte_offset) in &features {
    let is_resolution = feature_name == "resolution"
      || feature_name == "min-resolution"
      || feature_name == "max-resolution";
    let base = params_base_offset + value_byte_offset;
    check_value(feature_value, opts, is_resolution, base, rule_name, diags);
  }

  if features.is_empty() {
    check_value(params, opts, false, params_base_offset, rule_name, diags);
  }
}

/// Splits an at-rule prelude into `(name, value, value_offset)` triples.
fn extract_media_features(params: &str) -> Vec<(String, String, usize)> {
  let mut results = Vec::new();
  let lower = params.to_ascii_lowercase();
  let chars: Vec<char> = lower.chars().collect();
  let len = chars.len();
  let mut i = 0;

  let mut char_to_byte: Vec<usize> = Vec::with_capacity(len + 1);
  {
    let mut bp = 0;
    for ch in lower.chars() {
      char_to_byte.push(bp);
      bp += ch.len_utf8();
    }
    char_to_byte.push(bp);
  }

  while i < len {
    if chars[i] == '(' {
      i += 1;
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
        i += 2;
        while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
      }
      let name_start = i;
      while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
        i += 1;
      }
      let name: String = chars[name_start..i].iter().collect();
      while i < len && chars[i].is_ascii_whitespace() {
        i += 1;
      }
      if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
        i += 2;
        while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
          i += 1;
        }
        if i + 1 < len {
          i += 2;
        }
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
      }
      if i < len && (chars[i] == ':' || chars[i] == '<' || chars[i] == '>' || chars[i] == '=') {
        i += 1;
        if i < len && chars[i] == '=' {
          i += 1;
        }
        while i < len && chars[i].is_ascii_whitespace() {
          i += 1;
        }
        let val_start = i;
        let mut depth = 1;
        while i < len && depth > 0 {
          if chars[i] == '(' {
            depth += 1;
          } else if chars[i] == ')' {
            depth -= 1;
            if depth == 0 {
              break;
            }
          }
          i += 1;
        }
        let val: String = chars[val_start..i].iter().collect();
        if !name.is_empty() {
          results.push((name, val.trim().to_string(), char_to_byte[val_start]));
        }
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
  use gale_css_parser::{CssNode, Declaration, Span as ParserSpan, StyleRule};

  use crate::testing::{configured, configured_in, ctx, invalid_options};

  fn style_with_value(val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "width".to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_unknown_unit() {
    let rule = configured(&UnitNoUnknown, None);
    let d = rule.check(&style_with_value("10xyz"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("xyz"));
  }

  #[test]
  fn allows_known_units() {
    let rule = configured(&UnitNoUnknown, None);
    assert!(rule.check(&style_with_value("10px"), &ctx()).is_empty());
    assert!(rule.check(&style_with_value("2rem"), &ctx()).is_empty());
    assert!(rule.check(&style_with_value("50%"), &ctx()).is_empty());
  }

  #[test]
  fn does_not_extract_unit_from_function_names() {
    let rule = configured(&UnitNoUnknown, None);
    let d = rule.check(&style_with_value("scale3d(1.1, 1.1, 1.1)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_scss_map_keys() {
    // SCSS map keys like `2xl:` should not be treated as "number 2 + unit xl"
    let units = extract_units_with_context("2xl: 100px");
    let unit_names: Vec<&str> = units.iter().map(|u| u.unit.as_str()).collect();
    assert!(
      !unit_names.contains(&"xl"),
      "should not extract 'xl' from SCSS map key '2xl:'; got: {:?}",
      unit_names
    );
    // The value part (100px) should still be parsed
    assert!(
      unit_names.contains(&"px"),
      "should still extract 'px' from value; got: {:?}",
      unit_names
    );
  }

  #[test]
  fn skips_scss_map_key_with_whitespace() {
    // `2xs :` with whitespace before colon
    let units = extract_units_with_context("2xs : 50rem");
    let unit_names: Vec<&str> = units.iter().map(|u| u.unit.as_str()).collect();
    assert!(
      !unit_names.contains(&"xs"),
      "should not extract 'xs' from map key '2xs :'; got: {:?}",
      unit_names
    );
  }

  #[test]
  fn still_reports_unknown_unit_not_followed_by_colon() {
    let units = extract_units_with_context("2xl");
    let unit_names: Vec<&str> = units.iter().map(|u| u.unit.as_str()).collect();
    assert!(
      unit_names.contains(&"xl"),
      "should extract 'xl' when not followed by colon; got: {:?}",
      unit_names
    );
  }

  #[test]
  fn units_newer_than_the_projects_stylelint_are_unknown() {
    let older = LintEnv {
      stylelint_version: Some((14, 16, 1)),
    };
    let node = style_with_value("100dvh");
    assert!(
      configured(&UnitNoUnknown, None)
        .check(&node, &ctx())
        .is_empty()
    );
    let d = configured_in(&UnitNoUnknown, None, older).check(&node, &ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Unexpected unknown unit \"dvh\"");
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreUnits": ["/^my-/", "x"], "ignoreFunctions": "image-set" }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreUnits": [true] }])),
      ["Invalid value \"true\" for option \"ignoreUnits\" of rule \"unit-no-unknown\""]
    );
  }
}
