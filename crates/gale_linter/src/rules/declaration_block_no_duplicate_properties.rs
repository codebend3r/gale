use std::collections::HashMap;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow duplicate properties within declaration blocks.
///
/// Equivalent to Stylelint's `declaration-block-no-duplicate-properties` rule.
pub struct DeclarationBlockNoDuplicateProperties;

impl Rule for DeclarationBlockNoDuplicateProperties {
  fn name(&self) -> &'static str {
    "declaration-block-no-duplicate-properties"
  }

  fn description(&self) -> &'static str {
    "Disallow duplicate properties within declaration blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    // Stylelint skips rules whose selector contains SCSS interpolation
    // (`#{...}`) via `isStandardSyntaxRule`. Match that behavior.
    if matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
    ) && rule.selector.contains("#{")
    {
      return vec![];
    }

    // Check for ignore options
    let ignore_list: Vec<String> = ctx
      .options
      .and_then(|v| v.get("ignore"))
      .and_then(|v| {
        if let Some(arr) = v.as_array() {
          Some(
            arr
              .iter()
              .filter_map(|item| item.as_str().map(|s| s.to_string()))
              .collect(),
          )
        } else if let Some(s) = v.as_str() {
          Some(vec![s.to_string()])
        } else {
          None
        }
      })
      .unwrap_or_default();

    let ignore_consecutive = ignore_list.iter().any(|s| s == "consecutive-duplicates");
    let ignore_diff_values = ignore_list
      .iter()
      .any(|s| s == "consecutive-duplicates-with-different-values");
    let ignore_diff_syntaxes = ignore_list
      .iter()
      .any(|s| s == "consecutive-duplicates-with-different-syntaxes");
    let ignore_prefixless_same = ignore_list
      .iter()
      .any(|s| s == "consecutive-duplicates-with-same-prefixless-values");

    // Parse ignoreProperties option (supports strings and regex patterns like "/background-/")
    let ignore_properties: Vec<PropertyMatcher> = ctx
      .options
      .and_then(|v| v.get("ignoreProperties"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|item| item.as_str().map(PropertyMatcher::from_pattern))
          .collect()
      })
      .unwrap_or_default();

    let is_preprocessor = matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass | gale_css_parser::Syntax::Less
    );

    // Track declarations we've seen: lowercase property -> index into `decls` vec.
    // This mirrors Stylelint's algorithm: store all seen declarations, check for
    // duplicates by looking up the previous declaration with the same property name.
    let mut decl_map: HashMap<String, usize> = HashMap::new();

    /// Tracked declaration info
    struct DeclInfo {
      property: String,
      lower_prop: String,
      value: String,
      important: bool,
      span: gale_css_parser::Span,
    }

    let mut decls: Vec<DeclInfo> = Vec::new();
    let mut diagnostics = Vec::new();

    // Sort declarations by source offset. lightningcss separates important
    // and non-important declarations, which can produce them out of source
    // order. We need source order for correct consecutive-duplicate checks.
    let mut sorted_decls: Vec<&gale_css_parser::Declaration> = rule.declarations.iter().collect();
    sorted_decls.sort_by_key(|d| d.span.offset);

    for decl in &sorted_decls {
      let prop = &decl.property;
      let lower_prop = prop.to_ascii_lowercase();

      // Skip properties with SCSS/Less interpolation — we can't resolve the
      // actual name, so duplicate detection would produce false positives.
      if is_preprocessor && prop.contains("#{") {
        decls.push(DeclInfo {
          property: prop.clone(),
          lower_prop: lower_prop.clone(),
          value: decl.value.clone(),
          important: decl.important,
          span: decl.span,
        });
        continue;
      }

      // Skip non-standard syntax properties (SCSS $variables, Less @variables)
      if prop.starts_with('$') || (is_preprocessor && prop.starts_with('@')) {
        decls.push(DeclInfo {
          property: prop.clone(),
          lower_prop: lower_prop.clone(),
          value: decl.value.clone(),
          important: decl.important,
          span: decl.span,
        });
        continue;
      }

      // Skip custom properties (--*)
      if prop.starts_with("--") {
        decls.push(DeclInfo {
          property: prop.clone(),
          lower_prop: lower_prop.clone(),
          value: decl.value.clone(),
          important: decl.important,
          span: decl.span,
        });
        continue;
      }

      // Skip `src` property (commonly duplicated in @font-face)
      if lower_prop == "src" {
        decls.push(DeclInfo {
          property: prop.clone(),
          lower_prop: lower_prop.clone(),
          value: decl.value.clone(),
          important: decl.important,
          span: decl.span,
        });
        continue;
      }

      // Skip properties matching ignoreProperties patterns
      if ignore_properties
        .iter()
        .any(|m| m.matches(&lower_prop, prop))
      {
        decls.push(DeclInfo {
          property: prop.clone(),
          lower_prop: lower_prop.clone(),
          value: decl.value.clone(),
          important: decl.important,
          span: decl.span,
        });
        continue;
      }

      let current_index = decls.len();

      if let Some(&dup_index) = decl_map.get(&lower_prop) {
        let dup_decl = &decls[dup_index];
        let dup_value = &dup_decl.value;
        let dup_important = dup_decl.important;
        let current_value = &decl.value;
        let current_important = decl.important;

        // Is the duplicate more important than the current declaration?
        let duplicate_is_more_important = !current_important && dup_important;

        // Are the duplicates consecutive? (dup_index is the last index in decls)
        let duplicates_are_consecutive = dup_index == current_index - 1;

        // Unprefixed values are equal?
        let unprefixed_dup_equal = strip_vendor_prefix_from_value(current_value)
          == strip_vendor_prefix_from_value(dup_value);

        // Handle the ignore options (matching Stylelint's logic)
        if ignore_diff_values || ignore_diff_syntaxes || ignore_prefixless_same {
          // Non-consecutive duplicates are always reported
          if !duplicates_are_consecutive || (ignore_prefixless_same && !unprefixed_dup_equal) {
            // Report
            let (report_prop, report_span) = if duplicate_is_more_important {
              (prop.clone(), decl.span)
            } else {
              (dup_decl.property.clone(), dup_decl.span)
            };

            if !duplicate_is_more_important {
              // Replace the tracked duplicate with the current one
              decl_map.insert(lower_prop.clone(), current_index);
            }

            diagnostics.push(
              Diagnostic::new(
                self.name(),
                format!("Unexpected duplicate \"{}\"", report_prop),
              )
              .severity(self.default_severity())
              .span(Span::new(report_span.offset, report_span.length)),
            );

            decls.push(DeclInfo {
              property: prop.clone(),
              lower_prop: lower_prop.clone(),
              value: decl.value.clone(),
              important: decl.important,
              span: decl.span,
            });
            continue;
          }

          if ignore_diff_syntaxes {
            // For consecutive-duplicates-with-different-syntaxes, Stylelint
            // skips ALL consecutive duplicates in a fallback group, regardless
            // of whether the values have the same or different syntax. This
            // matches the CSS fallback pattern: `cursor: move; cursor: grab;
            // cursor: grab;` is valid and none are reported.
            // Update decl_map so the next entry sees a consecutive duplicate.
            decl_map.insert(lower_prop.clone(), current_index);
            decls.push(DeclInfo {
              property: prop.clone(),
              lower_prop: lower_prop.clone(),
              value: decl.value.clone(),
              important: decl.important,
              span: decl.span,
            });
            continue;
          }

          // If values differ, skip (allowed by these ignore modes).
          // IMPORTANT: do NOT update decl_map here. Stylelint keeps the
          // original declaration as the reference point, so subsequent
          // duplicates with the same value still compare against the first
          // (different-syntax) one and are also skipped. Updating decl_map
          // would make the next identical duplicate compare against itself
          // (same value → equal syntax → falsely reported).
          if current_value != dup_value {
            decls.push(DeclInfo {
              property: prop.clone(),
              lower_prop: lower_prop.clone(),
              value: decl.value.clone(),
              important: decl.important,
              span: decl.span,
            });
            // For ignore_diff_values, update decl_map so the next entry
            // checks consecutiveness against this one (not the original).
            // This handles chains like `display:-webkit-box; display:-ms-flexbox; display:flex`.
            if ignore_diff_values {
              decl_map.insert(lower_prop.clone(), current_index);
            }
            continue;
          }

          // Same value consecutive duplicate - report
          let (report_prop, report_span) = if duplicate_is_more_important {
            (prop.clone(), decl.span)
          } else {
            (dup_decl.property.clone(), dup_decl.span)
          };

          if !duplicate_is_more_important {
            decl_map.insert(lower_prop.clone(), current_index);
          }

          diagnostics.push(
            Diagnostic::new(
              self.name(),
              format!("Unexpected duplicate \"{}\"", report_prop),
            )
            .severity(self.default_severity())
            .span(Span::new(report_span.offset, report_span.length)),
          );

          decls.push(DeclInfo {
            property: prop.clone(),
            lower_prop: lower_prop.clone(),
            value: decl.value.clone(),
            important: decl.important,
            span: decl.span,
          });
          continue;
        }

        // ignore: consecutive-duplicates
        if ignore_consecutive && duplicates_are_consecutive {
          decls.push(DeclInfo {
            property: prop.clone(),
            lower_prop: lower_prop.clone(),
            value: decl.value.clone(),
            important: decl.important,
            span: decl.span,
          });
          if !duplicate_is_more_important {
            decl_map.insert(lower_prop.clone(), current_index);
          }
          continue;
        }

        // Default: report all duplicates
        let (report_prop, report_span) = if duplicate_is_more_important {
          (prop.clone(), decl.span)
        } else {
          (dup_decl.property.clone(), dup_decl.span)
        };

        if !duplicate_is_more_important {
          decl_map.insert(lower_prop.clone(), current_index);
        }

        diagnostics.push(
          Diagnostic::new(
            self.name(),
            format!("Unexpected duplicate \"{}\"", report_prop),
          )
          .severity(self.default_severity())
          .span(Span::new(report_span.offset, report_span.length)),
        );
      } else {
        decl_map.insert(lower_prop.clone(), current_index);
      }

      decls.push(DeclInfo {
        property: prop.clone(),
        lower_prop: lower_prop.clone(),
        value: decl.value.clone(),
        important: decl.important,
        span: decl.span,
      });
    }

    diagnostics
  }
}

