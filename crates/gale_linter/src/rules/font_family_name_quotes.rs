use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow quotes for font family names.
///
/// Options:
/// - `"always-where-recommended"` (default): names that need quoting per CSS
///   spec must be quoted; generic family keywords must NOT be quoted.
/// - `"always-unless-keyword"`: every non-keyword family must be quoted.
/// - `"always-where-required"`: only names that MUST be quoted (contain
///   whitespace/special chars) need quotes; single identifiers are fine
///   unquoted.
///
/// Equivalent to Stylelint's `font-family-name-quotes` rule.
pub struct FontFamilyNameQuotes;

/// Generic font families that should never be quoted.
const GENERIC_FAMILIES: &[&str] = &[
  "serif",
  "sans-serif",
  "monospace",
  "cursive",
  "fantasy",
  "system-ui",
  "ui-serif",
  "ui-sans-serif",
  "ui-monospace",
  "ui-rounded",
  "emoji",
  "math",
  "fangsong",
  "inherit",
  "initial",
  "unset",
  "revert",
  "revert-layer",
];

/// System font keywords and vendor-prefixed identifiers that should not be quoted.
const SYSTEM_FONT_KEYWORDS: &[&str] = &["-apple-system", "blinkmacsystemfont"];

fn is_generic_family(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  GENERIC_FAMILIES.iter().any(|g| *g == lower)
}

fn is_system_font_keyword(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  SYSTEM_FONT_KEYWORDS.iter().any(|g| *g == lower)
}

fn is_keyword_font(name: &str) -> bool {
  is_generic_family(name) || is_system_font_keyword(name) || is_vendor_prefixed_keyword(name)
}

/// Returns true if the name is a vendor-prefixed keyword (e.g., `-webkit-control`, `-moz-button`).
fn is_vendor_prefixed_keyword(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  lower.starts_with("-webkit-")
    || lower.starts_with("-moz-")
    || lower.starts_with("-ms-")
    || lower.starts_with("-o-")
}

/// Returns true if a font family name STRICTLY requires quoting per CSS spec.
/// In `always-where-required` mode, whitespace alone doesn't require quoting
/// (CSS parsers handle multi-word font names). Only names with characters
/// that can't be part of a CSS custom-ident require quoting.
fn strictly_requires_quoting(name: &str) -> bool {
  let trimmed = name.trim();
  if trimmed.is_empty() {
    return false;
  }
  // Starts with a digit — not a valid CSS ident start
  if trimmed.starts_with(|c: char| c.is_ascii_digit()) {
    return true;
  }
  // Contains special punctuation like `/`, `!` that can't be in idents
  // (but allow hyphens, underscores, and whitespace between words)
  if trimmed
    .contains(|c: char| !c.is_alphanumeric() && c != '-' && c != '_' && !c.is_ascii_whitespace())
  {
    return true;
  }
  // Non-ASCII characters
  if trimmed.contains(|c: char| !c.is_ascii()) {
    return true;
  }
  // Contains digits (e.g. "Hawaii 5-0")
  if trimmed.chars().any(|c| c.is_ascii_digit()) {
    return true;
  }
  false
}

/// Returns true if a font family name needs quoting per CSS spec recommendations.
fn needs_quoting_recommended(name: &str) -> bool {
  let trimmed = name.trim();
  if trimmed.is_empty() {
    return false;
  }
  // Whitespace
  if trimmed.contains(|c: char| c.is_ascii_whitespace()) {
    return true;
  }
  // Starts with a digit
  if trimmed.starts_with(|c: char| c.is_ascii_digit()) {
    return true;
  }
  // Contains special chars (not alphanumeric, hyphen, or underscore)
  if trimmed.contains(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_') {
    return true;
  }
  // Contains digits anywhere (e.g. "Something6")
  if trimmed.chars().any(|c| c.is_ascii_digit()) {
    return true;
  }
  // Contains underscores
  if trimmed.contains('_') {
    return true;
  }
  // Non-ASCII characters
  if trimmed.contains(|c: char| !c.is_ascii()) {
    return true;
  }
  false
}

