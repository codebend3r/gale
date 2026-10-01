use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};
use crate::selector::postcss::{self, Escape, Kind};
use crate::standard_syntax::is_standard_syntax_selector;

/// Require or disallow quotes for attribute values in attribute selectors.
///
/// Equivalent to Stylelint's `selector-attribute-quotes` rule.  With
/// `"always"` (the default) every attribute value must be quoted; with
/// `"never"` none may be, unless the value is not a valid identifier without
/// them.  Attribute selectors without a value (e.g. `[disabled]`) are
/// ignored.  The fix requotes the value the way postcss-selector-parser
/// serialises it: in double quotes, or unquoted with identifier escapes.
pub struct SelectorAttributeQuotes;

impl Rule for SelectorAttributeQuotes {
  fn name(&self) -> &'static str {
    "selector-attribute-quotes"
  }

  fn description(&self) -> &'static str {
    "Require or disallow quotes for attribute values in attribute selectors"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Flags attribute selectors whose value quoting does not match the
  /// option, in every style rule's selector as written.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let never = ctx.primary_option_str() == Some("never");
    let mut diags = Vec::new();
    for rule in &ctx.scanned_rules().style_rules {
      if !has_attribute_with_operator(&rule.prelude) || !is_standard_syntax_selector(&rule.prelude)
      {
        continue;
      }
      let Some(selectors) = postcss::parse(&rule.prelude, rule.offset) else {
        continue;
      };
      postcss::walk(&selectors, &mut |visit| {
        let node = visit.node;
        if node.kind != Kind::Attribute {
          return;
        }
        let Some(attr) = &node.attr else { return };
        let Some(value) = &attr.value else { return };
        if attr.operator.is_none() || value.unescaped.is_empty() {
          return;
        }
        let quoted = value.quote.is_some();
        let (message, replacement) = if !never && !quoted {
          (
            format!("Expected quotes around \"{}\"", value.unescaped),
            postcss::cssesc(&value.unescaped, Escape::DoubleQuoted),
          )
        } else if never && quoted {
          // Some values need their quotes to be valid.
          let inner = value.raw.get(1..value.raw.len() - 1).unwrap_or("");
          if !is_valid_identifier(inner) {
            return;
          }
          let mut unquoted = postcss::cssesc(&value.unescaped, Escape::Identifier);
          // postcss-selector-parser separates an unquoted value from a
          // case-sensitivity flag that followed the quote directly.
          let flag_follows = attr.flag.is_some()
            && ctx
              .source_from(value.end)
              .is_some_and(|rest| !rest.starts_with(|c: char| c.is_whitespace() || c == '/'));
          if flag_follows {
            unquoted.push(' ');
          }
          (
            format!("Expected no quotes around \"{}\"", value.unescaped),
            unquoted,
          )
        } else {
          return;
        };
        let span = Span::from_range(value.start, value.end);
        diags.push(
          Diagnostic::new(self.name(), message)
            .severity(self.default_severity())
            .span(span)
            .fix(Fix::new(
              format!("Write the value as {replacement}"),
              vec![Edit::new(span, replacement)],
            )),
        );
      });
    }
    diags
  }
}

/// Stylelint's `mayIncludeRegexes.attributeSelectorWithOperator`
/// (`/\[.*=/`): a `[` followed by `=` on the same line.
fn has_attribute_with_operator(selector: &str) -> bool {
  selector.lines().any(|line| {
    line
      .find('[')
      .is_some_and(|open| line[open..].contains('='))
  })
}

