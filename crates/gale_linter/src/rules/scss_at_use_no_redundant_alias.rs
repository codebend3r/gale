use std::sync::LazyLock;

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};
use regex::Regex;

use crate::postcss_tree::{Node, NodeKind};
use crate::rule::{Rule, RuleContext};

/// Disallow a `@use` alias that repeats the namespace Sass would give the
/// module anyway (`@use "sass:math" as math`).
///
/// Equivalent to stylelint-scss's `scss/at-use-no-redundant-alias`,
/// including its autofix, which removes the `as <alias>`.  The default
/// namespace is worked out the way upstream does it, quirks included: the
/// last `/` or `:` segment of the URL, minus the first `.` and the character
/// after it.
pub struct ScssAtUseNoRedundantAlias;

/// The characters JavaScript's `\s` matches, for a regex character class.
const JS_SPACE: &str =
  r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// Compile a pattern written with `{S}` for JavaScript's `\s` characters.
fn js_regex(pattern: &str) -> Regex {
  Regex::new(&pattern.replace("{S}", JS_SPACE)).expect("valid regex")
}

/// Upstream's `/\s+as\s+|\s+with\s+/`, which splits the params.
static SEPARATOR: LazyLock<Regex> = LazyLock::new(|| js_regex(r"[{S}]+as[{S}]+|[{S}]+with[{S}]+"));

/// Upstream's `/([^/:]+)$/`: the last segment of the URL.
static LAST_SEGMENT: LazyLock<Regex> = LazyLock::new(|| js_regex(r"([^/:]+)$"));

/// Upstream's `/\.[^.]|"+$/`, which it removes once from the last segment.
static EXTENSION: LazyLock<Regex> = LazyLock::new(|| js_regex(r#"\.[^.]|"+$"#));

/// Upstream's `/as\s+\S+/`: the alias it reports.
static ALIAS: LazyLock<Regex> = LazyLock::new(|| js_regex(r"as[{S}]+[^{S}]+"));

/// Upstream's fix pattern, `/\s*as\s* [^\s*]+\s*/`, which it replaces with a
/// space.
static FIX_ALIAS: LazyLock<Regex> = LazyLock::new(|| js_regex(r"[{S}]*as[{S}]* [^{S}*]+[{S}]*"));

impl Rule for ScssAtUseNoRedundantAlias {
  fn name(&self) -> &'static str {
    "scss/at-use-no-redundant-alias"
  }

  fn description(&self) -> &'static str {
    "Disallow redundant namespace aliases"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports the `as <alias>` of every `@use` whose alias is the module's
  /// default namespace, with a fix that removes it.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return Vec::new();
    }
    let tree = ctx.postcss_tree();
    let mut diags = Vec::new();

    for at_rule in tree
      .nodes
      .iter()
      .filter(|n| n.kind == NodeKind::AtRule && n.name == "use")
    {
      let Some((module, alias)) = separate_each_params(&at_rule.params) else {
        continue;
      };
      if default_namespace(&module) != alias {
        continue;
      }
      let Some(matched) = ALIAS.find(&at_rule.params) else {
        continue;
      };
      // Upstream's `atRuleParamIndex`: where the params start, counted from
      // the `@`, plus where the alias is in the (comment-free) params.
      let start =
        at_rule.start + 1 + at_rule.name.len() + at_rule.after_name.len() + matched.start();
      let mut diag = Diagnostic::new(self.name(), "Unexpected redundant namespace.")
        .severity(self.default_severity())
        .span(Span::new(start, matched.len()));
      if let Some(edit) = fix(ctx, at_rule) {
        diag = diag.fix(Fix::new("Remove the redundant namespace", vec![edit]));
      }
      diags.push(diag);
    }
    diags
  }
}

/// Upstream's `separateEachParams`: the URL and the alias, from the params
/// with every quote removed and split at `as` and `with`.  `None` when there
/// is nothing to split.
fn separate_each_params(params: &str) -> Option<(String, String)> {
  let unquoted = params.replace(['"', '\''], "");
  let mut parts = SEPARATOR.split(&unquoted);
  let module = parts.next()?.to_string();
  let alias = parts.next()?.to_string();
  Some((module, alias))
}

/// Upstream's `getDefaultNamespace`: the last segment of `module`, with the
/// first `.x` (or trailing quotes) removed, or `""` when there is none.
fn default_namespace(module: &str) -> String {
  match LAST_SEGMENT.captures(module).and_then(|caps| caps.get(1)) {
    Some(segment) => EXTENSION.replace(segment.as_str(), "").into_owned(),
    None => String::new(),
  }
}

/// The edit upstream's fix makes: the at-rule printed with its first
/// `as <alias>` replaced by a space, then printed again with its original
/// whitespace around the params.  `None` when the pattern does not match,
/// which leaves the at-rule as it is, or for an at-rule with a block.
fn fix(ctx: &RuleContext, at_rule: &Node) -> Option<Edit> {
  if at_rule.block_open.is_some() {
    return None;
  }
  let text = ctx.source_slice(at_rule.start, at_rule.end)?;
  // `atRule.toString()` leaves out the `;`.
  let text_end = at_rule.end - usize::from(text.ends_with(';'));
  let printed = ctx.source_slice(at_rule.start, text_end)?;
  let replaced = FIX_ALIAS.replace(printed, " ");
  if replaced == printed {
    return None;
  }
  // The replacement is parsed as a new at-rule, which then takes the old
  // one's raws: its `afterName` and `between` stay as they were written.
  let params = replaced
    .get(1 + at_rule.name.len()..)?
    .trim_matches(is_css_space);
  let between = ctx.source_slice(at_rule.value_span.end, text_end)?;
  Some(Edit::new(
    Span::from_range(at_rule.start, text_end),
    format!("@{}{}{params}{between}", at_rule.name, at_rule.after_name),
  ))
}