/// A font family token parsed from the source text.
struct FontFamilyToken {
  name: String,
  quoted: bool,
  /// Byte offset within the source text (absolute).
  abs_offset: usize,
  /// Length of the token in the source (including quotes if present).
  src_length: usize,
}

/// Parse font family names directly from the source text starting at `start_offset`.
/// This avoids issues with lightningcss normalizing/stripping quotes.
fn parse_font_families_from_source(
  source: &str,
  start_offset: usize,
  end_offset: usize,
) -> Vec<FontFamilyToken> {
  let mut families = Vec::new();
  let region = &source[start_offset..end_offset.min(source.len())];
  let bytes = region.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Skip whitespace and commas
    while i < len && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
      i += 1;
    }
    if i >= len {
      break;
    }

    if bytes[i] == b'"' || bytes[i] == b'\'' {
      // Quoted name
      let quote = bytes[i];
      let start = i;
      i += 1;
      let name_start = i;
      while i < len {
        if bytes[i] == b'\\' {
          i += 2;
          continue;
        }
        if bytes[i] == quote {
          break;
        }
        i += 1;
      }
      let name = &region[name_start..i];
      if i < len {
        i += 1; // skip closing quote
      }
      families.push(FontFamilyToken {
        name: name.to_string(),
        quoted: true,
        abs_offset: start_offset + start,
        src_length: i - start,
      });
    } else if bytes[i] == b';' || bytes[i] == b'}' {
      // End of declaration value
      break;
    } else if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
      // Skip block comments inside the value (e.g. `/*rtl:...*/`).
      i += 2;
      while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
        i += 1;
      }
      i += 2; // skip */
    } else {
      // Unquoted name — spans multiple words until comma, semicolon, or comment end.
      // Track paren depth so function calls like var(...) are consumed as a unit
      // and the closing `)` is never included in the family name.
      let start = i;
      let mut paren_depth: usize = 0;
      while i < len {
        match bytes[i] {
          b'(' => paren_depth += 1,
          b')' => {
            if paren_depth > 0 {
              paren_depth -= 1;
            } else {
              break; // unmatched closing paren — stop
            }
          }
          b',' | b';' | b'}' if paren_depth == 0 => break,
          _ => {
            // Stop at block comments (e.g. `serif/*rtl:...*/`)
            if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
              break;
            }
            // Stop at `!important`
            if bytes[i] == b'!' {
              let rest = &region[i..];
              if rest.to_ascii_lowercase().starts_with("!important") {
                break;
              }
            }
          }
        }
        i += 1;
      }
      let raw = region[start..i].trim();
      if !raw.is_empty() {
        // Compute offset to the trimmed content
        let trim_start = region[start..i]
          .find(|c: char| !c.is_ascii_whitespace())
          .unwrap_or(0);
        families.push(FontFamilyToken {
          name: raw.to_string(),
          quoted: false,
          abs_offset: start_offset + start + trim_start,
          src_length: raw.len(),
        });
      } else if i == start {
        // No progress was made (e.g. we immediately hit `!important`
        // or another break condition at the very start of the token).
        // Advance past the current byte to avoid an infinite loop.
        i += 1;
      }
    }
  }

  families
}

/// Find where the value starts in the source after the property name and colon.
fn find_value_start(source: &str, decl_offset: usize, property_len: usize) -> usize {
  let start = decl_offset + property_len;
  if start >= source.len() {
    return start;
  }
  let rest = &source[start..];
  let mut off = 0;
  let bytes = rest.as_bytes();
  while off < bytes.len() && (bytes[off] == b':' || bytes[off].is_ascii_whitespace()) {
    off += 1;
  }
  start + off
}

/// Find the end of the declaration value in the source (semicolon or closing brace).
fn find_value_end(source: &str, value_start: usize) -> usize {
  let rest = &source[value_start..];
  // Find ; or } while respecting quotes and parentheses
  let bytes = rest.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  let mut paren_depth = 0;
  while i < len {
    match bytes[i] {
      b'(' => paren_depth += 1,
      b')' if paren_depth > 0 => paren_depth -= 1,
      b'"' | b'\'' if paren_depth == 0 => {
        let quote = bytes[i];
        i += 1;
        while i < len {
          if bytes[i] == b'\\' {
            i += 2;
            continue;
          }
          if bytes[i] == quote {
            break;
          }
          i += 1;
        }
      }
      b';' | b'}' if paren_depth == 0 => {
        return value_start + i;
      }
      _ => {}
    }
    i += 1;
  }
  value_start + len
}

