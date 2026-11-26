//! Function names CSS defines, and the function names a value calls.

use crate::chars::is_word_or_hyphen_byte;

/// Whether `name` is a function CSS itself defines (stylelint-scss's
/// `isNativeCssFunction`), matched exactly: Sass treats a call to one as
/// plain CSS.
pub fn is_native_css_function(name: &str) -> bool {
  NATIVE_CSS_FUNCTIONS.binary_search(&name).is_ok()
}

/// stylelint-scss's `nativeCssFunctions`, in byte order for the binary
/// search.
const NATIVE_CSS_FUNCTIONS: &[&str] = &[
  "abs",
  "acos",
  "anchor",
  "anchor-size",
  "annotation",
  "asin",
  "atan",
  "atan2",
  "attr",
  "blur",
  "brightness",
  "calc",
  "calc-size",
  "character-variant",
  "circle",
  "clamp",
  "color",
  "color-contrast",
  "color-mix",
  "conic-gradient",
  "contrast",
  "cos",
  "counter",
  "counters",
  "cross-fade",
  "cubic-bezier",
  "device-cmyk",
  "drop-shadow",
  "element",
  "ellipse",
  "env",
  "exp",
  "fit-content",
  "format",
  "frames",
  "grayscale",
  "hsl",
  "hsla",
  "hue-rotate",
  "hwb",
  "hypot",
  "image",
  "image-set",
  "inset",
  "invert",
  "lab",
  "layer",
  "lch",
  "leader",
  "light-dark",
  "linear",
  "linear-gradient",
  "local",
  "log",
  "matrix",
  "matrix3d",
  "max",
  "min",
  "minmax",
  "mod",
  "oklab",
  "oklch",
  "opacity",
  "ornaments",
  "paint",
  "palette-mix",
  "path",
  "perspective",
  "polygon",
  "pow",
  "radial-gradient",
  "ray",
  "rect",
  "rem",
  "repeat",
  "repeating-conic-gradient",
  "repeating-linear-gradient",
  "repeating-radial-gradient",
  "rgb",
  "rgba",
  "rotate",
  "rotate3d",
  "rotateX",
  "rotateY",
  "rotateZ",
  "rotatex",
  "rotatey",
  "rotatez",
  "round",
  "saturate",
  "scale",
  "scale3d",
  "scaleX",
  "scaleY",
  "scaleZ",
  "scalex",
  "scaley",
  "scalez",
  "scroll",
  "sepia",
  "shape",
  "sign",
  "sin",
  "skew",
  "skewX",
  "skewY",
  "skewx",
  "skewy",
  "sqrt",
  "steps",
  "styleset",
  "stylistic",
  "swash",
  "symbols",
  "tan",
  "target-counter",
  "target-counters",
  "target-text",
  "translate",
  "translate3d",
  "translateX",
  "translateY",
  "translateZ",
  "translatex",
  "translatey",
  "translatez",
  "url",
  "var",
  "view",
  "xywh",
];

/// Stylelint's `mathFunctions`, in its order.
pub const MATH_FUNCTIONS: &[&str] = &[
  "abs",
  "acos",
  "asin",
  "atan",
  "calc",
  "cos",
  "exp",
  "sign",
  "sin",
  "sqrt",
  "tan",
  "atan2",
  "calc-size",
  "clamp",
  "hypot",
  "log",
  "max",
  "min",
  "mod",
  "pow",
  "rem",
  "round",
];

/// The function names called in `value`: the run of ASCII letters, digits,
/// `-` and `_` right before each `(`.  A module call such as
/// `color.adjust(` yields `adjust`.  `function-no-unknown` and
/// `scss/function-no-unknown` both read values this way.
pub fn extract_functions(value: &str) -> Vec<String> {
  let bytes = value.as_bytes();
  bytes
    .iter()
    .enumerate()
    .filter(|&(_, &b)| b == b'(')
    .filter_map(|(end, _)| {
      let name_len = bytes[..end]
        .iter()
        .rev()
        .take_while(|&&b| is_word_or_hyphen_byte(b))
        .count();
      (name_len > 0).then(|| value[end - name_len..end].to_string())
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn native_css_functions_are_sorted_for_the_binary_search() {
    assert!(
      NATIVE_CSS_FUNCTIONS
        .windows(2)
        .all(|pair| pair[0] < pair[1])
    );
    assert!(is_native_css_function("translateX"));
    assert!(!is_native_css_function("darken"));
  }
}
