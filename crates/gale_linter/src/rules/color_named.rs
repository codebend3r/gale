use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow/require named colors in CSS declarations.
///
/// Equivalent to Stylelint's `color-named` rule.
/// Options: `"never"` (default) disallows named colors; `"always-where-possible"` requires them.
pub struct ColorNamed;

/// Full list of CSS named colors (Level 4).
const NAMED_COLORS: &[&str] = &[
  "aliceblue",
  "antiquewhite",
  "aqua",
  "aquamarine",
  "azure",
  "beige",
  "bisque",
  "black",
  "blanchedalmond",
  "blue",
  "blueviolet",
  "brown",
  "burlywood",
  "cadetblue",
  "chartreuse",
  "chocolate",
  "coral",
  "cornflowerblue",
  "cornsilk",
  "crimson",
  "cyan",
  "darkblue",
  "darkcyan",
  "darkgoldenrod",
  "darkgray",
  "darkgreen",
  "darkgrey",
  "darkkhaki",
  "darkmagenta",
  "darkolivegreen",
  "darkorange",
  "darkorchid",
  "darkred",
  "darksalmon",
  "darkseagreen",
  "darkslateblue",
  "darkslategray",
  "darkslategrey",
  "darkturquoise",
  "darkviolet",
  "deeppink",
  "deepskyblue",
  "dimgray",
  "dimgrey",
  "dodgerblue",
  "firebrick",
  "floralwhite",
  "forestgreen",
  "fuchsia",
  "gainsboro",
  "ghostwhite",
  "gold",
  "goldenrod",
  "gray",
  "green",
  "greenyellow",
  "grey",
  "honeydew",
  "hotpink",
  "indianred",
  "indigo",
  "ivory",
  "khaki",
  "lavender",
  "lavenderblush",
  "lawngreen",
  "lemonchiffon",
  "lightblue",
  "lightcoral",
  "lightcyan",
  "lightgoldenrodyellow",
  "lightgray",
  "lightgreen",
  "lightgrey",
  "lightpink",
  "lightsalmon",
  "lightseagreen",
  "lightskyblue",
  "lightslategray",
  "lightslategrey",
  "lightsteelblue",
  "lightyellow",
  "lime",
  "limegreen",
  "linen",
  "magenta",
  "maroon",
  "mediumaquamarine",
  "mediumblue",
  "mediumorchid",
  "mediumpurple",
  "mediumseagreen",
  "mediumslateblue",
  "mediumspringgreen",
  "mediumturquoise",
  "mediumvioletred",
  "midnightblue",
  "mintcream",
  "mistyrose",
  "moccasin",
  "navajowhite",
  "navy",
  "oldlace",
  "olive",
  "olivedrab",
  "orange",
  "orangered",
  "orchid",
  "palegoldenrod",
  "palegreen",
  "paleturquoise",
  "palevioletred",
  "papayawhip",
  "peachpuff",
  "peru",
  "pink",
  "plum",
  "powderblue",
  "purple",
  "rebeccapurple",
  "red",
  "rosybrown",
  "royalblue",
  "saddlebrown",
  "salmon",
  "sandybrown",
  "seagreen",
  "seashell",
  "sienna",
  "silver",
  "skyblue",
  "slateblue",
  "slategray",
  "slategrey",
  "snow",
  "springgreen",
  "steelblue",
  "tan",
  "teal",
  "thistle",
  "tomato",
  "turquoise",
  "violet",
  "wheat",
  "white",
  "whitesmoke",
  "yellow",
  "yellowgreen",
];

/// CSS system colors and special keywords that look like named colors but aren't.
const SYSTEM_COLORS: &[&str] = &[
  "transparent",
  "currentcolor",
  "inherit",
  "initial",
  "unset",
  "revert",
  "revert-layer",
  // CSS system colors
  "activeborder",
  "activecaption",
  "appworkspace",
  "background",
  "buttonface",
  "buttonhighlight",
  "buttonshadow",
  "buttontext",
  "captiontext",
  "graytext",
  "greytext",
  "highlight",
  "highlighttext",
  "inactiveborder",
  "inactivecaption",
  "inactivecaptiontext",
  "infobackground",
  "infotext",
  "menu",
  "menutext",
  "scrollbar",
  "threeddarkshadow",
  "threedface",
  "threedhighlight",
  "threedlightshadow",
  "threedshadow",
  "window",
  "windowframe",
  "windowtext",
  "canvas",
  "canvastext",
  "linktext",
  "visitedtext",
  "activetext",
  "field",
  "fieldtext",
  "mark",
  "marktext",
  "selecteditem",
  "selecteditemtext",
  "accentcolor",
  "accentcolortext",
];

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