/// For a `font` shorthand, find where the font-family portion starts.
/// The font shorthand is: [style] [variant] [weight] [stretch] size[/line-height] family
/// The family starts after the size (which contains a CSS length value).
fn find_font_family_start_in_source(
  source: &str,
  value_start: usize,
  value_end: usize,
) -> Option<usize> {
  let region = &source[value_start..value_end];
  let bytes = region.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  // Tokenize and find the size token
  while i < len {
    // Skip whitespace
    while i < len && bytes[i].is_ascii_whitespace() {
      i += 1;
    }
    if i >= len {
      break;
    }

    // Skip quoted strings
    if bytes[i] == b'"' || bytes[i] == b'\'' {
      let quote = bytes[i];
      i += 1;
      while i < len {
        if bytes[i] == b'\\' {
          i += 2;
          continue;
        }
        if bytes[i] == quote {
          i += 1;
          break;
        }
        i += 1;
      }
      continue;
    }

    // Read a token
    let token_start = i;
    while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b',' && bytes[i] != b';' {
      // Handle slash in size/line-height
      if bytes[i] == b'/' {
        i += 1;
        while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b',' {
          i += 1;
        }
        break;
      }
      i += 1;
    }

    let token = &region[token_start..i];
    let lower = token.to_ascii_lowercase();

    // Check if this looks like a font-size (number + unit or slash)
    let looks_like_size = lower
      .chars()
      .next()
      .map(|c| c.is_ascii_digit() || c == '.')
      .unwrap_or(false)
      && (lower.contains("px")
        || lower.contains("em")
        || lower.contains("rem")
        || lower.contains("pt")
        || lower.contains('%')
        || lower.contains("vh")
        || lower.contains("vw")
        || lower.contains("ex")
        || lower.contains("ch")
        || lower.contains('/'));

    if looks_like_size {
      // Family starts after the size token (skip whitespace)
      while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
      }
      return Some(value_start + i);
    }
  }

  None
}

impl Rule for FontFamilyNameQuotes {
  fn name(&self) -> &'static str {
    "font-family-name-quotes"
  }

  fn description(&self) -> &'static str {
    "Require or disallow quotes for font family names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let style = match node {
      CssNode::Style(s) => s,
      _ => return vec![],
    };

    let mode = ctx
      .primary_option_str()
      .unwrap_or("always-where-recommended");
    let source = ctx.source;
    let mut diagnostics = Vec::new();

    for decl in &style.declarations {
      let prop_lower = decl.property.to_ascii_lowercase();
      if prop_lower != "font-family" && prop_lower != "font" {
        continue;
      }

      if source.is_empty() || decl.span.length == 0 {
        // Without source or span, fall back to parsed value (less accurate)
        self.check_from_parsed_value(decl, mode, &mut diagnostics);
        continue;
      }

      // Use the original source text so that quote presence/absence is
      // preserved exactly as written. lightningcss normalises font-family
      // values and may strip quotes, producing false positives if we rely
      // on the parsed value.
      let value_start = find_value_start(source, decl.span.offset, decl.property.len());
      let value_end = find_value_end(source, value_start);

      // Determine the font-family portion for `font` shorthand.
      let (family_start, family_end) = if prop_lower == "font" {
        let fs =
          find_font_family_start_in_source(source, value_start, value_end).unwrap_or(value_start);
        (fs, value_end)
      } else {
        (value_start, value_end)
      };

      let families = parse_font_families_from_source(source, family_start, family_end);
      for family in families {
        // Skip SCSS variables, var(), interpolations, and other
        // non-standard constructs that Stylelint also skips.
        if family.name.starts_with('$')
          || family.name.starts_with("var(")
          || family.name.starts_with('@')
          || family.name.contains("#{")
          || family.name.contains('(')
          || family.name.contains(".$")
          || family.name.contains('{')
          || family.name.contains('}')
          || family.name.contains(';')
        {
          continue;
        }
        self.check_family(&family, mode, &mut diagnostics);
      }
    }

    diagnostics
  }
}

