use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity};

/// Context passed to each rule when checking a node.
pub struct RuleContext<'a> {
  /// The path to the file being linted.
  pub file_path: &'a str,
  /// The full source text of the file.
  pub source: &'a str,
  /// The CSS syntax variant.
  pub syntax: Syntax,
  /// Per-rule options from the config (e.g. max value, ignore lists).
  pub options: Option<&'a serde_json::Value>,
}

impl<'a> RuleContext<'a> {
  /// Extract the **primary option** from Stylelint-format options.
  ///
  /// Options may arrive in several shapes depending on how the config was
  /// written and resolved:
  ///
  /// - A plain value (string, number, bool): `"always"`, `4`, `true`
  /// - An array `[primary, secondary]`: `["always", { "except": [...] }]`
  ///
  /// This method normalises all forms and returns the primary option value.
  pub fn primary_option(&self) -> Option<&'a serde_json::Value> {
    let value = self.options?;
    match value {
      serde_json::Value::Array(arr) => arr.first(),
      other => Some(other),
    }
  }

  /// Extract the primary option as a `&str`, handling both plain strings
  /// and array-format options `["value", { ... }]`.
  pub fn primary_option_str(&self) -> Option<&'a str> {
    self.primary_option().and_then(|v| v.as_str())
  }

  /// Extract the **secondary options object** from Stylelint-format options.
  ///
  /// Present when options are in array form `[primary, secondary]` (returns
  /// the second element), OR when options is a bare object (returned directly,
  /// e.g. from preset configs that store `{"ignore": [...]}` without a
  /// primary option wrapper).
  pub fn secondary_options(&self) -> Option<&'a serde_json::Value> {
    secondary_options_of(self.options?)
  }
}

/// The secondary options object inside a Stylelint-format rule setting.
///
/// Array form `[primary, secondary]` yields the second element; a bare object
/// (as preset configs store `{"ignore": [...]}`) is returned as-is.
pub fn secondary_options_of(value: &serde_json::Value) -> Option<&serde_json::Value> {
  match value {
    serde_json::Value::Array(arr) => arr.get(1),
    serde_json::Value::Object(_) => Some(value),
    _ => None,
  }
}

/// A single lint rule that can inspect CSS AST nodes and emit diagnostics.
pub trait Rule: Send + Sync {
  /// Unique rule name, e.g. "block-no-empty".
  fn name(&self) -> &'static str;

  /// Human-readable description of what this rule checks.
  fn description(&self) -> &'static str;

  /// The default severity for diagnostics produced by this rule.
  fn default_severity(&self) -> Severity;

  /// Check a single CSS node and return any diagnostics.
  fn check(&self, node: &CssNode, context: &RuleContext) -> Vec<Diagnostic> {
    let _ = (node, context);
    vec![]
  }

  /// Check the entire document (all top-level nodes). Used for rules that need
  /// document-level context like duplicate detection or source-level checks.
  fn check_root(&self, _nodes: &[CssNode], _context: &RuleContext) -> Vec<Diagnostic> {
    vec![]
  }
}
