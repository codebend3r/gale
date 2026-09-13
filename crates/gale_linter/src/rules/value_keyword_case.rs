use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::stylelint_version::stylelint_major_version;

/// Enforce consistent case for keyword values.
///
/// Equivalent to Stylelint's `value-keyword-case` rule.
/// Supports `"lower"` (default) and `"upper"` primary options.
///
/// Secondary options:
/// - `ignoreKeywords`: array of keyword strings or regex patterns to ignore
/// - `ignoreProperties`: array of property strings or regex patterns to ignore
/// - `ignoreFunctions`: array of function strings or regex patterns to ignore
/// - `camelCaseSvgKeywords`: bool -- when true, allow SVG camelCase keywords like `currentColor`
pub struct ValueKeywordCase;

// ---------------------------------------------------------------------------
// Data tables
// ---------------------------------------------------------------------------

/// CSS2 system colors (deprecated) — used by Stylelint ≤13.
const SYSTEM_COLORS_CSS2: &[&str] = &[
  "ActiveBorder",
  "ActiveCaption",
  "AppWorkspace",
  "Background",
  "ButtonFace",
  "ButtonHighlight",
  "ButtonShadow",
  "ButtonText",
  "CaptionText",
  "GrayText",
  "Highlight",
  "HighlightText",
  "InactiveBorder",
  "InactiveCaption",
  "InactiveCaptionText",
  "InfoBackground",
  "InfoText",
  "Menu",
  "MenuText",
  "Scrollbar",
  "ThreeDDarkShadow",
  "ThreeDFace",
  "ThreeDHighlight",
  "ThreeDLightShadow",
  "ThreeDShadow",
  "Window",
  "WindowFrame",
  "WindowText",
];

/// CSS Color Level 4 system colors — added in Stylelint 14+.
const SYSTEM_COLORS_CSS4: &[&str] = &[
  "AccentColor",
  "AccentColorText",
  "ActiveText",
  "ButtonBorder",
  "Canvas",
  "CanvasText",
  "Field",
  "FieldText",
  "LinkText",
  "Mark",
  "MarkText",
  "SelectedItem",
  "SelectedItemText",
  "VisitedText",
];

fn is_system_color(s: &str) -> bool {
  let v = stylelint_major_version();
  if SYSTEM_COLORS_CSS2.iter().any(|c| c.eq_ignore_ascii_case(s)) {
    return true;
  }
  if v >= 14 && SYSTEM_COLORS_CSS4.iter().any(|c| c.eq_ignore_ascii_case(s)) {
    return true;
  }
  false
}

/// SVG keywords that have camelCase canonical forms.
const SVG_CAMEL_CASE_KEYWORDS: &[&str] = &[
  "currentColor",
  "optimizeSpeed",
  "optimizeLegibility",
  "optimizeQuality",
  "crispEdges",
  "geometricPrecision",
  "visiblePainted",
  "visibleFill",
  "visibleStroke",
  "sRGB",
  "linearRGB",
];

fn is_svg_camel_case_keyword(s: &str) -> bool {
  SVG_CAMEL_CASE_KEYWORDS
    .iter()
    .any(|k| k.eq_ignore_ascii_case(s))
}

/// Properties whose values are entirely custom identifiers.
const CUSTOM_IDENT_PROPERTIES: &[&str] = &[
  "animation-name",
  "counter-increment",
  "counter-reset",
  "counter-set",
  "grid-row",
  "grid-column",
  "grid-area",
  "grid-row-start",
  "grid-row-end",
  "grid-column-start",
  "grid-column-end",
  "list-style-type",
  "will-change",
];

fn is_custom_ident_property(prop: &str) -> bool {
  let lower = prop.to_ascii_lowercase();
  let stripped = strip_vendor_prefix(&lower);
  CUSTOM_IDENT_PROPERTIES.contains(&stripped)
}

/// Properties where some positions are keywords and some are custom idents.
const MIXED_IDENT_PROPERTIES: &[&str] = &["animation", "font", "font-family", "list-style"];

fn is_mixed_ident_property(prop: &str) -> bool {
  let lower = prop.to_ascii_lowercase();
  let stripped = strip_vendor_prefix(&lower);
  MIXED_IDENT_PROPERTIES.contains(&stripped)
}