/// Pattern matcher for ignoreProperties - supports plain strings and regex-like "/pattern/"
enum PropertyMatcher {
  Exact(String),
  Regex(regex::Regex),
}

impl PropertyMatcher {
  fn from_pattern(pattern: &str) -> Self {
    if pattern.starts_with('/') && pattern.ends_with('/') && pattern.len() > 2 {
      let re_str = &pattern[1..pattern.len() - 1];
      match regex::Regex::new(re_str) {
        Ok(re) => PropertyMatcher::Regex(re),
        Err(_) => PropertyMatcher::Exact(pattern.to_ascii_lowercase()),
      }
    } else {
      PropertyMatcher::Exact(pattern.to_ascii_lowercase())
    }
  }

  fn matches(&self, lower_prop: &str, original_prop: &str) -> bool {
    match self {
      PropertyMatcher::Exact(s) => lower_prop == s,
      PropertyMatcher::Regex(re) => re.is_match(original_prop) || re.is_match(lower_prop),
    }
  }
}

/// Strip vendor prefix from a CSS value.
/// E.g., `-moz-fit-content` -> `fit-content`, `-webkit-flex` -> `flex`
fn strip_vendor_prefix_from_value(value: &str) -> String {
  let trimmed = value.trim();
  // Match vendor prefix pattern at the start of the value
  if let Some(rest) = trimmed
    .strip_prefix("-webkit-")
    .or_else(|| trimmed.strip_prefix("-moz-"))
    .or_else(|| trimmed.strip_prefix("-ms-"))
    .or_else(|| trimmed.strip_prefix("-o-"))
  {
    return rest.to_string();
  }
  // Also handle case-insensitive
  let lower = trimmed.to_ascii_lowercase();
  if let Some(pos) = lower
    .find("-webkit-")
    .or_else(|| lower.find("-moz-"))
    .or_else(|| lower.find("-ms-"))
    .or_else(|| lower.find("-o-"))
  {
    if pos == 0 {
      let prefix_end = lower[pos..].find('-').unwrap() + 1;
      let prefix_end2 = lower[pos + prefix_end..].find('-').unwrap() + prefix_end + 1;
      return trimmed[prefix_end2..].to_string();
    }
  }
  trimmed.to_string()
}

