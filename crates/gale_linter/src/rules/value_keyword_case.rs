use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::chars::is_word_or_hyphen_byte;
use crate::data::is_css_wide_keyword;
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, LetterCase, Matchers, Setting};
use crate::rule::{Check, RuleContext, declare_rule};
use crate::standard_syntax::is_standard_syntax_value;
use crate::stylelint_version::stylelint_major_version;

mod keywords;
mod tokenize;

use keywords::{
  SVG_CAMEL_CASE_KEYWORDS, is_custom_ident_property, is_list_style_type_keyword,
  is_mixed_ident_property, is_svg_camel_case_keyword, is_system_color,
};
use tokenize::{TokenKind, ValueToken, tokenize_value};

use crate::data::is_generic_font_family;

const NAME: &str = "value-keyword-case";

declare_rule! {
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
  pub struct ValueKeywordCase => ValueKeywordCaseCheck {
    name: NAME,
    description: "Specify lowercase or uppercase for keyword values",
    default_severity: Warning,
  }
}

/// `value-keyword-case` as configured.
pub struct ValueKeywordCaseCheck {
  /// The case keywords are written in.
  case: LetterCase,
  /// `ignoreKeywords`: names and `/regex/` literals.
  ignore_keywords: Matchers,
  /// `ignoreProperties`, whose names match in any case.
  ignore_properties: Matchers,
  /// `ignoreFunctions`.
  ignore_functions: Matchers,
  /// `camelCaseSvgKeywords`.
  camel_case_svg_keywords: bool,
}

impl ValueKeywordCaseCheck {
  /// Stylelint's options: `"lower"` or `"upper"`, then `ignoreProperties`,
  /// `ignoreKeywords`, `ignoreFunctions` and `camelCaseSvgKeywords`.  Gale
  /// reads no option as `"lower"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let case = options.primary_case();
    let ignore_properties = options.strings("ignoreProperties");
    let ignore_keywords = options.strings("ignoreKeywords");
    let ignore_functions = options.strings("ignoreFunctions");
    let camel_case_svg_keywords = options.flag("camelCaseSvgKeywords");
    options.finish()?;
    Ok(Self {
      case,
      ignore_keywords: Matchers::new(&ignore_keywords),
      ignore_properties: Matchers::ignoring_case(&ignore_properties),
      ignore_functions: Matchers::new(&ignore_functions),
      camel_case_svg_keywords,
    })
  }
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
    if is_css_wide_keyword(token_text) {
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
  if is_css_wide_keyword(token_text) {
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
fn is_in_ignored_function(value: &str, token_offset: usize, ignore_fns: &Matchers) -> bool {
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
      while i < value.len() && is_word_or_hyphen_byte(bytes[i]) {
        i += 1;
      }
      if i < value.len() && bytes[i] == b'(' {
        let name = value[start..i].to_string();
        i += 1;
        func_stack.push((name, i));
        continue;
      }
      if start == token_offset {
        return func_stack.iter().any(|(name, _)| ignore_fns.matches(name));
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

impl Check for ValueKeywordCaseCheck {
  /// Flags keyword values not in the configured case, honouring the ignore
  /// secondaries and the SVG camelCase allowance.
  ///
  /// Every declaration counts, as with Stylelint's `walkDecls`: those
  /// directly inside at-rules, SCSS variables and nested properties, and
  /// ones whose value the CSS parser would reject.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let case = self.case;
    let ignore_keywords = &self.ignore_keywords;
    let ignore_properties = &self.ignore_properties;
    let ignore_functions = &self.ignore_functions;

    // Stylelint <=13 always treats camelCase SVG keywords (currentColor, etc.) as
    // canonical even without `camelCaseSvgKeywords: true`.  Stylelint 14+ introduced
    // the explicit option; without it the keywords are normalised to lowercase.
    let camel_case_svg = if stylelint_major_version() <= 13 {
      true
    } else {
      self.camel_case_svg_keywords
    };

    let mut diags = Vec::new();

    for decl in tree.walk_decls() {
      // Stylelint skips a declaration whose value is not standard syntax
      // (a variable, a module member, interpolation).
      if !is_standard_syntax_value(&decl.value) {
        continue;
      }
      let original_prop = decl.name.as_str();
      let prop = &original_prop.to_ascii_lowercase();

      // Check ignoreProperties
      if !ignore_properties.is_empty() {
        let prop_matches =
          ignore_properties.matches(prop) || ignore_properties.matches(original_prop);
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
      let value_to_tokenize = decl.written_value();
      let value_abs_start = decl.value_span.start;

      let tokens = tokenize_value(value_to_tokenize);

      for (idx, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident && token.kind != TokenKind::Important {
          continue;
        }

        let text = &token.text;
        let abs_offset = value_abs_start + token.offset;

        // SVG camelCase keywords (currentColor, optimizeSpeed, crispEdges, etc.)
        if is_svg_camel_case_keyword(text) && camel_case_svg && case == LetterCase::Lower {
          // "lower" + camelCaseSvgKeywords:true: expected = camelCase canonical
          let canonical = SVG_CAMEL_CASE_KEYWORDS
            .iter()
            .find(|k| k.eq_ignore_ascii_case(text))
            .unwrap();
          if *text != **canonical {
            diags.push(
              Diagnostic::new(
                NAME,
                format!("Expected \"{}\" to be \"{}\"", text, canonical),
              )
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

        // Property-context filtering
        if token.kind == TokenKind::Ident && !should_check_keyword(text, prop, &tokens, idx) {
          continue;
        }

        // ignoreKeywords
        if !ignore_keywords.is_empty() && ignore_keywords.matches(text) {
          continue;
        }

        // ignoreFunctions
        if !ignore_functions.is_empty()
          && is_in_ignored_function(value_to_tokenize, token.offset, ignore_functions)
        {
          continue;
        }

        if case.is_wrong(text) {
          let expected = case.apply(text);
          diags.push(
            Diagnostic::new(
              NAME,
              format!("Expected \"{}\" to be \"{}\"", text, expected),
            )
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
mod tests;
