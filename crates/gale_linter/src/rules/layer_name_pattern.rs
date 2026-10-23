use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Pattern, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const DEFAULT_PATTERN: &str = "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$";

const NAME: &str = "layer-name-pattern";

declare_rule! {
  /// Enforces a naming pattern for CSS layer names.
  ///
  /// Checks `@layer` at-rules. Layer names can be dotted (`base.utilities`)
  /// and comma-separated (`@layer reset, base`). Each segment of a dotted
  /// name is validated individually.
  ///
  /// Accepts a regex string as the primary option.
  /// Defaults to kebab-case pattern if no option is provided.
  ///
  /// Equivalent to Stylelint's `layer-name-pattern` rule.
  pub struct LayerNamePattern => LayerNamePatternCheck {
    name: NAME,
    description: "Specify a pattern for layer names",
    default_severity: Warning,
  }
}

/// `layer-name-pattern` as configured.
pub struct LayerNamePatternCheck {
  /// The pattern every segment of a layer name must match.
  pattern: Pattern,
}

impl LayerNamePatternCheck {
  /// Stylelint's options: a regular expression, as a string or `/regex/`.
  /// Gale reads no option as kebab-case, and a pattern that does not
  /// compile is an invalid option.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let pattern = options.primary_pattern();
    options.finish()?;
    let pattern = match pattern {
      Some(pattern) => pattern,
      None => Pattern::compile(DEFAULT_PATTERN).expect("the default pattern compiles"),
    };
    Ok(Self { pattern })
  }
}

impl Check for LayerNamePatternCheck {
  /// Flags `@layer` names that do not match the configured regex, validating
  /// each segment of a dotted name separately and reporting at the segment.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let pattern_str = self.pattern.source.as_str();
    let mut diags = Vec::new();
    for at in tree.walk_at_rules() {
      if !at.name().eq_ignore_ascii_case("layer") {
        continue;
      }
      let params = at.raw_params();
      let base = at.params_start();
      // Layer names can be comma-separated (`@layer reset, base, utilities`)
      // and dotted (`base.utilities`): validate each segment.
      for (segment, offset) in segments(&params) {
        // A string is not a name (Stylelint only checks the words).
        if segment.starts_with(['"', '\'']) || self.pattern.is_match(segment) {
          continue;
        }
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Expected layer name segment \"{segment}\" to match pattern \"{pattern_str}\""),
          )
          .span(Span::new(base + offset, segment.len()))
          .message_args([segment, pattern_str]),
        );
      }
    }
    diags
  }
}

/// The non-empty, trimmed segments of the layer names in `params`, each
/// with its offset in `params`.
fn segments(params: &str) -> Vec<(&str, usize)> {
  let mut out = Vec::new();
  let mut name_start = 0;
  for name in params.split(',') {
    let mut segment_start = name_start;
    for segment in name.split('.') {
      let trimmed = segment.trim();
      if !trimmed.is_empty() {
        let lead = segment.len() - segment.trim_start().len();
        out.push((trimmed, segment_start + lead));
      }
      segment_start += segment.len() + 1;
    }
    name_start += name.len() + 1;
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::{Value, json};

  use crate::testing::lint;

  /// The reports for `@layer <params>;` with the default pattern.
  fn layer(params: &str) -> Vec<Diagnostic> {
    check(json!(DEFAULT_PATTERN), &format!("@layer {params};"))
  }

  fn check(options: Value, css: &str) -> Vec<Diagnostic> {
    lint(NAME, options, css, Syntax::Css)
  }

  #[test]
  fn reports_camel_case_layer_name_at_the_name() {
    let d = layer("myLayer");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myLayer"));
    assert_eq!((d[0].span.offset, d[0].span.length), (7, 7));
  }

  #[test]
  fn allows_kebab_case_layer_name() {
    assert!(layer("my-layer").is_empty());
    let block = "@LAYER my-layer { a { color: red } }";
    assert!(check(json!(DEFAULT_PATTERN), block).is_empty());
  }

  #[test]
  fn reports_bad_segment_in_dotted_name() {
    let d = layer("base.myUtils");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myUtils"));
    assert_eq!((d[0].span.offset, d[0].span.length), (12, 7));
  }

  #[test]
  fn allows_kebab_case_dotted_name() {
    assert!(layer("base.utilities").is_empty());
  }

  #[test]
  fn reports_bad_names_in_comma_list() {
    let d = layer("reset, myBase, utilities");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("myBase"));
    assert_eq!((d[0].span.offset, d[0].span.length), (14, 6));
  }

  #[test]
  fn allows_all_kebab_case_comma_list_and_anonymous_layers() {
    assert!(layer("reset, base, utilities").is_empty());
    assert!(layer("\"Quoted\"").is_empty());
    assert!(check(json!(DEFAULT_PATTERN), "@layer { a {} }").is_empty());
  }

  #[test]
  fn ignores_non_layer_at_rule() {
    assert!(check(json!(DEFAULT_PATTERN), "@media (min-width: 768px) {}").is_empty());
  }

  #[test]
  fn custom_pattern() {
    let opts = json!("^[A-Z][a-zA-Z0-9]+$");
    // PascalCase should pass
    assert!(check(opts.clone(), "@layer MyLayer;").is_empty());
    // kebab-case should fail
    assert_eq!(check(opts, "@layer my-layer;").len(), 1);
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    use crate::testing::invalid_options;
    assert!(
      invalid_options(NAME, serde_json::json!(["/^[a-z]+$/i", { "message": "x" }])).is_empty()
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!(5)),
      ["Invalid option value \"5\" for rule \"layer-name-pattern\""]
    );
    let broken = invalid_options(NAME, serde_json::json!("^[a-z"));
    assert_eq!(broken.len(), 1);
    assert!(
      broken[0].starts_with("Invalid option value \"^[a-z\" for rule \"layer-name-pattern\": "),
      "{broken:?}"
    );
  }
}
