use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity};

use crate::empty_lines::{empty_line_report, has_empty_line};
use crate::pattern::{self, option_matches};
use crate::postcss_tree::NodeKind;
use crate::rule::{Rule, RuleContext};

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
pub struct CommentEmptyLineBefore;

impl Rule for CommentEmptyLineBefore {
  fn name(&self) -> &'static str {
    "comment-empty-line-before"
  }

  fn description(&self) -> &'static str {
    "Require or disallow an empty line before comments"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Checks every comment node in the document, in source order.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let always = ctx.primary_option_str() == Some("always");
    let secondary = ctx.secondary_options();
    let option = |name: &str| secondary.and_then(|s| s.get(name));
    let except = |name: &str| option_matches(option("except"), name);
    let ignore = |name: &str| option_matches(option("ignore"), name);
    let tree = ctx.postcss_tree();

    let mut diags = Vec::new();
    for i in 0..tree.nodes.len() {
      let node = &tree.nodes[i];
      if node.kind != NodeKind::Comment || tree.is_first_node_of_root(i) {
        continue;
      }
      if (ignore("stylelint-commands") && is_configuration_comment(&node.name))
        || (ignore("after-comment") && tree.is_after_comment(i))
        || ignores_comment(option("ignoreComments"), &node.name)
        || tree.is_shared_line_comment(i, None)
        || node.inline
      {
        continue;
      }

      let expect_empty_line = always && !(except("first-nested") && tree.is_first_nested(i));
      if expect_empty_line == has_empty_line(tree.before(i)) {
        continue;
      }
      diags.push(empty_line_report(
        self.name(),
        self.default_severity(),
        &tree,
        i,
        expect_empty_line,
        "comment",
      ));
    }
    diags
  }
}

/// Whether an `ignoreComments` entry matches the comment text: a
/// `/regex/` entry as a regex, any other entry exactly, as Stylelint's
/// `optionsMatches` does.
///
/// A regex literal in a JavaScript config reaches the rule as its bare
/// pattern (`/^\s*@todo/` becomes `"^\s*@todo"`), so an entry that is not
/// an exact match is also tried as a regex.
fn ignores_comment(option: Option<&serde_json::Value>, text: &str) -> bool {
  let matches = |entry: &str| {
    pattern::match_regex_entry(entry, text).unwrap_or_else(|| {
      entry == text || pattern::compile(entry).is_ok_and(|re| pattern::is_match(&re, text))
    })
  };
  match option {
    Some(serde_json::Value::String(entry)) => matches(entry),
    Some(serde_json::Value::Array(entries)) => entries
      .iter()
      .filter_map(serde_json::Value::as_str)
      .any(matches),
    _ => false,
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
  use crate::empty_lines::fix_with;
  use gale_css_parser::Syntax;

  /// The messages for `source` in `syntax` with `options`.
  fn messages(source: &str, syntax: Syntax, options: serde_json::Value) -> Vec<String> {
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax,
      options: Some(&options),
      cache: None,
    };
    CommentEmptyLineBefore
      .check_root(&[], &ctx)
      .into_iter()
      .map(|d| d.message)
      .collect()
  }

  /// `source` fixed with the rule set to `options`.
  fn fix(source: &str, syntax: Syntax, options: serde_json::Value) -> String {
    fix_with("comment-empty-line-before", options, source, syntax)
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
      fix("/** a */\r\n/** b */", Syntax::Css, always.clone()),
      "/** a */\r\n\r\n/** b */"
    );
    assert_eq!(
      fix("a {\n  /* c */\n  color: pink;\n}", Syntax::Css, always),
      "a {\n\n  /* c */\n  color: pink;\n}"
    );
    let first_nested = serde_json::json!(["always", { "except": ["first-nested"] }]);
    assert_eq!(
      fix(
        "a { /* shared */\n\n  /* c */\n  color: pink;\n}",
        Syntax::Css,
        first_nested
      ),
      "a { /* shared */\n  /* c */\n  color: pink;\n}"
    );
    assert_eq!(
      fix(
        "a {}\r\n\r\n\r\n/** c */",
        Syntax::Css,
        serde_json::json!("never")
      ),
      "a {}\r\n/** c */"
    );
  }
}