/// Named color → hex mapping for "always-where-possible" mode.
/// Only includes colors that can be represented as standard hex (#RRGGBB).
fn named_color_to_rgb(name: &str) -> Option<(u8, u8, u8)> {
  match name {
    "black" => Some((0, 0, 0)),
    "silver" => Some((192, 192, 192)),
    "gray" | "grey" => Some((128, 128, 128)),
    "white" => Some((255, 255, 255)),
    "maroon" => Some((128, 0, 0)),
    "red" => Some((255, 0, 0)),
    "purple" => Some((128, 0, 128)),
    "fuchsia" | "magenta" => Some((255, 0, 255)),
    "green" => Some((0, 128, 0)),
    "lime" => Some((0, 255, 0)),
    "olive" => Some((128, 128, 0)),
    "yellow" => Some((255, 255, 0)),
    "navy" => Some((0, 0, 128)),
    "blue" => Some((0, 0, 255)),
    "teal" => Some((0, 128, 128)),
    "aqua" | "cyan" => Some((0, 255, 255)),
    "aliceblue" => Some((240, 248, 255)),
    "antiquewhite" => Some((250, 235, 215)),
    "aquamarine" => Some((127, 255, 212)),
    "azure" => Some((240, 255, 255)),
    "beige" => Some((245, 245, 220)),
    "bisque" => Some((255, 228, 196)),
    "blanchedalmond" => Some((255, 235, 205)),
    "blueviolet" => Some((138, 43, 226)),
    "brown" => Some((165, 42, 42)),
    "burlywood" => Some((222, 184, 135)),
    "cadetblue" => Some((95, 158, 160)),
    "chartreuse" => Some((127, 255, 0)),
    "chocolate" => Some((210, 105, 30)),
    "coral" => Some((255, 127, 80)),
    "cornflowerblue" => Some((100, 149, 237)),
    "cornsilk" => Some((255, 248, 220)),
    "crimson" => Some((220, 20, 60)),
    "darkblue" => Some((0, 0, 139)),
    "darkcyan" => Some((0, 139, 139)),
    "darkgoldenrod" => Some((184, 134, 11)),
    "darkgray" | "darkgrey" => Some((169, 169, 169)),
    "darkgreen" => Some((0, 100, 0)),
    "darkkhaki" => Some((189, 183, 107)),
    "darkmagenta" => Some((139, 0, 139)),
    "darkolivegreen" => Some((85, 107, 47)),
    "darkorange" => Some((255, 140, 0)),
    "darkorchid" => Some((153, 50, 204)),
    "darkred" => Some((139, 0, 0)),
    "darksalmon" => Some((233, 150, 122)),
    "darkseagreen" => Some((143, 188, 143)),
    "darkslateblue" => Some((72, 61, 139)),
    "darkslategray" | "darkslategrey" => Some((47, 79, 79)),
    "darkturquoise" => Some((0, 206, 209)),
    "darkviolet" => Some((148, 0, 211)),
    "deeppink" => Some((255, 20, 147)),
    "deepskyblue" => Some((0, 191, 255)),
    "dimgray" | "dimgrey" => Some((105, 105, 105)),
    "dodgerblue" => Some((30, 144, 255)),
    "firebrick" => Some((178, 34, 34)),
    "floralwhite" => Some((255, 250, 240)),
    "forestgreen" => Some((34, 139, 34)),
    "gainsboro" => Some((220, 220, 220)),
    "ghostwhite" => Some((248, 248, 255)),
    "gold" => Some((255, 215, 0)),
    "goldenrod" => Some((218, 165, 32)),
    "greenyellow" => Some((173, 255, 47)),
    "honeydew" => Some((240, 255, 240)),
    "hotpink" => Some((255, 105, 180)),
    "indianred" => Some((205, 92, 92)),
    "indigo" => Some((75, 0, 130)),
    "ivory" => Some((255, 255, 240)),
    "khaki" => Some((240, 230, 140)),
    "lavender" => Some((230, 230, 250)),
    "lavenderblush" => Some((255, 240, 245)),
    "lawngreen" => Some((124, 252, 0)),
    "lemonchiffon" => Some((255, 250, 205)),
    "lightblue" => Some((173, 216, 230)),
    "lightcoral" => Some((240, 128, 128)),
    "lightcyan" => Some((224, 255, 255)),
    "lightgoldenrodyellow" => Some((250, 250, 210)),
    "lightgray" | "lightgrey" => Some((211, 211, 211)),
    "lightgreen" => Some((144, 238, 144)),
    "lightpink" => Some((255, 182, 193)),
    "lightsalmon" => Some((255, 160, 122)),
    "lightseagreen" => Some((32, 178, 170)),
    "lightskyblue" => Some((135, 206, 250)),
    "lightslategray" | "lightslategrey" => Some((119, 136, 153)),
    "lightsteelblue" => Some((176, 196, 222)),
    "lightyellow" => Some((255, 255, 224)),
    "limegreen" => Some((50, 205, 50)),
    "linen" => Some((250, 240, 230)),
    "mediumaquamarine" => Some((102, 205, 170)),
    "mediumblue" => Some((0, 0, 205)),
    "mediumorchid" => Some((186, 85, 211)),
    "mediumpurple" => Some((147, 111, 219)),
    "mediumseagreen" => Some((60, 179, 113)),
    "mediumslateblue" => Some((123, 104, 238)),
    "mediumspringgreen" => Some((0, 250, 154)),
    "mediumturquoise" => Some((72, 209, 204)),
    "mediumvioletred" => Some((199, 21, 133)),
    "midnightblue" => Some((25, 25, 112)),
    "mintcream" => Some((245, 255, 250)),
    "mistyrose" => Some((255, 228, 225)),
    "moccasin" => Some((255, 228, 181)),
    "navajowhite" => Some((255, 222, 173)),
    "oldlace" => Some((253, 245, 230)),
    "olivedrab" => Some((107, 142, 35)),
    "orange" => Some((255, 165, 0)),
    "orangered" => Some((255, 69, 0)),
    "orchid" => Some((218, 112, 214)),
    "palegoldenrod" => Some((238, 232, 170)),
    "palegreen" => Some((152, 251, 152)),
    "paleturquoise" => Some((175, 238, 238)),
    "palevioletred" => Some((219, 112, 147)),
    "papayawhip" => Some((255, 239, 213)),
    "peachpuff" => Some((255, 218, 185)),
    "peru" => Some((205, 133, 63)),
    "pink" => Some((255, 192, 203)),
    "plum" => Some((221, 160, 221)),
    "powderblue" => Some((176, 224, 230)),
    "rebeccapurple" => Some((102, 51, 153)),
    "rosybrown" => Some((188, 143, 143)),
    "royalblue" => Some((65, 105, 225)),
    "saddlebrown" => Some((139, 69, 19)),
    "salmon" => Some((250, 128, 114)),
    "sandybrown" => Some((244, 164, 96)),
    "seagreen" => Some((46, 139, 87)),
    "seashell" => Some((255, 245, 238)),
    "sienna" => Some((160, 82, 45)),
    "skyblue" => Some((135, 206, 235)),
    "slateblue" => Some((106, 90, 205)),
    "slategray" | "slategrey" => Some((112, 128, 144)),
    "snow" => Some((255, 250, 250)),
    "springgreen" => Some((0, 255, 127)),
    "steelblue" => Some((70, 130, 180)),
    "tan" => Some((210, 180, 140)),
    "thistle" => Some((216, 191, 216)),
    "tomato" => Some((255, 99, 71)),
    "turquoise" => Some((64, 224, 208)),
    "violet" => Some((238, 130, 238)),
    "wheat" => Some((245, 222, 179)),
    "whitesmoke" => Some((245, 245, 245)),
    "yellowgreen" => Some((154, 205, 50)),
    _ => None,
  }
}