/// Generic font family names (these ARE keywords in font-family/font).
const GENERIC_FONT_FAMILIES: &[&str] = &[
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
];

fn is_generic_font_family(s: &str) -> bool {
  let lower = s.to_ascii_lowercase();
  GENERIC_FONT_FAMILIES.iter().any(|f| *f == lower)
}

/// Global CSS keywords.
const GLOBAL_KEYWORDS: &[&str] = &["inherit", "initial", "unset", "revert", "revert-layer"];

fn is_global_keyword(s: &str) -> bool {
  let lower = s.to_ascii_lowercase();
  GLOBAL_KEYWORDS.iter().any(|k| *k == lower)
}

/// Known CSS list-style-type keywords (includes predefined counter styles).
const LIST_STYLE_TYPE_KEYWORDS: &[&str] = &[
  "none",
  "disc",
  "circle",
  "square",
  "decimal",
  "decimal-leading-zero",
  "lower-roman",
  "upper-roman",
  "lower-greek",
  "lower-latin",
  "upper-latin",
  "lower-alpha",
  "upper-alpha",
  "armenian",
  "georgian",
  "cjk-ideographic",
  "hiragana",
  "katakana",
  "hiragana-iroha",
  "katakana-iroha",
  // CSS Counter Styles Level 3 predefined counter styles
  "arabic-indic",
  "bengali",
  "cambodian",
  "cjk-decimal",
  "cjk-earthly-branch",
  "cjk-heavenly-stem",
  "devanagari",
  "ethiopic-numeric",
  "gujarati",
  "gurmukhi",
  "hebrew",
  "japanese-formal",
  "japanese-informal",
  "kannada",
  "khmer",
  "korean-hangul-formal",
  "korean-hanja-formal",
  "korean-hanja-informal",
  "lao",
  "malayalam",
  "mongolian",
  "myanmar",
  "oriya",
  "persian",
  "simp-chinese-formal",
  "simp-chinese-informal",
  "tamil",
  "telugu",
  "thai",
  "tibetan",
  "trad-chinese-formal",
  "trad-chinese-informal",
  "disclosure-closed",
  "disclosure-open",
  "ethiopic-halehame-ti-er",
  "ethiopic-halehame-ti-et",
  "ethiopic-halehame",
  "hangul",
  "hangul-consonant",
  "somali",
  "inherit",
  "initial",
  "unset",
  "revert",
  "revert-layer",
];

