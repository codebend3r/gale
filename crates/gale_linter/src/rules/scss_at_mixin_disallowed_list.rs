use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify a list of disallowed SCSS mixin names.
///
/// Primary option: an array of mixin name strings or regex patterns (wrapped
/// in `/`).
///
/// ```json
/// ["breakpoint", "legacy-mixin", "/^old-/"]
/// ```
///
/// For each `@include` at-rule found in SCSS files, the mixin name is checked
/// against the list.  Plain strings are matched exactly (case-sensitive).
/// Regex patterns (delimited by `/`) are matched against the full mixin name.
///
/// Equivalent to `scss/at-mixin-disallowed-list`.
pub struct ScssAtMixinDisallowedList;

impl Rule for ScssAtMixinDisallowedList {
    fn name(&self) -> &'static str {
        "scss/at-mixin-disallowed-list"
    }

    fn description(&self) -> &'static str {
        "Specify a list of disallowed mixins"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
        if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
            return vec![];
        }

        let CssNode::AtRule(at) = node else {
            return vec![];
        };

        if at.name != "include" {
            return vec![];
        }

        let disallowed: Vec<String> = match ctx.options {
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => return vec![],
        };

        if disallowed.is_empty() {
            return vec![];
        }

        // Extract the mixin name from params.
        // params is e.g. "breakpoint(red)" or "mixin-name" or "mixin-name()"
        let params = at.params.trim();
        let mixin_name = match params.find('(') {
            Some(pos) => params[..pos].trim(),
            None => params,
        };

        if mixin_name.is_empty() {
            return vec![];
        }

        if is_disallowed(mixin_name, &disallowed) {
            vec![
                Diagnostic::new(self.name(), format!("Unexpected mixin \"{}\"", mixin_name))
                    .severity(self.default_severity())
                    .span(Span::new(at.span.offset, at.span.length)),
            ]
        } else {
            vec![]
        }
    }
}

/// Check whether `name` matches any entry in the disallowed list.
///
/// Plain strings are compared exactly (case-sensitive).  Entries wrapped in
/// `/` are treated as regex patterns.
fn is_disallowed(name: &str, disallowed: &[String]) -> bool {
    for pattern in disallowed {
        if pattern.starts_with('/') && pattern.len() > 1 {
            if let Some(inner) = pattern.strip_prefix('/').and_then(|s| s.strip_suffix('/')) {
                if let Ok(re) = regex::Regex::new(inner) {
                    if re.is_match(name) {
                        return true;
                    }
                }
            }
        } else if pattern == name {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::{AtRule, Span as ParserSpan, Syntax};

    fn scss_ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.scss",
            source: "",
            syntax: Syntax::Scss,
            options: Some(opts),
        }
    }

    fn css_ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.css",
            source: "",
            syntax: Syntax::Css,
            options: Some(opts),
        }
    }

    fn include_node(mixin_params: &str) -> CssNode {
        CssNode::AtRule(AtRule {
            name: "include".to_string(),
            params: mixin_params.to_string(),
            span: ParserSpan::new(0, 10),
            children: vec![],
        })
    }

    #[test]
    fn detects_disallowed_mixin_exact_name() {
        let opts = serde_json::json!(["breakpoint"]);
        let d = ScssAtMixinDisallowedList.check(
            &include_node("breakpoint(medium)"),
            &scss_ctx_with_options(&opts),
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("breakpoint"));
    }

    #[test]
    fn detects_disallowed_mixin_no_args() {
        let opts = serde_json::json!(["breakpoint"]);
        let d = ScssAtMixinDisallowedList
            .check(&include_node("breakpoint"), &scss_ctx_with_options(&opts));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("breakpoint"));
    }

    #[test]
    fn allows_non_disallowed_mixin() {
        let opts = serde_json::json!(["breakpoint"]);
        let d = ScssAtMixinDisallowedList.check(
            &include_node("button-styles(primary)"),
            &scss_ctx_with_options(&opts),
        );
        assert!(d.is_empty());
    }

    #[test]
    fn regex_pattern_matching() {
        let opts = serde_json::json!(["/^old-/"]);
        let d = ScssAtMixinDisallowedList.check(
            &include_node("old-button(red)"),
            &scss_ctx_with_options(&opts),
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("old-button"));
    }

    #[test]
    fn regex_does_not_match_non_matching() {
        let opts = serde_json::json!(["/^old-/"]);
        let d = ScssAtMixinDisallowedList.check(
            &include_node("new-button(red)"),
            &scss_ctx_with_options(&opts),
        );
        assert!(d.is_empty());
    }

    #[test]
    fn only_triggers_on_scss() {
        let opts = serde_json::json!(["breakpoint"]);
        let d = ScssAtMixinDisallowedList.check(
            &include_node("breakpoint(medium)"),
            &css_ctx_with_options(&opts),
        );
        assert!(d.is_empty());
    }

    #[test]
    fn multiple_disallowed_mixins() {
        let opts = serde_json::json!(["breakpoint", "legacy-mixin", "/^old-/"]);

        let d1 = ScssAtMixinDisallowedList.check(
            &include_node("breakpoint(medium)"),
            &scss_ctx_with_options(&opts),
        );
        assert_eq!(d1.len(), 1);

        let d2 = ScssAtMixinDisallowedList
            .check(&include_node("legacy-mixin"), &scss_ctx_with_options(&opts));
        assert_eq!(d2.len(), 1);

        let d3 = ScssAtMixinDisallowedList
            .check(&include_node("old-header"), &scss_ctx_with_options(&opts));
        assert_eq!(d3.len(), 1);

        let d4 = ScssAtMixinDisallowedList
            .check(&include_node("new-button"), &scss_ctx_with_options(&opts));
        assert!(d4.is_empty());
    }

    #[test]
    fn no_options_no_report() {
        let ctx = RuleContext {
            file_path: "t.scss",
            source: "",
            syntax: Syntax::Scss,
            options: None,
        };
        let d = ScssAtMixinDisallowedList.check(&include_node("breakpoint(medium)"), &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_include_at_rules() {
        let opts = serde_json::json!(["breakpoint"]);
        let node = CssNode::AtRule(AtRule {
            name: "media".to_string(),
            params: "breakpoint".to_string(),
            span: ParserSpan::new(0, 10),
            children: vec![],
        });
        let d = ScssAtMixinDisallowedList.check(&node, &scss_ctx_with_options(&opts));
        assert!(d.is_empty());
    }

    #[test]
    fn exact_match_is_case_sensitive() {
        let opts = serde_json::json!(["Breakpoint"]);
        let d = ScssAtMixinDisallowedList
            .check(&include_node("breakpoint"), &scss_ctx_with_options(&opts));
        assert!(d.is_empty());
    }

    #[test]
    fn works_with_empty_parens() {
        let opts = serde_json::json!(["reset"]);
        let d = ScssAtMixinDisallowedList
            .check(&include_node("reset()"), &scss_ctx_with_options(&opts));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("reset"));
    }
}