/// Reverse lookup: find a named color for (r, g, b), if one exists.
fn rgb_to_named_color(r: u8, g: u8, b: u8) -> Option<&'static str> {
  // We iterate named colors and check for a match.
  for &name in NAMED_COLORS {
    if let Some((nr, ng, nb)) = named_color_to_rgb(name) {
      if nr == r && ng == g && nb == b {
        return Some(name);
      }
    }
  }
  None
}

impl Rule for ColorNamed {
  fn name(&self) -> &'static str {
    "color-named"
  }

  fn description(&self) -> &'static str {
    "Require or disallow named colors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let option = ctx.primary_option_str().unwrap_or("never");

    // Read secondary options
    let secondary = ctx
      .secondary_options()
      .or_else(|| ctx.options.filter(|v| v.is_object()));

    let ignore_inside_function = secondary
      .and_then(|obj| obj.get("ignore"))
      .and_then(|v| v.as_array())
      .map(|arr| arr.iter().any(|v| v.as_str() == Some("inside-function")))
      .unwrap_or(false);

    let ignore_properties: Vec<String> = secondary
      .and_then(|obj| obj.get("ignoreProperties"))
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|v| v.as_str().map(|s| s.to_string()))
          .collect()
      })
      .unwrap_or_default();

    let mut diags = Vec::new();

    for decl in &rule.declarations {
      // Check if this property should be ignored
      let prop_lower = decl.property.to_ascii_lowercase();
      if should_ignore_property(&prop_lower, &ignore_properties) {
        continue;
      }

      // Get value from source text
      let decl_start = decl.span.offset;
      let decl_end = decl_start + decl.span.length;
      let (value, value_offset) = if decl_end <= ctx.source.len() && decl_start < decl_end {
        let full = &ctx.source[decl_start..decl_end];
        if let Some(colon_pos) = full.find(':') {
          let after_colon = &full[colon_pos + 1..];
          let trimmed = after_colon.trim_start();
          let leading_ws = after_colon.len() - trimmed.len();
          let val = trimmed.trim_end_matches(';').trim_end();
          (val, decl_start + colon_pos + 1 + leading_ws)
        } else {
          (&ctx.source[decl_start..decl_end], decl_start)
        }
      } else {
        (decl.value.as_str(), decl_start)
      };

      match option {
        "never" => {
          check_never(
            self,
            value,
            value_offset,
            &prop_lower,
            ignore_inside_function,
            &mut diags,
          );
        }
        "always-where-possible" => {
          check_always_where_possible(self, value, value_offset, &mut diags);
        }
        _ => {}
      }
    }
    diags
  }
}

