use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "declaration-no-important";

declare_rule! {
  /// Reports use of `!important` in declarations.
  ///
  /// Equivalent to Stylelint's `declaration-no-important` rule.
  pub struct DeclarationNoImportant => DeclarationNoImportantCheck {
    name: NAME,
    description: "Disallow !important within declarations",
    default_severity: Warning,
  }
}

/// `declaration-no-important`, which takes no options.
pub struct DeclarationNoImportantCheck;

impl DeclarationNoImportantCheck {
  /// Stylelint validates `{ actual: primary }` with nothing possible: the
  /// rule takes nothing but `true`.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    options.no_primary();
    options.finish()?;
    Ok(Self)
  }
}

impl Check for DeclarationNoImportantCheck {
  /// Flags `!important` declarations, locating the token in the source so the
  /// fix removes it along with the whitespace in front of it.
  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };
    let mut diags = Vec::new();
    for decl in &rule.declarations {
      if decl.important {
        let decl_start = decl.span.offset;
        let decl_end = decl_start + decl.span.length;

        // Try to find `!important` in the source span to build a precise fix.
        let mut fix_opt = None;
        if decl_end <= ctx.source.len() && decl_start < decl_end {
          let slice = ctx.source_slice(decl_start, decl_end).unwrap_or("");
          if let Some(pos) = find_important(slice) {
            let abs_pos = decl_start + pos;
            let imp_text = &slice[pos..];
            // The `!important` token may have whitespace before the `!`.
            // Find the exact length: `!important` is 10 chars, but we
            // also want to remove any preceding whitespace.
            let imp_len = imp_text
              .find(';')
              .unwrap_or(imp_text.len())
              .min("!important".len());
            // Also remove leading whitespace before `!`.
            let leading_ws = slice[..pos]
              .bytes()
              .rev()
              .take_while(|b| *b == b' ' || *b == b'\t')
              .count();
            let fix_start = abs_pos - leading_ws;
            let fix_len = imp_len + leading_ws;
            fix_opt = Some(Fix::new(
              "Remove !important",
              vec![Edit::new(Span::new(fix_start, fix_len), "")],
            ));
          }
        }

        // Point the diagnostic span at the `!important` text when
        // possible so that `disable-line` comments on the same line
        // as `!important` (which may differ from the declaration
        // start line for multi-line declarations) correctly suppress
        // the diagnostic.
        let diag_span = if let Some(pos) = fix_opt.as_ref().and_then(|_| {
          if decl_end <= ctx.source.len() && decl_start < decl_end {
            let slice = ctx.source_slice(decl_start, decl_end).unwrap_or("");
            find_important(slice).map(|p| decl_start + p)
          } else {
            None
          }
        }) {
          Span::new(pos, "!important".len())
        } else {
          decl.span
        };

        let mut diag = Diagnostic::new(
          NAME,
          format!("Unexpected !important in declaration \"{}\"", decl.property),
        )
        .span(diag_span);

        if let Some(fix) = fix_opt {
          diag = diag.fix(fix);
        }
        diags.push(diag);
      }
    }
    diags
  }
}

/// Find the byte offset of `!important` (case-insensitive) in a string slice.
fn find_important(s: &str) -> Option<usize> {
  let lower = s.to_ascii_lowercase();
  lower.find("!important")
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, StyleRule};

  use crate::testing::{configured, ctx_with_source};

  fn style_node(prop: &str, val: &str, important: bool, offset: usize, len: usize) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: Span::new(offset, len),
        important,
      }],
      span: Span::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_important() {
    let src = "a { color: red !important; }";
    let node = style_node("color", "red", true, 4, 22);
    let d = configured(&DeclarationNoImportant, None).check(&node, &ctx_with_source(src));
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("!important"));
    assert!(d[0].fix.is_some());
  }

  #[test]
  fn allows_no_important() {
    let src = "a { color: red; }";
    let node = style_node("color", "red", false, 4, 11);
    let d = configured(&DeclarationNoImportant, None).check(&node, &ctx_with_source(src));
    assert!(d.is_empty());
  }
}