/// Check if a value uses "standard" CSS syntax (not SCSS variables, interpolation, etc.)
fn is_standard_syntax_value(value: &str) -> bool {
  let mut v = value.trim();
  // Ignore operators before variables
  if v.starts_with('-') || v.starts_with('+') || v.starts_with('*') || v.starts_with('/') {
    v = &v[1..];
  }
  // SCSS variable
  if v.starts_with('$') {
    return false;
  }
  // Less variable
  if v.starts_with('@') {
    return false;
  }
  // SCSS interpolation
  if v.contains("#{") {
    return false;
  }
  // Less interpolation
  if v.contains("@{") {
    return false;
  }
  // Styled-component interpolation
  if v.contains("${") {
    return false;
  }
  // Underscore-prefixed SCSS variable references like _$a
  if v.starts_with('_') && v.contains('$') {
    return false;
  }
  true
}

/// Parsed value token for syntax comparison
#[derive(Debug, PartialEq)]
enum ValueToken {
  /// A number with optional unit (e.g., "100vw", "16px", "1rem")
  Dimension { unit: String },
  /// A percentage value
  Percentage,
  /// A plain number
  Number,
  /// A function call with name and nested tokens (e.g., "calc(...)")
  Function {
    name: String,
    children: Vec<ValueToken>,
  },
  /// An identifier/keyword (e.g., "red", "fit-content")
  Ident(String),
  /// A string literal
  StringLiteral,
  /// A URL token
  Url,
  /// Whitespace
  Whitespace,
  /// An operator like +, -, *, /
  Operator(char),
  /// A comma separator
  Comma,
  /// A hash/color value
  Hash,
  /// Unknown/other
  Other(String),
}

