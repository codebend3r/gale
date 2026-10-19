use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "@stylistic/declaration-block-trailing-semicolon";

declare_rule! {
  /// Require or disallow a trailing semicolon within declaration blocks.
  ///
  /// Equivalent to `@stylistic/declaration-block-trailing-semicolon`.
  pub struct StylisticDeclarationBlockTrailingSemicolon => StylisticDeclarationBlockTrailingSemicolonCheck {
    name: NAME,
    description: "Require or disallow a trailing semicolon within declaration blocks",
    default_severity: Warning,
  }
}

/// `@stylistic/declaration-block-trailing-semicolon` as configured.
pub struct StylisticDeclarationBlockTrailingSemicolonCheck {
  /// The primary option: `"always"` or `"never"`.
  option: &'static str,
  /// `ignore: ["single-declaration"]`: pass over a block holding nothing but
  /// one declaration.
  ignore_single_declaration: bool,
}

impl StylisticDeclarationBlockTrailingSemicolonCheck {
  /// The plugin's options: `"always"` or `"never"`, then `ignore:
  /// ["single-declaration"]`.  Gale reads no option as `"always"`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let option = options.primary_one_of(&["always", "never"]);
    let ignore = options.keywords("ignore", &["single-declaration"]);
    options.finish()?;
    Ok(Self {
      option: option.unwrap_or("always"),
      ignore_single_declaration: ignore.contains(&"single-declaration"),
    })
  }
}

impl Check for StylisticDeclarationBlockTrailingSemicolonCheck {
  /// Flags a block whose last declaration's trailing semicolon does not match
  /// the option.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let option = self.option;

    if rule.declarations.is_empty() {
      return vec![];
    }

    // The plugin passes over a block whose only node, comments aside, is
    // this declaration.
    if self.ignore_single_declaration
      && rule.declarations.len() == 1
      && rule.children.is_empty()
      && rule
        .nested_at_rules
        .iter()
        .all(|node| matches!(node, CssNode::Comment(_)))
    {
      return vec![];
    }

    // Skip SCSS nested property blocks (value ends with '}')
    {
      let last = &rule.declarations[rule.declarations.len() - 1];
      if last.value.trim_end().ends_with('}') {
        return vec![];
      }
    }

    // Skip SCSS nested property blocks (selector ends with ':')
    if rule.selector.trim_end().ends_with(':') {
      return vec![];
    }

    let source = ctx.source;
    let rule_end = rule.span.offset + rule.span.length;

    // Find the closing brace
    if rule_end == 0 || rule_end > source.len() {
      return vec![];
    }

    // Look for the closing brace of this rule
    let block_source = &source[rule.span.offset..rule_end.min(source.len())];
    let closing_brace = match block_source.rfind('}') {
      Some(pos) => rule.span.offset + pos,
      None => return vec![],
    };

    // Find the last declaration's end
    let last_decl = &rule.declarations[rule.declarations.len() - 1];
    let last_decl_end = last_decl.span.offset + last_decl.span.length;

    // Check text between last declaration end and closing brace
    if last_decl_end > closing_brace || last_decl_end > source.len() {
      return vec![];
    }
    let between = &source[last_decl_end..closing_brace];

    // The parser's declaration span may or may not include the trailing
    // semicolon (lightningcss includes it, hand-crafted test spans may
    // not).  We must check both inside the span AND in the gap after it.
    let span_includes_semi = last_decl.span.length > 0
      && last_decl_end > 0
      && last_decl_end <= source.len()
      && source.as_bytes()[last_decl_end - 1] == b';';
    let has_semicolon = span_includes_semi || between.contains(';');

    match option {
      "always" if !has_semicolon => {
        vec![
          Diagnostic::new(NAME, "Expected a trailing semicolon")
            .span(Span::new(last_decl_end, 0))
            .fix(Fix::new(
              "Add trailing semicolon",
              vec![Edit::new(Span::new(last_decl_end, 0), ";")],
            )),
        ]
      }
      "never" if has_semicolon => {
        // Find the semicolon position — it may be the last byte of the
        // declaration span or somewhere in the gap before the closing
        // brace.
        let abs_semi = if span_includes_semi {
          last_decl_end - 1
        } else if let Some(pos) = between.find(';') {
          last_decl_end + pos
        } else {
          return vec![];
        };
        vec![
          Diagnostic::new(NAME, "Unexpected trailing semicolon")
            .span(Span::new(abs_semi, 1))
            .fix(Fix::new(
              "Remove trailing semicolon",
              vec![Edit::new(Span::new(abs_semi, 1), "")],
            )),
        ]
      }
      _ => vec![],
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, StyleRule, Syntax};

