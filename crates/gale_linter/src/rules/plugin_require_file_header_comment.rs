use gale_css_parser::postcss_tree::{NodeRef, PostcssTree};
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "plugin/require-file-header-comment";

declare_rule! {
  /// Require that CSS files begin with a comment matching a specified pattern.
  ///
  /// It inspects the first node in the file.
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
  /// The comment's text is PostCSS's `comment.text`: no delimiters, no
  /// whitespace around it.
  fn check(&self, tree: &PostcssTree, _context: &RuleContext) -> Vec<Diagnostic> {
    let Some(pattern) = &self.pattern else {
      return vec![];
    };
    match tree.root_nodes().next().and_then(NodeRef::as_comment) {
      Some(comment) if pattern.matches(comment.text()) => vec![],
      Some(comment) => vec![
        Diagnostic::new(
          NAME,
          "File header comment does not match the required pattern",
        )
        .span(comment.span()),
      ],
      // No comment at the start of the file, or an empty file: insert a
      // default header at the top.
      None => vec![
        Diagnostic::new(
          NAME,
          "Missing file header comment matching the required pattern",
        )
        .span(Span::new(0, 0))
        .fix(Fix::new(
          "Insert file header comment",
          vec![Edit::new(
            Span::new(0, 0),
            "/* TODO: Add file header comment */\n".to_string(),
          )],
        )),
      ],
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{CheckSource, configured, context};

  /// What the rule reports for `source` in `syntax` with `pattern`.
  fn check(source: &str, syntax: Syntax, pattern: &str) -> Vec<Diagnostic> {
    configured(
      &PluginRequireFileHeaderComment,
      json!([true, { "pattern": pattern }]),
    )
    .check_source(&context(source, syntax))
  }

  #[test]
  fn passes_with_matching_comment() {
    let source = "/* Copyright 2024 Acme Corp */\na {}";
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
      assert!(check(source, syntax, "/Copyright|License/i").is_empty());
    }
    assert!(check("// MIT License\na {}", Syntax::Scss, "/Copyright|License/i").is_empty());
  }

  #[test]
  fn the_text_is_trimmed() {
    assert!(check("/*\n  Copyright\n*/", Syntax::Css, "/^Copyright$/").is_empty());
  }

  #[test]
  fn fails_with_wrong_comment() {
    let source = "/* This is just a comment */\na {}";
    let diags = check(source, Syntax::Css, "/Copyright|License/i");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("does not match"));
    assert_eq!((diags[0].span.offset, diags[0].span.length), (0, 28));
  }

  #[test]
  fn fails_with_no_comment() {
    let diags = check("a {}\n/* Copyright */", Syntax::Css, "/Copyright/");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("Missing file header comment"));
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
  fn fails_with_empty_file() {
    let diags = check("", Syntax::Css, "/Copyright/");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].fix.is_some());
  }

  #[test]
  fn no_options_returns_empty() {
    let rule = configured(&PluginRequireFileHeaderComment, None);
    assert!(rule.check_source(&context("", Syntax::Css)).is_empty());
  }

  #[test]
  fn literal_pattern() {
    assert!(check("/* Copyright 2024 */", Syntax::Css, "Copyright").is_empty());
  }

  #[test]
  fn case_insensitive_regex() {
    assert!(check("/* COPYRIGHT 2024 */", Syntax::Css, "/copyright/i").is_empty());
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
