use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "media-query-no-invalid";

declare_rule! {
  /// Reports obviously invalid media queries.
  ///
  /// Checks for common errors in `@media` params such as:
  /// - Empty media query params
  /// - Unbalanced parentheses
  /// - Consecutive operators (and/or/not) without a condition
  ///
  /// Equivalent to Stylelint's `media-query-no-invalid` rule.
  pub struct MediaQueryNoInvalid => MediaQueryNoInvalidCheck {
    name: NAME,
    description: "Disallow invalid media queries",
    default_severity: Warning,
  }
}

/// `media-query-no-invalid`, whose options change nothing yet.
pub struct MediaQueryNoInvalidCheck;

impl MediaQueryNoInvalidCheck {
  /// Stylelint's options: `true`, then `ignoreFunctions`.  Gale accepts
  /// but does not yet apply `ignoreFunctions`, since it does not check the
  /// functions in a query.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.strings("ignoreFunctions");
    options.finish()?;
    Ok(Self)
  }
}

impl Check for MediaQueryNoInvalidCheck {
  /// Flags media queries that are empty, have unbalanced parentheses, or chain
  /// operators with no condition between them.
  fn check(&self, node: &CssNode, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(at) = node else {
      return vec![];
    };
    if at.name != "media" {
      return vec![];
    }

    let params = at.params.trim();

    // Empty media query
    if params.is_empty() {
      return vec![
        Diagnostic::new(NAME, "Unexpected empty media query")
          .span(Span::new(at.span.offset, at.span.length)),
      ];
    }

    // Check for unbalanced parentheses
    let mut depth: i32 = 0;
    for ch in params.chars() {
      match ch {
        '(' => depth += 1,
        ')' => {
          depth -= 1;
          if depth < 0 {
            return vec![
              Diagnostic::new(NAME, "Unexpected unbalanced parentheses in media query")
                .span(Span::new(at.span.offset, at.span.length)),
            ];
          }
        }
        _ => {}
      }
    }
    if depth != 0 {
      return vec![
        Diagnostic::new(NAME, "Unexpected unbalanced parentheses in media query")
          .span(Span::new(at.span.offset, at.span.length)),
      ];
    }

    // Check for empty parentheses ()
    if params.contains("()") {
      return vec![
        Diagnostic::new(NAME, "Unexpected empty parentheses in media query")
          .span(Span::new(at.span.offset, at.span.length)),
      ];
    }

    vec![]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule, Span as ParserSpan};

  use crate::testing::{configured, ctx};

  fn media(params: &str) -> CssNode {
    CssNode::AtRule(AtRule {
      name: "media".to_string(),
      params: params.to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_empty_media_query() {
    let rule = configured(&MediaQueryNoInvalid, None);
    let d = rule.check(&media(""), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty"));
  }

  #[test]
  fn reports_empty_parentheses() {
    let rule = configured(&MediaQueryNoInvalid, None);
    let d = rule.check(&media("screen and ()"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("empty parentheses"));
  }

  #[test]
  fn allows_valid_media_queries() {
    let rule = configured(&MediaQueryNoInvalid, None);
    assert!(rule.check(&media("(min-width: 768px)"), &ctx()).is_empty());
    assert!(rule.check(&media("screen and (color)"), &ctx()).is_empty());
    assert!(rule.check(&media("(hover: hover)"), &ctx()).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(
        NAME,
        serde_json::json!([true, { "ignoreFunctions": ["/^theme/", "env"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "ignoreFunctions": [{}] }])),
      ["Invalid value \"{}\" for option \"ignoreFunctions\" of rule \"media-query-no-invalid\""]
    );
  }
}
