use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::data::{is_named_color, named_color_for_rgb};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "color-named";

declare_rule! {
  /// Disallow/require named colors in CSS declarations.
  ///
  /// Equivalent to Stylelint's `color-named` rule.
  /// Options: `"never"` (default) disallows named colors; `"always-where-possible"` requires them.
  pub struct ColorNamed => ColorNamedCheck {
    name: NAME,
    description: "Require or disallow named colors",
    default_severity: Warning,
  }
}

/// `color-named` as configured.
pub struct ColorNamedCheck {
  /// `"never"` or `"always-where-possible"`.
  expectation: &'static str,
  /// `ignore: ["inside-function"]`.
  ignore_inside_function: bool,
  /// `ignoreProperties`: names (any case) and `/regex/` literals.
  ignore_properties: Matchers,
}

impl ColorNamedCheck {
  /// Stylelint's options: `"never"` or `"always-where-possible"`, then
  /// `ignore: ["inside-function"]`, `ignoreProperties` and
  /// `ignoreFunctions`.  Gale reads no option as `"never"`, and accepts but
  /// does not yet apply `ignoreFunctions`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options.primary_one_of(&["never", "always-where-possible"]);
    let ignore = options.keywords("ignore", &["inside-function"]);
    let ignore_properties = Matchers::ignoring_case(&options.strings("ignoreProperties"));
    options.strings("ignoreFunctions");
    options.finish()?;
    Ok(Self {
      expectation: expectation.unwrap_or("never"),
      ignore_inside_function: ignore.contains(&"inside-function"),
      ignore_properties,
    })
  }
}

/// Properties that DO NOT accept color values and should be skipped.
const NON_COLOR_PROPERTIES: &[&str] = &[
  "animation",
  "animation-name",
  "font",
  "font-family",
  "list-style-type",
  "composes",
  "counter-increment",
  "counter-reset",
  "counter-set",
  "grid-area",
  "grid-column",
  "grid-column-end",
  "grid-column-start",
  "grid-row",
  "grid-row-end",
  "grid-row-start",
  "grid-template",
  "grid-template-areas",
  "grid-template-columns",
  "grid-template-rows",
  "transition",
  "transition-property",
  "will-change",
  "contain",
  "content",
];

impl Check for ColorNamedCheck {
  /// Flags named colors under "never", or hex/function colors that have an exact
  /// name under "always-where-possible", honouring the ignore secondaries.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let option = self.expectation;
    let ignore_inside_function = self.ignore_inside_function;
    let ignore_properties = &self.ignore_properties;

    let mut diags = Vec::new();

    // The declarations of style rules.
    for decl in tree.walk_decls() {
      if !decl.in_style_rule() {
        continue;
      }
      // Check if this property should be ignored
      let prop_lower = decl.prop().to_ascii_lowercase();
      if ignore_properties.matches(&prop_lower) {
        continue;
      }

      // The value as written, `!important` included: everything after the
      // first `:` of the declaration.
      let decl_start = decl.span().offset;
      let full = decl.source_text();
      let (value, value_offset) = match full.find(':') {
        Some(colon_pos) => {
          let after_colon = &full[colon_pos + 1..];
          let trimmed = after_colon.trim_start();
          let leading_ws = after_colon.len() - trimmed.len();
          let val = trimmed.trim_end_matches(';').trim_end();
          (val, decl_start + colon_pos + 1 + leading_ws)
        }
        None => (full, decl_start),
      };

      match option {
        "never" => {
          check_never(
            value,
            value_offset,
            &prop_lower,
            ignore_inside_function,
            &mut diags,
          );
        }
        "always-where-possible" => {
          check_always_where_possible(value, value_offset, &mut diags);
        }
        _ => {}
      }
    }
    diags
  }
}

/// Check "never" mode: find named colors that should not be used.
fn check_never(
  value: &str,
  value_offset: usize,
  property: &str,
  ignore_inside_function: bool,
  diags: &mut Vec<Diagnostic>,
) {
  // Skip non-color properties
  if is_non_color_property(property) {
    return;
  }

  // Parse value into tokens, respecting functions and strings
  let tokens = tokenize_value(value);

  for token in &tokens {
    match token {
      ValueToken::Word { text, offset } => {
        let lower = text.to_ascii_lowercase();
        if is_named_color(&lower) {
          let abs_offset = value_offset + offset;
          diags.push(
            Diagnostic::new(NAME, format!("Unexpected named color \"{}\"", lower))
              .span(Span::new(abs_offset, text.len())),
          );
        }
      }
      ValueToken::Function {
        name: fn_name,
        args,
        offset: fn_offset,
        ..
      } => {
        // Check inside functions unless ignore_inside_function is set
        if !ignore_inside_function {
          let fn_name_lower = fn_name.to_ascii_lowercase();
          // Skip url() contents entirely
          if fn_name_lower == "url" {
            continue;
          }
          // Recurse into function arguments
          check_never(
            args,
            value_offset + fn_offset,
            property,
            false, // don't pass ignore_inside_function to nested
            diags,
          );
        }
      }
      _ => {}
    }
  }
}

