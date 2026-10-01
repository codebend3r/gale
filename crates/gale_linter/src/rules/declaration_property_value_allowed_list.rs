use gale_css_parser::{CssNode, Declaration};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::pattern;
use crate::rule::{Rule, RuleContext};
use crate::source_text;

/// Only allow specified values for specific properties.
///
/// Options: an object mapping properties to the values allowed for them.
/// Keys and list entries are exact strings or `/regex/` literals, as in
/// Stylelint: a string must equal the whole value, a regex need only match
/// part of it.  Keys are matched against the property without its vendor
/// prefix.
/// Example: `{"display": ["block", "flex"], "/^border/": ["/var\\(/", "none"]}`
///
/// Equivalent to Stylelint's `declaration-property-value-allowed-list` rule.
pub struct DeclarationPropertyValueAllowedList;

/// `property` without a leading vendor prefix (`-webkit-`, `-moz-`, ...),
/// as Stylelint's `vendor.unprefixed` strips `^-\w+-`.
fn unprefixed(property: &str) -> &str {
  let Some(rest) = property.strip_prefix('-') else {
    return property;
  };
  let word = rest
    .bytes()
    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
    .count();
  match rest[word..].strip_prefix('-') {
    Some(name) if word > 0 => name,
    _ => property,
  }
}

/// Whether `value` matches one of the entries listed for a property: a
/// list of strings and `/regex/` literals, or a single one.
fn list_allows(entries: &serde_json::Value, value: &str) -> bool {
  pattern::option_matches(Some(entries), value)
}

impl DeclarationPropertyValueAllowedList {
  /// The report for `decl` when the configured lists that apply to its
  /// property all reject its value.
  fn check_declaration(
    &self,
    decl: &Declaration,
    allowed: &serde_json::Map<String, serde_json::Value>,
    ctx: &RuleContext,
  ) -> Option<Diagnostic> {
    let property = unprefixed(&decl.property);
    let mut lists = allowed
      .iter()
      .filter(|(key, _)| pattern::matches_entry(key, property))
      .map(|(_, entries)| entries)
      .peekable();
    lists.peek()?;
    // The value as written: the parser's re-serialised one can differ.
    let (value, offset) = source_text::declaration_value(ctx.source, decl)
      .unwrap_or((decl.value.as_str(), decl.span.offset));
    if lists.any(|entries| list_allows(entries, value)) {
      return None;
    }
    Some(
      Diagnostic::new(
        self.name(),
        format!(
          "Unexpected value \"{value}\" for property \"{}\"",
          decl.property
        ),
      )
      .severity(self.default_severity())
      .span(Span::new(offset, value.len())),
    )
  }
}