fn is_list_style_type_keyword(s: &str) -> bool {
  let lower = s.to_ascii_lowercase();
  LIST_STYLE_TYPE_KEYWORDS.iter().any(|k| *k == lower)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn strip_vendor_prefix(s: &str) -> &str {
  if (s.starts_with("-webkit-")
    || s.starts_with("-moz-")
    || s.starts_with("-ms-")
    || s.starts_with("-o-"))
    && let Some(pos) = s[1..].find('-')
  {
    return &s[pos + 2..];
  }
  s
}

fn matches_pattern(value: &str, pattern: &str) -> bool {
  if pattern.starts_with('/') && pattern.ends_with('/') {
    let re_str = &pattern[1..pattern.len() - 1];
    if let Ok(re) = regex::Regex::new(re_str) {
      re.is_match(value)
    } else {
      false
    }
  } else {
    value == pattern
  }
}

fn matches_pattern_case_insensitive(value: &str, pattern: &str) -> bool {
  if pattern.starts_with('/') && pattern.ends_with('/') {
    matches_pattern(value, pattern)
  } else {
    value.eq_ignore_ascii_case(pattern)
  }
}

// ---------------------------------------------------------------------------
// Value tokenizer
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ValueToken {
  text: String,
  offset: usize,
  kind: TokenKind,
}

#[derive(Debug, PartialEq)]
enum TokenKind {
  Ident,
  Function,
  String,
  UrlContent,
  Important,
  Other,
}

/// Tokenize a CSS value string, tracking function nesting.
fn tokenize_value(value: &str) -> Vec<ValueToken> {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut tokens = Vec::new();
  let mut i = 0;
  let mut func_stack: Vec<String> = Vec::new();

  while i < len {
    let b = bytes[i];

    // Whitespace
    if b.is_ascii_whitespace() {
      i += 1;
      continue;
    }

    // Punctuation that isn't meaningful
    if b == b','
      || b == b'/' && !(i + 1 < len && (bytes[i + 1] == b'*' || bytes[i + 1] == b'/'))
      || b == b'+'
      || b == b'*'
      || b == b'='
    {
      i += 1;
      continue;
    }

    // Opening paren without preceding ident
    if b == b'(' {
      func_stack.push(String::new());
      i += 1;
      continue;
    }

    // Closing paren
    if b == b')' {
      func_stack.pop();
      i += 1;
      continue;
    }

    // Square brackets (grid line names) -- skip contents
    if b == b'[' {
      while i < len && bytes[i] != b']' {
        i += 1;
      }
      if i < len {
        i += 1;
      }
      continue;
    }

    // String literal
    if b == b'"' || b == b'\'' {
      let quote = b;
      let start = i;
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
      tokens.push(ValueToken {
        text: value[start..i].to_string(),
        offset: start,
        kind: TokenKind::String,
      });
      continue;
    }

    // CSS comment /* ... */
    if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
      i += 2;
      while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
        i += 1;
      }
      if i + 1 < len {
        i += 2;
      }
      continue;
    }

    // SCSS/Less single-line comment.
    // Inside parenthesised expressions (SCSS maps), PostCSS-SCSS treats
    // `//` (double slash) as a comment, but `///` (triple slash, SassDoc)
    // is NOT treated as a comment — identifiers after `///` remain visible
    // to rules like value-keyword-case.
    if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
      let is_triple = i + 2 < len && bytes[i + 2] == b'/';
      // At top level: always skip as comment.
      // Inside parens: skip `//` (double) as comment, but NOT `///` (triple).
      if func_stack.is_empty() || !is_triple {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }
      // `///` inside parens: skip the three slashes so the remaining
      // `//` is not re-interpreted as a double-slash comment.
      i += 3;
      continue;
    }

    // Hash (color or ID, or SCSS interpolation)
    if b == b'#' {
      i += 1;
      if i < len && bytes[i] == b'{' {
        let mut depth = 1;
        i += 1;
        while i < len && depth > 0 {
          if bytes[i] == b'{' {
            depth += 1;
          } else if bytes[i] == b'}' {
            depth -= 1;
          }
          i += 1;
        }
        continue;
      }
      while i < len && bytes[i].is_ascii_alphanumeric() {
        i += 1;
      }
      continue;
    }

    // `!` -- `!important` keyword
    if b == b'!' {
      i += 1;
      while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
      }
      let start = i;
      while i < len && bytes[i].is_ascii_alphanumeric() {
        i += 1;
      }
      if i > start {
        tokens.push(ValueToken {
          text: value[start..i].to_string(),
          offset: start,
          kind: TokenKind::Important,
        });
      }
      continue;
    }

    // Number (possibly with unit)
    if b.is_ascii_digit() || (b == b'.' && i + 1 < len && bytes[i + 1].is_ascii_digit()) {
      while i < len
        && (bytes[i].is_ascii_digit()
          || bytes[i] == b'.'
          || bytes[i] == b'e'
          || bytes[i] == b'E'
          || bytes[i] == b'+'
          || bytes[i] == b'-'
          || bytes[i] == b'%')
      {
        i += 1;
      }
      // Skip unit
      while i < len && bytes[i].is_ascii_alphabetic() {
        i += 1;
      }
      continue;
    }

    // SCSS variable $...
    if b == b'$' {
      i += 1;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }
      continue;
    }

    // Less variable @...
    if b == b'@' {
      i += 1;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }
      continue;
    }

    // Semicolons, colons
    if b == b';' || b == b':' {
      i += 1;
      continue;
    }

    // Identifier or function name
    if b.is_ascii_alphabetic() || b == b'-' || b == b'_' {
      let start = i;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }
      let text = &value[start..i];

      // Function call
      if i < len && bytes[i] == b'(' {
        let func_lower = text.to_ascii_lowercase();
        func_stack.push(func_lower.clone());
        i += 1;

        // For url(), skip content entirely
        if func_lower == "url" {
          let content_start = i;
          let mut depth = 1;
          while i < len && depth > 0 {
            if bytes[i] == b'(' {
              depth += 1;
            } else if bytes[i] == b')' {
              depth -= 1;
            }
            if depth > 0 {
              i += 1;
            }
          }
          tokens.push(ValueToken {
            text: value[content_start..i].to_string(),
            offset: content_start,
            kind: TokenKind::UrlContent,
          });
          if i < len {
            i += 1;
          }
          func_stack.pop();
          continue;
        }

        tokens.push(ValueToken {
          text: text.to_string(),
          offset: start,
          kind: TokenKind::Function,
        });
        continue;
      }

      // Regular identifier -- suppress if inside var()/attr()/counter()/counters()
      let in_suppressed_fn = func_stack
        .iter()
        .any(|f| f == "var" || f == "attr" || f == "counter" || f == "counters");

      tokens.push(ValueToken {
        text: text.to_string(),
        offset: start,
        kind: if in_suppressed_fn {
          TokenKind::Other
        } else {
          TokenKind::Ident
        },
      });
      continue;
    }

    i += 1;
  }

  tokens
}

