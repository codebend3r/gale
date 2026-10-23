use std::collections::HashMap;

use gale_css_parser::postcss_tree::PostcssTree;
use gale_css_parser::value_parser::{self, NodeKind as ValueKind, ValueNode};
use gale_diagnostics::Diagnostic;

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "no-duplicate-at-import-rules";

declare_rule! {
  /// Disallow duplicate `@import` rules within a stylesheet.
  ///
  /// Equivalent to Stylelint's `no-duplicate-at-import-rules` rule.
  pub struct NoDuplicateAtImportRules => NoDuplicateAtImportRulesCheck {
    name: NAME,
    description: "Disallow duplicate @import rules",
    default_severity: Warning,
  }
}

/// `no-duplicate-at-import-rules`, which takes no options.
pub struct NoDuplicateAtImportRulesCheck;

impl NoDuplicateAtImportRulesCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for NoDuplicateAtImportRulesCheck {
  /// Flags an `@import` of a URL already imported with no conditions, or
  /// with one of the same conditions, as Stylelint does.
  fn check(&self, tree: &PostcssTree, _context: &RuleContext) -> Vec<Diagnostic> {
    // The conditions each URL has been imported with.
    let mut imports: HashMap<String, Vec<String>> = HashMap::new();
    let mut diagnostics = Vec::new();
    for at in tree
      .walk_at_rules()
      .filter(|at| at.name().eq_ignore_ascii_case("import"))
    {
      let params = at.raw_params();
      let nodes = value_parser::parse(&params);
      let Some((first, rest)) = nodes.split_first() else {
        continue;
      };
      // The URL, out of `url()` if it is in one.
      let uri = match first.nodes.first() {
        Some(inner) if first.is_function_named("url") => inner.value,
        _ => first.value,
      };
      let conditions = import_conditions(rest);
      let imported = imports.get(uri);
      let duplicate = if conditions.is_empty() {
        imported.is_some()
      } else {
        conditions
          .iter()
          .any(|condition| imported.is_some_and(|seen| seen.contains(condition)))
      };
      if duplicate {
        diagnostics.push(
          Diagnostic::new(NAME, format!("Unexpected duplicate @import rule \"{uri}\""))
            .span(at.span()),
        );
        continue;
      }
      imports
        .entry(uri.to_string())
        .or_default()
        .extend(conditions);
    }
    diagnostics
  }
}

/// Stylelint's `listImportConditions`: the media queries an `@import`'s
/// `rest` lists, each after its `layer` and `supports()` conditions, with
/// whitespace removed; or those conditions alone.
fn import_conditions(rest: &[ValueNode]) -> Vec<String> {
  let condition = |nodes: &[ValueNode]| -> String {
    value_parser::stringify(nodes)
      .chars()
      .filter(|c| !c.is_whitespace())
      .collect()
  };
  let mut shared: Vec<String> = Vec::new();
  let mut media: Vec<String> = Vec::new();
  let mut query: Vec<ValueNode> = Vec::new();
  for node in rest {
    if matches!(node.kind, ValueKind::Space | ValueKind::Comment) {
      continue;
    }
    // `layer` and `supports()` precede the media queries.
    if media.is_empty()
      && ((node.is_function() && matches!(node.value, "supports" | "layer"))
        || (node.is_word() && node.value == "layer"))
    {
      shared.push(condition(std::slice::from_ref(node)));
      continue;
    }
    if node.is_comma() {
      media.push(condition(&query));
      query.clear();
      continue;
    }
    query.push(node.clone());
  }
  if !query.is_empty() {
    media.push(condition(&query));
  }
  if shared.is_empty() {
    return media;
  }
  let shared = shared.join(" ");
  if media.is_empty() {
    return vec![shared];
  }
  media.into_iter().map(|m| format!("{shared} {m}")).collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{CheckSource, configured, context};

  /// What the rule reports for `source` in `syntax`: each message and the
  /// text it points at.
  fn check(source: &str, syntax: Syntax) -> Vec<(String, &str)> {
    configured(&NoDuplicateAtImportRules, None)
      .check_source(&context(source, syntax))
      .into_iter()
      .map(|d| {
        (
          d.message,
          &source[d.span.offset..d.span.offset + d.span.length],
        )
      })
      .collect()
  }

  #[test]
  fn reports_duplicate_imports() {
    let source = "@import \"reset.css\";\n@import url(normalize.css);\n@import url('reset.css');";
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
      assert_eq!(
        check(source, syntax),
        [(
          "Unexpected duplicate @import rule \"reset.css\"".to_string(),
          "@import url('reset.css');"
        )]
      );
    }
  }

  #[test]
  fn ignores_unique_imports() {
    assert!(
      check(
        "@import \"reset.css\";\n@import \"normalize.css\";",
        Syntax::Css
      )
      .is_empty()
    );
  }

  #[test]
  fn media_queries_and_layers_tell_imports_apart() {
    let source = "@import \"a.css\" screen;\n@import \"a.css\" print;\n@import \"a.css\" layer(x);";
    assert!(check(source, Syntax::Css).is_empty());
    // One query in common is a duplicate, and so is no condition after one.
    let source =
      "@import \"a.css\" screen, print;\n@import \"a.css\" PRINT;\n@import \"a.css\" print;";
    assert_eq!(check(source, Syntax::Css).len(), 1);
    assert_eq!(
      check("@import \"a.css\" tv;\n@import \"a.css\";", Syntax::Css).len(),
      1
    );
  }

  #[test]
  fn imports_after_rules_and_in_blocks_count() {
    let source = "@import \"a.css\";\na {}\n@IMPORT \"a.css\";\nb { @import \"a.css\"; }";
    assert_eq!(check(source, Syntax::Scss).len(), 2);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(invalid_options("no-duplicate-at-import-rules", serde_json::json!(true)).is_empty());
    // The rule takes nothing but `true`, and ignores secondary options.
    assert!(
      invalid_options(
        "no-duplicate-at-import-rules",
        serde_json::json!([true, { "foo": 1 }])
      )
      .is_empty()
    );
    assert_eq!(
      invalid_options("no-duplicate-at-import-rules", serde_json::json!("always")),
      ["Unexpected option value \"always\" for rule \"no-duplicate-at-import-rules\""]
    );
  }
}
