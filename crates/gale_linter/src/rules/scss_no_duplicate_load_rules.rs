use std::collections::HashMap;
use std::sync::LazyLock;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::postcss_tree::NodeKind as StatementKind;
use crate::rule::{Rule, RuleContext};
use crate::value_parser::{self, NodeKind, ValueNode};

/// Disallow loading the same module twice with `@use`, `@forward` or
/// `@import`.
///
/// Equivalent to stylelint-scss's `scss/no-duplicate-load-rules`.  Each kind
/// of load rule is compared with its own kind.  An `@import` repeated with a
/// different media query, `layer` or `supports()` condition is not a
/// duplicate, nor is a `@forward` with different `as`, `show`, `hide` or
/// `with` clauses; a `@use` is a duplicate whatever its namespace or
/// configuration.
///
/// Upstream runs on any syntax (its own tests are plain CSS), so this rule
/// does too.
pub struct ScssNoDuplicateLoadRules;

/// The characters JavaScript's `\s` matches, for a regex character class.
const JS_SPACE: &str =
  r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// Upstream's `/\s+(as|with|show|hide)\s[^;]*/g`, removed from the params
/// of `@use` and `@import` before they are compared.
static EXPLICIT_NAMESPACE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(&format!(
    r"[{JS_SPACE}]+(as|with|show|hide)[{JS_SPACE}][^;]*"
  ))
  .expect("valid regex")
});

/// JavaScript's `\s`, which upstream removes from each condition.
static SPACE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(&format!("[{JS_SPACE}]")).expect("valid regex"));

impl Rule for ScssNoDuplicateLoadRules {
  fn name(&self) -> &'static str {
    "scss/no-duplicate-load-rules"
  }

  fn description(&self) -> &'static str {
    "Disallow duplicate load rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports every `@use`, `@forward` or `@import` that loads a URL an
  /// earlier one of its kind already loaded under the same conditions.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let tree = ctx.postcss_tree();
    // Conditions loaded so far, by at-rule name and URL.
    let mut imports: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
    let mut diags = Vec::new();

    for at_rule in tree.nodes.iter().filter(|n| {
      n.kind == StatementKind::AtRule
        && ["forward", "import", "use"]
          .iter()
          .any(|name| n.name.eq_ignore_ascii_case(name))
    }) {
      let Some(raw) = ctx.source_slice(at_rule.value_span.start, at_rule.value_span.end) else {
        continue;
      };
      // Upstream's `getAtRuleParams` prefers `raws.params.raw`, which
      // PostCSS sets only when it dropped comments from the params, over
      // `params`.  The namespace is stripped from `params` alone (and never
      // from a `@forward`), so params with comments are compared as written.
      let params = if raw != at_rule.params || at_rule.name == "forward" {
        raw.to_string()
      } else {
        EXPLICIT_NAMESPACE
          .replace_all(&at_rule.params, "")
          .into_owned()
      };
      let parsed = value_parser::parse(&params);
      let Some((first, rest)) = parsed.split_first() else {
        continue;
      };
      // The URL inside `url()`, if it is one.
      let uri = match first.nodes.first() {
        Some(inner) if first.is_function() && first.value == "url" => inner.value,
        _ => first.value,
      };
      let media = list_import_conditions(rest);
      let loaded = imports
        .entry(at_rule.name.to_ascii_lowercase())
        .or_default();
      let is_duplicate = match loaded.get(uri) {
        Some(conditions) if !media.is_empty() => media.iter().any(|q| conditions.contains(q)),
        Some(_) => true,
        None => false,
      };

      if is_duplicate {
        // Upstream reports the word `atRule.toString()`: the at-rule as
        // written, without its `;`.
        let text = ctx.source_slice(at_rule.start, at_rule.end).unwrap_or("");
        let end = if at_rule.block_open.is_none() && text.ends_with(';') {
          at_rule.end - 1
        } else {
          at_rule.end
        };
        diags.push(
          Diagnostic::new(self.name(), format!("Unexpected duplicate load rule {uri}"))
            .message_args([uri])
            .severity(self.default_severity())
            .span(Span::from_range(at_rule.start, end)),
        );
        continue;
      }
      loaded.entry(uri.to_string()).or_default().extend(media);
    }
    diags
  }
}