// ---------------------------------------------------------------------------
// Property-context filtering
// ---------------------------------------------------------------------------

fn should_check_keyword(
  token_text: &str,
  property: &str,
  _tokens: &[ValueToken],
  _idx: usize,
) -> bool {
  let prop_lower = property.to_ascii_lowercase();
  let prop_stripped = strip_vendor_prefix(&prop_lower);

  // System colors — skip (Stylelint skips them too).
  if is_system_color(token_text) {
    return false;
  }

  let lower_text = token_text.to_ascii_lowercase();

  // Custom ident properties
  if is_custom_ident_property(prop_stripped) {
    if is_global_keyword(token_text) {
      return true;
    }
    if (prop_stripped.starts_with("grid-") || prop_stripped == "grid-area")
      && (lower_text == "span" || lower_text == "auto")
    {
      return true;
    }
    if prop_stripped == "list-style-type" {
      return is_list_style_type_keyword(token_text);
    }
    return false;
  }

  // Mixed ident properties
  if is_mixed_ident_property(prop_stripped) {
    return should_check_in_mixed_property(token_text, prop_stripped);
  }

  true
}

fn should_check_in_mixed_property(token_text: &str, prop: &str) -> bool {
  if is_global_keyword(token_text) {
    return true;
  }
  let lower = token_text.to_ascii_lowercase();
  match prop {
    "font-family" => is_generic_font_family(token_text),
    "font" => {
      let font_kws = [
        "normal",
        "italic",
        "oblique",
        "small-caps",
        "bold",
        "bolder",
        "lighter",
        "ultra-condensed",
        "extra-condensed",
        "condensed",
        "semi-condensed",
        "semi-expanded",
        "expanded",
        "extra-expanded",
        "ultra-expanded",
        "caption",
        "icon",
        "menu",
        "message-box",
        "small-caption",
        "status-bar",
      ];
      font_kws.iter().any(|k| *k == lower) || is_generic_font_family(token_text)
    }
    "animation" => {
      let anim_kws = [
        "none",
        "ease",
        "ease-in",
        "ease-out",
        "ease-in-out",
        "linear",
        "step-start",
        "step-end",
        "infinite",
        "normal",
        "reverse",
        "alternate",
        "alternate-reverse",
        "forwards",
        "backwards",
        "both",
        "running",
        "paused",
      ];
      anim_kws.iter().any(|k| *k == lower)
    }
    "list-style" => {
      let pos_kws = ["inside", "outside"];
      pos_kws.iter().any(|k| *k == lower) || is_list_style_type_keyword(token_text)
    }
    _ => true,
  }
}

// ---------------------------------------------------------------------------
// ignoreFunctions support
// ---------------------------------------------------------------------------

fn is_in_ignored_function(value: &str, token_offset: usize, ignore_fns: &[String]) -> bool {
  let bytes = value.as_bytes();
  let mut i = 0;
  let mut func_stack: Vec<(String, usize)> = Vec::new();

  while i < value.len() {
    let b = bytes[i];

    if b == b'"' || b == b'\'' {
      let quote = b;
      i += 1;
      while i < value.len() && bytes[i] != quote {
        if bytes[i] == b'\\' {
          i += 1;
        }
        i += 1;
      }
      if i < value.len() {
        i += 1;
      }
      continue;
    }

    if b.is_ascii_alphabetic() || b == b'-' || b == b'_' {
      let start = i;
      while i < value.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
      {
        i += 1;
      }
      if i < value.len() && bytes[i] == b'(' {
        let name = value[start..i].to_string();
        i += 1;
        func_stack.push((name, i));
        continue;
      }
      if start == token_offset {
        return func_stack
          .iter()
          .any(|(name, _)| ignore_fns.iter().any(|pat| matches_pattern(name, pat)));
      }
      continue;
    }

    if b == b'(' {
      func_stack.push((String::new(), i + 1));
      i += 1;
      continue;
    }

    if b == b')' {
      func_stack.pop();
      i += 1;
      continue;
    }

    i += 1;
  }

  false
}