impl FontFamilyNameQuotes {
  fn check_family(&self, family: &FontFamilyToken, mode: &str, diagnostics: &mut Vec<Diagnostic>) {
    let is_kw = is_keyword_font(&family.name);

    match mode {
      "always-unless-keyword" => {
        if is_kw {
          if family.quoted {
            diagnostics.push(self.make_diag(
              format!("Unexpected quotes around \"{}\"", family.name),
              family,
            ));
          }
        } else if !family.quoted {
          diagnostics.push(self.make_diag(
            format!("Expected quotes around \"{}\"", family.name),
            family,
          ));
        }
      }
      "always-where-required" => {
        if is_kw {
          if family.quoted {
            diagnostics.push(self.make_diag(
              format!("Unexpected quotes around \"{}\"", family.name),
              family,
            ));
          }
        } else if !family.quoted && strictly_requires_quoting(&family.name) {
          diagnostics.push(self.make_diag(
            format!("Expected quotes around \"{}\"", family.name),
            family,
          ));
        } else if family.quoted && !strictly_requires_quoting(&family.name) {
          diagnostics.push(self.make_diag(
            format!("Unexpected quotes around \"{}\"", family.name),
            family,
          ));
        }
      }
      // "always-where-recommended" (default)
      _ => {
        if is_kw {
          if family.quoted {
            diagnostics.push(self.make_diag(
              format!("Unexpected quotes around \"{}\"", family.name),
              family,
            ));
          }
        } else if !family.quoted && needs_quoting_recommended(&family.name) {
          diagnostics.push(self.make_diag(
            format!("Expected quotes around \"{}\"", family.name),
            family,
          ));
        } else if family.quoted && !needs_quoting_recommended(&family.name) {
          // Unnecessarily quoted single-word name like "Arial"
          diagnostics.push(self.make_diag(
            format!("Unexpected quotes around \"{}\"", family.name),
            family,
          ));
        }
      }
    }
  }

  fn make_diag(&self, message: String, family: &FontFamilyToken) -> Diagnostic {
    Diagnostic::new(self.name(), message)
      .severity(self.default_severity())
      .span(Span::new(family.abs_offset, family.src_length))
  }

  /// Fallback: check from the parsed value when source is not available.
  fn check_from_parsed_value(
    &self,
    decl: &gale_css_parser::Declaration,
    mode: &str,
    diagnostics: &mut Vec<Diagnostic>,
  ) {
    let value = &decl.value;
    let prop_lower = decl.property.to_ascii_lowercase();

    // For `font` shorthand, extract only the font-family portion.
    let family_value = if prop_lower == "font" {
      extract_font_families_from_shorthand(value)
    } else {
      value.to_string()
    };

    let families = parse_font_families_from_value(&family_value);
    for (name, quoted) in families {
      if name.starts_with('$')
                || name.starts_with("var(")
                || name.starts_with('@')
                || name.contains("#{")
                || name.contains('(')
                // Skip SCSS namespace references like `variables.$font-family-body`
                || name.contains(".$")
                // Skip values that look like SCSS nested property syntax
                // (e.g. `font: { size: 0.8em; style: italic; }`)
                || name.contains('{')
                || name.contains('}')
                || name.contains(';')
      {
        continue;
      }
      let family = FontFamilyToken {
        name: name.clone(),
        quoted,
        abs_offset: decl.span.offset,
        src_length: decl.span.length,
      };
      self.check_family(&family, mode, diagnostics);
    }
  }
}

