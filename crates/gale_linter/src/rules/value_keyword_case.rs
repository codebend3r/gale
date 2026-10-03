use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::pattern;
use crate::rule::{Rule, RuleContext};
use crate::standard_syntax::is_standard_syntax_value;
use crate::stylelint_version::{installed_at_least, stylelint_major_version};

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

/// Whether `s` is a system color, gated on the installed Stylelint's major
/// version since CSS4 added more.
fn is_system_color(s: &str) -> bool {
  let v = stylelint_major_version();
  if SYSTEM_COLORS_CSS2.iter().any(|c| c.eq_ignore_ascii_case(s)) {
    return true;
  }
  if v >= 14 && SYSTEM_COLORS_CSS4.iter().any(|c| c.eq_ignore_ascii_case(s)) {
    return true;
  }
  if installed_at_least(16, 12)
    && PREFIXED_SYSTEM_COLORS
      .iter()
      .any(|c| c.eq_ignore_ascii_case(s))
  {
    return true;
  }
  false
}

/// Vendor-prefixed system colors, which Stylelint ignores since 16.12.
const PREFIXED_SYSTEM_COLORS: &[&str] = &[
  "-moz-buttondefault",
  "-moz-buttonhoverface",
  "-moz-buttonhovertext",
  "-moz-cellhighlight",
  "-moz-cellhighlighttext",
  "-moz-combobox",
  "-moz-comboboxtext",
  "-moz-dialog",
  "-moz-dialogtext",
  "-moz-dragtargetzone",
  "-moz-eventreerow",
  "-moz-field",
  "-moz-fieldtext",
  "-moz-html-cellhighlight",
  "-moz-html-cellhighlighttext",
  "-moz-mac-accentdarkestshadow",
  "-moz-mac-accentdarkshadow",
  "-moz-mac-accentface",
  "-moz-mac-accentlightesthighlight",
  "-moz-mac-accentlightshadow",
  "-moz-mac-accentregularhighlight",
  "-moz-mac-accentregularshadow",
  "-moz-mac-chrome-active",
  "-moz-mac-chrome-inactive",
  "-moz-mac-focusring",
  "-moz-mac-menuselect",
  "-moz-mac-menushadow",
  "-moz-mac-menutextselect",
  "-moz-menubarhovertext",
  "-moz-menubartext",
  "-moz-menuhover",
  "-moz-menuhovertext",
  "-moz-nativehyperlinktext",
  "-moz-oddtreerow",
  "-moz-win-accentcolor",
  "-moz-win-accentcolortext",
  "-moz-win-communicationstext",
  "-moz-win-mediatext",
  "-ms-hotlight",
];

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

/// Whether `s` is an SVG keyword that is legitimately camelCase.
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
  "grid-row",
  "grid-column",
  "grid-area",
  "grid-row-start",
  "grid-row-end",
  "grid-column-start",
  "grid-column-end",
  "list-style-type",
];

/// Whether the property takes a `<custom-ident>`, whose case is author-chosen.
fn is_custom_ident_property(prop: &str) -> bool {
  CUSTOM_IDENT_PROPERTIES.contains(&prop.to_ascii_lowercase().as_str())
}

/// Properties where some positions are keywords and some are custom idents.
const MIXED_IDENT_PROPERTIES: &[&str] = &["animation", "font", "font-family", "list-style"];