/// Check "always-where-possible" mode: find hex/rgb/hsl values that could be named colors.
fn check_always_where_possible(value: &str, value_offset: usize, diags: &mut Vec<Diagnostic>) {
  let tokens = tokenize_value(value);

  for token in &tokens {
    match token {
      ValueToken::Hash { text, offset } => {
        // Check if this hex color has a named equivalent
        if let Some(rgb) = parse_hex_to_rgb(text) {
          if let Some(name) = named_color_for_rgb([rgb.0, rgb.1, rgb.2]) {
            let abs_offset = value_offset + offset;
            diags.push(
              Diagnostic::new(NAME, format!("Expected \"{name}\" instead of \"#{text}\""))
                .span(Span::new(abs_offset, text.len() + 1)), // +1 for #
            );
          }
        }
      }
      ValueToken::Function {
        name,
        args,
        offset: fn_offset,
        full_len,
      } => {
        let fn_lower = name.to_ascii_lowercase();
        match fn_lower.as_str() {
          "rgb" | "rgba" => {
            if let Some((r, g, b, fully_opaque)) = parse_rgb_args(args) {
              if fully_opaque {
                if let Some(color_name) = named_color_for_rgb([r, g, b]) {
                  let abs_offset = value_offset + fn_offset - name.len() - 1;
                  // Point to the start of the function name
                  let func_start = value_offset + fn_offset - name.len() - 1;
                  diags.push(
                    Diagnostic::new(
                      NAME,
                      format!(
                        "Expected \"{color_name}\" instead of \"{fn_lower}({})\"|",
                        args.trim()
                      ),
                    )
                    .span(Span::new(func_start, *full_len)),
                  );
                }
              }
            }
          }
          "hsl" | "hsla" => {
            if let Some((r, g, b, fully_opaque)) = parse_hsl_args(args) {
              if fully_opaque {
                if let Some(color_name) = named_color_for_rgb([r, g, b]) {
                  let func_start = value_offset + fn_offset - name.len() - 1;
                  diags.push(
                    Diagnostic::new(
                      NAME,
                      format!(
                        "Expected \"{color_name}\" instead of \"{fn_lower}({})\"|",
                        args.trim()
                      ),
                    )
                    .span(Span::new(func_start, *full_len)),
                  );
                }
              }
            }
          }
          "hwb" => {
            if let Some((r, g, b, fully_opaque)) = parse_hwb_args(args) {
              if fully_opaque {
                if let Some(color_name) = named_color_for_rgb([r, g, b]) {
                  let func_start = value_offset + fn_offset - name.len() - 1;
                  diags.push(
                    Diagnostic::new(
                      NAME,
                      format!(
                        "Expected \"{color_name}\" instead of \"{fn_lower}({})\"|",
                        args.trim()
                      ),
                    )
                    .span(Span::new(func_start, *full_len)),
                  );
                }
              }
            }
          }
          "gray" => {
            if let Some((r, g, b, fully_opaque)) = parse_gray_args(args) {
              if fully_opaque {
                if let Some(color_name) = named_color_for_rgb([r, g, b]) {
                  let func_start = value_offset + fn_offset - name.len() - 1;
                  diags.push(
                    Diagnostic::new(
                      NAME,
                      format!(
                        "Expected \"{color_name}\" instead of \"{fn_lower}({})\"|",
                        args.trim()
                      ),
                    )
                    .span(Span::new(func_start, *full_len)),
                  );
                }
              }
            }
          }
          "color" => {
            // color(#000 a(50%)) — check the first argument if it's a hex
            let inner_tokens = tokenize_value(args);
            for inner in &inner_tokens {
              if let ValueToken::Hash { text, offset: _ } = inner {
                if let Some(rgb) = parse_hex_to_rgb(text) {
                  if let Some(color_name) = named_color_for_rgb([rgb.0, rgb.1, rgb.2]) {
                    let func_start = value_offset + fn_offset - name.len() - 1;
                    diags.push(
                      Diagnostic::new(
                        NAME,
                        format!("Expected \"{color_name}\" instead of \"#{text}\""),
                      )
                      .span(Span::new(func_start, *full_len)),
                    );
                  }
                }
              }
            }
          }
          _ => {}
        }
      }
      _ => {}
    }
  }
}

