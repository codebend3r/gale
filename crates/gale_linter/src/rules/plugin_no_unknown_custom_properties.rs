use std::collections::HashSet;

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};
use regex::Regex;

use crate::rule::{Rule, RuleContext};

/// Report usage of CSS custom properties (`var(--foo)`) that are not defined
/// in the current file or a configurable allowlist.
///
/// This is a `check_root` rule that needs full file context to collect
/// definitions before checking usages.
///
/// Options (secondary): an object with an `allowedProperties` key containing
/// an array of allowed property patterns (exact strings or `/regex/`).
///
/// Example config:
/// ```json
/// ["plugin/no-unknown-custom-properties", [true, {
///   "allowedProperties": ["--theme-color", "/^--global-/"]
/// }]]
/// ```
pub struct PluginNoUnknownCustomProperties;

/// A compiled pattern — either a literal string match or a regex.
enum AllowPattern {
    Exact(String),
    Regex(Regex),
}

impl AllowPattern {
    fn from_str(s: &str) -> Self {
        if let Some(inner) = s.strip_prefix('/').and_then(|s| {
            if let Some(pos) = s.rfind('/') {
                Some((&s[..pos], &s[pos + 1..]))
            } else {
                None
            }
        }) {
            let (pattern, flags) = inner;
            let regex_str = if flags.contains('i') {
                format!("(?i){}", pattern)
            } else {
                pattern.to_string()
            };
            match Regex::new(&regex_str) {
                Ok(re) => AllowPattern::Regex(re),
                Err(_) => AllowPattern::Exact(s.to_string()),
            }
        } else {
            AllowPattern::Exact(s.to_string())
        }
    }

    fn matches(&self, name: &str) -> bool {
        match self {
            AllowPattern::Exact(s) => name == s,
            AllowPattern::Regex(re) => re.is_match(name),
        }
    }
}

fn parse_allowed_properties(context: &RuleContext) -> Vec<AllowPattern> {
    let secondary = match context.secondary_options() {
        Some(opts) => opts,
        None => return vec![],
    };

    let allowed = match secondary.get("allowedProperties") {
        Some(serde_json::Value::Array(arr)) => arr,
        _ => return vec![],
    };

    allowed
        .iter()
        .filter_map(|v| v.as_str().map(AllowPattern::from_str))
        .collect()
}

/// Collect all custom property definitions (`--name: value`) from nodes.
fn collect_definitions(nodes: &[CssNode], defs: &mut HashSet<String>) {
    for node in nodes {
        match node {
            CssNode::Declaration(decl) => {
                if decl.property.starts_with("--") {
                    defs.insert(decl.property.clone());
                }
            }
            CssNode::Style(rule) => {
                for decl in &rule.declarations {
                    if decl.property.starts_with("--") {
                        defs.insert(decl.property.clone());
                    }
                }
                // Recurse into nested children
                let child_nodes: Vec<CssNode> = rule
                    .children
                    .iter()
                    .map(|c| CssNode::Style(c.clone()))
                    .collect();
                collect_definitions(&child_nodes, defs);
                collect_definitions(&rule.nested_at_rules, defs);
            }
            CssNode::AtRule(at_rule) => {
                collect_definitions(&at_rule.children, defs);
            }
            CssNode::Comment(_) => {}
        }
    }
}

/// A var() usage found in the source.
struct VarUsage {
    name: String,
    /// Byte offset of the declaration containing this var() reference.
    decl_offset: usize,
    /// Byte length of the declaration containing this var() reference.
    decl_length: usize,
}

/// Find all `var(--name)` usages in declarations within nodes.
fn collect_usages(nodes: &[CssNode], usages: &mut Vec<VarUsage>) {
    for node in nodes {
        match node {
            CssNode::Declaration(decl) => {
                collect_var_refs_from_value(
                    &decl.value,
                    decl.span.offset,
                    decl.span.length,
                    usages,
                );
            }
            CssNode::Style(rule) => {
                for decl in &rule.declarations {
                    collect_var_refs_from_value(
                        &decl.value,
                        decl.span.offset,
                        decl.span.length,
                        usages,
                    );
                }
                let child_nodes: Vec<CssNode> = rule
                    .children
                    .iter()
                    .map(|c| CssNode::Style(c.clone()))
                    .collect();
                collect_usages(&child_nodes, usages);
                collect_usages(&rule.nested_at_rules, usages);
            }
            CssNode::AtRule(at_rule) => {
                collect_usages(&at_rule.children, usages);
            }
            CssNode::Comment(_) => {}
        }
    }
}

/// Extract var(--name) references from a value string.
fn collect_var_refs_from_value(
    value: &str,
    decl_offset: usize,
    decl_length: usize,
    usages: &mut Vec<VarUsage>,
) {
    let bytes = value.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if i + 4 <= len
            && (bytes[i] == b'v' || bytes[i] == b'V')
            && (bytes[i + 1] == b'a' || bytes[i + 1] == b'A')
            && (bytes[i + 2] == b'r' || bytes[i + 2] == b'R')
            && bytes[i + 3] == b'('
        {
            // Make sure this is not part of another identifier
            if i > 0
                && (bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'-'
                    || bytes[i - 1] == b'_')
            {
                i += 1;
                continue;
            }

            i += 4; // skip "var("

            // Skip whitespace
            while i < len && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
                i += 1;
            }

            if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
                let name_start = i;
                while i < len
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
                {
                    i += 1;
                }
                let prop_name = &value[name_start..i];
                usages.push(VarUsage {
                    name: prop_name.to_string(),
                    decl_offset,
                    decl_length,
                });
                continue;
            }
        }
        i += 1;
    }
}