/// Whether the property mixes keywords with author-chosen identifiers.
fn is_mixed_ident_property(prop: &str) -> bool {
  MIXED_IDENT_PROPERTIES.contains(&prop.to_ascii_lowercase().as_str())
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

/// Whether `s` is a generic family keyword such as `serif`.
fn is_generic_font_family(s: &str) -> bool {
  let lower = s.to_ascii_lowercase();
  GENERIC_FONT_FAMILIES.iter().any(|f| *f == lower)
}

/// Global CSS keywords.
const GLOBAL_KEYWORDS: &[&str] = &["inherit", "initial", "unset", "revert", "revert-layer"];

/// Whether `s` is `inherit`, `initial`, `unset` or similar.
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

/// Whether `s` is a `list-style-type` keyword.
fn is_list_style_type_keyword(s: &str) -> bool {
  let lower = s.to_ascii_lowercase();
  LIST_STYLE_TYPE_KEYWORDS.iter().any(|k| *k == lower)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The string with any vendor prefix removed.
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

/// Matches `value` against a `/…/` regex pattern, else an exact string.
fn matches_pattern(value: &str, pattern: &str) -> bool {
  pattern::match_regex_entry(pattern, value).unwrap_or_else(|| value == pattern)
}

/// As the above, but exact strings compare case-insensitively.
fn matches_pattern_case_insensitive(value: &str, pattern: &str) -> bool {
  pattern::match_regex_entry(pattern, value).unwrap_or_else(|| value.eq_ignore_ascii_case(pattern))
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

    // Square brackets (grid line names): `[name]` is one word to
    // postcss-value-parser, brackets included.  With whitespace inside,
    // the names are words of their own.
    if b == b'[' {
      let start = i;
      let end = value[i..]
        .find(|c: char| c == ']' || c.is_whitespace() || matches!(c, ',' | '(' | ')' | '/'))
        .map(|at| i + at);
      if let Some(end) = end.filter(|&end| bytes[end] == b']' && end > start + 1) {
        i = end + 1;
        tokens.push(ValueToken {
          text: value[start..i].to_string(),
          offset: start,
          kind: TokenKind::Ident,
        });
      } else {
        i += 1;
      }
      continue;
    }
    if b == b']' {
      i += 1;
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

    // `!word` (`!default`, a mistyped `!import`): one word to
    // postcss-value-parser, `!` included.  `!important` itself never gets
    // here, since PostCSS keeps it out of the value.
    if b == b'!' {
      let start = i;
      i += 1;
      while i < len && bytes[i].is_ascii_alphanumeric() {
        i += 1;
      }
      if i > start + 1 {
        tokens.push(ValueToken {
          text: value[start..i].to_string(),
          offset: start,
          kind: TokenKind::Important,
        });
      }
      continue;
    }

    // Number (possibly with unit), signed or not: postcss-value-parser's
    // `unit()` reads `-2px` as a dimension, which the rule skips.
    let starts_number = |at: usize| {
      bytes.get(at).is_some_and(u8::is_ascii_digit)
        || (bytes.get(at) == Some(&b'.') && bytes.get(at + 1).is_some_and(u8::is_ascii_digit))
    };
    if b == b'-' && starts_number(i + 1) {
      i += 1;
    }
    if starts_number(i) {
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

    // A unicode range (`U+0400-045F`) is no word to postcss-value-parser.
    if (b == b'u' || b == b'U')
      && bytes.get(i + 1) == Some(&b'+')
      && bytes
        .get(i + 2)
        .is_some_and(|c| c.is_ascii_hexdigit() || *c == b'?')
    {
      i += 2;
      while i < len && (bytes[i].is_ascii_hexdigit() || bytes[i] == b'?' || bytes[i] == b'-') {
        i += 1;
      }
      continue;
    }

    // Identifier or function name
    if b.is_ascii_alphabetic() || b == b'-' || b == b'_' {
      let start = i;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }
      // A Sass module member (`map.get(`, `theme.$color`) is one word.
      let mut module_member = false;
      while i + 1 < len
        && bytes[i] == b'.'
        && (bytes[i + 1].is_ascii_alphabetic() || matches!(bytes[i + 1], b'$' | b'_' | b'-'))
      {
        module_member = true;
        i += 1;
        while i < len
          && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'-' | b'_' | b'$'))
        {
          i += 1;
        }
      }
      let text = &value[start..i];
      if module_member && !(i < len && bytes[i] == b'(') {
        // `ns.$var` is not standard syntax, and Stylelint skips it.
        if !text.contains(".$") {
          tokens.push(ValueToken {
            text: text.to_string(),
            offset: start,
            kind: TokenKind::Ident,
          });
        }
        continue;
      }

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

/// Whether this token is a keyword whose case the rule owns, rather than an
/// author-chosen identifier, system color or other exempt value.
fn should_check_keyword(
  token_text: &str,
  property: &str,
  _tokens: &[ValueToken],
  _idx: usize,
) -> bool {
  // Stylelint matches the property exactly (lowercased): `-webkit-animation`
  // is not `animation`.
  let prop_lower = property.to_ascii_lowercase();
  let prop_stripped = prop_lower.as_str();

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
    // Stylelint's animationNameKeywords and counter keywords add `none`.
    return lower_text == "none"
      && matches!(
        prop_stripped,
        "animation-name" | "counter-increment" | "counter-reset"
      );
  }

  // Mixed ident properties
  if is_mixed_ident_property(prop_stripped) {
    return should_check_in_mixed_property(token_text, prop_stripped);
  }

  true
}

/// For a property mixing keywords and identifiers, whether this token is one of
/// the property's keywords.
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

/// Whether the token at `token_offset` sits inside a function on the ignore list.
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

  /// Flags keyword values not in the configured case, honouring the ignore
  /// secondaries and the SVG camelCase allowance.
  ///
  /// Every declaration counts, as with Stylelint's `walkDecls`: those
  /// directly inside at-rules, SCSS variables and nested properties, and
  /// ones whose value the CSS parser would reject.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let tree = ctx.postcss_tree();

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

    let mut diags = Vec::new();

    for decl in tree.decls() {
      // Stylelint skips a declaration whose value is not standard syntax
      // (a variable, a module member, interpolation).
      if !is_standard_syntax_value(&decl.value) {
        continue;
      }
      let original_prop = decl.name.as_str();
      let prop = &original_prop.to_ascii_lowercase();

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

      // Stylelint skips every keyword of a value holding a `#` (a hex
      // color, an interpolation) anywhere.
      if decl.value.contains('#') {
        continue;
      }

      // The raw value as written (Stylelint's `getDeclarationValue`):
      // comments kept, `!important` left out.
      let Some(value_to_tokenize) = ctx.source_slice(decl.value_span.start, decl.value_span.end)
      else {
        continue;
      };
      let value_abs_start = decl.value_span.start;

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
  use gale_css_parser::Syntax;

  use crate::testing::{context, fix};

  /// The diagnostics for `source` in `syntax` with `options`.
  fn lint(source: &str, syntax: Syntax, options: Option<&serde_json::Value>) -> Vec<Diagnostic> {
    let ctx = context(source, syntax, options.cloned());
    ValueKeywordCase.check_root(&[], &ctx)
  }

  /// The diagnostics for `a { <prop>: <value>; }` with no options.
  fn decl(prop: &str, value: &str) -> Vec<Diagnostic> {
    lint(&format!("a {{ {prop}: {value}; }}"), Syntax::Css, None)
  }

  #[test]
  fn reports_uppercase_keyword() {
    let d = decl("display", "BLOCK");
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Expected \"BLOCK\" to be \"block\"");
    assert_eq!(d[0].span.offset, 13);
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn allows_lowercase_keyword() {
    assert!(decl("display", "block").is_empty());
  }

  #[test]
  fn reports_mixed_case() {
    let d = decl("color", "Inherit");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("\"Inherit\""));
  }

  #[test]
  fn skips_urls_strings_and_animation_names() {
    assert!(decl("background", "url(BLOCK)").is_empty());
    assert!(decl("content", "\"BLOCK\"").is_empty());
    assert!(decl("animation-name", "ANIMATION-NAME").is_empty());
    assert!(decl("font-family", "Gill Sans Extrabold").is_empty());
  }

  #[test]
  fn skips_system_colors() {
    // Stylelint does not enforce lowercase for CSS system colors (e.g. GrayText, ButtonText).
    assert!(decl("color", "InactiveCaptionText").is_empty());
    assert!(decl("color", "-moz-NativeHyperlinkText").is_empty());
  }

  #[test]
  fn reports_font_family_generic() {
    assert_eq!(decl("font-family", "MONOSPACE").len(), 1);
  }

  #[test]
  fn current_color_follows_camel_case_svg_keywords() {
    // `lower` without camelCaseSvgKeywords wants `currentcolor`.
    let d = decl("color", "currentColor");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("currentcolor"));
    assert_eq!(decl("border-color", "currentColor").len(), 1);
    assert!(decl("color", "currentcolor").is_empty());
    let camel = serde_json::json!(["lower", { "camelCaseSvgKeywords": true }]);
    assert!(lint("a { color: currentColor; }", Syntax::Css, Some(&camel)).is_empty());
    assert_eq!(
      lint("a { color: currentcolor; }", Syntax::Css, Some(&camel)).len(),
      1
    );
  }

  #[test]
  fn reports_georgia_in_custom_property() {
    let source = ":root {\n  --font-serif: var(--font-roboto-serif), ui-serif, Georgia;\n}";
    let d = lint(source, Syntax::Css, None);
    assert!(d.iter().any(|diag| diag.message.contains("Georgia")));
  }

  #[test]
  fn checks_every_declaration_postcss_sees() {
    // Directly inside an at-rule, and with a value the CSS parser rejects.
    assert_eq!(
      lint("@media (min-width: 1px) { color: Red; }", Syntax::Css, None).len(),
      1
    );
    let d = lint("a { display: block !Import; }", Syntax::Css, None);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].message, "Expected \"!Import\" to be \"!import\"");
    assert_eq!(d[0].span.offset, 19);
    // SCSS variables and nested properties are declarations too.
    let scss = "$c: Red;\na { font: { family: Arial; } }";
    assert_eq!(lint(scss, Syntax::Scss, None).len(), 2);
    // `!important` is not part of the value, and a variable value is skipped.
    assert!(lint("a { color: red !IMPORTANT; top: $X; }", Syntax::Scss, None).is_empty());
  }

  #[test]
  fn fix_rewrites_the_keyword_in_place() {
    assert_eq!(
      fix(
        "value-keyword-case",
        serde_json::json!("lower"),
        "@media screen { color: GREEN; @media (min-width: 1px) { color: Red !important; } }",
        Syntax::Css,
      ),
      "@media screen { color: green; @media (min-width: 1px) { color: red !important; } }"
    );
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