/// Tokenize a CSS value into structural tokens for syntax comparison.
fn tokenize_value(value: &str) -> Vec<ValueToken> {
  let mut tokens = Vec::new();
  let input = value.trim();
  let chars: Vec<char> = input.chars().collect();
  let mut i = 0;

  while i < chars.len() {
    let ch = chars[i];

    // Skip whitespace
    if ch.is_ascii_whitespace() {
      i += 1;
      // Don't add whitespace tokens - we compare structure only
      continue;
    }

    // Skip CSS comments
    if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
      i += 2;
      while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
        i += 1;
      }
      i += 2; // skip */
      continue;
    }

    // Comma
    if ch == ',' {
      tokens.push(ValueToken::Comma);
      i += 1;
      continue;
    }

    // Operators (when standalone, not part of a number)
    if ch == '+' || ch == '*' || ch == '/' {
      tokens.push(ValueToken::Operator(ch));
      i += 1;
      continue;
    }

    // Hash/color
    if ch == '#' {
      i += 1;
      while i < chars.len() && (chars[i].is_ascii_alphanumeric()) {
        i += 1;
      }
      tokens.push(ValueToken::Hash);
      continue;
    }

    // String literal
    if ch == '"' || ch == '\'' {
      let quote = ch;
      i += 1;
      while i < chars.len() && chars[i] != quote {
        if chars[i] == '\\' {
          i += 1;
        }
        i += 1;
      }
      if i < chars.len() {
        i += 1; // closing quote
      }
      tokens.push(ValueToken::StringLiteral);
      continue;
    }

    // Number (possibly with unit) or dimension
    // Also handle negative numbers
    if ch.is_ascii_digit()
      || ch == '.'
      || (ch == '-'
        && i + 1 < chars.len()
        && (chars[i + 1].is_ascii_digit() || chars[i + 1] == '.'))
    {
      let start = i;
      if ch == '-' {
        i += 1;
      }
      // Integer/decimal part
      while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
        i += 1;
      }
      // Check for unit
      let unit_start = i;
      while i < chars.len()
        && (chars[i].is_ascii_alphabetic() || chars[i] == '-' || chars[i] == '%')
      {
        i += 1;
      }
      if i > unit_start {
        let unit: String = chars[unit_start..i].iter().collect();
        if unit == "%" {
          tokens.push(ValueToken::Percentage);
        } else {
          tokens.push(ValueToken::Dimension {
            unit: unit.to_ascii_lowercase(),
          });
        }
      } else {
        tokens.push(ValueToken::Number);
      }
      continue;
    }

    // Identifier or function
    if ch.is_ascii_alphabetic() || ch == '-' || ch == '_' {
      let start = i;
      while i < chars.len()
        && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
      {
        i += 1;
      }
      let name: String = chars[start..i].iter().collect();

      // Check if it's a function call
      if i < chars.len() && chars[i] == '(' {
        let lower_name = name.to_ascii_lowercase();
        i += 1; // skip (
        // Find matching closing paren (handle nesting)
        let mut depth = 1;
        let args_start = i;
        while i < chars.len() && depth > 0 {
          if chars[i] == '(' {
            depth += 1;
          } else if chars[i] == ')' {
            depth -= 1;
          }
          if depth > 0 {
            i += 1;
          }
        }
        let args: String = chars[args_start..i].iter().collect();
        if i < chars.len() {
          i += 1; // skip )
        }
        let children = tokenize_value(&args);
        tokens.push(ValueToken::Function {
          name: lower_name,
          children,
        });
      } else {
        tokens.push(ValueToken::Ident(name.to_ascii_lowercase()));
      }
      continue;
    }

    // Parentheses (standalone, not part of a function)
    if ch == '(' {
      i += 1;
      let mut depth = 1;
      let inner_start = i;
      while i < chars.len() && depth > 0 {
        if chars[i] == '(' {
          depth += 1;
        } else if chars[i] == ')' {
          depth -= 1;
        }
        if depth > 0 {
          i += 1;
        }
      }
      if i < chars.len() {
        i += 1;
      }
      continue;
    }

    // Skip anything else
    i += 1;
    tokens.push(ValueToken::Other(ch.to_string()));
  }

  tokens
}

