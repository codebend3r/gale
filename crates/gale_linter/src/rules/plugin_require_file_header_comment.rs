use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "plugin/require-file-header-comment";

declare_rule! {
  /// Require that CSS files begin with a comment matching a specified pattern.
  ///
  /// This is a `check_root` rule that inspects the first node in the file.
  ///
  /// Options (secondary): an object with a `pattern` key containing a regex
  /// pattern (as a string, optionally wrapped in `/regex/flags`).
  ///
  /// Example config:
  /// ```json
  /// ["plugin/require-file-header-comment", [true, {
  ///   "pattern": "/Copyright|License/i"
  /// }]]
  /// ```
  pub struct PluginRequireFileHeaderComment => PluginRequireFileHeaderCommentCheck {
    name: NAME,
    description: "Require a file header comment matching a pattern",
    default_severity: Warning,
  }
}

/// `plugin/require-file-header-comment` as configured.
pub struct PluginRequireFileHeaderCommentCheck {
  /// `pattern`; the rule checks nothing without one.
  pattern: Option<HeaderPattern>,
}

impl PluginRequireFileHeaderCommentCheck {
  /// Gale's own rule, standing in for projects' custom plugins, so it is
  /// lenient: it reads only `pattern`, which must be a string, and ignores
  /// its primary option and any other key.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = setting
      .secondary()
      .and_then(|secondary| secondary.get("pattern"));
    if let Some(pattern) = pattern
      && !pattern.is_string()
    {
      options.report(InvalidOption::option_value(NAME, "pattern", pattern));
    }
    options.finish()?;
    Ok(Self {
      pattern: pattern
        .and_then(serde_json::Value::as_str)
        .map(HeaderPattern::from_str),
    })
  }
}

/// A compiled pattern for matching comment text.
enum HeaderPattern {
  Regex(std::sync::Arc<crate::pattern::Regex>),
  Literal(String),
}

impl HeaderPattern {
  /// Builds a pattern: `/…/` (optionally `i`-flagged) is a regex, else a substring.
  fn from_str(s: &str) -> Self {
    // Patterns wrapped in `/` (optionally flagged, like `/…/i`) are regexes.
    // One that does not compile is reported as an invalid option by the
    // runner and compared as a literal here.
    match crate::pattern::regex_entry(s) {
      Some(re) => HeaderPattern::Regex(re),
      None => HeaderPattern::Literal(s.to_string()),
    }
  }

  /// Whether the comment text matches this pattern.
  fn matches(&self, text: &str) -> bool {
    match self {
      HeaderPattern::Regex(re) => crate::pattern::is_match(re, text),
      HeaderPattern::Literal(s) => text.contains(s.as_str()),
    }
  }
}