/// Stylelint's `isValidIdentifier`, applied to a quoted value's raw
/// content: after dropping hex and single-character escapes, only word
/// characters and `-` remain, and it does not start with a digit or `-`
/// and a digit.
fn is_valid_identifier(ident: &str) -> bool {
  let trimmed = ident.trim();
  if trimmed.is_empty() {
    return false;
  }
  let mut rest = String::with_capacity(trimmed.len());
  let mut chars = trimmed.chars().peekable();
  while let Some(c) = chars.next() {
    if c != '\\' {
      rest.push(c);
      continue;
    }
    if chars.peek().is_some_and(char::is_ascii_hexdigit) {
      let mut digits = 0;
      while digits < 6 && chars.peek().is_some_and(char::is_ascii_hexdigit) {
        chars.next();
        digits += 1;
      }
      if chars
        .peek()
        .is_some_and(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\x0c'))
      {
        chars.next();
      }
    } else if chars.peek().is_some_and(|c| *c != '\n' && *c != '\r') {
      chars.next();
    } else {
      rest.push(c);
    }
  }
  if rest
    .chars()
    .any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
  {
    return false;
  }
  let mut chars = rest.chars();
  match chars.next() {
    Some(first) if first.is_ascii_digit() => false,
    Some('-') => !chars.next().is_some_and(|c| c.is_ascii_digit()),
    _ => true,
  }
}

#[cfg(test)]
mod tests {
  use gale_css_parser::Syntax;

  use crate::testing::{fix, lint};

  const RULE: &str = "selector-attribute-quotes";

  #[test]
  fn always_wraps_values_in_double_quotes() {
    let always = serde_json::json!("always");
    assert_eq!(
      fix(RULE, always.clone(), "a[ title=flower ] { }", Syntax::Css),
      "a[ title=\"flower\" ] { }"
    );
    assert_eq!(
      fix(RULE, always.clone(), "[class ^= top] { }", Syntax::Css),
      "[class ^= \"top\"] { }"
    );
    assert_eq!(
      fix(RULE, always.clone(), "[frame=hsides i] { }", Syntax::Css),
      "[frame=\"hsides\" i] { }"
    );
    assert_eq!(
      fix(RULE, always.clone(), "[href=te\\'s\\\"t] { }", Syntax::Css),
      "[href=\"te's\\\"t\"] { }"
    );
    assert_eq!(
      fix(RULE, always.clone(), "[href=\\'test\\'] { }", Syntax::Css),
      "[href=\"'test'\"] { }"
    );
    let warnings = lint(RULE, always, "a[title=flower] { }", Syntax::Css);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].message, "Expected quotes around \"flower\"");
    assert_eq!((warnings[0].span.offset, warnings[0].span.length), (8, 6));
  }

  #[test]
  fn never_unwraps_values_that_are_identifiers() {
    let never = serde_json::json!("never");
    assert_eq!(
      fix(RULE, never.clone(), "a[target=\"_blank\"] { }", Syntax::Css),
      "a[target=_blank] { }"
    );
    assert_eq!(
      fix(RULE, never.clone(), "[frame='hsides' i] { }", Syntax::Css),
      "[frame=hsides i] { }"
    );
    assert_eq!(
      fix(RULE, never.clone(), "[frame='hsides'i] { }", Syntax::Css),
      "[frame=hsides i] { }"
    );
    assert_eq!(
      fix(RULE, never.clone(), "[href='te\\'s\\'t'] { }", Syntax::Css),
      "[href=te\\'s\\'t] { }"
    );
    assert_eq!(
      fix(
        RULE,
        never.clone(),
        "a[target=\"_blank\"], /* comment */ a { }",
        Syntax::Css
      ),
      "a[target=_blank], /* comment */ a { }"
    );
    for css in ["[href=\"te'st\"] { }", "[a=\"1x\"] { }", "[a=\"b c\"] { }"] {
      assert!(
        lint(RULE, never.clone(), css, Syntax::Css).is_empty(),
        "{css}"
      );
    }
  }

  #[test]
  fn skips_valueless_and_interpolated_selectors() {
    let always = serde_json::json!("always");
    assert!(lint(RULE, always.clone(), "[title] { }", Syntax::Css).is_empty());
    assert!(
      lint(
        RULE,
        always.clone(),
        "[class=#{$variable}] { }",
        Syntax::Scss
      )
      .is_empty()
    );
    assert!(lint(RULE, always, "[class=@{variable}] { }", Syntax::Less).is_empty());
  }

  #[test]
  fn disable_fix_keeps_the_warning_but_not_the_fix() {
    let options = serde_json::json!(["always", { "disableFix": true }]);
    assert_eq!(
      lint(RULE, options.clone(), "[a=b] { }", Syntax::Css).len(),
      1
    );
    assert_eq!(fix(RULE, options, "[a=b] { }", Syntax::Css), "[a=b] { }");
  }
}