impl Rule for PluginNoUnknownCustomProperties {
    fn name(&self) -> &'static str {
        "plugin/no-unknown-custom-properties"
    }

    fn description(&self) -> &'static str {
        "Disallow usage of unknown custom properties"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
        let allowed = parse_allowed_properties(context);

        // Collect all definitions in the file
        let mut defined: HashSet<String> = HashSet::new();
        collect_definitions(nodes, &mut defined);

        // Collect all var() usages
        let mut usages: Vec<VarUsage> = Vec::new();
        collect_usages(nodes, &mut usages);

        let mut diagnostics = Vec::new();
        for usage in &usages {
            // Skip if defined in the file
            if defined.contains(&usage.name) {
                continue;
            }
            // Skip if in allowlist
            if allowed.iter().any(|p| p.matches(&usage.name)) {
                continue;
            }
            diagnostics.push(
                Diagnostic::new(
                    self.name(),
                    format!("Unexpected unknown custom property \"{}\"", usage.name),
                )
                .severity(self.default_severity())
                .span(Span::new(usage.decl_offset, usage.decl_length)),
            );
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};
    use serde_json::json;

    fn ctx_with_options(opts: &serde_json::Value) -> RuleContext<'_> {
        RuleContext {
            file_path: "test.css",
            source: "",
            syntax: Syntax::Css,
            options: Some(opts),
        }
    }

    fn ctx() -> RuleContext<'static> {
        RuleContext {
            file_path: "test.css",
            source: "",
            syntax: Syntax::Css,
            options: None,
        }
    }

    fn make_nodes(decls: Vec<(&str, &str)>) -> Vec<CssNode> {
        vec![CssNode::Style(StyleRule {
            selector: ":root".to_string(),
            declarations: decls
                .into_iter()
                .map(|(p, v)| Declaration {
                    property: p.to_string(),
                    value: v.to_string(),
                    span: ParserSpan::new(0, 10),
                    important: false,
                })
                .collect(),
            span: ParserSpan::new(0, 0),
            children: vec![],
            nested_at_rules: vec![],
        })]
    }

    #[test]
    fn no_diagnostic_when_defined() {
        let rule = PluginNoUnknownCustomProperties;
        let ctx = ctx();
        let nodes = make_nodes(vec![("--my-color", "red"), ("color", "var(--my-color)")]);
        let diags = rule.check_root(&nodes, &ctx);
        assert!(diags.is_empty());
    }

    #[test]
    fn reports_unknown_custom_property() {
        let rule = PluginNoUnknownCustomProperties;
        let ctx = ctx();
        let nodes = make_nodes(vec![("color", "var(--unknown)")]);
        let diags = rule.check_root(&nodes, &ctx);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("--unknown"));
    }

    #[test]
    fn allowed_property_skipped() {
        let rule = PluginNoUnknownCustomProperties;
        let opts = json!([true, {
            "allowedProperties": ["--theme-color"]
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes = make_nodes(vec![("color", "var(--theme-color)")]);
        let diags = rule.check_root(&nodes, &ctx);
        assert!(diags.is_empty());
    }

    #[test]
    fn allowed_regex_pattern() {
        let rule = PluginNoUnknownCustomProperties;
        let opts = json!([true, {
            "allowedProperties": ["/^--global-/"]
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes = make_nodes(vec![("color", "var(--global-text)")]);
        let diags = rule.check_root(&nodes, &ctx);
        assert!(diags.is_empty());
    }

    #[test]
    fn regex_does_not_match_reports() {
        let rule = PluginNoUnknownCustomProperties;
        let opts = json!([true, {
            "allowedProperties": ["/^--global-/"]
        }]);
        let ctx = ctx_with_options(&opts);
        let nodes = make_nodes(vec![("color", "var(--local-text)")]);
        let diags = rule.check_root(&nodes, &ctx);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn multiple_vars_in_one_value() {
        let rule = PluginNoUnknownCustomProperties;
        let ctx = ctx();
        let nodes = make_nodes(vec![
            ("--known", "red"),
            ("color", "var(--known) var(--unknown)"),
        ]);
        let diags = rule.check_root(&nodes, &ctx);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("--unknown"));
    }

    #[test]
    fn nested_var_checked() {
        let rule = PluginNoUnknownCustomProperties;
        let ctx = ctx();
        let nodes = make_nodes(vec![("color", "var(--a, var(--b))")]);
        let diags = rule.check_root(&nodes, &ctx);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn definitions_in_nested_rules() {
        let rule = PluginNoUnknownCustomProperties;
        let ctx = ctx();
        let nodes = vec![CssNode::Style(StyleRule {
            selector: ":root".to_string(),
            declarations: vec![Declaration {
                property: "--my-var".to_string(),
                value: "blue".to_string(),
                span: ParserSpan::new(0, 10),
                important: false,
            }],
            span: ParserSpan::new(0, 0),
            children: vec![StyleRule {
                selector: ".child".to_string(),
                declarations: vec![Declaration {
                    property: "color".to_string(),
                    value: "var(--my-var)".to_string(),
                    span: ParserSpan::new(20, 15),
                    important: false,
                }],
                span: ParserSpan::new(20, 15),
                children: vec![],
                nested_at_rules: vec![],
            }],
            nested_at_rules: vec![],
        })];
        let diags = rule.check_root(&nodes, &ctx);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_options_works() {
        let rule = PluginNoUnknownCustomProperties;
        let ctx = ctx();
        let nodes = make_nodes(vec![("color", "var(--undefined)")]);
        let diags = rule.check_root(&nodes, &ctx);
        assert_eq!(diags.len(), 1);
    }
}