impl Rule for DeclarationPropertyValueAllowedList {
  fn name(&self) -> &'static str {
    "declaration-property-value-allowed-list"
  }

  fn description(&self) -> &'static str {
    "Specify a list of allowed property and value pairs within declarations"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags a declaration whose property has allowed values configured and
  /// whose value matches none of them.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let Some(allowed) = ctx.primary_option().and_then(|v| v.as_object()) else {
      return vec![];
    };
    if allowed.is_empty() {
      return vec![];
    }
    match node {
      CssNode::Style(rule) => rule
        .declarations
        .iter()
        .filter_map(|decl| self.check_declaration(decl, allowed, ctx))
        .collect(),
      CssNode::Declaration(decl) => self
        .check_declaration(decl, allowed, ctx)
        .into_iter()
        .collect(),
      _ => vec![],
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};
  use serde_json::json;

  fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: Some(opts),
    }
  }

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_decl(prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, 10),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn allows_all_when_no_options() {
    let d = DeclarationPropertyValueAllowedList.check(&style_with_decl("display", "none"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_listed_value() {
    let opts = json!({"display": ["block", "flex"]});
    let d = DeclarationPropertyValueAllowedList.check(
      &style_with_decl("display", "flex"),
      &ctx_with_options(&opts),
    );
    assert!(d.is_empty());
  }

  #[test]
  fn rejects_unlisted_value() {
    let opts = json!({"display": ["block", "flex"]});
    let d = DeclarationPropertyValueAllowedList.check(
      &style_with_decl("display", "none"),
      &ctx_with_options(&opts),
    );
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("none"));
  }

  #[test]
  fn case_sensitive_value_match() {
    let opts = json!({"display": ["FLEX"]});
    let d = DeclarationPropertyValueAllowedList.check(
      &style_with_decl("display", "flex"),
      &ctx_with_options(&opts),
    );
    // "FLEX" does not match "flex" -- strict matching
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn plain_entries_must_equal_the_whole_value() {
    let opts = json!({"display": ["flex"]});
    let d = DeclarationPropertyValueAllowedList.check(
      &style_with_decl("display", "inline-flex"),
      &ctx_with_options(&opts),
    );
    assert_eq!(d.len(), 1, "a plain entry is no substring match");
  }

  /// Lint `source` with the real parser and options `opts`, returning
  /// `(line:column, message)` for each report.
  fn lint(source: &str, opts: &serde_json::Value) -> Vec<(String, String)> {
    let parsed = gale_css_parser::parse(source, Syntax::Css).expect("parses");
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: Some(opts),
    };
    let mut out = Vec::new();
    for node in &parsed.nodes {
      for d in DeclarationPropertyValueAllowedList.check(node, &ctx) {
        let before = &source[..d.span.offset];
        let line = before.matches('\n').count() + 1;
        let column = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
        out.push((format!("{line}:{column}"), d.message));
      }
    }
    out
  }

  #[test]
  fn regex_entries_and_keys_match_like_stylelint() {
    // jupyterlab's config: regex values, some anchored, some not.
    let opts = json!({
      "color": ["/var\\(/", "/^unset|inherit|white|black$/", "/^transparent$/"],
      "/^border(-color)?$/": ["/var\\(/", "/^none$/", "/^0$/"],
      "font-family": ["/^var\\(/", "/^[\"']?MJX/"]
    });
    let source = "a {\n  color: var(--jp-ui-font-color1);\n  color: rgba(0 0 0 / 50%);\n  border: 0;\n  border-color: red;\n  font-family: 'MJXZERO';\n}\n";
    assert_eq!(
      lint(source, &opts),
      vec![
        (
          "3:10".to_string(),
          "Unexpected value \"rgba(0 0 0 / 50%)\" for property \"color\"".to_string()
        ),
        (
          "5:17".to_string(),
          "Unexpected value \"red\" for property \"border-color\"".to_string()
        ),
      ]
    );
  }

  #[test]
  fn checks_and_prints_the_value_as_written() {
    // The CSS parser turns `transparent` into `none` for `background`, and
    // shortens `#ffffff`; the rule must see neither.
    let opts = json!({"background": ["transparent"], "color": ["/^#f{6}$/"]});
    let source = "a { background: transparent; color: #ffffff; }";
    assert!(lint(source, &opts).is_empty());
    let source = "a { background: transparent  !important; color: #FFF; }";
    assert_eq!(
      lint(source, &opts),
      vec![(
        "1:49".to_string(),
        "Unexpected value \"#FFF\" for property \"color\"".to_string()
      )]
    );
  }

  #[test]
  fn vendor_prefixed_property_matches_its_unprefixed_key() {
    let opts = json!({"display": ["block"]});
    let d = DeclarationPropertyValueAllowedList.check(
      &style_with_decl("-webkit-display", "flex"),
      &ctx_with_options(&opts),
    );
    // Stylelint strips the vendor prefix before looking the property up.
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Unexpected value \"flex\" for property \"-webkit-display\""
    );
  }

  #[test]
  fn ignores_unconfigured_properties() {
    let opts = json!({"display": ["block"]});
    let d = DeclarationPropertyValueAllowedList
      .check(&style_with_decl("color", "red"), &ctx_with_options(&opts));
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(
      DeclarationPropertyValueAllowedList.name(),
      "declaration-property-value-allowed-list"
    );
  }
}