/// Check if two sets of value tokens represent "equal" syntaxes.
/// This is the core of Stylelint's `isEqualValueNodes`.
fn is_equal_value_tokens(tokens1: &[ValueToken], tokens2: &[ValueToken], property: &str) -> bool {
  // Different lengths indicate different syntaxes
  if tokens1.len() != tokens2.len() {
    return false;
  }

  for (t1, t2) in tokens1.iter().zip(tokens2.iter()) {
    match (t1, t2) {
      (ValueToken::Dimension { unit: u1 }, ValueToken::Dimension { unit: u2 }) => {
        if u1 != u2 {
          return false;
        }
      }
      (ValueToken::Percentage, ValueToken::Percentage) => {}
      (ValueToken::Number, ValueToken::Number) => {}
      (
        ValueToken::Function {
          name: n1,
          children: c1,
        },
        ValueToken::Function {
          name: n2,
          children: c2,
        },
      ) => {
        if n1 != n2 {
          return false;
        }
        if !is_equal_value_tokens(c1, c2, property) {
          return false;
        }
      }
      (ValueToken::Ident(name1), ValueToken::Ident(name2)) => {
        // Named colors have the same syntax for color properties
        if is_color_property(property) && is_named_color(name1) && is_named_color(name2) {
          continue;
        }
        if name1 != name2 {
          return false;
        }
      }
      (ValueToken::StringLiteral, ValueToken::StringLiteral) => {}
      (ValueToken::Hash, ValueToken::Hash) => {}
      (ValueToken::Url, ValueToken::Url) => {}
      (ValueToken::Whitespace, ValueToken::Whitespace) => {}
      (ValueToken::Operator(o1), ValueToken::Operator(o2)) => {
        if o1 != o2 {
          return false;
        }
      }
      (ValueToken::Comma, ValueToken::Comma) => {}
      // Different token types = different syntaxes
      _ => {
        // Check for special cases: Dimension vs Percentage, Ident vs Function, etc.
        // These are different syntaxes
        return false;
      }
    }
  }

  true
}

/// Check if two CSS values have equal syntaxes.
/// Used for `consecutive-duplicates-with-different-syntaxes`.
fn is_equal_value_syntaxes(value1: &str, value2: &str, property: &str) -> bool {
  if value1 == value2 {
    return true;
  }

  // Non-standard syntax values (SCSS vars, interpolation) are never equal
  if !is_standard_syntax_value(value1) || !is_standard_syntax_value(value2) {
    return false;
  }

  let tokens1 = tokenize_value(value1);
  let tokens2 = tokenize_value(value2);

  is_equal_value_tokens(&tokens1, &tokens2, property)
}

