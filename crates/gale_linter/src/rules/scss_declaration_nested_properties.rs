use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Require or disallow properties nested in SCSS namespace declarations.
///
/// SCSS allows nesting properties by namespace:
///
/// ```scss
/// // Nested properties
/// font: {
///   weight: bold;
///   size: 1em;
/// }
/// ```
///
/// Default: `"never"` — disallow nested properties.
///
/// Equivalent to `scss/declaration-nested-properties`.
pub struct ScssDeclarationNestedProperties;

impl Rule for ScssDeclarationNestedProperties {
    fn name(&self) -> &'static str {
        "scss/declaration-nested-properties"
    }

    fn description(&self) -> &'static str {
        "Require or disallow nested properties in SCSS"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
        if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
            return vec![];
        }

        let option = ctx.primary_option_str().unwrap_or("never");

        if option != "never" {
            return vec![];
        }

        let source = ctx.source;
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut diagnostics = Vec::new();

        // Scan for patterns like `property-namespace: {` which indicates nested
        // properties. This is a simplified detection that looks for `: {` patterns
        // where the colon is preceded by a CSS property name.
        let mut i = 0;
        while i < len {
            if bytes[i] == b':' {
                // Check what's before the colon (should be a property-like identifier)
                let mut before = i;
                before = before.saturating_sub(1);
                while before > 0 && (bytes[before] == b' ' || bytes[before] == b'\t') {
                    before -= 1;
                }
                let is_property_like =
                    before < i && (bytes[before].is_ascii_alphabetic() || bytes[before] == b'-');

                // Check what's after the colon (skip whitespace, look for `{`)
                let mut after = i + 1;
                while after < len && (bytes[after] == b' ' || bytes[after] == b'\t') {
                    after += 1;
                }

                if is_property_like && after < len && bytes[after] == b'{' {
                    // This looks like a nested property declaration
                    // Find the start of the property name
                    let mut prop_start = before;
                    while prop_start > 0
                        && (bytes[prop_start - 1].is_ascii_alphanumeric()
                            || bytes[prop_start - 1] == b'-')
                    {
                        prop_start -= 1;
                    }

                    // Make sure we're not inside a selector (check if preceded by
                    // whitespace/newline/semicolon/brace, not another property context)
                    if prop_start == 0
                        || matches!(
                            bytes[prop_start - 1],
                            b'\n' | b'\r' | b';' | b'{' | b' ' | b'\t'
                        )
                    {
                        diagnostics.push(
                            Diagnostic::new(
                                self.name(),
                                "Unexpected nested property declaration".to_string(),
                            )
                            .severity(self.default_severity())
                            .span(Span::new(prop_start, after - prop_start + 1)),
                        );
                    }
                }
            }
            i += 1;
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::Syntax;

    fn scss_ctx_with_source(source: &str) -> RuleContext<'_> {
        RuleContext {
            file_path: "t.scss",
            source,
            syntax: Syntax::Scss,
            options: None,
        }
    }

    #[test]
    fn reports_nested_properties() {
        let ctx = scss_ctx_with_source(".foo {\n  font: {\n    weight: bold;\n  }\n}");
        let d = ScssDeclarationNestedProperties.check_root(&[], &ctx);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_regular_declarations() {
        let ctx = scss_ctx_with_source(".foo {\n  font-weight: bold;\n}");
        let d = ScssDeclarationNestedProperties.check_root(&[], &ctx);
        assert!(d.is_empty());
    }

    #[test]
    fn skips_non_scss() {
        let ctx = RuleContext {
            file_path: "t.css",
            source: ".foo { font: { weight: bold; } }",
            syntax: Syntax::Css,
            options: None,
        };
        assert!(
            ScssDeclarationNestedProperties
                .check_root(&[], &ctx)
                .is_empty()
        );
    }
}