// ---------------------------------------------------------------------------
// Value tokenizer
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum ValueToken {
  /// A plain word (identifier) e.g. "red", "solid"
  Word { text: String, offset: usize },
  /// A hash value e.g. "#fff" (text is without #)
  Hash { text: String, offset: usize },
  /// A function call e.g. "rgb(0, 0, 0)"
  Function {
    name: String,
    args: String,
    /// Offset of the first char INSIDE the parens (after `(`)
    offset: usize,
    /// Full length from function name start to closing `)`
    full_len: usize,
  },
  /// Whitespace, comma, etc.
  Other,
}

/// Tokenize a CSS value string into meaningful parts.
fn tokenize_value(value: &str) -> Vec<ValueToken> {
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  let mut tokens = Vec::new();

  while i < len {
    let b = bytes[i];

    // Skip whitespace
    if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
      i += 1;
      continue;
    }

    // Skip quoted strings
    if b == b'"' || b == b'\'' {
      let quote = b;
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
      tokens.push(ValueToken::Other);
      continue;
    }

    // Hash (#hex)
    if b == b'#' {
      let start = i;
      i += 1;
      while i < len && (bytes[i].is_ascii_alphanumeric()) {
        i += 1;
      }
      let text = &value[start + 1..i];
      tokens.push(ValueToken::Hash {
        text: text.to_string(),
        offset: start,
      });
      continue;
    }

    // SCSS variable
    if b == b'$' {
      i += 1;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }
      tokens.push(ValueToken::Other);
      continue;
    }

    // SCSS interpolation #{...}
    if b == b'#' && i + 1 < len && bytes[i + 1] == b'{' {
      let mut depth = 1;
      i += 2;
      while i < len && depth > 0 {
        if bytes[i] == b'{' {
          depth += 1;
        } else if bytes[i] == b'}' {
          depth -= 1;
        }
        i += 1;
      }
      tokens.push(ValueToken::Other);
      continue;
    }

    // Word or function
    if b.is_ascii_alphabetic() || b == b'-' || b == b'_' {
      let word_start = i;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }
      let word = &value[word_start..i];

      // Check if this is a function call
      if i < len && bytes[i] == b'(' {
        let fn_name = word.to_string();
        i += 1; // skip (
        let args_start = i;
        let mut depth = 1;
        while i < len && depth > 0 {
          if bytes[i] == b'(' {
            depth += 1;
          } else if bytes[i] == b')' {
            depth -= 1;
          } else if bytes[i] == b'"' || bytes[i] == b'\'' {
            let q = bytes[i];
            i += 1;
            while i < len && bytes[i] != q {
              if bytes[i] == b'\\' && i + 1 < len {
                i += 1;
              }
              i += 1;
            }
            // Skip the closing quote if we found it
            if i < len {
              i += 1;
            }
            continue; // already advanced past the quoted string
          }
          if depth > 0 && i < len {
            i += 1;
          }
        }
        let args_end = i.min(len);
        let args = &value[args_start..args_end];
        let full_len = if i < len {
          i += 1; // skip )
          i - word_start
        } else {
          i - word_start
        };
        tokens.push(ValueToken::Function {
          name: fn_name,
          args: args.to_string(),
          offset: args_start,
          full_len,
        });
      } else if bytes.get(i) == Some(&b';') {
        // value-parser reads `red;` (in an `if()` branch, say) as one
        // word, which names no color.
        tokens.push(ValueToken::Other);
      } else {
        tokens.push(ValueToken::Word {
          text: word.to_string(),
          offset: word_start,
        });
      }
      continue;
    }

    // Skip other characters (commas, slashes, numbers, etc.)
    i += 1;
  }

  tokens
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Whether `prop` never takes a color, so its values can be skipped.
fn is_non_color_property(prop: &str) -> bool {
  NON_COLOR_PROPERTIES.contains(&prop)
}