/// Check if a property is a color property
fn is_color_property(prop: &str) -> bool {
  matches!(
    prop,
    "color"
      | "background-color"
      | "border-color"
      | "border-top-color"
      | "border-right-color"
      | "border-bottom-color"
      | "border-left-color"
      | "border-block-color"
      | "border-block-start-color"
      | "border-block-end-color"
      | "border-inline-color"
      | "border-inline-start-color"
      | "border-inline-end-color"
      | "outline-color"
      | "text-decoration-color"
      | "text-emphasis-color"
      | "column-rule-color"
      | "caret-color"
      | "accent-color"
      | "flood-color"
      | "lighting-color"
      | "stop-color"
      | "scrollbar-color"
  )
}

/// Check if a value is a CSS named color keyword
fn is_named_color(name: &str) -> bool {
  matches!(
    name,
    "aliceblue"
      | "antiquewhite"
      | "aqua"
      | "aquamarine"
      | "azure"
      | "beige"
      | "bisque"
      | "black"
      | "blanchedalmond"
      | "blue"
      | "blueviolet"
      | "brown"
      | "burlywood"
      | "cadetblue"
      | "chartreuse"
      | "chocolate"
      | "coral"
      | "cornflowerblue"
      | "cornsilk"
      | "crimson"
      | "cyan"
      | "darkblue"
      | "darkcyan"
      | "darkgoldenrod"
      | "darkgray"
      | "darkgreen"
      | "darkgrey"
      | "darkkhaki"
      | "darkmagenta"
      | "darkolivegreen"
      | "darkorange"
      | "darkorchid"
      | "darkred"
      | "darksalmon"
      | "darkseagreen"
      | "darkslateblue"
      | "darkslategray"
      | "darkslategrey"
      | "darkturquoise"
      | "darkviolet"
      | "deeppink"
      | "deepskyblue"
      | "dimgray"
      | "dimgrey"
      | "dodgerblue"
      | "firebrick"
      | "floralwhite"
      | "forestgreen"
      | "fuchsia"
      | "gainsboro"
      | "ghostwhite"
      | "gold"
      | "goldenrod"
      | "gray"
      | "green"
      | "greenyellow"
      | "grey"
      | "honeydew"
      | "hotpink"
      | "indianred"
      | "indigo"
      | "ivory"
      | "khaki"
      | "lavender"
      | "lavenderblush"
      | "lawngreen"
      | "lemonchiffon"
      | "lightblue"
      | "lightcoral"
      | "lightcyan"
      | "lightgoldenrodyellow"
      | "lightgray"
      | "lightgreen"
      | "lightgrey"
      | "lightpink"
      | "lightsalmon"
      | "lightseagreen"
      | "lightskyblue"
      | "lightslategray"
      | "lightslategrey"
      | "lightsteelblue"
      | "lightyellow"
      | "lime"
      | "limegreen"
      | "linen"
      | "magenta"
      | "maroon"
      | "mediumaquamarine"
      | "mediumblue"
      | "mediumorchid"
      | "mediumpurple"
      | "mediumseagreen"
      | "mediumslateblue"
      | "mediumspringgreen"
      | "mediumturquoise"
      | "mediumvioletred"
      | "midnightblue"
      | "mintcream"
      | "mistyrose"
      | "moccasin"
      | "navajowhite"
      | "navy"
      | "oldlace"
      | "olive"
      | "olivedrab"
      | "orange"
      | "orangered"
      | "orchid"
      | "palegoldenrod"
      | "palegreen"
      | "paleturquoise"
      | "palevioletred"
      | "papayawhip"
      | "peachpuff"
      | "peru"
      | "pink"
      | "plum"
      | "powderblue"
      | "purple"
      | "rebeccapurple"
      | "red"
      | "rosybrown"
      | "royalblue"
      | "saddlebrown"
      | "salmon"
      | "sandybrown"
      | "seagreen"
      | "seashell"
      | "sienna"
      | "silver"
      | "skyblue"
      | "slateblue"
      | "slategray"
      | "slategrey"
      | "snow"
      | "springgreen"
      | "steelblue"
      | "tan"
      | "teal"
      | "thistle"
      | "tomato"
      | "transparent"
      | "turquoise"
      | "violet"
      | "wheat"
      | "white"
      | "whitesmoke"
      | "yellow"
      | "yellowgreen"
  )
}