  use crate::testing::{configured, ctx_with_source, invalid_options};

  #[test]
  fn allows_trailing_semicolon() {
    let rule = configured(&StylisticDeclarationBlockTrailingSemicolon, None);
    // Span does NOT include the semicolon (semi is in the gap)
    let source = "a { color: red; }";
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: Span::new(4, 10),
        important: false,
      }],
      span: Span::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_trailing_semicolon_span_includes_semi() {
    let rule = configured(&StylisticDeclarationBlockTrailingSemicolon, None);
    // Span includes the semicolon (what lightningcss actually produces)
    let source = "a { color: red; }";
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: Span::new(4, 11), // includes the ';'
        important: false,
      }],
      span: Span::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert!(d.is_empty());
  }

  #[test]
  fn allows_multiline_trailing_semicolon() {
    let rule = configured(&StylisticDeclarationBlockTrailingSemicolon, None);
    // Mimics the Grafana false-positive case: multi-declaration block
    // where the parser span includes the ';' on the last declaration.
    let source = ".foo {\n  position: absolute;\n  top: 0;\n}";
    let ctx = ctx_with_source(source);
    // "top: 0;" starts at byte 29, length 7 (includes ';')
    let node = CssNode::Style(StyleRule {
      selector: ".foo".to_string(),
      declarations: vec![
        Declaration {
          property: "position".to_string(),
          value: "absolute".to_string(),
          span: Span::new(9, 19), // "position: absolute;"
          important: false,
        },
        Declaration {
          property: "top".to_string(),
          value: "0".to_string(),
          span: Span::new(31, 7), // "top: 0;"
          important: false,
        },
      ],
      span: Span::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert!(
      d.is_empty(),
      "should not report when trailing ; exists, got: {:?}",
      d
    );
  }

  #[test]
  fn reports_missing_trailing_semicolon() {
    let rule = configured(&StylisticDeclarationBlockTrailingSemicolon, None);
    let source = "a { color: red }";
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: Span::new(4, 10),
        important: false,
      }],
      span: Span::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("trailing semicolon"));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn never_reports_with_span_including_semi() {
    // "never" mode: span includes the ';'
    let source = "a { color: red; }";
    let opts = serde_json::json!(["never"]);
    let rule = configured(&StylisticDeclarationBlockTrailingSemicolon, opts.clone());
    let ctx = ctx_with_source(source);
    let node = CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: "color".to_string(),
        value: "red".to_string(),
        span: Span::new(4, 11), // includes ';'
        important: false,
      }],
      span: Span::new(0, source.len()),
      ..Default::default()
    });
    let d = rule.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("Unexpected"));
    // The fix should point at byte 14 (the ';')
    assert_eq!(d[0].span.offset, 14);
    assert_eq!(d[0].span.length, 1);
  }

  #[test]
  fn ignore_single_declaration_passes_over_a_lone_declaration() {
    let options = serde_json::json!(["always", { "ignore": ["single-declaration"] }]);
    let lint = |source| crate::testing::lint(NAME, options.clone(), source, Syntax::Css);
    assert!(lint("a { color: red }").is_empty());
    assert_eq!(lint("a { color: red; top: 0 }").len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        NAME,
        serde_json::json!(["never", { "ignore": ["single-declaration"] }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!("sometimes")),
      [
        "Invalid option value \"sometimes\" for rule \"@stylistic/declaration-block-trailing-semicolon\""
      ]
    );
    assert_eq!(
      invalid_options(
        NAME,
        serde_json::json!(["always", { "ignore": ["multi-declaration"] }])
      ),
      [
        "Invalid value \"multi-declaration\" for option \"ignore\" of rule \"@stylistic/declaration-block-trailing-semicolon\""
      ]
    );
  }
}
