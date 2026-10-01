use gale_css_parser::{AtRule, CssNode, Declaration};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::pattern::{self, Regex};
use crate::rule::{Rule, RuleContext};
use crate::source_text;
use crate::value_parser::{self, ValueNode};

/// Enforces a naming pattern for container names.
///
/// Checks the names in `container-name` and `container` declarations (up to
/// the `/` that starts the container type) and in `@container` params.  The
/// primary option is the pattern, a regex string.
///
/// Equivalent to Stylelint's `container-name-pattern` rule.  Like it, names
/// with Sass or Less syntax in them (`$name`, `#{$prefix}name`, `@name`)
/// are left alone, and so are the CSS-wide keywords in declarations and
/// `and`, `or`, `not` and `none` in `@container` params.
pub struct ContainerNamePattern;

/// CSS-wide keywords, never container names (Stylelint's `basicKeywords`).
const CSS_WIDE_KEYWORDS: &[&str] = &["initial", "inherit", "revert", "revert-layer", "unset"];

/// Words in `@container` params that are not container names.
const AT_RULE_KEYWORDS: &[&str] = &["and", "or", "none", "not"];

/// Whether `value` is plain CSS, not a Sass or Less variable, namespace or
/// interpolation: Stylelint's `isStandardSyntaxValue`.
pub(crate) fn is_standard_syntax_value(value: &str) -> bool {
  let normalized = value.strip_prefix(['-', '+', '*', '/']).unwrap_or(value);
  if normalized.starts_with('$') || normalized.starts_with('@') {
    return false;
  }
  // `namespace.$variable` and `namespace.function(`: a `.` after at least
  // one character, then `$` or an identifier and `(`.
  let namespaced = value.match_indices('.').any(|(dot, _)| {
    let after = &value[dot + 1..];
    let ident = after
      .bytes()
      .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
      .count();
    dot > 0 && (after.starts_with('$') || (ident > 0 && after[ident..].starts_with('(')))
  });
  if namespaced {
    return false;
  }
  !has_interpolation(normalized) && !has_webextension_keyword(value)
}

/// Whether `text` holds Sass `#{...}`, Less `@{...}`, template `{...}` or
/// `$(...)` interpolation, as Stylelint's `hasInterpolation` tests.
fn has_interpolation(text: &str) -> bool {
  // `{`, at least one character, then `}` covers the Sass, Less and
  // template forms alike; `$(`, at least one character, then `)` is the
  // last.
  let braces = text
    .find('{')
    .and_then(|open| text.get(open + 2..))
    .is_some_and(|rest| rest.contains('}'));
  let psv = text
    .find("$(")
    .and_then(|open| text.get(open + 3..))
    .is_some_and(|rest| rest.contains(')'));
  braces || psv
}

/// Whether `text` holds a WebExtension `__MSG_name__` placeholder.
fn has_webextension_keyword(text: &str) -> bool {
  text.match_indices("__MSG_").any(|(start, _)| {
    let rest = &text[start + 6..];
    let name = rest
      .bytes()
      .take_while(|b| !b.is_ascii_whitespace())
      .count();
    rest[..name].len() > 2
      && rest[..name]
        .get(1..)
        .is_some_and(|tail| tail.contains("__"))
  })
}

impl ContainerNamePattern {
  /// The report for one name that does not match `pattern`.
  fn report(&self, name: &str, pattern: &str, offset: usize) -> Diagnostic {
    Diagnostic::new(
      self.name(),
      format!("Expected \"{name}\" to match pattern \"{pattern}\""),
    )
    .severity(self.default_severity())
    .span(Span::new(offset, name.len()))
    .message_args([name, pattern])
  }

  /// Reports for the names in a `container` or `container-name` declaration.
  fn check_declaration(
    &self,
    decl: &Declaration,
    re: &Regex,
    pattern: &str,
    ctx: &RuleContext,
  ) -> Vec<Diagnostic> {
    let property = decl.property.to_ascii_lowercase();
    if property != "container" && property != "container-name" {
      return vec![];
    }
    let Some((value, offset)) = source_text::declaration_value(ctx.source, decl) else {
      return vec![];
    };
    let mut diags = Vec::new();
    let mut container_type = false;
    value_parser::walk(&value_parser::parse(value), &mut |node: &ValueNode| {
      if container_type {
        return true;
      }
      if node.is_slash() {
        container_type = true;
      }
      if !node.is_word() {
        return false;
      }
      let name = node.value;
      let keyword = CSS_WIDE_KEYWORDS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(name));
      if !keyword && is_standard_syntax_value(name) && !pattern::is_match(re, name) {
        diags.push(self.report(name, pattern, offset + node.source_index));
      }
      true
    });
    diags
  }

  /// Reports for the names in `@container` params.
  fn check_at_rule(
    &self,
    at: &AtRule,
    re: &Regex,
    pattern: &str,
    ctx: &RuleContext,
  ) -> Vec<Diagnostic> {
    if !at.name.eq_ignore_ascii_case("container") {
      return vec![];
    }
    let Some((params, offset)) = source_text::at_rule_params(ctx.source, at) else {
      return vec![];
    };
    let mut diags = Vec::new();
    value_parser::walk(&value_parser::parse(params), &mut |node: &ValueNode| {
      if !node.is_word() {
        return false;
      }
      let name = node.value;
      let keyword = AT_RULE_KEYWORDS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(name));
      if !keyword && !pattern::is_match(re, name) {
        diags.push(self.report(name, pattern, offset + node.source_index));
      }
      true
    });
    diags
  }
}