/// Whitespace as the PostCSS tokenizer reads it.
fn is_css_space(c: char) -> bool {
  matches!(c, ' ' | '\n' | '\t' | '\r' | '\u{c}')
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;
  use gale_diagnostics::SourceLineIndex;
  use serde_json::json;

  use super::default_namespace;
  use crate::testing::{fix, lint};

  const RULE: &str = "scss/at-use-no-redundant-alias";

  /// The `(line, column, endLine, endColumn)` of each warning for `source`.
  fn ranges(source: &str, syntax: Syntax) -> Vec<(usize, usize, usize, usize)> {
    let index = SourceLineIndex::build(source);
    lint(RULE, json!(true), source, syntax)
      .iter()
      .map(|d| {
        assert_eq!(d.message, "Unexpected redundant namespace.");
        let (line, column) = index.offset_to_location(d.span.offset);
        let (end_line, end_column) = index.offset_to_location(d.span.end());
        (line, column, end_line, end_column)
      })
      .collect()
  }

  #[test]
  fn accepts_a_default_or_different_namespace() {
    for source in [
      "\n      @use \"foo\";\n    ",
      "\n      @use \"foo/bar\" as foo;\n    ",
      "\n      @use 'foo/bar' as foo;\n    ",
      "\n      @use \"sass:math\" as *;\n    ",
      "\n      @use \"sass:math\";\n    ",
    ] {
      assert_eq!(ranges(source, Syntax::Scss), vec![], "{source}");
    }
  }

  #[test]
  fn reports_the_redundant_alias() {
    let cases = [
      ("\n      @use \"foo\" as foo;\n    ", (2, 18, 2, 24)),
      (
        "\n      @use \"src/corners\" as corners;\n    ",
        (2, 26, 2, 36),
      ),
      ("\n      @use \"sass:math\" as math;\n    ", (2, 24, 2, 31)),
      ("\n      @use \"foo\"   as   foo;\n    ", (2, 20, 2, 28)),
      (
        "\n      @use \"foo\" as foo with ($baz: 1px);\n    ",
        (2, 18, 2, 24),
      ),
      (
        "\n      @use \"sass:color\" as color with (\n        $baz: 1px\n      );\n    ",
        (2, 25, 2, 33),
      ),
      ("\n      @use 'foo' as foo;\n    ", (2, 18, 2, 24)),
      ("@use \"foo\" as foo;", (1, 12, 1, 18)),
      ("@use 'foo' as foo;", (1, 12, 1, 18)),
      ("@use \"src/corners\" as corners;", (1, 20, 1, 30)),
      ("@use \"sass:math\" as math;", (1, 18, 1, 25)),
      ("@use \"foo\"   as   foo;", (1, 14, 1, 22)),
      ("@use \"foo\" as foo with ($baz: 1px);", (1, 12, 1, 18)),
    ];
    for (source, range) in cases {
      assert_eq!(ranges(source, Syntax::Scss), vec![range], "{source}");
    }
  }

  #[test]
  fn fixes_by_removing_the_alias() {
    let cases = [
      (
        "\n      @use \"foo\";\n      ",
        "\n      @use \"foo\";\n      ",
      ),
      ("@use \"foo\" as foo;", "@use \"foo\";"),
      ("@use 'foo' as foo;", "@use 'foo';"),
      ("@use \"src/corners\" as corners;", "@use \"src/corners\";"),
      ("@use \"sass:math\" as math;", "@use \"sass:math\";"),
      ("@use \"foo\"   as   foo;", "@use \"foo\";"),
      (
        "@use \"foo\" as foo with ($baz: 1px);",
        "@use \"foo\" with ($baz: 1px);",
      ),
      (
        "\n      @use \"sass:color\" as color with (\n        $baz: 1px\n      );\n    ",
        "\n      @use \"sass:color\" with (\n        $baz: 1px\n      );\n    ",
      ),
    ];
    for (source, fixed) in cases {
      assert_eq!(
        fix(RULE, json!(true), source, Syntax::Scss),
        fixed,
        "{source}"
      );
    }
  }

  #[test]
  fn keeps_the_whitespace_before_the_semicolon() {
    assert_eq!(
      fix(RULE, json!(true), "@use  \"foo\" as foo ;", Syntax::Scss),
      "@use  \"foo\" ;"
    );
  }

  #[test]
  fn reports_but_cannot_fix_an_alias_after_a_tab() {
    // Upstream's fix wants a space before the alias, so it changes nothing.
    let source = "@use \"foo\" as\tfoo;";
    assert_eq!(ranges(source, Syntax::Scss), vec![(1, 12, 1, 18)]);
    assert_eq!(fix(RULE, json!(true), source, Syntax::Scss), source);
  }

  #[test]
  fn the_default_namespace_drops_the_first_dot_and_the_character_after_it() {
    assert_eq!(default_namespace("sass:math"), "math");
    assert_eq!(default_namespace("src/corners"), "corners");
    assert_eq!(default_namespace("foo.scss"), "foocss");
    assert_eq!(default_namespace("foo/"), "");
    // So `foocss` is the alias upstream calls redundant, not `foo`.
    assert_eq!(ranges("@use \"foo.scss\" as foo;", Syntax::Scss), vec![]);
    assert_eq!(
      ranges("@use \"foo.scss\" as foocss;", Syntax::Scss),
      vec![(1, 17, 1, 26)]
    );
  }

  #[test]
  fn ignores_plain_css() {
    assert_eq!(ranges("@use \"foo\" as foo;", Syntax::Css), vec![]);
  }
}
