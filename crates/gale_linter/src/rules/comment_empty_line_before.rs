use gale_diagnostics::Diagnostic;

use crate::empty_lines::{empty_line_report, has_empty_line};
use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::pattern::{self, Regex};
use crate::rule::{Check, RuleContext, declare_rule};
use gale_css_parser::postcss_tree::{NodeKind, PostcssTree};
use std::sync::Arc;

const NAME: &str = "comment-empty-line-before";

declare_rule! {
  /// Require or disallow an empty line before comments.
  ///
  /// Equivalent to Stylelint's `comment-empty-line-before` rule, autofix
  /// included.  Primary option `"always"` or `"never"`; secondary options:
  ///   - `except`: `first-nested`
  ///   - `ignore`: `stylelint-commands`, `after-comment`
  ///   - `ignoreComments`: strings or `/regex/` entries matched against the
  ///     comment text
  ///
  /// Only comment nodes count, as in Stylelint's PostCSS tree: a comment
  /// inside a selector, a declaration value (an SCSS map, say) or at-rule
  /// params is part of that node.  `//` comments, comments sharing a line
  /// with other code and the first node of the stylesheet are never checked.
  pub struct CommentEmptyLineBefore => CommentEmptyLineBeforeCheck {
    name: NAME,
    description: "Require or disallow an empty line before comments",
    default_severity: Warning,
  }
}

/// `comment-empty-line-before` as configured.
pub struct CommentEmptyLineBeforeCheck {
  /// `"always"` rather than `"never"`.  Gale has read no option as
  /// `"never"` here.
  always: bool,
  /// `except: ["first-nested"]`.
  except_first_nested: bool,
  /// The `ignore` keywords.
  ignore: Vec<&'static str>,
  /// `ignoreComments`.
  ignore_comments: Vec<CommentMatcher>,
}

impl CommentEmptyLineBeforeCheck {
  /// Stylelint's options: `"always"` or `"never"`, then
  /// `except: ["first-nested"]`, `ignore` and `ignoreComments`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let expectation = options.primary_one_of(&["always", "never"]);
    let except = options.keywords("except", &["first-nested"]);
    let ignore = options.keywords("ignore", &["stylelint-commands", "after-comment"]);
    let ignore_comments = options.strings("ignoreComments");
    options.finish()?;
    Ok(Self {
      always: expectation == Some("always"),
      except_first_nested: except.contains(&"first-nested"),
      ignore,
      ignore_comments: ignore_comments
        .iter()
        .map(|entry| CommentMatcher::new(entry))
        .collect(),
    })
  }
}

impl Check for CommentEmptyLineBeforeCheck {
  /// Checks every comment node in the document, in source order.
  fn check(&self, tree: &PostcssTree, ctx: &RuleContext) -> Vec<Diagnostic> {
    let always = self.always;
    let ignore = |name: &str| self.ignore.contains(&name);

    let mut diags = Vec::new();
    for i in 0..tree.nodes.len() {
      let node = &tree.nodes[i];
      if node.kind != NodeKind::Comment || tree.is_first_node_of_root(i) {
        continue;
      }
      if (ignore("stylelint-commands") && is_configuration_comment(&node.name))
        || (ignore("after-comment") && tree.is_after_comment(i))
        || self
          .ignore_comments
          .iter()
          .any(|matcher| matcher.matches(&node.name))
        || tree.is_shared_line_comment(i, None)
        || node.inline
      {
        continue;
      }

      let expect_empty_line = always && !(self.except_first_nested && tree.is_first_nested(i));
      if expect_empty_line == has_empty_line(tree.before(i)) {
        continue;
      }
      diags.push(empty_line_report(
        NAME,
        tree,
        i,
        expect_empty_line,
        "comment",
      ));
    }
    diags
  }
}

/// One `ignoreComments` entry, compiled once: a `/regex/` entry is a
/// regex, any other entry must equal the comment text, as Stylelint's
/// `optionsMatches` does.
///
/// A regex literal in a JavaScript config reaches the rule as its bare
/// pattern (`/^\s*@todo/` becomes `"^\s*@todo"`), so an entry that is not
/// an exact match is also tried as a regex.
enum CommentMatcher {
  /// A `/regex/` entry; `None` when it does not compile, and it matches
  /// nothing.
  Literal(Option<Arc<Regex>>),
  /// Any other entry, with its compiled form when it compiles as a regex.
  Text(String, Option<Arc<Regex>>),
}

impl CommentMatcher {
  /// The matcher for one entry.
  fn new(entry: &str) -> Self {
    if pattern::split_literal(entry).is_some() {
      Self::Literal(pattern::compile(entry).ok())
    } else {
      Self::Text(entry.to_string(), pattern::compile(entry).ok())
    }
  }

  /// Whether the entry matches the comment text.
  fn matches(&self, text: &str) -> bool {
    match self {
      Self::Literal(regex) => regex.as_ref().is_some_and(|re| pattern::is_match(re, text)),
      Self::Text(entry, regex) => {
        entry == text || regex.as_ref().is_some_and(|re| pattern::is_match(re, text))
      }
    }
  }
}

