use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::pattern;
use crate::rule::{Rule, RuleContext};
use crate::selector::postcss::{self, Kind, Visit};
use crate::standard_syntax::is_standard_syntax_selector;

/// Require lowercase or uppercase for type selectors.
///
/// Equivalent to Stylelint's `selector-type-case` rule: every type selector
/// in a style rule's selector as written, nested ones included, is checked
/// against the `"lower"` (default) or `"upper"` option, and the fix
/// rewrites its case in place.  Case-sensitive SVG element names, the
/// arguments of `:nth-*()`, `:lang()`, `:dir()` and `::part()`, and names
/// in `ignoreTypes` are left alone.
pub struct SelectorTypeCase;

/// Stylelint's `mixedCaseSvgTypeSelectors`.
const SVG_CASE_SENSITIVE: &[&str] = &[
  "altGlyph",
  "altGlyphDef",
  "altGlyphItem",
  "animateColor",
  "animateMotion",
  "animateTransform",
  "clipPath",
  "feBlend",
  "feColorMatrix",
  "feComponentTransfer",
  "feComposite",
  "feConvolveMatrix",
  "feDiffuseLighting",
  "feDisplacementMap",
  "feDistantLight",
  "feDropShadow",
  "feFlood",
  "feFuncA",
  "feFuncB",
  "feFuncG",
  "feFuncR",
  "feGaussianBlur",
  "feImage",
  "feMerge",
  "feMergeNode",
  "feMorphology",
  "feOffset",
  "fePointLight",
  "feSpecularLighting",
  "feSpotLight",
  "feTile",
  "feTurbulence",
  "foreignObject",
  "glyphRef",
  "hatchPath",
  "linearGradient",
  "radialGradient",
  "textPath",
];

/// Pseudo-classes and pseudo-elements whose arguments postcss-selector-parser
/// reads as tags that are not type selectors.
const NON_SELECTOR_ARGUMENT_PSEUDOS: &[&str] = &[
  "nth-column",
  "nth-last-column",
  "nth-last-of-type",
  "nth-of-type",
  "nth-child",
  "nth-last-child",
  "dir",
  "lang",
  "part",
];

/// Stylelint's `namedTimelineRangeKeywords`.
const TIMELINE_RANGES: &[&str] = &[
  "contain",
  "cover",
  "entry",
  "entry-crossing",
  "exit",
  "exit-crossing",
];

impl Rule for SelectorTypeCase {
  fn name(&self) -> &'static str {
    "selector-type-case"
  }

  fn description(&self) -> &'static str {
    "Specify lowercase or uppercase for type selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags type selectors not in the configured case, reading every style
  /// rule's selector as written.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let upper = ctx.primary_option_str() == Some("upper");
    let ignore_types = ctx.secondary_options().and_then(|v| v.get("ignoreTypes"));

    let mut diags = Vec::new();
    for rule in &ctx.scanned_rules().style_rules {
      let clean = postcss::strip_comments(&rule.prelude);
      if !starts_a_tag_name(&clean) {
        continue;
      }
      let has_wrong_case = if upper {
        clean.bytes().any(|b| b.is_ascii_lowercase())
      } else {
        clean.bytes().any(|b| b.is_ascii_uppercase())
      };
      if !has_wrong_case || !is_standard_syntax_selector(&clean) {
        continue;
      }
      if postcss::rule_selectors(&clean)
        .iter()
        .any(|s| is_keyframe_selector(s))
      {
        continue;
      }
      let Some(selectors) = postcss::parse(&rule.prelude, rule.offset) else {
        continue;
      };
      postcss::walk(&selectors, &mut |visit| {
        let tag = visit.node;
        if tag.kind != Kind::Tag || !is_standard_syntax_type_selector(visit) {
          return;
        }
        if SVG_CASE_SENSITIVE.contains(&tag.value.as_str())
          || pattern::option_matches(ignore_types, &tag.value)
        {
          return;
        }
        let expected = if upper {
          tag.value.to_ascii_uppercase()
        } else {
          tag.value.to_ascii_lowercase()
        };
        if expected == tag.value {
          return;
        }
        let span = Span::new(tag.start, tag.value.len());
        diags.push(
          Diagnostic::new(
            self.name(),
            format!("Expected \"{}\" to be \"{expected}\"", tag.value),
          )
          .severity(self.default_severity())
          .span(span)
          .fix(Fix::new(
            format!("Change \"{}\" to \"{expected}\"", tag.value),
            vec![Edit::new(span, expected.clone())],
          )),
        );
      });
    }
    diags
  }
}