/// Split a property name into its vendor prefix and unprefixed name.
/// E.g., `-webkit-transform` -> (`-webkit-`, `transform`)
///       `color` -> (``, `color`)
fn split_vendor_prefix_from_prop(prop: &str) -> (&str, &str) {
  static PREFIXES: &[&str] = &["-webkit-", "-moz-", "-ms-", "-o-"];
  for prefix in PREFIXES {
    if prop.starts_with(prefix) {
      return (&prop[..prefix.len()], &prop[prefix.len()..]);
    }
  }
  ("", prop)
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
  fn reports_duplicate_properties() {
    let rule = DeclarationBlockNoDuplicateProperties;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(4, 10),
          important: false,
        },
        Declaration {
          property: "display".to_string(),
          value: "block".to_string(),
          span: ParserSpan::new(15, 14),
          important: false,
        },
        Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(30, 11),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 45),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "Unexpected duplicate \"color\"");
  }

  #[test]
  fn ignores_unique_properties() {
    let rule = DeclarationBlockNoDuplicateProperties;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(4, 10),
          important: false,
        },
        Declaration {
          property: "display".to_string(),
          value: "block".to_string(),
          span: ParserSpan::new(15, 14),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 30),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn case_insensitive_detection() {
    let rule = DeclarationBlockNoDuplicateProperties;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        Declaration {
          property: "Color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(4, 10),
          important: false,
        },
        Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(15, 11),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 30),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn flags_consecutive_duplicates_with_different_values_by_default() {
    // Default behavior: flag all duplicates, even consecutive with different values
    let rule = DeclarationBlockNoDuplicateProperties;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        Declaration {
          property: "color".to_string(),
          value: "pink".to_string(),
          span: ParserSpan::new(4, 11),
          important: false,
        },
        Declaration {
          property: "color".to_string(),
          value: "orange".to_string(),
          span: ParserSpan::new(16, 13),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 30),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(
      diags.len(),
      1,
      "consecutive duplicates with different values should be flagged by default"
    );
  }

  #[test]
  fn flags_non_consecutive_duplicates_with_different_values() {
    let rule = DeclarationBlockNoDuplicateProperties;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        Declaration {
          property: "color".to_string(),
          value: "red".to_string(),
          span: ParserSpan::new(4, 10),
          important: false,
        },
        Declaration {
          property: "display".to_string(),
          value: "block".to_string(),
          span: ParserSpan::new(15, 14),
          important: false,
        },
        Declaration {
          property: "color".to_string(),
          value: "blue".to_string(),
          span: ParserSpan::new(30, 11),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 45),
      ..Default::default()
    });
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn test_tokenize_value() {
    let tokens = tokenize_value("100vw");
    assert_eq!(tokens.len(), 1);
    assert!(matches!(&tokens[0], ValueToken::Dimension { unit } if unit == "vw"));

    let tokens = tokenize_value("100dvw");
    assert_eq!(tokens.len(), 1);
    assert!(matches!(&tokens[0], ValueToken::Dimension { unit } if unit == "dvw"));
  }

  #[test]
  fn test_equal_value_syntaxes_different_units() {
    assert!(!is_equal_value_syntaxes("100vw", "100dvw", "width"));
    assert!(is_equal_value_syntaxes("100vw", "100vw", "width"));
    assert!(is_equal_value_syntaxes("100vw", "50vw", "width"));
  }

  #[test]
  fn test_equal_value_syntaxes_different_functions() {
    assert!(!is_equal_value_syntaxes(
      "min(10px, 11px)",
      "max(10px, 11px)",
      "width"
    ));
    assert!(!is_equal_value_syntaxes("100%", "fit-content", "width"));
  }

  #[test]
  fn ignore_option_accepts_single_string() {
    // The `ignore` option should accept a single string, not just an array
    let rule = DeclarationBlockNoDuplicateProperties;
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![
        Declaration {
          property: "color".to_string(),
          value: "pink".to_string(),
          span: ParserSpan::new(4, 11),
          important: false,
        },
        Declaration {
          property: "color".to_string(),
          value: "orange".to_string(),
          span: ParserSpan::new(16, 13),
          important: false,
        },
      ],
      span: ParserSpan::new(0, 30),
      ..Default::default()
    });
    let opts = serde_json::json!({
        "ignore": "consecutive-duplicates-with-different-values"
    });
    let ctx = RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(&opts),
    };
    let diags = rule.check(&node, &ctx);
    assert!(
      diags.is_empty(),
      "consecutive duplicates with different values should be allowed when ignore is a single string; got {} diagnostics",
      diags.len()
    );
  }
}