impl Rule for ContainerNamePattern {
  fn name(&self) -> &'static str {
    "container-name-pattern"
  }

  fn description(&self) -> &'static str {
    "Specify a pattern for container names"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags container names in declarations and `@container` params that do
  /// not match the configured pattern.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(pattern_str) = ctx.primary_option_str() else {
      return vec![];
    };

    // A pattern that does not compile is reported as an invalid option,
    // as Stylelint does, rather than silently switching the rule off.
    let re = match pattern::for_rule(self.name(), pattern_str) {
      Ok(re) => re,
      Err(invalid) => return vec![invalid],
    };

    match node {
      CssNode::Style(rule) => rule
        .declarations
        .iter()
        .flat_map(|decl| self.check_declaration(decl, &re, pattern_str, ctx))
        .collect(),
      CssNode::Declaration(decl) => self.check_declaration(decl, &re, pattern_str, ctx),
      CssNode::AtRule(at) => self.check_at_rule(at, &re, pattern_str, ctx),
      CssNode::Comment(_) => vec![],
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  const KEBAB: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

  /// Lint `source` with the real parser, returning `(offset, message)` for
  /// each report.
  fn lint(source: &str, syntax: Syntax, options: serde_json::Value) -> Vec<(usize, String)> {
    let parsed = gale_css_parser::parse(source, syntax).expect("parses");
    let ctx = RuleContext {
      file_path: "t.scss",
      source,
      syntax,
      options: Some(&options),
      cache: None,
    };
    let mut out = Vec::new();
    let mut stack: Vec<CssNode> = parsed.nodes.clone();
    while let Some(node) = stack.pop() {
      for d in ContainerNamePattern.check(&node, &ctx) {
        out.push((d.span.offset, d.message));
      }
      match &node {
        CssNode::AtRule(at) => stack.extend(at.children.iter().cloned()),
        CssNode::Style(rule) => {
          stack.extend(rule.nested_at_rules.iter().cloned());
          stack.extend(rule.children.iter().cloned().map(CssNode::Style));
        }
        _ => {}
      }
    }
    out.sort();
    out
  }

  #[test]
  fn reports_each_bad_name_at_its_own_offset() {
    let source = ".a { container-name: good myContainer Other; }";
    assert_eq!(
      lint(source, Syntax::Css, serde_json::json!(KEBAB)),
      vec![
        (
          26,
          format!("Expected \"myContainer\" to match pattern \"{KEBAB}\"")
        ),
        (
          38,
          format!("Expected \"Other\" to match pattern \"{KEBAB}\"")
        ),
      ]
    );
  }

  #[test]
  fn container_shorthand_stops_at_the_type() {
    let source = ".a { container: myName / inline-size; }";
    let got = lint(source, Syntax::Css, serde_json::json!(KEBAB));
    assert_eq!(got.len(), 1);
    assert!(got[0].1.contains("\"myName\""));
  }

  #[test]
  fn keywords_and_sass_syntax_are_not_names() {
    let source = "@include root {\n  container-name: #{$prefix}contain-viewport #{$prefix}contain-table;\n}\n.a { container-name: $name inherit none; }\n";
    // The option as patternfly's extended standard config writes it.
    let options = serde_json::json!(["^(--)?([a-z][a-z0-9]*)(-[a-z0-9]+)*$", { "message": "x" }]);
    assert!(lint(source, Syntax::Scss, options).is_empty());
  }

  #[test]
  fn checks_names_in_container_params() {
    let source = "@container sideBar (min-width: 400px) and style(--x: y) {\n  a { color: red; }\n}\n@container (min-width: 1px) {\n  a { color: red; }\n}\n";
    assert_eq!(
      lint(source, Syntax::Css, serde_json::json!(KEBAB)),
      vec![(
        11,
        format!("Expected \"sideBar\" to match pattern \"{KEBAB}\"")
      )]
    );
  }

  #[test]
  fn uses_the_configured_pattern() {
    let source = ".a { container-name: myContainer my-container; }";
    let got = lint(
      source,
      Syntax::Css,
      serde_json::json!("^[a-z][a-zA-Z0-9]+$"),
    );
    assert_eq!(got.len(), 1);
    assert!(got[0].1.contains("\"my-container\""));
  }

  #[test]
  fn standard_syntax_values() {
    assert!(is_standard_syntax_value("sidebar"));
    assert!(is_standard_syntax_value("-sidebar"));
    assert!(is_standard_syntax_value("a{}"));
    assert!(is_standard_syntax_value(".5"));
    for value in [
      "$x",
      "-$x",
      "@x",
      "#{$a}b",
      "@{a}b",
      "{}a}",
      "ns.$x",
      "a.b.$c",
      "ns.fn(",
      "__MSG_name__",
    ] {
      assert!(!is_standard_syntax_value(value), "{value}");
    }
  }
}