impl Check for PluginRequireFileHeaderCommentCheck {
  /// Flags a file whose first node is not a comment matching the pattern.
  fn check_root(&self, nodes: &[CssNode], _context: &RuleContext) -> Vec<Diagnostic> {
    let Some(pattern) = &self.pattern else {
      return vec![];
    };

    // Find the first non-whitespace node
    let first_node = nodes.first();

    match first_node {
      Some(CssNode::Comment(comment)) => {
        if pattern.matches(&comment.text) {
          // Comment matches pattern — all good
          vec![]
        } else {
          vec![
            Diagnostic::new(
              NAME,
              "File header comment does not match the required pattern",
            )
            .span(Span::new(comment.span.offset, comment.span.length)),
          ]
        }
      }
      _ => {
        // No comment at the start of file, or file is empty
        // Use span at the start of the file
        let span = Span::new(0, 0);
        let mut diag = Diagnostic::new(
          NAME,
          "Missing file header comment matching the required pattern",
        )
        .span(span);

        // Provide autofix: insert a default header comment at the top
        diag = diag.fix(Fix::new(
          "Insert file header comment",
          vec![Edit::new(
            Span::new(0, 0),
            "/* TODO: Add file header comment */\n".to_string(),
          )],
        ));

        vec![diag]
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Comment, Span as ParserSpan, StyleRule};
  use serde_json::json;

  use crate::testing::{configured, ctx, ctx_with_options};

  #[test]
  fn passes_with_matching_comment() {
    let opts = json!([true, {
        "pattern": "/Copyright|License/i"
    }]);
    let rule = configured(&PluginRequireFileHeaderComment, opts.clone());
    let ctx = ctx_with_options(&opts);
    let nodes = vec![CssNode::Comment(Comment {
      text: " Copyright 2024 Acme Corp ".to_string(),
      span: ParserSpan::new(0, 30),
      is_line: false,
    })];
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn passes_with_license_comment() {
    let opts = json!([true, {
        "pattern": "/Copyright|License/i"
    }]);
    let rule = configured(&PluginRequireFileHeaderComment, opts.clone());
    let ctx = ctx_with_options(&opts);
    let nodes = vec![CssNode::Comment(Comment {
      text: " MIT License ".to_string(),
      span: ParserSpan::new(0, 16),
      is_line: false,
    })];
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn fails_with_wrong_comment() {
    let opts = json!([true, {
        "pattern": "/Copyright|License/i"
    }]);
    let rule = configured(&PluginRequireFileHeaderComment, opts.clone());
    let ctx = ctx_with_options(&opts);
    let nodes = vec![CssNode::Comment(Comment {
      text: " This is just a comment ".to_string(),
      span: ParserSpan::new(0, 28),
      is_line: false,
    })];
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("does not match"));
  }

  #[test]
  fn fails_with_no_comment() {
    let opts = json!([true, {
        "pattern": "/Copyright/"
    }]);
    let rule = configured(&PluginRequireFileHeaderComment, opts.clone());
    let ctx = ctx_with_options(&opts);
    let nodes = vec![CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, 10),
      children: vec![],
      nested_at_rules: vec![],
    })];
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("Missing file header comment"));
    assert!(diags[0].fix.is_some());
  }

  #[test]
  fn fails_with_empty_file() {
    let opts = json!([true, {
        "pattern": "/Copyright/"
    }]);
    let rule = configured(&PluginRequireFileHeaderComment, opts.clone());
    let ctx = ctx_with_options(&opts);
    let nodes: Vec<CssNode> = vec![];
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
    assert!(diags[0].fix.is_some());
  }

  #[test]
  fn no_options_returns_empty() {
    let rule = configured(&PluginRequireFileHeaderComment, None);
    let ctx = ctx();
    let nodes: Vec<CssNode> = vec![];
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn literal_pattern() {
    let opts = json!([true, {
        "pattern": "Copyright"
    }]);
    let rule = configured(&PluginRequireFileHeaderComment, opts.clone());
    let ctx = ctx_with_options(&opts);
    let nodes = vec![CssNode::Comment(Comment {
      text: " Copyright 2024 ".to_string(),
      span: ParserSpan::new(0, 20),
      is_line: false,
    })];
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn case_insensitive_regex() {
    let opts = json!([true, {
        "pattern": "/copyright/i"
    }]);
    let rule = configured(&PluginRequireFileHeaderComment, opts.clone());
    let ctx = ctx_with_options(&opts);
    let nodes = vec![CssNode::Comment(Comment {
      text: " COPYRIGHT 2024 ".to_string(),
      span: ParserSpan::new(0, 20),
      is_line: false,
    })];
    let diags = rule.check_root(&nodes, &ctx);
    assert!(diags.is_empty());
  }

  #[test]
  fn autofix_inserts_comment() {
    let opts = json!([true, {
        "pattern": "/Copyright/"
    }]);
    let rule = configured(&PluginRequireFileHeaderComment, opts.clone());
    let ctx = ctx_with_options(&opts);
    let nodes = vec![CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![],
      span: ParserSpan::new(0, 10),
      children: vec![],
      nested_at_rules: vec![],
    })];
    let diags = rule.check_root(&nodes, &ctx);
    assert_eq!(diags.len(), 1);
    let fix = diags[0].fix.as_ref().unwrap();
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].span.offset, 0);
    assert!(
      fix.edits[0]
        .new_text
        .contains("TODO: Add file header comment")
    );
  }

  #[test]
  fn options_are_validated_leniently() {
    use crate::testing::invalid_options;
    // Gale's own rule ignores its primary option and keys it does not read.
    let valid = serde_json::json!(["anything", { "pattern": "/copyright/i", "other": 1 }]);
    assert!(invalid_options(NAME, valid).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!([true, { "pattern": ["a"] }])),
      [
        "Invalid value \"[\"a\"]\" for option \"pattern\" of rule \"plugin/require-file-header-comment\""
      ]
    );
  }
}
