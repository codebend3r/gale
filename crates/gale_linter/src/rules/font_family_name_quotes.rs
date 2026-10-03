use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::source_text::WrittenDeclaration;
use crate::standard_syntax::is_standard_syntax_value;
use crate::value_parser::{self, NodeKind, ValueNode};

/// Require or disallow quotes for font family names.
///
/// Options:
/// - `"always-where-recommended"`: names the CSS spec recommends quoting
///   (anything but letters and hyphens) must be quoted, others must not be.
/// - `"always-unless-keyword"`: every non-keyword family must be quoted.
/// - `"always-where-required"`: only names that are not valid identifiers
///   must be quoted; the rest must not be.
///
/// Keywords (`serif`, `inherit`, system fonts) must never be quoted.
///
/// Equivalent to Stylelint's `font-family-name-quotes` rule: families are
/// found in `font-family` and `font` values as written, the way Stylelint's
/// `findFontFamily` finds them, and the fix adds `"` quotes or removes the
/// existing ones in place.
pub struct FontFamilyNameQuotes;

/// Stylelint's `basicKeywords`.
const BASIC_KEYWORDS: &[&str] = &["initial", "inherit", "revert", "revert-layer", "unset"];

/// Stylelint's `fontFamilyKeywords`, without the basic keywords.
const FONT_FAMILY_KEYWORDS: &[&str] = &[
  "serif",
  "sans-serif",
  "cursive",
  "fantasy",
  "monospace",
  "system-ui",
  "ui-serif",
  "ui-sans-serif",
  "ui-monospace",
  "ui-rounded",
  "emoji",
  "math",
  "fangsong",
];

/// Stylelint's `fontSizeKeywords`, without the basic keywords.
const FONT_SIZE_KEYWORDS: &[&str] = &[
  "xx-small",
  "x-small",
  "small",
  "medium",
  "large",
  "x-large",
  "xx-large",
  "xxx-large",
  "larger",
  "smaller",
  "math",
  "-konq-xxx-large",
  "-webkit-xxx-large",
];

/// The other keywords of Stylelint's `fontShorthandKeywords`: font style,
/// variant, weight, stretch and line height keywords.
const OTHER_FONT_SHORTHAND_KEYWORDS: &[&str] = &[
  "normal",
  "italic",
  "oblique",
  "none",
  "small-caps",
  "bold",
  "bolder",
  "lighter",
  "100",
  "200",
  "300",
  "400",
  "500",
  "600",
  "700",
  "800",
  "900",
  "semi-condensed",
  "condensed",
  "extra-condensed",
  "ultra-condensed",
  "semi-expanded",
  "expanded",
  "extra-expanded",
  "ultra-expanded",
];

/// Stylelint's `prefixedSystemFonts`.
const PREFIXED_SYSTEM_FONTS: &[&str] = &[
  "-apple-system",
  "-apple-system-headline",
  "-apple-system-body",
  "-apple-system-subheadline",
  "-apple-system-footnote",
  "-apple-system-caption1",
  "-apple-system-caption2",
  "-apple-system-short-headline",
  "-apple-system-short-body",
  "-apple-system-short-subheadline",
  "-apple-system-short-footnote",
  "-apple-system-short-caption1",
  "-apple-system-tall-body",
  "-apple-system-title0",
  "-apple-system-title1",
  "-apple-system-title2",
  "-apple-system-title3",
  "-apple-system-title4",
  "-moz-button",
  "-moz-desktop",
  "-moz-dialog",
  "-moz-document",
  "-moz-field",
  "-moz-fixed",
  "-moz-info",
  "-moz-list",
  "-moz-pull-down-menu",
  "-moz-window",
  "-moz-workspace",
  "-webkit-body",
  "-webkit-control",
  "-webkit-mini-control",
  "-webkit-pictograph",
  "-webkit-small-control",
  "-webkit-standard",
];

/// Stylelint's `lengthUnits`.
const LENGTH_UNITS: &[&str] = &[
  "cap", "ch", "em", "ex", "ic", "lh", "rcap", "rch", "rem", "rex", "ric", "rlh", "dvb", "dvh",
  "dvi", "dvmax", "dvmin", "dvw", "lvb", "lvh", "lvi", "lvmax", "lvmin", "lvw", "svb", "svh",
  "svi", "svmax", "svmin", "svw", "vb", "vh", "vi", "vw", "vmin", "vmax", "vm", "px", "mm", "cm",
  "in", "pt", "pc", "q", "mozmm", "fr", "cqw", "cqh", "cqi", "cqb", "cqmin", "cqmax",
];