/// Stylelint's `STARTS_A_TAG_NAME_REGEX` (`/(?:[^.#[:a-z-]|^)[a-z]/i`): a
/// letter at the start, or after something that cannot begin a class, id,
/// attribute or pseudo.
fn starts_a_tag_name(selector: &str) -> bool {
  let bytes = selector.as_bytes();
  bytes.iter().enumerate().any(|(i, &b)| {
    b.is_ascii_alphabetic()
      && (i == 0 || {
        let prev = bytes[i - 1];
        !(matches!(prev, b'.' | b'#' | b'[' | b':' | b'-') || prev.is_ascii_alphabetic())
      })
  })
}

/// Stylelint's `isKeyframeSelector`: `from`, `to`, a percentage, or a
/// timeline range with a percentage.
fn is_keyframe_selector(selector: &str) -> bool {
  if selector == "from" || selector == "to" {
    return true;
  }
  if is_percentage(selector) {
    return true;
  }
  selector
    .split_once(char::is_whitespace)
    .is_some_and(|(range, rest)| {
      TIMELINE_RANGES
        .iter()
        .any(|r| r.eq_ignore_ascii_case(range))
        && is_percentage(rest.trim_start())
    })
}

/// Whether `text` is `/^(?:\d+|\d*\.\d+)%$/`.
fn is_percentage(text: &str) -> bool {
  let Some(number) = text.strip_suffix('%') else {
    return false;
  };
  match number.split_once('.') {
    Some((int, frac)) => {
      int.bytes().all(|b| b.is_ascii_digit())
        && !frac.is_empty()
        && frac.bytes().all(|b| b.is_ascii_digit())
    }
    None => !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()),
  }
}

/// Stylelint's `isStandardSyntaxTypeSelector`: not an argument of a pseudo
/// that takes no selectors, not a suffix after `&`, not a Sass placeholder
/// and not a `/deep/`-style combinator.
fn is_standard_syntax_type_selector(visit: &Visit) -> bool {
  if let Some(parent) = visit.parent_pseudo {
    let name = parent.value.trim_start_matches(':').to_ascii_lowercase();
    if NON_SELECTOR_ARGUMENT_PSEUDOS.contains(&name.as_str()) {
      return false;
    }
  }
  if visit.prev().is_some_and(|prev| prev.kind == Kind::Nesting) {
    return false;
  }
  let value = &visit.node.value;
  !(value.starts_with('%') || (value.starts_with('/') && value.ends_with('/')))
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "selector-type-case";

  #[test]
  fn lowercases_type_selectors_in_place() {
    let lower = serde_json::json!("lower");
    assert_eq!(
      fix(RULE, lower.clone(), "DIV::before {}", Syntax::Css),
      "div::before {}"
    );
    assert_eq!(
      fix(RULE, lower.clone(), "a { & B {}}", Syntax::Css),
      "a { & b {}}"
    );
    assert_eq!(
      fix(RULE, lower.clone(), "A:nth-child(even) {}", Syntax::Css),
      "a:nth-child(even) {}"
    );
    assert_eq!(
      fix(
        RULE,
        lower.clone(),
        "/* x */\nA, /* y */\nA:not(B) {}",
        Syntax::Css
      ),
      "/* x */\na, /* y */\na:not(b) {}"
    );
    let warnings = lint(RULE, lower, "a B {}", Syntax::Css);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].message, "Expected \"B\" to be \"b\"");
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (2, 1));
  }

  #[test]
  fn uppercases_with_the_upper_option() {
    let upper = serde_json::json!("upper");
    assert_eq!(
      fix(
        RULE,
        upper.clone(),
        "a { &:nth-child(3n + 1) {} }",
        Syntax::Css
      ),
      "A { &:nth-child(3n + 1) {} }"
    );
    assert_eq!(
      fix(RULE, upper.clone(), "A /*c*/\n b {}", Syntax::Css),
      "A /*c*/\n B {}"
    );
    for css in ["&LI {}", "A:nth-child(odd) {}", ".foo {}", "A, B, * {}"] {
      assert!(
        lint(RULE, upper.clone(), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
    for css in [".foo { &-bar {} }", "%foo {}", "#{$variable} {}"] {
      assert!(
        lint(RULE, upper.clone(), css, Syntax::Scss).is_empty(),
        "{css}"
      );
    }
  }

  #[test]
  fn leaves_svg_names_keyframes_and_ignored_types_alone() {
    let lower = serde_json::json!(["lower", { "ignoreTypes": ["/(p|P)arent.*/", "/foo$/i"] }]);
    for css in [
      "foreignObject {}",
      "html textPath { fill: red; }",
      "@include keyframes(identifier) { TO, 50.0% {} }",
      "myParentClass {}",
      "myFoo {}",
    ] {
      assert!(
        lint(RULE, lower.clone(), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!(["lower", { "disableFix": true }]);
    assert_eq!(lint(RULE, options.clone(), "A {}", Syntax::Css).len(), 1);
    assert_eq!(fix(RULE, options, "A {}", Syntax::Css), "A {}");
  }
}