/// Parse a hex color string (without #) to RGB.
fn parse_hex_to_rgb(hex: &str) -> Option<(u8, u8, u8)> {
  let hex = hex.to_ascii_lowercase();
  match hex.len() {
    3 => {
      let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
      let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
      let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
      Some((r * 17, g * 17, b * 17))
    }
    4 => {
      // #RGBA — only opaque (A=F) counts
      let a = u8::from_str_radix(&hex[3..4], 16).ok()?;
      if a != 15 {
        return None;
      }
      let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
      let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
      let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
      Some((r * 17, g * 17, b * 17))
    }
    6 => {
      let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
      let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
      let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
      Some((r, g, b))
    }
    8 => {
      // #RRGGBBAA — only opaque (AA=FF) counts
      let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
      if a != 255 {
        return None;
      }
      let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
      let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
      let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
      Some((r, g, b))
    }
    _ => None,
  }
}

/// Parse RGB/RGBA function arguments to (r, g, b, fully_opaque).
fn parse_rgb_args(args: &str) -> Option<(u8, u8, u8, bool)> {
  // Handle both comma-separated and space-separated with /
  let clean = args.replace(['\n', '\r'], " ");
  let clean = clean.trim();

  // Skip if contains calc() or var() or other complex expressions
  let lower = clean.to_ascii_lowercase();
  if lower.contains("calc(") || lower.contains("var(") {
    return None;
  }

  // Try comma-separated: rgb(r, g, b) or rgba(r, g, b, a)
  if clean.contains(',') {
    let parts: Vec<&str> = clean.split(',').map(|s| s.trim()).collect();
    if parts.len() < 3 {
      return None;
    }
    let r = parse_color_component(parts[0], 255.0)?;
    let g = parse_color_component(parts[1], 255.0)?;
    let b = parse_color_component(parts[2], 255.0)?;
    let fully_opaque = if parts.len() >= 4 {
      is_fully_opaque(parts[3])
    } else {
      true
    };
    return Some((r, g, b, fully_opaque));
  }

  // Try space-separated: rgb(r g b) or rgb(r g b / a)
  let (color_part, alpha_part) = if let Some(pos) = clean.find('/') {
    (&clean[..pos], Some(clean[pos + 1..].trim()))
  } else {
    (clean, None)
  };

  let parts: Vec<&str> = color_part.split_whitespace().collect();
  if parts.len() != 3 {
    return None;
  }
  let r = parse_color_component(parts[0], 255.0)?;
  let g = parse_color_component(parts[1], 255.0)?;
  let b = parse_color_component(parts[2], 255.0)?;
  let fully_opaque = match alpha_part {
    Some(a) => is_fully_opaque(a),
    None => true,
  };
  Some((r, g, b, fully_opaque))
}

/// Parse HSL/HSLA arguments to (r, g, b, fully_opaque).
fn parse_hsl_args(args: &str) -> Option<(u8, u8, u8, bool)> {
  let clean = args.replace(['\n', '\r'], " ");
  let clean = clean.trim();

  let lower = clean.to_ascii_lowercase();
  if lower.contains("calc(") || lower.contains("var(") {
    return None;
  }

  // Comma-separated: hsl(h, s%, l%) or hsla(h, s%, l%, a)
  if clean.contains(',') {
    let parts: Vec<&str> = clean.split(',').map(|s| s.trim()).collect();
    if parts.len() < 3 {
      return None;
    }
    let h: f64 = parts[0].trim().parse().ok()?;
    let s = parse_percentage(parts[1])?;
    let l = parse_percentage(parts[2])?;
    let fully_opaque = if parts.len() >= 4 {
      is_fully_opaque(parts[3])
    } else {
      true
    };
    let (r, g, b) = hsl_to_rgb(h, s, l);
    return Some((r, g, b, fully_opaque));
  }

  // Space-separated
  let (color_part, alpha_part) = if let Some(pos) = clean.find('/') {
    (&clean[..pos], Some(clean[pos + 1..].trim()))
  } else {
    (clean, None)
  };
  let parts: Vec<&str> = color_part.split_whitespace().collect();
  if parts.len() != 3 {
    return None;
  }
  let h: f64 = parts[0].trim().parse().ok()?;
  let s = parse_percentage(parts[1])?;
  let l = parse_percentage(parts[2])?;
  let fully_opaque = match alpha_part {
    Some(a) => is_fully_opaque(a),
    None => true,
  };
  let (r, g, b) = hsl_to_rgb(h, s, l);
  Some((r, g, b, fully_opaque))
}