/// Whether `lower` is one of Stylelint's `fontFamilyKeywords`.
fn is_font_family_keyword(lower: &str) -> bool {
  BASIC_KEYWORDS.contains(&lower) || FONT_FAMILY_KEYWORDS.contains(&lower)
}

/// Whether `word` is one of Stylelint's `fontSizeKeywords`.
fn is_font_size_keyword(word: &str) -> bool {
  BASIC_KEYWORDS.contains(&word) || FONT_SIZE_KEYWORDS.contains(&word)
}

/// Whether `lower` is one of Stylelint's `fontShorthandKeywords`.
fn is_font_shorthand_keyword(lower: &str) -> bool {
  is_font_family_keyword(lower)
    || is_font_size_keyword(lower)
    || OTHER_FONT_SHORTHAND_KEYWORDS.contains(&lower)
}

/// Stylelint's `isSystemFontKeyword` (case-sensitive).
fn is_system_font_keyword(font: &str) -> bool {
  PREFIXED_SYSTEM_FONTS.contains(&font) || font == "BlinkMacSystemFont"
}

/// Stylelint's `isValidFontSize`: a font size keyword, a percentage or a
/// length.
fn is_valid_font_size(word: &str) -> bool {
  if word.is_empty() {
    return false;
  }
  if is_font_size_keyword(word) {
    return true;
  }
  value_parser::unit(word).is_some_and(|(_, unit)| {
    unit == "%" || LENGTH_UNITS.contains(&unit.to_ascii_lowercase().as_str())
  })
}

/// Stylelint's `isNumbery`: whether JavaScript's `Number()` reads the text
/// as a number.
fn is_numbery(value: &str) -> bool {
  let text = value.trim();
  if text.is_empty() {
    return false;
  }
  let radix = |prefix: &str, radix: u32| {
    text
      .get(..2)
      .is_some_and(|p| p.eq_ignore_ascii_case(prefix))
      && text.len() > 2
      && text[2..].chars().all(|c| c.is_digit(radix))
  };
  if radix("0x", 16) || radix("0o", 8) || radix("0b", 2) {
    return true;
  }
  let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
  if unsigned == "Infinity" {
    return true;
  }
  let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
    Some(i) => (&unsigned[..i], Some(&unsigned[i + 1..])),
    None => (unsigned, None),
  };
  let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
  let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
  let mantissa_ok = (!int.is_empty() || !frac.is_empty()) && digits(int) && digits(frac);
  let exponent_ok = exponent.is_none_or(|e| {
    let e = e.strip_prefix(['+', '-']).unwrap_or(e);
    !e.is_empty() && digits(e)
  });
  mantissa_ok && exponent_ok
}

/// Stylelint's `quotesRecommended`: anything but letters and hyphens.
fn quotes_recommended(family: &str) -> bool {
  family.is_empty() || !family.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-')
}

/// Stylelint's `quotesRequired`: some whitespace-separated word starts with
/// a digit (optionally after `-`) or `--`, or holds characters an
/// identifier cannot.
fn quotes_required(family: &str) -> bool {
  family.split(char::is_whitespace).any(|word| {
    let bytes = word.as_bytes();
    let starts_badly = bytes.first().is_some_and(u8::is_ascii_digit)
      || (bytes.first() == Some(&b'-')
        && bytes
          .get(1)
          .is_some_and(|b| b.is_ascii_digit() || *b == b'-'));
    let ident_chars = !word.is_empty()
      && word
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c >= '\u{a0}');
    starts_badly || !ident_chars
  })
}

/// A font family found in a value.
struct Family {
  /// The name: a word, a string's content, or words joined by the
  /// whitespace or divider between them.
  name: String,
  /// The quote of a string family.
  quote: Option<char>,
  /// Byte offset in the value where the family starts.
  source_index: usize,
}

impl Family {
  /// The family as written: its name, quoted if it was.
  fn raw_name(&self) -> String {
    match self.quote {
      Some(q) => format!("{q}{}{q}", self.name),
      None => self.name.clone(),
    }
  }
}

