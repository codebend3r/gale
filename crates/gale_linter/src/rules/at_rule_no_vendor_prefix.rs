use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Reports vendor-prefixed at-rules (e.g. `@-webkit-keyframes`).
///
/// Equivalent to Stylelint's `at-rule-no-vendor-prefix` rule.
pub struct AtRuleNoVendorPrefix;

/// Strip the vendor prefix from an at-rule name.
/// E.g. `-webkit-keyframes` → `keyframes`.
fn strip_vendor_prefix(name: &str) -> &str {
  let lower = name.to_ascii_lowercase();
  for prefix in &["-webkit-", "-moz-", "-ms-", "-o-"] {
    if lower.starts_with(prefix) {
      return &name[prefix.len()..];
    }
  }
  name
}

impl Rule for AtRuleNoVendorPrefix {
  fn name(&self) -> &'static str {
    "at-rule-no-vendor-prefix"
  }

  fn description(&self) -> &'static str {
    "Disallow vendor prefixes for at-rules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::AtRule(rule) = node else {
      return vec![];
    };
    if rule.name.starts_with('-') {
      let unprefixed = strip_vendor_prefix(&rule.name);
      // Search the source within the at-rule span for `@-prefix-name`
      let rule_start = rule.span.offset;
      let rule_end = rule_start + rule.span.length;
      let prefixed_at = format!("@{}", rule.name);
      let unprefixed_at = format!("@{unprefixed}");

      let fix = if rule_end <= ctx.source.len() && rule_start < rule_end {
        let search_area = &ctx.source[rule_start..rule_end];
        // Case-insensitive search for the prefixed at-rule name
        let lower_search = search_area.to_ascii_lowercase();
        let lower_target = prefixed_at.to_ascii_lowercase();
        lower_search.find(&lower_target).map(|rel_offset| {
          let abs_offset = rule_start + rel_offset;
          Fix::new(
            format!("Remove vendor prefix from @{}", rule.name),
            vec![Edit::new(
              Span::new(abs_offset, prefixed_at.len()),
              &unprefixed_at,
            )],
          )
        })
      } else {
        None
      };

      let mut diag = Diagnostic::new(
        self.name(),
        format!("Unexpected vendor-prefixed at-rule \"@{}\"", rule.name),
      )
      .severity(self.default_severity())
      .span(Span::new(rule.span.offset, rule.span.length));

      if let Some(f) = fix {
        diag = diag.fix(f);
      }

      vec![diag]
    } else {
      vec![]
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{AtRule as CssAtRule, Span as ParserSpan, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn at_rule(name: &str) -> CssNode {
    CssNode::AtRule(CssAtRule {
      name: name.to_string(),
      params: "fade".to_string(),
      span: ParserSpan::new(0, 0),
      children: vec![],
    })
  }

  #[test]
  fn reports_vendor_prefixed_at_rule() {
    let d = AtRuleNoVendorPrefix.check(&at_rule("-webkit-keyframes"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("-webkit-keyframes"));
  }

  #[test]
  fn allows_standard_at_rule() {
    assert!(
      AtRuleNoVendorPrefix
        .check(&at_rule("keyframes"), &ctx())
        .is_empty()
    );
    assert!(
      AtRuleNoVendorPrefix
        .check(&at_rule("media"), &ctx())
        .is_empty()
    );
  }

  #[test]
  fn emits_fix_for_vendor_prefixed_at_rule() {
    let source = "@-webkit-keyframes fade { }";
    let ctx = RuleContext {
      file_path: "t.css",
      source,
      syntax: Syntax::Css,
      options: None,
    };
    let node = CssNode::AtRule(CssAtRule {
      name: "-webkit-keyframes".to_string(),
      params: "fade".to_string(),
      span: ParserSpan::new(0, source.len()),
      children: vec![],
    });
    let d = AtRuleNoVendorPrefix.check(&node, &ctx);
    assert_eq!(d.len(), 1);
    assert!(d[0].fix.is_some());
    let fix = d[0].fix.as_ref().unwrap();
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].new_text, "@keyframes");
  }
}