// ---------------------------------------------------------------------------
// Rule implementation
// ---------------------------------------------------------------------------

impl Rule for ValueKeywordCase {
  fn name(&self) -> &'static str {
    "value-keyword-case"
  }

  fn description(&self) -> &'static str {
    "Specify lowercase or uppercase for keyword values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    // Collect declarations from either a style rule or a standalone declaration
    // (e.g., inside a @mixin, @function, @if at-rule).
    let decls: Vec<&gale_css_parser::Declaration> = match node {
      CssNode::Style(rule) => rule.declarations.iter().collect(),
      CssNode::Declaration(decl) => vec![decl],
      _ => return vec![],
    };

    let expect_upper = ctx.primary_option_str().is_some_and(|s| s == "upper");

    let secondary = ctx.secondary_options();

    let ignore_keywords: Vec<String> = secondary
      .and_then(|v| v.get("ignoreKeywords"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|i| i.as_str().map(String::from))
          .collect()
      })
      .unwrap_or_default();

    let ignore_properties: Vec<String> = secondary
      .and_then(|v| v.get("ignoreProperties"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|i| i.as_str().map(String::from))
          .collect()
      })
      .unwrap_or_default();

    let ignore_functions: Vec<String> = secondary
      .and_then(|v| v.get("ignoreFunctions"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|i| i.as_str().map(String::from))
          .collect()
      })
      .unwrap_or_default();

    // Stylelint <=13 always treats camelCase SVG keywords (currentColor, etc.) as
    // canonical even without `camelCaseSvgKeywords: true`.  Stylelint 14+ introduced
    // the explicit option; without it the keywords are normalised to lowercase.
    let camel_case_svg = if stylelint_major_version() <= 13 {
      true
    } else {
      secondary
        .and_then(|v| v.get("camelCaseSvgKeywords"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    };

    let is_scss = matches!(
      ctx.syntax,
      gale_css_parser::Syntax::Scss | gale_css_parser::Syntax::Sass
    );

    let mut diags = Vec::new();

    for decl in &decls {
      let prop = &decl.property;

      let decl_start = decl.span.offset;
      let decl_end = decl_start + decl.span.length;
      let has_source = decl_end <= ctx.source.len() && decl_start < decl_end;

      let source_slice = if has_source {
        &ctx.source[decl_start..decl_end]
      } else {
        ""
      };

      // Extract original property name from source for ignoreProperties matching
      let original_prop = if has_source {
        source_slice
          .find(':')
          .map(|c| source_slice[..c].trim())
          .unwrap_or(prop)
      } else {
        prop
      };

      // Check ignoreProperties
      if !ignore_properties.is_empty() {
        let prop_matches = ignore_properties.iter().any(|pat| {
          matches_pattern(prop, pat)
            || matches_pattern(original_prop, pat)
            || matches_pattern_case_insensitive(prop, pat)
        });
        if prop_matches {
          continue;
        }
      }

      // Skip SCSS interpolation in values
      if is_scss && decl.value.contains("#{") {
        continue;
      }

      // Find where the value starts in the source slice (after "property:")
      let value_offset_in_source = if has_source {
        source_slice.find(':').map(|c| c + 1).unwrap_or(0)
      } else {
        0
      };

      // IMPORTANT: Extract value from ORIGINAL source, not decl.value.
      // lightningcss normalizes keyword values to lowercase in its printer,
      // which would make case checking impossible.
      let (value_to_tokenize, value_abs_start) = if has_source {
        let raw = &source_slice[value_offset_in_source..];
        let trimmed_end =
          raw.trim_end_matches(|c: char| c == ';' || c == '}' || c.is_ascii_whitespace());
        let leading_ws = trimmed_end.len() - trimmed_end.trim_start().len();
        let trimmed = trimmed_end.trim_start();
        (trimmed, decl_start + value_offset_in_source + leading_ws)
      } else {
        (decl.value.as_str(), decl_start)
      };

      let tokens = tokenize_value(value_to_tokenize);

      for (idx, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident && token.kind != TokenKind::Important {
          continue;
        }

        let text = &token.text;
        let abs_offset = value_abs_start + token.offset;

        // SVG camelCase keywords (currentColor, optimizeSpeed, crispEdges, etc.)
        if is_svg_camel_case_keyword(text) {
          if camel_case_svg && !expect_upper {
            // "lower" + camelCaseSvgKeywords:true: expected = camelCase canonical
            let canonical = SVG_CAMEL_CASE_KEYWORDS
              .iter()
              .find(|k| k.eq_ignore_ascii_case(text))
              .unwrap();
            if *text != **canonical {
              diags.push(
                Diagnostic::new(
                  self.name(),
                  format!("Expected \"{}\" to be \"{}\"", text, canonical),
                )
                .severity(self.default_severity())
                .span(Span::new(abs_offset, text.len()))
                .fix(Fix::new(
                  format!("Convert to \"{}\"", canonical),
                  vec![Edit::new(Span::new(abs_offset, text.len()), *canonical)],
                )),
              );
            }
            // camelCaseSvgKeywords:true + lower: handled above as canonical camelCase.
            continue;
          }
          // camelCaseSvgKeywords:false (default) or upper mode: fall through to
          // normal lower/upper check. Stylelint treats these as regular keywords
          // (e.g. currentColor → currentcolor with `lower` option).
        }

        // Property-context filtering
        if token.kind == TokenKind::Ident && !should_check_keyword(text, prop, &tokens, idx) {
          continue;
        }

        // ignoreKeywords
        if !ignore_keywords.is_empty()
          && ignore_keywords.iter().any(|pat| matches_pattern(text, pat))
        {
          continue;
        }

        // ignoreFunctions
        if !ignore_functions.is_empty()
          && is_in_ignored_function(value_to_tokenize, token.offset, &ignore_functions)
        {
          continue;
        }

        // Check case
        let expected = if expect_upper {
          text.to_ascii_uppercase()
        } else {
          text.to_ascii_lowercase()
        };

        if *text != expected {
          diags.push(
            Diagnostic::new(
              self.name(),
              format!("Expected \"{}\" to be \"{}\"", text, expected),
            )
            .severity(self.default_severity())
            .span(Span::new(abs_offset, text.len()))
            .fix(Fix::new(
              format!("Convert to \"{}\"", expected),
              vec![Edit::new(Span::new(abs_offset, text.len()), &expected)],
            )),
          );
        }
      }
    }
    diags
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn ctx_with_opts(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(opts),
    }
  }

  fn style_with_decl(prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 0),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_uppercase_keyword() {
    let d = ValueKeywordCase.check(&style_with_decl("display", "BLOCK"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("\"BLOCK\""));
    assert!(d[0].message.contains("\"block\""));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn allows_lowercase_keyword() {
    let d = ValueKeywordCase.check(&style_with_decl("display", "block"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_mixed_case() {
    let d = ValueKeywordCase.check(&style_with_decl("color", "Inherit"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("\"Inherit\""));
  }

  #[test]
  fn skips_url_content() {
    let d = ValueKeywordCase.check(&style_with_decl("background-url", "url(BLOCK)"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_string_content() {
    let d = ValueKeywordCase.check(&style_with_decl("content", "\"BLOCK\""), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn skips_system_colors() {
    // Stylelint does not enforce lowercase for CSS system colors (e.g. GrayText, ButtonText).
    // These are defined with mixed case in the CSS spec.
    let d = ValueKeywordCase.check(&style_with_decl("color", "InactiveCaptionText"), &ctx());
    assert!(
      d.is_empty(),
      "system color InactiveCaptionText should not be flagged"
    );
  }

  #[test]
  fn skips_animation_name() {
    let d = ValueKeywordCase.check(&style_with_decl("animation-name", "ANIMATION-NAME"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn reports_font_family_generic() {
    let d = ValueKeywordCase.check(&style_with_decl("font-family", "MONOSPACE"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn reports_current_color_in_lower_mode() {
    // With `lower` mode (default) and camelCaseSvgKeywords:false (default),
    // Stylelint v16 reports currentColor → expected currentcolor.
    let d = ValueKeywordCase.check(&style_with_decl("color", "currentColor"), &ctx());
    assert_eq!(d.len(), 1, "currentColor should be flagged in lower mode");
    assert!(d[0].message.contains("currentcolor"));
  }

  #[test]
  fn allows_currentcolor_lowercase() {
    let d = ValueKeywordCase.check(&style_with_decl("color", "currentcolor"), &ctx());
    assert!(
      d.is_empty(),
      "currentcolor (lowercase) should not be flagged"
    );
  }

  #[test]
  fn reports_current_color_in_border_lower_mode() {
    let d = ValueKeywordCase.check(&style_with_decl("border-color", "currentColor"), &ctx());
    assert_eq!(
      d.len(),
      1,
      "currentColor in border-color should be flagged in lower mode"
    );
  }

  #[test]
  fn allows_current_color_when_camel_case_svg_keywords_enabled() {
    // camelCaseSvgKeywords:true + lower: expected = camelCase canonical (currentColor)
    let opts = serde_json::json!(["lower", { "camelCaseSvgKeywords": true }]);
    let ctx = ctx_with_opts(&opts);
    let d = ValueKeywordCase.check(&style_with_decl("color", "currentColor"), &ctx);
    assert!(
      d.is_empty(),
      "currentColor should be allowed when camelCaseSvgKeywords:true"
    );
    // Lowercase should be flagged
    let d2 = ValueKeywordCase.check(&style_with_decl("color", "currentcolor"), &ctx);
    assert_eq!(
      d2.len(),
      1,
      "currentcolor should be flagged when camelCaseSvgKeywords:true"
    );
  }

  #[test]
  fn skips_font_family_custom() {
    let d = ValueKeywordCase.check(
      &style_with_decl("font-family", "Gill Sans Extrabold"),
      &ctx(),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn reports_georgia_in_custom_property() {
    let source = ":root {\n  --font-serif: var(--font-roboto-serif), ui-serif, Georgia;\n}";
    let var_offset = source.find("--font-serif:").unwrap();
    let var_end = source.find("Georgia;").unwrap() + "Georgia;".len();
    let node = CssNode::Style(StyleRule {
      selector: ":root".to_string(),
      declarations: vec![Declaration {
        property: "--font-serif".to_string(),
        value: "var(--font-roboto-serif), ui-serif, Georgia".to_string(),
        span: ParserSpan::new(var_offset, var_end - var_offset),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    let d = ValueKeywordCase.check(&node, &ctx);
    assert!(d.iter().any(|diag| diag.message.contains("Georgia")));
  }

  #[test]
  fn flags_uppercase_ident_after_triple_slash_in_value() {
    // PostCSS-SCSS does NOT treat `///` (triple-slash Sass doc comments) as line comments.
    // Identifiers after `///` remain visible to value-keyword-case.
    let tokens = tokenize_value("(\n  /// Prevent blah\n  inherit\n)");
    let ident_tokens: Vec<_> = tokens
      .iter()
      .filter(|t| t.kind == TokenKind::Ident)
      .collect();
    let texts: Vec<&str> = ident_tokens.iter().map(|t| t.text.as_str()).collect();
    assert!(
      texts.contains(&"Prevent"),
      "Prevent after /// should be tokenized as Ident (/// is not a comment), got: {:?}",
      texts
    );
    assert!(
      texts.contains(&"inherit"),
      "inherit should be tokenized as Ident, got: {:?}",
      texts
    );
  }

  #[test]
  fn skips_double_slash_comment_in_value() {
    // Regular `//` SCSS comments ARE skipped by the tokenizer.
    // Identifiers after `//` should NOT be flagged.
    let tokens = tokenize_value("(\n  // Prevent blah\n  inherit\n)");
    let ident_tokens: Vec<_> = tokens
      .iter()
      .filter(|t| t.kind == TokenKind::Ident)
      .collect();
    let texts: Vec<&str> = ident_tokens.iter().map(|t| t.text.as_str()).collect();
    assert!(
      !texts.contains(&"Prevent"),
      "Prevent after // should be skipped (// is a comment), got: {:?}",
      texts
    );
    assert!(
      texts.contains(&"inherit"),
      "inherit after // comment should still be tokenized, got: {:?}",
      texts
    );
  }
}