/// A [`Family`] for a single value node.
fn family_of(node: &ValueNode) -> Family {
  Family {
    name: node.value.to_string(),
    quote: (node.kind == NodeKind::String)
      .then_some(node.quote)
      .flatten(),
    source_index: node.source_index,
  }
}

/// Whether a node is the `,` divider.
fn is_comma(node: &ValueNode) -> bool {
  node.kind == NodeKind::Div && node.value == ","
}

/// Stylelint's `findFontFamily`: the font families in a `font` or
/// `font-family` value, skipping the other parts of the `font` shorthand.
fn find_font_family(value: &str) -> Vec<Family> {
  let nodes = value_parser::parse(value);
  if let [only] = nodes.as_slice()
    && BASIC_KEYWORDS.contains(&only.value.to_ascii_lowercase().as_str())
  {
    return vec![family_of(only)];
  }
  let mut families: Vec<Family> = Vec::new();
  let mut merge: Option<&str> = None;
  for (index, node) in nodes.iter().enumerate() {
    if !matches!(
      node.kind,
      NodeKind::Word | NodeKind::String | NodeKind::Space | NodeKind::Div
    ) {
      continue;
    }
    let lower = node.value.to_ascii_lowercase();
    if !is_standard_syntax_value(&lower) || lower.starts_with("var(") {
      continue;
    }
    let family_keyword = is_font_family_keyword(&lower);
    if !family_keyword && is_font_shorthand_keyword(&lower) {
      continue;
    }
    if !family_keyword && is_valid_font_size(node.value) {
      continue;
    }
    let next = nodes.get(index + 1);
    let prev = index.checked_sub(1).map(|i| &nodes[i]);
    let prev_prev = index.checked_sub(2).map(|i| &nodes[i]);
    // `math` is a size and a family: a family only at the end of the list
    // or after a comma.
    if family_keyword
      && is_font_size_keyword(&lower)
      && !(next.is_none_or(is_comma) || nodes[..index].iter().any(is_comma))
    {
      continue;
    }
    // A line height after `<font-size>/`.
    if prev.is_some_and(|p| p.value == "/")
      && prev_prev.is_some_and(|p| is_valid_font_size(p.value))
    {
      continue;
    }
    if is_numbery(&lower) {
      continue;
    }
    let separator = node.kind == NodeKind::Space || node.kind == NodeKind::Div;
    if separator && !(node.kind == NodeKind::Div && node.value == ",") && !families.is_empty() {
      merge = Some(node.value);
      continue;
    }
    if separator {
      continue;
    }
    match (merge.take(), families.last_mut()) {
      (Some(between), Some(last)) => {
        last.name.push_str(between);
        last.name.push_str(node.value);
      }
      _ => families.push(family_of(node)),
    }
  }
  families
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

  /// Flags font family names in `font-family` and `font` values whose
  /// quoting does not match the option.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let mode = ctx
      .primary_option_str()
      .unwrap_or("always-where-recommended");
    let mut diags = Vec::new();
    for &decl in ctx.written_declarations(node).iter() {
      let prop = decl.prop.to_ascii_lowercase();
      if (prop != "font" && prop != "font-family") || !is_standard_syntax_value(decl.value) {
        continue;
      }
      for family in find_font_family(decl.value) {
        self.check_family(ctx, &decl, &family, mode, &mut diags);
      }
    }
    diags
  }
}