/// Parse HWB arguments to (r, g, b, fully_opaque).
fn parse_hwb_args(args: &str) -> Option<(u8, u8, u8, bool)> {
  let clean = args.replace(['\n', '\r'], " ");
  let clean = clean.trim();

  // HWB can be comma-separated (non-standard but in tests) or space-separated
  if clean.contains(',') {
    let parts: Vec<&str> = clean.split(',').map(|s| s.trim()).collect();
    if parts.len() < 3 {
      return None;
    }
    let h: f64 = parts[0].trim().parse().ok()?;
    let w = parse_percentage(parts[1])?;
    let b_val = parse_percentage(parts[2])?;
    let fully_opaque = if parts.len() >= 4 {
      is_fully_opaque(parts[3])
    } else {
      true
    };
    let (r, g, b) = hwb_to_rgb(h, w, b_val);
    return Some((r, g, b, fully_opaque));
  }

  let (color_part, alpha_part) = if let Some(pos) = clean.find('/') {
    (&clean[..pos], Some(clean[pos + 1..].trim()))
  } else {
    (clean, None)
  };
  let parts: Vec<&str> = color_part.split_whitespace().collect();
  if parts.len() != 3 {
    return None;
  }
  let h: f64 = parts[0].trim().parse().ok()?;
  let w = parse_percentage(parts[1])?;
  let b_val = parse_percentage(parts[2])?;
  let fully_opaque = match alpha_part {
    Some(a) => is_fully_opaque(a),
    None => true,
  };
  let (r, g, b) = hwb_to_rgb(h, w, b_val);
  Some((r, g, b, fully_opaque))
}

/// Parse gray() function arguments to (r, g, b, fully_opaque).
fn parse_gray_args(args: &str) -> Option<(u8, u8, u8, bool)> {
  let clean = args.replace(['\n', '\r'], " ");
  let clean = clean.trim();

  if clean.contains(',') {
    let parts: Vec<&str> = clean.split(',').map(|s| s.trim()).collect();
    let gray_val = parse_color_component(parts[0], 255.0)?;
    let fully_opaque = if parts.len() >= 2 {
      is_fully_opaque(parts[1])
    } else {
      true
    };
    return Some((gray_val, gray_val, gray_val, fully_opaque));
  }

  let (color_part, alpha_part) = if let Some(pos) = clean.find('/') {
    (&clean[..pos], Some(clean[pos + 1..].trim()))
  } else {
    (clean, None)
  };
  let gray_val = parse_color_component(color_part.trim(), 255.0)?;
  let fully_opaque = match alpha_part {
    Some(a) => is_fully_opaque(a),
    None => true,
  };
  Some((gray_val, gray_val, gray_val, fully_opaque))
}

/// Parse a color component value (number or percentage).
fn parse_color_component(s: &str, max: f64) -> Option<u8> {
  let s = s.trim();
  if let Some(pct) = s.strip_suffix('%') {
    let val: f64 = pct.trim().parse().ok()?;
    Some((val / 100.0 * max).round() as u8)
  } else {
    let val: f64 = s.parse().ok()?;
    Some(val.round() as u8)
  }
}

/// Parse a percentage value (e.g., "50%" -> 0.5).
fn parse_percentage(s: &str) -> Option<f64> {
  let s = s.trim();
  let pct = s.strip_suffix('%')?;
  pct.trim().parse::<f64>().ok().map(|v| v / 100.0)
}

/// Check if an alpha value represents full opacity.
fn is_fully_opaque(s: &str) -> bool {
  let s = s.trim();
  if let Some(pct) = s.strip_suffix('%') {
    if let Ok(val) = pct.trim().parse::<f64>() {
      return (val - 100.0).abs() < 0.001;
    }
  }
  if let Ok(val) = s.parse::<f64>() {
    return (val - 1.0).abs() < 0.001;
  }
  false
}

/// Convert HSL to RGB.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
  if s == 0.0 {
    let v = (l * 255.0).round() as u8;
    return (v, v, v);
  }

  let h = ((h % 360.0) + 360.0) % 360.0 / 360.0;
  let q = if l < 0.5 {
    l * (1.0 + s)
  } else {
    l + s - l * s
  };
  let p = 2.0 * l - q;

  let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
  let g = hue_to_rgb(p, q, h);
  let b = hue_to_rgb(p, q, h - 1.0 / 3.0);

  (
    (r * 255.0).round() as u8,
    (g * 255.0).round() as u8,
    (b * 255.0).round() as u8,
  )
}