/// Check "never" mode: find named colors that should not be used.
fn check_never(
  rule: &ColorNamed,
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
        if is_named_color(&lower) && !is_system_color(&lower) {
          let abs_offset = value_offset + offset;
          diags.push(
            Diagnostic::new(rule.name(), format!("Unexpected named color \"{}\"", lower))
              .severity(rule.default_severity())
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
            rule,
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
fn check_always_where_possible(
  rule: &ColorNamed,
  value: &str,
  value_offset: usize,
  diags: &mut Vec<Diagnostic>,
) {
  let tokens = tokenize_value(value);

  for token in &tokens {
    match token {
      ValueToken::Hash { text, offset } => {
        // Check if this hex color has a named equivalent
        if let Some(rgb) = parse_hex_to_rgb(text) {
          if let Some(name) = rgb_to_named_color(rgb.0, rgb.1, rgb.2) {
            let abs_offset = value_offset + offset;
            diags.push(
              Diagnostic::new(
                rule.name(),
                format!("Expected \"{name}\" instead of \"#{text}\""),
              )
              .severity(rule.default_severity())
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
                if let Some(color_name) = rgb_to_named_color(r, g, b) {
                  let abs_offset = value_offset + fn_offset - name.len() - 1;
                  // Point to the start of the function name
                  let func_start = value_offset + fn_offset - name.len() - 1;
                  diags.push(
                    Diagnostic::new(
                      rule.name(),
                      format!(
                        "Expected \"{color_name}\" instead of \"{fn_lower}({})\"|",
                        args.trim()
                      ),
                    )
                    .severity(rule.default_severity())
                    .span(Span::new(func_start, *full_len)),
                  );
                }
              }
            }
          }
          "hsl" | "hsla" => {
            if let Some((r, g, b, fully_opaque)) = parse_hsl_args(args) {
              if fully_opaque {
                if let Some(color_name) = rgb_to_named_color(r, g, b) {
                  let func_start = value_offset + fn_offset - name.len() - 1;
                  diags.push(
                    Diagnostic::new(
                      rule.name(),
                      format!(
                        "Expected \"{color_name}\" instead of \"{fn_lower}({})\"|",
                        args.trim()
                      ),
                    )
                    .severity(rule.default_severity())
                    .span(Span::new(func_start, *full_len)),
                  );
                }
              }
            }
          }
          "hwb" => {
            if let Some((r, g, b, fully_opaque)) = parse_hwb_args(args) {
              if fully_opaque {
                if let Some(color_name) = rgb_to_named_color(r, g, b) {
                  let func_start = value_offset + fn_offset - name.len() - 1;
                  diags.push(
                    Diagnostic::new(
                      rule.name(),
                      format!(
                        "Expected \"{color_name}\" instead of \"{fn_lower}({})\"|",
                        args.trim()
                      ),
                    )
                    .severity(rule.default_severity())
                    .span(Span::new(func_start, *full_len)),
                  );
                }
              }
            }
          }
          "gray" => {
            if let Some((r, g, b, fully_opaque)) = parse_gray_args(args) {
              if fully_opaque {
                if let Some(color_name) = rgb_to_named_color(r, g, b) {
                  let func_start = value_offset + fn_offset - name.len() - 1;
                  diags.push(
                    Diagnostic::new(
                      rule.name(),
                      format!(
                        "Expected \"{color_name}\" instead of \"{fn_lower}({})\"|",
                        args.trim()
                      ),
                    )
                    .severity(rule.default_severity())
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
                  if let Some(color_name) = rgb_to_named_color(rgb.0, rgb.1, rgb.2) {
                    let func_start = value_offset + fn_offset - name.len() - 1;
                    diags.push(
                      Diagnostic::new(
                        rule.name(),
                        format!("Expected \"{color_name}\" instead of \"#{text}\""),
                      )
                      .severity(rule.default_severity())
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

fn is_named_color(word: &str) -> bool {
  NAMED_COLORS.contains(&word)
}

fn is_system_color(word: &str) -> bool {
  SYSTEM_COLORS.contains(&word)
}

fn is_non_color_property(prop: &str) -> bool {
  NON_COLOR_PROPERTIES.contains(&prop)
}

/// Check if property should be ignored based on ignoreProperties config.
fn should_ignore_property(prop: &str, ignore_properties: &[String]) -> bool {
  for pattern in ignore_properties {
    if pattern.starts_with('/') && pattern.ends_with('/') {
      // Regex pattern
      let regex_str = &pattern[1..pattern.len() - 1];
      if let Ok(re) = regex::Regex::new(regex_str) {
        if re.is_match(prop) {
          return true;
        }
      }
    } else if pattern.eq_ignore_ascii_case(prop) {
      return true;
    }
  }
  false
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
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_decl(property: &str, value: &str) -> (CssNode, String) {
    let source = format!("a {{ {property}: {value}; }}");
    let prop_start = 4;
    let decl_text = format!("{property}: {value}");
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: property.to_string(),
        value: value.to_string(),
        span: ParserSpan::new(prop_start, decl_text.len()),
        important: false,
      }],
      span: ParserSpan::new(0, source.len()),
      ..Default::default()
    });
    (node, source)
  }

  #[test]
  fn reports_named_color() {
    let (node, source) = style_with_decl("color", "red");
    let c = RuleContext {
      source: &source,
      ..ctx()
    };
    let d = ColorNamed.check(&node, &c);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("red"));
  }

  #[test]
  fn allows_hex_color() {
    let (node, source) = style_with_decl("color", "#ff0000");
    let c = RuleContext {
      source: &source,
      ..ctx()
    };
    let d = ColorNamed.check(&node, &c);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_url_function() {
    let (node, source) = style_with_decl("background-image", "url(./black.png)");
    let c = RuleContext {
      source: &source,
      ..ctx()
    };
    let d = ColorNamed.check(&node, &c);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_animation_property() {
    let (node, source) = style_with_decl("animation", "blue 2s linear");
    let c = RuleContext {
      source: &source,
      ..ctx()
    };
    let d = ColorNamed.check(&node, &c);
    assert!(d.is_empty());
  }

  #[test]
  fn skips_font_family() {
    let (node, source) = style_with_decl("font-family", "blue");
    let c = RuleContext {
      source: &source,
      ..ctx()
    };
    let d = ColorNamed.check(&node, &c);
    assert!(d.is_empty());
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
}