impl FontFamilyNameQuotes {
  /// Report `family` if its quoting does not match `mode`.
  fn check_family(
    &self,
    ctx: &RuleContext,
    decl: &WrittenDeclaration,
    family: &Family,
    mode: &str,
    diags: &mut Vec<Diagnostic>,
  ) {
    let raw = family.raw_name();
    if raw.to_ascii_lowercase().starts_with("var(") {
      return;
    }
    let quoted = family.quote.is_some();
    let name = family.name.as_str();
    let keyword =
      is_font_family_keyword(&name.to_ascii_lowercase()) || is_system_font_keyword(name);
    let expect_quotes = if keyword {
      false
    } else {
      match mode {
        // Quotes are never wrong here, only missing ones.
        "always-unless-keyword" if quoted => return,
        "always-unless-keyword" => true,
        "always-where-required" => quotes_required(name),
        _ => quotes_recommended(name),
      }
    };
    if expect_quotes == quoted {
      return;
    }
    // Stylelint reports at the first occurrence of the family in the
    // declaration, or the whole declaration when it is not found there.
    let decl_text = ctx
      .source_slice(decl.prop_start, decl.value_start + decl.value.len())
      .unwrap_or("");
    let span = decl_text.find(&raw).map_or_else(
      || Span::new(decl.prop_start, decl_text.len()),
      |i| Span::new(decl.prop_start + i, raw.len()),
    );
    let start = decl.value_start + family.source_index;
    let (message, edit) = if expect_quotes {
      (
        format!("Expected quotes around \"{name}\""),
        Edit::new(Span::new(start, name.len()), format!("\"{name}\"")),
      )
    } else {
      (
        format!("Unexpected quotes around \"{name}\""),
        Edit::new(Span::new(start, name.len() + 2), name.to_string()),
      )
    };
    diags.push(
      Diagnostic::new(self.name(), message)
        .severity(self.default_severity())
        .span(span)
        .fix(Fix::new(
          if expect_quotes {
            "Add quotes"
          } else {
            "Remove quotes"
          },
          vec![edit],
        )),
    );
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "font-family-name-quotes";

  #[test]
  fn recommended_quotes_names_with_spaces_digits_and_punctuation() {
    let recommended = serde_json::json!("always-where-recommended");
    assert_eq!(
      fix(
        RULE,
        recommended.clone(),
        "a { font: 1em Lucida Grande, Arial, sans-serif; }",
        Syntax::Css
      ),
      "a { font: 1em \"Lucida Grande\", Arial, sans-serif; }"
    );
    assert_eq!(
      fix(
        RULE,
        recommended.clone(),
        "a { font-family: Arial, Ahem!, \"sans-serif\"; }",
        Syntax::Css
      ),
      "a { font-family: Arial, \"Ahem!\", sans-serif; }"
    );
    assert_eq!(
      fix(
        RULE,
        recommended.clone(),
        "a { font-family: \"Arial\"; }",
        Syntax::Css
      ),
      "a { font-family: Arial; }"
    );
    let warnings = lint(
      RULE,
      recommended,
      "a { font-family: Times, Times New Roman, serif; }",
      Syntax::Css,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(
      warnings[0].message,
      "Expected quotes around \"Times New Roman\""
    );
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (24, 15));
  }

  #[test]
  fn unless_keyword_quotes_everything_but_keywords() {
    let unless = serde_json::json!("always-unless-keyword");
    assert_eq!(
      fix(
        RULE,
        unless.clone(),
        "a { font-family: system-ui, '-apple-system', BlinkMacSystemFont, Segoe UI; }",
        Syntax::Css
      ),
      "a { font-family: system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\"; }"
    );
    assert_eq!(
      fix(
        RULE,
        unless.clone(),
        "a { font: italic 300 16px/30px Arial, serif; }",
        Syntax::Css
      ),
      "a { font: italic 300 16px/30px \"Arial\", serif; }"
    );
    assert_eq!(
      fix(RULE, unless, "a { font-family: \"inherit\"; }", Syntax::Css),
      "a { font-family: inherit; }"
    );
  }

  #[test]
  fn required_unquotes_valid_identifiers() {
    let required = serde_json::json!("always-where-required");
    assert_eq!(
      fix(
        RULE,
        required.clone(),
        "a { font-family: \"Lucida Grande\", Hawaii 5-0, serif; }",
        Syntax::Css
      ),
      "a { font-family: Lucida Grande, \"Hawaii 5-0\", serif; }"
    );
    for css in [
      "a { font-family: 1234, sans-serif; }",
      "a { font: normal 30px/1 dashicons; }",
      "a { font-family: var(--x); }",
      "a { font-family: $sassy; }",
    ] {
      assert!(
        lint(RULE, required.clone(), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!(["always-unless-keyword", { "disableFix": true }]);
    let css = "a { font-family: Arial; }";
    assert_eq!(lint(RULE, options.clone(), css, Syntax::Css).len(), 1);
    assert_eq!(fix(RULE, options, css, Syntax::Css), css);
  }
}