/// One channel of the HSL-to-RGB conversion, for the given hue offset `t`.
fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
  if t < 0.0 {
    t += 1.0;
  }
  if t > 1.0 {
    t -= 1.0;
  }
  if t < 1.0 / 6.0 {
    return p + (q - p) * 6.0 * t;
  }
  if t < 1.0 / 2.0 {
    return q;
  }
  if t < 2.0 / 3.0 {
    return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
  }
  p
}

/// Convert HWB to RGB.
fn hwb_to_rgb(h: f64, w: f64, b: f64) -> (u8, u8, u8) {
  // If white + black >= 1, it's a gray
  if w + b >= 1.0 {
    let gray = (w / (w + b) * 255.0).round() as u8;
    return (gray, gray, gray);
  }

  let (r, g, bl) = hsl_to_rgb(h, 1.0, 0.5);
  let r = (r as f64 / 255.0 * (1.0 - w - b) + w) * 255.0;
  let g = (g as f64 / 255.0 * (1.0 - w - b) + w) * 255.0;
  let bl = (bl as f64 / 255.0 * (1.0 - w - b) + w) * 255.0;

  (r.round() as u8, g.round() as u8, bl.round() as u8)
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::testing::{CheckSource, configured, ctx_with_source, invalid_options, lint};

  fn check(property: &str, value: &str) -> Vec<Diagnostic> {
    let source = format!("a {{ {property}: {value}; }}");
    configured(&ColorNamed, None).check_source(&ctx_with_source(&source))
  }

  #[test]
  fn reports_named_color() {
    let d = check("color", "red");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("red"));
    assert_eq!(d[0].span, Span::new(11, 3));
  }

  #[test]
  fn allows_hex_color() {
    assert!(check("color", "#ff0000").is_empty());
  }

  #[test]
  fn skips_url_function() {
    assert!(check("background-image", "url(./black.png)").is_empty());
  }

  #[test]
  fn skips_animation_property() {
    assert!(check("animation", "blue 2s linear").is_empty());
  }

  #[test]
  fn skips_font_family() {
    assert!(check("font-family", "blue").is_empty());
  }

  #[test]
  fn reads_the_declarations_of_style_rules() {
    let rule = configured(&ColorNamed, None);
    let at = |source| rule.check_source(&ctx_with_source(source)).len();
    // `@nest` is a style rule too; `@media` is not.
    assert_eq!(at(".a { @nest .b & { color: blue; } }"), 1);
    assert_eq!(at("@media print { color: blue; }"), 0);
  }

  #[test]
  fn a_word_glued_to_a_semicolon_is_no_color() {
    // value-parser reads `red;` as one word, as in an `if()` branch.
    assert!(check("color", "if(media(print): red; else: blue;)").is_empty());
    assert_eq!(check("color", "if(media(print): red; else: blue)").len(), 1);
  }

  #[test]
  fn hex_to_rgb_parsing() {
    assert_eq!(parse_hex_to_rgb("000"), Some((0, 0, 0)));
    assert_eq!(parse_hex_to_rgb("fff"), Some((255, 255, 255)));
    assert_eq!(parse_hex_to_rgb("000000"), Some((0, 0, 0)));
    assert_eq!(parse_hex_to_rgb("ffffff"), Some((255, 255, 255)));
    assert_eq!(parse_hex_to_rgb("000f"), Some((0, 0, 0))); // #000F = black with full alpha
    assert_eq!(parse_hex_to_rgb("000000ff"), Some((0, 0, 0)));
  }

  #[test]
  fn secondary_options_change_what_is_reported() {
    let css = "a { color: red; fill: blue; }";
    let lint_with = |options| lint(NAME, options, css, gale_css_parser::Syntax::Css).len();
    assert_eq!(lint_with(serde_json::json!("never")), 2);
    assert_eq!(
      lint_with(serde_json::json!(["never", { "ignoreProperties": ["FILL"] }])),
      1
    );
    assert_eq!(
      lint_with(serde_json::json!(["never", { "ignoreProperties": ["/^col/"] }])),
      1
    );
    assert_eq!(lint_with(serde_json::json!("always-where-possible")), 0);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["never", {
          "ignore": ["inside-function"],
          "ignoreProperties": ["fill"],
          "ignoreFunctions": ["/^var/"]
        }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("always")),
      ["Invalid option value \"always\" for rule \"color-named\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["never", { "ignoreEverything": true }])
      ),
      ["Invalid option name \"ignoreEverything\" for rule \"color-named\""]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["never", { "ignoreProperties": [1] }])
      ),
      ["Invalid value \"1\" for option \"ignoreProperties\" of rule \"color-named\""]
    );
  }
}