/// Stylelint's `isConfigurationComment`: the comment's first word is a
/// `stylelint-` (or `gale-`) `disable`, `disable-line`, `disable-next-line`
/// or `enable` command.
fn is_configuration_comment(text: &str) -> bool {
  let command = text.split(char::is_whitespace).next().unwrap_or("");
  ["stylelint", "gale"].iter().any(|prefix| {
    command.strip_prefix(prefix).is_some_and(|rest| {
      matches!(
        rest,
        "-disable" | "-disable-line" | "-disable-next-line" | "-enable"
      )
    })
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context, fix, invalid_options};

  const RULE: &str = "comment-empty-line-before";

  /// The messages for `source` in `syntax` with `options`.
  fn messages(source: &str, syntax: Syntax, options: serde_json::Value) -> Vec<String> {
    let ctx = context(source, syntax);
    configured(&CommentEmptyLineBefore, options)
      .check_source(&ctx)
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  #[test]
  fn reports_missing_and_unexpected_empty_lines() {
    let always = serde_json::json!("always");
    assert_eq!(
      messages("a {}\n/* comment */", Syntax::Css, always.clone()),
      vec!["Expected empty line before comment"]
    );
    assert!(messages("a {}\n\n/* comment */", Syntax::Css, always.clone()).is_empty());
    assert!(messages("/* first */", Syntax::Css, always.clone()).is_empty());
    assert_eq!(
      messages(
        "a {\n  /* comment */\n  color: pink;\n}",
        Syntax::Css,
        always
      )
      .len(),
      1,
      "the first node in a block is checked unless except first-nested"
    );
    assert_eq!(
      messages(
        "a { color: pink;\n\n/* comment */\ntop: 0; }",
        Syntax::Css,
        serde_json::json!("never")
      ),
      vec!["Expected no empty line before comment"]
    );
  }

  #[test]
  fn skips_shared_line_line_and_value_comments() {
    let always = serde_json::json!("always");
    assert!(
      messages(
        "a { color: pink; /* c */\ntop: 0; }",
        Syntax::Css,
        always.clone()
      )
      .is_empty()
    );
    assert!(messages("a { /* c */\n color: pink; }", Syntax::Css, always.clone()).is_empty());
    assert!(
      messages(
        "a { color: pink;\n// c\ntop: 0; }",
        Syntax::Scss,
        always.clone()
      )
      .is_empty()
    );
    assert!(
      messages(
        "a { color: pink;\n// c\ntop: 0; }",
        Syntax::Less,
        always.clone()
      )
      .is_empty()
    );
    // fundamental-styles: a comment inside an SCSS map is part of the value.
    let map = "$a: 1;\n$config: (\n  /**\n   * Doc\n   */\n  \"x\": 1,\n);\n";
    assert!(messages(map, Syntax::Scss, always).is_empty());
  }

  #[test]
  fn options_ignore_and_except() {
    let first_nested = serde_json::json!(["always", { "except": ["first-nested"] }]);
    assert_eq!(
      messages(
        "a {\n\n  /* comment */\n  color: pink;\n}",
        Syntax::Css,
        first_nested.clone()
      ),
      vec!["Expected no empty line before comment"]
    );
    assert!(
      messages(
        "a { /* shared */\n  /* comment */\n  color: pink;\n}",
        Syntax::Css,
        first_nested
      )
      .is_empty()
    );
    let commands = serde_json::json!(["always", { "ignore": ["stylelint-commands"] }]);
    assert!(
      messages(
        "a {\ncolor: pink;\n/* stylelint-disable something */\ntop: 0;\n}",
        Syntax::Css,
        commands
      )
      .is_empty()
    );
    let after_comment = serde_json::json!(["always", { "ignore": ["after-comment"] }]);
    assert!(
      messages(
        "a { color: pink;\n\n/* a */\n/* b */\ntop: 0; }",
        Syntax::Css,
        after_comment
      )
      .is_empty()
    );
    let ignored =
      serde_json::json!(["always", { "ignoreComments": ["/^ignore/", "exact", "^\\s*@todo"] }]);
    assert!(
      messages(
        "a {\ncolor: pink;\n/* ignore me */\n/* exact */\n/* @todo x */\ntop: 0;\n}",
        Syntax::Css,
        ignored
      )
      .is_empty()
    );
  }

  #[test]
  fn recognises_configuration_comments() {
    assert!(is_configuration_comment("stylelint-disable something"));
    assert!(is_configuration_comment("stylelint-enable"));
    assert!(is_configuration_comment("gale-disable-next-line a"));
    assert!(!is_configuration_comment("stylelint-config"));
    assert!(!is_configuration_comment("note stylelint-disable"));
  }

  #[test]
  fn fix_adds_and_removes_empty_lines() {
    let always = serde_json::json!("always");
    assert_eq!(
      fix(RULE, always.clone(), "/** a */\r\n/** b */", Syntax::Css),
      "/** a */\r\n\r\n/** b */"
    );
    assert_eq!(
      fix(
        RULE,
        always,
        "a {\n  /* c */\n  color: pink;\n}",
        Syntax::Css
      ),
      "a {\n\n  /* c */\n  color: pink;\n}"
    );
    let first_nested = serde_json::json!(["always", { "except": ["first-nested"] }]);
    assert_eq!(
      fix(
        RULE,
        first_nested,
        "a { /* shared */\n\n  /* c */\n  color: pink;\n}",
        Syntax::Css
      ),
      "a { /* shared */\n  /* c */\n  color: pink;\n}"
    );
    assert_eq!(
      fix(
        RULE,
        serde_json::json!("never"),
        "a {}\r\n\r\n\r\n/** c */",
        Syntax::Css
      ),
      "a {}\r\n/** c */"
    );
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(
      invalid_options(
        RULE,
        serde_json::json!(["always", {
          "except": ["first-nested"],
          "ignore": ["stylelint-commands", "after-comment"],
          "ignoreComments": ["/^ignore/", "todo"]
        }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options(RULE, serde_json::json!("sometimes")),
      ["Invalid option value \"sometimes\" for rule \"comment-empty-line-before\""]
    );
    assert_eq!(
      invalid_options(
        RULE,
        serde_json::json!(["always", { "except": ["after-comment"] }])
      ),
      [
        "Invalid value \"after-comment\" for option \"except\" of rule \"comment-empty-line-before\""
      ]
    );
  }
}