/// Extract the font-family portion from a `font` shorthand value.
/// Format: [style] [variant] [weight] [stretch] size[/line-height] family[, family]*
/// The family starts after the size/line-height token.
fn extract_font_families_from_shorthand(value: &str) -> String {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  loop {
    // Skip whitespace
    while i < len && bytes[i].is_ascii_whitespace() {
      i += 1;
    }
    if i >= len {
      return String::new();
    }

    // Read a token
    let token_start = i;
    while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b',' {
      if bytes[i] == b'/' {
        // size/line-height: consume rest of compound token
        i += 1;
        while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b',' {
          i += 1;
        }
        break;
      }
      i += 1;
    }

    let token = &value[token_start..i];

    // Check if this looks like a font-size (starts with digit, or contains `/`)
    let first_char = token.chars().next().unwrap_or(' ');
    let is_size = (first_char.is_ascii_digit() || first_char == '.')
      && (token.contains("px")
        || token.contains("em")
        || token.contains("rem")
        || token.contains("pt")
        || token.contains('%')
        || token.contains("vh")
        || token.contains("vw")
        || token.contains('/')
        || token.ends_with("0"));

    if is_size {
      // Everything after this is the font-family
      while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
      }
      return value[i..].to_string();
    }
  }
}

/// Simple value-based parsing (fallback when source unavailable).
fn parse_font_families_from_value(value: &str) -> Vec<(String, bool)> {
  let mut families = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  while i < len {
    while i < len && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
      i += 1;
    }
    if i >= len {
      break;
    }
    if bytes[i] == b'"' || bytes[i] == b'\'' {
      let quote = bytes[i];
      i += 1;
      let start = i;
      while i < len {
        if bytes[i] == b'\\' {
          i += 2;
          continue;
        }
        if bytes[i] == quote {
          break;
        }
        i += 1;
      }
      families.push((value[start..i].to_string(), true));
      if i < len {
        i += 1;
      }
    } else {
      let start = i;
      // Track paren depth to skip function calls like var(), env(), etc.
      let mut paren_depth = 0;
      while i < len {
        if bytes[i] == b'(' {
          paren_depth += 1;
        } else if bytes[i] == b')' {
          if paren_depth > 0 {
            paren_depth -= 1;
          } else {
            // Unmatched closing paren — stop here
            break;
          }
        } else if bytes[i] == b',' && paren_depth == 0 {
          break;
        }
        i += 1;
      }
      let raw = value[start..i].trim();
      if !raw.is_empty() {
        families.push((raw.to_string(), false));
      }
    }
  }
  families
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

  fn make_node(property: &str, value: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: property.to_string(),
        value: value.to_string(),
        span: ParserSpan::new(4, value.len() + property.len() + 2),
        important: false,
      }],
      span: ParserSpan::new(0, value.len() + property.len() + 20),
      ..Default::default()
    })
  }

  #[test]
  fn flags_unquoted_family_with_space() {
    let rule = FontFamilyNameQuotes;
    let node = make_node("font-family", "Times New Roman, serif");
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("Times New Roman"));
  }

  #[test]
  fn allows_quoted_family_with_space() {
    let rule = FontFamilyNameQuotes;
    let node = make_node("font-family", "\"Times New Roman\", serif");
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn allows_unquoted_single_word() {
    let rule = FontFamilyNameQuotes;
    let node = make_node("font-family", "Arial, sans-serif");
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn flags_quoted_generic_family() {
    let rule = FontFamilyNameQuotes;
    let node = make_node("font-family", "\"serif\"");
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("Unexpected quotes around"));
  }

  #[test]
  fn allows_unquoted_generic_family() {
    let rule = FontFamilyNameQuotes;
    let node = make_node("font-family", "monospace");
    let diags = rule.check(&node, &make_context());
    assert!(diags.is_empty());
  }

  #[test]
  fn flags_family_starting_with_digit() {
    let rule = FontFamilyNameQuotes;
    let node = make_node("font-family", "1234, sans-serif");
    let diags = rule.check(&node, &make_context());
    assert_eq!(diags.len(), 1);
  }

  #[test]
  fn font_shorthand_extracts_family_only() {
    let rule = FontFamilyNameQuotes;
    // `font: normal 30px/1 dashicons` — "dashicons" is a single-word family, no quoting needed
    let node = make_node("font", "normal 30px/1 dashicons");
    let diags = rule.check(&node, &make_context());
    assert!(
      diags.is_empty(),
      "single-word font family in shorthand should not be flagged: {:?}",
      diags
    );
  }

  #[test]
  fn is_generic_family_case_insensitive() {
    assert!(is_generic_family("Sans-Serif"));
    assert!(is_generic_family("MONOSPACE"));
    assert!(!is_generic_family("Arial"));
  }
}