/// Upstream's `stringifyCondition`: the nodes as text, without whitespace.
fn condition(nodes: &[&ValueNode<'_>]) -> String {
  let text: String = nodes.iter().map(|node| node.to_css()).collect();
  SPACE.replace_all(&text, "").into_owned()
}

/// Upstream's `listImportConditions`: one key per media query in `params`,
/// each prefixed with the `layer` and `supports()` conditions before them.
fn list_import_conditions(params: &[ValueNode<'_>]) -> Vec<String> {
  let mut shared: Vec<String> = Vec::new();
  let mut media: Vec<String> = Vec::new();
  let mut last_media_query: Vec<&ValueNode<'_>> = Vec::new();

  for param in params {
    if matches!(param.kind, NodeKind::Space | NodeKind::Comment) {
      continue;
    }
    // `layer` and `supports()` must come before the media queries.
    if media.is_empty()
      && ((param.is_function() && (param.value == "supports" || param.value == "layer"))
        || (param.is_word() && param.value == "layer"))
    {
      shared.push(condition(&[param]));
      continue;
    }
    if param.is_comma() {
      media.push(condition(&last_media_query));
      last_media_query.clear();
      continue;
    }
    last_media_query.push(param);
  }
  if !last_media_query.is_empty() {
    media.push(condition(&last_media_query));
  }

  if !media.is_empty() && shared.is_empty() {
    return media;
  }
  let shared = shared.join(" ");
  if media.is_empty() && !shared.is_empty() {
    return vec![shared];
  }
  media
    .into_iter()
    .map(|query| format!("{shared} {query}"))
    .collect()
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use gale_diagnostics::SourceLineIndex;
  use serde_json::json;

  use crate::testing::lint;

  const RULE: &str = "scss/no-duplicate-load-rules";

  /// The message and `(line, column, endLine, endColumn)` of each warning
  /// for `source`.
  fn warnings(source: &str, syntax: Syntax) -> Vec<(String, (usize, usize, usize, usize))> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, syntax)
      .iter()
      .map(|d| {
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (d.message.clone(), (line, column, end_line, end_column))
      })
      .collect()
  }

  /// Assert that every source in `sources` is accepted.
  fn accepts(sources: &[&str]) {
    for source in sources {
      assert_eq!(warnings(source, Syntax::Css), vec![], "{source}");
    }
  }

  /// Assert that `source` gets one warning about `uri` starting at
  /// `(line, column)`, and ending at `end` when given.
  fn rejects(source: &str, uri: &str, start: (usize, usize), end: Option<(usize, usize)>) {
    let found = warnings(source, Syntax::Css);
    assert_eq!(found.len(), 1, "{source}: {found:?}");
    let (message, (line, column, end_line, end_column)) = &found[0];
    assert_eq!(message, &format!("Unexpected duplicate load rule {uri}"));
    assert_eq!((*line, *column), start, "{source}");
    if let Some(end) = end {
      assert_eq!((*end_line, *end_column), end, "{source}");
    }
  }

  #[test]
  fn accepts_distinct_imports() {
    accepts(&[
      "@import \"a.css\";",
      "@import url('a.css');",
      "@import url(a.css);",
      "@import url(\"a.css\") projection, tv;",
      "@import 'a.css'; @import 'b.css';",
      "@import url('a.css') projection; @import url('a.css') tv;",
      "@import \"a.css\" screen; @import \"b.css\" tv; @import \"a.css\" tv;",
      "@IMPORT \"a.css\"; @ImPoRt \"b.css\";",
      "@import \"a.css\" supports(display: flex) tv; @import \"a.css\" layer tv;",
      "@import \"a.css\" supports(display: flex); @import \"a.css\" layer;",
      "@import \"a.css\" supports(display: flex) tv; @import \"a.css\" supports(display: grid) tv;",
    ]);
  }

  #[test]
  fn rejects_duplicate_imports() {
    let cases: &[(&str, (usize, usize), Option<(usize, usize)>)] = &[
      ("@import 'a.css'; @import 'a.css';", (1, 18), Some((1, 33))),
      (
        "@import url(\"a.css\"); @import url(\"a.css\");",
        (1, 23),
        Some((1, 43)),
      ),
      (
        "@import \"a.css\";\n@import 'a.css';",
        (2, 1),
        Some((2, 16)),
      ),
      (
        "@import \"a.css\"; @import 'b.css'; @import url(a.css);",
        (1, 35),
        Some((1, 53)),
      ),
      (
        "@import url('a.css') tv; @import 'a.css' tv;",
        (1, 26),
        None,
      ),
      (
        "@import url('a.css') tv, projection; @import 'a.css' projection, tv;",
        (1, 38),
        None,
      ),
      (
        "@import url('a.css') tv, projection; @import 'a.css' projection, screen, tv;",
        (1, 38),
        None,
      ),
      (
        "@import url('a.css') tv, projection; @import 'a.css' screen, tv;",
        (1, 38),
        None,
      ),
      (
        "@import url('a.css') /* a comment */ tv; @import 'a.css' tv /* a comment */;",
        (1, 42),
        None,
      ),
      (
        "@import \"a.css\" (min-width : 500px);@import url(a.css) (  min-width:500px   );",
        (1, 37),
        None,
      ),
      (
        "@import \"a.css\" tv, (min-width : 500px);@import url(a.css) (  min-width:500px   ), tv;",
        (1, 41),
        None,
      ),
      ("@IMPORT 'a.css'; @ImPoRt 'a.css';", (1, 18), None),
      (
        "@import url(\"a.css\") layer; @import url(\"a.css\") layer;",
        (1, 29),
        Some((1, 55)),
      ),
      (
        "@import url(\"a.css\") layer(base); @import url(\"a.css\") layer(base);",
        (1, 35),
        Some((1, 67)),
      ),
      (
        "@import url(\"a.css\") layer(base) supports(display: grid); @import url(\"a.css\") layer(base) supports(display: grid);",
        (1, 59),
        Some((1, 115)),
      ),
      (
        "@import url(\"a.css\") supports(display: grid); @import url(\"a.css\") supports(display: grid);",
        (1, 47),
        Some((1, 91)),
      ),
      (
        "@import url(\"a.css\") layer tv; @import url(\"a.css\") layer tv;",
        (1, 32),
        Some((1, 61)),
      ),
      (
        "@import url(\"a.css\") layer(base) tv; @import url(\"a.css\") layer(base) tv;",
        (1, 38),
        Some((1, 73)),
      ),
      (
        "@import url(\"a.css\") layer(base) supports(display: grid) tv; @import url(\"a.css\") layer(base) supports(display: grid) tv;",
        (1, 62),
        Some((1, 121)),
      ),
      (
        "@import url(\"a.css\") layer(base) supports(display: grid) screen, tv; @import url(\"a.css\") layer(base) supports(display: grid) tv;",
        (1, 70),
        Some((1, 129)),
      ),
    ];
    for &(source, start, end) in cases {
      rejects(source, "a.css", start, end);
    }
  }

  #[test]
  fn accepts_distinct_uses() {
    accepts(&[
      "@use \"a\";",
      "@use \"a\";\n@use \"b\";",
      "\n      @use \"sass:map\" as abc;\n      @use \"sass:math\" as xyz;\n      ",
      "\n      @use 'library' with (\n        $black: #222,\n        $border-radius: 0.1rem\n      );\n      @use 'lib' with ($blue: #333);",
    ]);
  }

  #[test]
  fn rejects_duplicate_uses_whatever_their_namespace() {
    rejects(
      "\n      @use \"a.css\";\n      @use \"a.css\";\n      ",
      "a.css",
      (3, 7),
      Some((3, 19)),
    );
    rejects(
      "\n      @use \"a\";\n      @use \"a\";\n      ",
      "a",
      (3, 7),
      Some((3, 15)),
    );
    rejects(
      "\n      @use \"sass:math\" as abc;\n      @use \"sass:math\" as xyz;\n      ",
      "sass:math",
      (3, 7),
      Some((3, 30)),
    );
    rejects(
      "\n      @use 'library' with (\n        $black: #222,\n        $border-radius: 0.1rem\n      );\n      @use 'library' with ($blue: #333);",
      "library",
      (6, 7),
      Some((6, 40)),
    );
  }

  #[test]
  fn accepts_distinct_forwards() {
    accepts(&[
      "@forward \"src/list\";",
      "@forward \"a\";\n@forward \"b\";",
      "\n      @forward \"a\" as list-*;\n      @forward \"b\" as list-2;\n      ",
      "\n      @forward \"a\" hide list-reset, $horizontal-list-gap;\n      @forward \"b\" hide $vertical-list-gap;\n      ",
      "\n      @forward \"a\" show list-reset, $horizontal-list-gap;\n      @forward \"b\" show $vertical-list-gap;\n      ",
      "\n      @forward 'a' with (\n        $black: #222,\n        $border-radius: 0.1rem\n      );\n      @forward 'b' with ($blue: #333);",
      "\n      @forward \"src/list\" as list-*;\n      @forward \"src/list\" as list-2;\n      ",
      "\n      @forward \"a\" hide list-reset, $horizontal-list-gap;\n      @forward \"a\" hide $vertical-list-gap;",
      "\n      @forward \"a\" show list-reset, $horizontal-list-gap;\n      @forward \"a\" show $vertical-list-gap;\n      ",
      "\n      @forward 'src/list' with (\n        $black: #222,\n        $border-radius: 0.1rem\n      );\n      @forward 'src/list' with ($blue: #333);",
      "\n      @use 'src/list';\n      @forward 'src/list' with ($blue: #333);",
      "\n      @use 'src/list';\n      @forward 'src/list';\n      ",
    ]);
  }

  #[test]
  fn rejects_duplicate_forwards() {
    rejects(
      "@forward \"src/list\";\n@forward \"src/list\";",
      "src/list",
      (2, 1),
      Some((2, 20)),
    );
    rejects(
      "\n      @forward \"src/list\" as list-*;\n      @forward \"src/list\" as list-*;\n      ",
      "src/list",
      (3, 7),
      Some((3, 36)),
    );
    rejects(
      "\n      @forward \"a\" hide list-reset, $horizontal-list-gap;\n      @forward \"a\" hide list-reset, $horizontal-list-gap;\n      ",
      "a",
      (3, 7),
      Some((3, 57)),
    );
    rejects(
      "\n      @forward \"a\" show list-reset, $horizontal-list-gap;\n      @forward \"a\" show list-reset, $horizontal-list-gap;\n      ",
      "a",
      (3, 7),
      Some((3, 57)),
    );
    rejects(
      "\n      @forward 'src/list' with (\n        $black: #222,\n        $border-radius: 0.1rem\n      );\n      @forward 'src/list' with (\n        $black: #222,\n        $border-radius: 0.1rem\n      );",
      "src/list",
      (6, 7),
      Some((9, 8)),
    );
  }

  #[test]
  fn runs_on_scss() {
    let source = "@use \"foo\";\n@use \"foo\" as f;\n@use \"bar\";\n";
    let found = warnings(source, Syntax::Scss);
    assert_eq!(
      found,
      vec![(
        "Unexpected duplicate load rule foo".to_string(),
        (2, 1, 2, 16)
      )]
    );
  }
}
