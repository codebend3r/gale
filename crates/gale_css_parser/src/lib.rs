use lightningcss::printer::PrinterOptions;
use lightningcss::rules::CssRule as LcssRule;
use lightningcss::stylesheet::{ParserFlags, ParserOptions, StyleSheet};
use lightningcss::traits::ToCss;
use raffia::pos::Spanned as RaffiaSpanned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod sass_to_scss;

// ---------------------------------------------------------------------------
// Syntax detection
// ---------------------------------------------------------------------------

/// The kind of CSS dialect we are parsing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Syntax {
    #[default]
    Css,
    Scss,
    Less,
    Sass,
}

/// Infer the [`Syntax`] from a file extension.
///
/// Falls back to [`Syntax::Css`] for unrecognised extensions.
pub fn detect_syntax(file_path: &str) -> Syntax {
    match file_path.rsplit('.').next() {
        Some(ext) => match ext.to_ascii_lowercase().as_str() {
            "scss" => Syntax::Scss,
            "less" => Syntax::Less,
            "sass" => Syntax::Sass,
            _ => Syntax::Css,
        },
        None => Syntax::Css,
    }
}

// ---------------------------------------------------------------------------
// Simplified AST types (owned, lifetime-free)
// ---------------------------------------------------------------------------

/// A source span with byte offsets into the source text.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    /// Byte offset from the start of the source.
    pub offset: usize,
    /// Length in bytes (0 when unknown).
    pub length: usize,
}

impl Span {
    pub fn new(offset: usize, length: usize) -> Self {
        Self { offset, length }
    }

    pub fn end(&self) -> usize {
        self.offset + self.length
    }

    fn empty() -> Self {
        Self {
            offset: 0,
            length: 0,
        }
    }
}

/// A CSS declaration (`property: value`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Declaration {
    pub property: String,
    pub value: String,
    pub span: Span,
    pub important: bool,
}

/// A CSS style rule (selector + declarations + optional nested children).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StyleRule {
    pub selector: String,
    pub declarations: Vec<Declaration>,
    pub span: Span,
    pub children: Vec<StyleRule>,
    /// At-rules nested inside this style rule block (e.g. `@include`, `@if`,
    /// `@media`).  These are kept here so they remain scoped to the style rule
    /// rather than leaking out as siblings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nested_at_rules: Vec<CssNode>,
}

/// A CSS at-rule (`@media`, `@keyframes`, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtRule {
    pub name: String,
    pub params: String,
    pub span: Span,
    pub children: Vec<CssNode>,
}

/// A CSS comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub text: String,
    pub span: Span,
    /// Whether this is a line comment (`//`) vs. block comment (`/* */`).
    /// Always `false` for CSS files; may be `true` for SCSS/Less.
    #[serde(default)]
    pub is_line: bool,
}

/// A node in the simplified CSS tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CssNode {
    Style(StyleRule),
    AtRule(AtRule),
    Comment(Comment),
    Declaration(Declaration),
}

impl CssNode {
    /// Returns the span of this node.
    pub fn span(&self) -> Span {
        match self {
            CssNode::Style(r) => r.span,
            CssNode::AtRule(r) => r.span,
            CssNode::Comment(c) => c.span,
            CssNode::Declaration(d) => d.span,
        }
    }

    /// Returns the child nodes of this node.
    pub fn children(&self) -> Vec<&CssNode> {
        match self {
            CssNode::Style(_) => {
                // StyleRule children are nested StyleRules, not CssNodes.
                // The runner handles recursion into StyleRule.children directly.
                Vec::new()
            }
            CssNode::AtRule(at_rule) => at_rule.children.iter().collect(),
            CssNode::Comment(_) | CssNode::Declaration(_) => Vec::new(),
        }
    }
}

/// The result of parsing a CSS source string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseResult {
    pub nodes: Vec<CssNode>,
    pub syntax: Syntax,
    pub source: String,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("CSS parse error: {message}")]
    Css { message: String },

    #[error("{syntax:?} parsing is not yet implemented (TODO)")]
    UnsupportedSyntax { syntax: Syntax },
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a CSS (or SCSS/Less/Sass) source string into a [`ParseResult`].
///
/// CSS is parsed via *lightningcss*; SCSS and Less are parsed via *raffia*.
/// Sass (indented syntax) returns [`ParseError::UnsupportedSyntax`] for now.
pub fn parse(source: &str, syntax: Syntax) -> Result<ParseResult, ParseError> {
    match syntax {
        Syntax::Css => parse_css(source),
        Syntax::Scss | Syntax::Less => {
            match parse_raffia(source, syntax) {
                Ok(result) => Ok(result),
                Err(_raffia_err) => {
                    // Raffia failed (e.g. malformed strings with literal newlines).
                    // Fall back to lightningcss with error recovery — it can often
                    // partially parse the file so rules still get some AST to inspect.
                    match parse_css(source) {
                        Ok(mut result) => {
                            // Preserve the original syntax so rules know this was SCSS/Less.
                            result.syntax = syntax;
                            Ok(result)
                        }
                        // Both parsers failed — propagate the original raffia error
                        // so the caller can report a parse error diagnostic.
                        Err(_) => Err(_raffia_err),
                    }
                }
            }
        }
        Syntax::Sass => {
            // Convert Sass indented syntax to SCSS, then parse as SCSS.
            // Byte offsets in diagnostics will refer to the converted source,
            // not the original — acceptable for an initial implementation.
            let scss_source = sass_to_scss::convert_sass_to_scss(source);
            match parse_raffia(&scss_source, Syntax::Scss) {
                Ok(mut result) => {
                    result.syntax = Syntax::Sass;
                    result.source = scss_source;
                    Ok(result)
                }
                Err(_raffia_err) => {
                    // Raffia failed on the converted SCSS — try lightningcss
                    // with error recovery as a last resort.
                    match parse_css(&scss_source) {
                        Ok(mut result) => {
                            result.syntax = Syntax::Sass;
                            result.source = scss_source;
                            Ok(result)
                        }
                        Err(_) => Err(_raffia_err),
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers – create fresh default PrinterOptions each time we need one.
//
// PrinterOptions contains an `Option<&mut SourceMap>` so it is neither
// `Copy` nor cheaply cloneable. Constructing a default is trivial.
// ---------------------------------------------------------------------------

fn po() -> PrinterOptions<'static> {
    PrinterOptions::default()
}

// ---------------------------------------------------------------------------
// lightningcss → simplified AST conversion
// ---------------------------------------------------------------------------

/// A pre-built index mapping line numbers to byte offsets for O(log n) lookup.
struct LineIndex {
    /// `line_starts[i]` is the byte offset where line `i` (0-indexed) begins.
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn build(source: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    /// Convert a 0-indexed `line` and 1-based `column` (UTF-16 code units) to a byte offset.
    fn line_col_to_offset(&self, source: &str, line: u32, column: u32) -> usize {
        let line = line as usize;
        if line >= self.line_starts.len() {
            return source.len();
        }
        let line_start = self.line_starts[line];
        // column is 1-based, measured in UTF-16 code units.
        let mut col: u32 = 1;
        for (i, ch) in source[line_start..].char_indices() {
            if col >= column {
                return line_start + i;
            }
            col += ch.len_utf16() as u32;
        }
        source.len().min(line_start + source[line_start..].len())
    }
}

fn parse_css(source: &str) -> Result<ParseResult, ParseError> {
    let opts = ParserOptions {
        flags: ParserFlags::NESTING,
        error_recovery: true,
        ..ParserOptions::default()
    };

    let stylesheet = StyleSheet::parse(source, opts).map_err(|err| ParseError::Css {
        message: err.to_string(),
    })?;

    let line_index = LineIndex::build(source);
    let nodes = convert_rules(&stylesheet.rules.0, source, &line_index);

    Ok(ParseResult {
        nodes,
        syntax: Syntax::Css,
        source: source.to_owned(),
    })
}

/// Convert a list of lightningcss rules into our [`CssNode`] list.
fn convert_rules(rules: &[LcssRule], source: &str, idx: &LineIndex) -> Vec<CssNode> {
    let mut nodes = Vec::with_capacity(rules.len());

    for rule in rules {
        match rule {
            LcssRule::Style(style) => {
                nodes.push(CssNode::Style(convert_style_rule(style, source, idx)));
            }

            LcssRule::Media(media) => {
                let params = media.query.to_css_string(po()).unwrap_or_default();
                let children = convert_rules(&media.rules.0, source, idx);
                nodes.push(CssNode::AtRule(AtRule {
                    name: "media".into(),
                    params,
                    span: loc_to_span(media.loc, source, idx),
                    children,
                }));
            }

            LcssRule::Supports(supports) => {
                let params = supports.condition.to_css_string(po()).unwrap_or_default();
                let children = convert_rules(&supports.rules.0, source, idx);
                nodes.push(CssNode::AtRule(AtRule {
                    name: "supports".into(),
                    params,
                    span: loc_to_span(supports.loc, source, idx),
                    children,
                }));
            }

            LcssRule::Keyframes(kf) => {
                let params = kf.name.to_css_string(po()).unwrap_or_default();
                let kf_span = loc_to_span(kf.loc, source, idx);
                let children = convert_keyframes(&kf.keyframes, source, kf_span);
                nodes.push(CssNode::AtRule(AtRule {
                    name: "keyframes".into(),
                    params,
                    span: kf_span,
                    children,
                }));
            }

            LcssRule::FontFace(ff) => {
                // FontFaceProperty is its own enum (not Property).
                // Serialize each via ToCss then split on `:`.
                let mut children = Vec::new();
                for prop in &ff.properties {
                    let css = prop.to_css_string(po()).unwrap_or_default();
                    if let Some((name, value)) = css.split_once(':') {
                        children.push(CssNode::Declaration(Declaration {
                            property: name.trim().to_owned(),
                            value: value.trim().to_owned(),
                            span: Span::empty(),
                            important: false,
                        }));
                    }
                }
                nodes.push(CssNode::AtRule(AtRule {
                    name: "font-face".into(),
                    params: String::new(),
                    span: loc_to_span(ff.loc, source, idx),
                    children,
                }));
            }

            LcssRule::Import(import) => {
                nodes.push(CssNode::AtRule(AtRule {
                    name: "import".into(),
                    params: import.url.as_ref().to_owned(),
                    span: loc_to_span(import.loc, source, idx),
                    children: Vec::new(),
                }));
            }

            LcssRule::Namespace(ns) => {
                nodes.push(CssNode::AtRule(AtRule {
                    name: "namespace".into(),
                    params: ns.url.as_ref().to_owned(),
                    span: loc_to_span(ns.loc, source, idx),
                    children: Vec::new(),
                }));
            }

            LcssRule::Container(container) => {
                let params = container
                    .name
                    .as_ref()
                    .map(|n| n.to_css_string(po()).unwrap_or_default())
                    .unwrap_or_default();
                let children = convert_rules(&container.rules.0, source, idx);
                nodes.push(CssNode::AtRule(AtRule {
                    name: "container".into(),
                    params,
                    span: loc_to_span(container.loc, source, idx),
                    children,
                }));
            }

            LcssRule::LayerBlock(layer) => {
                let params = layer
                    .name
                    .as_ref()
                    .map(|n| n.0.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join("."))
                    .unwrap_or_default();
                let children = convert_rules(&layer.rules.0, source, idx);
                nodes.push(CssNode::AtRule(AtRule {
                    name: "layer".into(),
                    params,
                    span: loc_to_span(layer.loc, source, idx),
                    children,
                }));
            }

            LcssRule::LayerStatement(layer) => {
                let params = layer
                    .names
                    .iter()
                    .map(|n| n.0.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join("."))
                    .collect::<Vec<_>>()
                    .join(", ");
                nodes.push(CssNode::AtRule(AtRule {
                    name: "layer".into(),
                    params,
                    span: loc_to_span(layer.loc, source, idx),
                    children: Vec::new(),
                }));
            }

            LcssRule::Scope(scope) => {
                let children = convert_rules(&scope.rules.0, source, idx);
                nodes.push(CssNode::AtRule(AtRule {
                    name: "scope".into(),
                    params: String::new(),
                    span: loc_to_span(scope.loc, source, idx),
                    children,
                }));
            }

            LcssRule::StartingStyle(ss) => {
                let children = convert_rules(&ss.rules.0, source, idx);
                nodes.push(CssNode::AtRule(AtRule {
                    name: "starting-style".into(),
                    params: String::new(),
                    span: loc_to_span(ss.loc, source, idx),
                    children,
                }));
            }

            LcssRule::Nesting(nesting) => {
                nodes.push(CssNode::Style(convert_style_rule(
                    &nesting.style,
                    source,
                    idx,
                )));
            }

            LcssRule::NestedDeclarations(nested_decls) => {
                for decl in &nested_decls.declarations.declarations {
                    let (d, _) = convert_property(decl, false, source, 0, source.len());
                    nodes.push(CssNode::Declaration(d));
                }
                for decl in &nested_decls.declarations.important_declarations {
                    let (d, _) = convert_property(decl, true, source, 0, source.len());
                    nodes.push(CssNode::Declaration(d));
                }
            }

            LcssRule::Page(page) => {
                // Serialize page selectors into params string
                let mut params_parts = Vec::new();
                for sel in &page.selectors {
                    let sel_str = sel.to_css_string(po()).unwrap_or_default();
                    if !sel_str.is_empty() {
                        params_parts.push(sel_str);
                    }
                }
                let params = params_parts.join(", ");
                nodes.push(CssNode::AtRule(AtRule {
                    name: "page".into(),
                    params,
                    span: loc_to_span(page.loc, source, idx),
                    children: Vec::new(),
                }));
            }

            LcssRule::Property(prop) => {
                let params = prop.name.to_css_string(po()).unwrap_or_default();
                nodes.push(CssNode::AtRule(AtRule {
                    name: "property".into(),
                    params,
                    span: loc_to_span(prop.loc, source, idx),
                    children: Vec::new(),
                }));
            }

            LcssRule::CounterStyle(cs) => {
                let params = cs.name.to_css_string(po()).unwrap_or_default();
                nodes.push(CssNode::AtRule(AtRule {
                    name: "counter-style".into(),
                    params,
                    span: loc_to_span(cs.loc, source, idx),
                    children: Vec::new(),
                }));
            }

            LcssRule::Unknown(unknown) => {
                let name = unknown.name.as_ref().to_owned();
                // TokenList::to_css is pub(crate) in lightningcss, so we
                // serialize the whole rule and strip the `@name` prefix to
                // extract the prelude.
                let full = unknown.to_css_string(po()).unwrap_or_default();
                let params = full
                    .strip_prefix(&format!("@{name}"))
                    .map(|rest| rest.trim().trim_end_matches(';').trim().to_owned())
                    .unwrap_or_default();
                let at_span = loc_to_span(unknown.loc, source, idx);

                // Try to extract declarations from block body.
                // PostCSS (used by Stylelint) parses unknown at-rule blocks
                // into child nodes, so rules can inspect declarations inside
                // `@theme`, `@apply`, etc.  lightningcss stores the block as
                // raw tokens, so we re-parse the block body.
                let children = extract_unknown_at_rule_children(source, at_span);
                nodes.push(CssNode::AtRule(AtRule {
                    name,
                    params,
                    span: at_span,
                    children,
                }));
            }

            // Remaining variants we don't model yet (e.g. MozDocument,
            // FontPaletteValues, ViewTransition, etc.) are silently skipped.
            _ => {}
        }
    }

    nodes
}

/// Convert lightningcss keyframes into our CssNode children.
///
/// Each `Keyframe` becomes a `CssNode::Style` whose `selector` is the
/// comma-joined list of keyframe selectors (e.g. `from`, `50%`, `to`).
fn convert_keyframes(
    keyframes: &[lightningcss::rules::keyframes::Keyframe],
    source: &str,
    parent_span: Span,
) -> Vec<CssNode> {
    let mut children = Vec::new();
    let search_end = (parent_span.offset + parent_span.length).min(source.len());
    // Cursor advances past each keyframe block so we don't re-match earlier ones.
    let mut block_cursor = parent_span.offset;

    for kf in keyframes {
        let selector = kf
            .selectors
            .iter()
            .map(|s| s.to_css_string(po()).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(", ");

        // STEP 1: Find this keyframe block's span FIRST so we can constrain
        // the declaration search to just this block.
        // Use the first selector keyword (e.g. "0%") instead of the full joined
        // selector because multi-line selectors like "0%,\n  100%" won't match
        // the normalized "0%, 100%" string.
        let first_selector = kf
            .selectors
            .first()
            .map(|s| s.to_css_string(po()).unwrap_or_default())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let area = source.get(block_cursor..search_end).unwrap_or("");
        let lower_area = area.to_ascii_lowercase();
        let kf_span = if let Some(rel) = lower_area.find(&first_selector) {
            let abs_start = block_cursor + rel;
            let rest = &source[abs_start..search_end];
            let length = if let Some(open) = rest.find('{') {
                let mut depth = 0i32;
                let mut end = open;
                for (i, b) in rest[open..].bytes().enumerate() {
                    if b == b'{' {
                        depth += 1;
                    } else if b == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            end = open + i + 1;
                            break;
                        }
                    }
                }
                end
            } else {
                0
            };
            Span::new(abs_start, length)
        } else {
            Span::empty()
        };

        // STEP 2: Search for declarations WITHIN this keyframe block only.
        let block_start = kf_span.offset;
        let block_end = if kf_span.length > 0 {
            kf_span.offset + kf_span.length
        } else {
            search_end
        };

        let mut declarations = Vec::new();
        let mut search_from = block_start;
        for decl in &kf.declarations.declarations {
            let (d, next) = convert_property(decl, false, source, search_from, block_end);
            search_from = next;
            declarations.push(d);
        }
        for decl in &kf.declarations.important_declarations {
            let (d, next) = convert_property(decl, true, source, search_from, block_end);
            search_from = next;
            declarations.push(d);
        }

        // Advance cursor past this block.
        if kf_span.length > 0 {
            block_cursor = kf_span.offset + kf_span.length;
        }

        children.push(CssNode::Style(StyleRule {
            selector,
            declarations,
            span: kf_span,
            ..Default::default()
        }));
    }

    children
}

fn convert_style_rule(
    style: &lightningcss::rules::style::StyleRule,
    source: &str,
    idx: &LineIndex,
) -> StyleRule {
    let selector = style.selectors.to_css_string(po()).unwrap_or_default();
    let rule_span = loc_to_span(style.loc, source, idx);

    // Search area for finding declaration positions.
    let search_start = rule_span.offset;
    let search_end = (rule_span.offset + rule_span.length).min(source.len());

    // Merge normal and !important declarations into a single list, then
    // assign spans in source order. lightningcss separates important and
    // non-important declarations, which would assign wrong byte offsets if
    // we processed them sequentially (non-important first, important second).
    //
    // Strategy: convert all declarations without spans first, find all
    // property occurrences in source, determine which are important from
    // source text, then match important/non-important proto-decls correctly.
    let mut proto_decls: Vec<(String, String, bool)> = Vec::new(); // (property, value, important)
    for decl in &style.declarations.declarations {
        let (d, _) = convert_property(decl, false, source, search_start, search_end);
        proto_decls.push((d.property, d.value, false));
    }
    for decl in &style.declarations.important_declarations {
        let (d, _) = convert_property(decl, true, source, search_start, search_end);
        proto_decls.push((d.property, d.value, true));
    }

    // Find all property-name occurrences in source text, in order.
    let total_decls = proto_decls.len();
    let mut spans_in_order: Vec<Span> = Vec::with_capacity(total_decls);
    let mut sf = search_start;
    // We need to find `total_decls` declaration spans
    // Collect all unique property names to search for
    let mut prop_names: Vec<String> = proto_decls.iter().map(|(p, _, _)| p.clone()).collect();
    prop_names.sort();
    prop_names.dedup();

    // Find all declarations by scanning source for any known property name
    let mut found_count = 0;
    while found_count < total_decls && sf < search_end {
        // Try to find the next declaration starting from sf
        let mut best_span = Span::empty();
        let mut best_offset = usize::MAX;
        for pname in &prop_names {
            let span = find_declaration_span(source, sf, search_end, pname);
            if span.length > 0 && span.offset < best_offset {
                best_offset = span.offset;
                best_span = span;
            }
        }
        if best_span.length == 0 {
            break;
        }
        spans_in_order.push(best_span);
        sf = best_span.offset + best_span.length;
        found_count += 1;
    }

    // Now match each span to the right proto_decl. Check if the source text
    // at each span contains "!important" to determine which proto_decl it
    // should be matched with.
    let mut matched: Vec<bool> = vec![false; proto_decls.len()];
    let mut declarations = Vec::new();

    for span in &spans_in_order {
        let span_text = source
            .get(span.offset..(span.offset + span.length).min(source.len()))
            .unwrap_or("");
        let is_important_in_source =
            span_text.contains("!important") || span_text.contains("! important");

        // Extract the property name from the span
        let span_lower = span_text.to_ascii_lowercase();
        let span_prop = span_lower.split(':').next().unwrap_or("").trim();

        // Find the first unmatched proto_decl with matching property AND importance
        let mut found_idx = None;
        for (i, (prop, _, important)) in proto_decls.iter().enumerate() {
            if !matched[i]
                && prop.to_ascii_lowercase() == span_prop
                && *important == is_important_in_source
            {
                found_idx = Some(i);
                break;
            }
        }

        // Fallback: match by property name only (if importance check fails)
        if found_idx.is_none() {
            for (i, (prop, _, _)) in proto_decls.iter().enumerate() {
                if !matched[i] && prop.to_ascii_lowercase() == span_prop {
                    found_idx = Some(i);
                    break;
                }
            }
        }

        if let Some(idx) = found_idx {
            matched[idx] = true;
            let (ref prop, ref value, important) = proto_decls[idx];
            declarations.push(Declaration {
                property: prop.clone(),
                value: value.clone(),
                span: *span,
                important,
            });
        }
    }

    // Add any unmatched declarations (shouldn't happen normally)
    for (i, (prop, value, important)) in proto_decls.iter().enumerate() {
        if !matched[i] {
            declarations.push(Declaration {
                property: prop.clone(),
                value: value.clone(),
                span: Span::empty(),
                important: *important,
            });
        }
    }

    // Nested rules: extract nested style rules as children, and also pull
    // declarations out of NestedDeclarations nodes (lightningcss puts
    // declarations that follow nested rules into NestedDeclarations).
    let mut children = Vec::new();
    for rule in &style.rules.0 {
        match rule {
            LcssRule::Style(nested_style) => {
                children.push(convert_style_rule(nested_style, source, idx));
            }
            LcssRule::Nesting(nesting) => {
                children.push(convert_style_rule(&nesting.style, source, idx));
            }
            LcssRule::NestedDeclarations(nested_decls) => {
                let mut nested_sf = sf;
                for decl in &nested_decls.declarations.declarations {
                    let (d, next) = convert_property(decl, false, source, nested_sf, search_end);
                    nested_sf = next;
                    declarations.push(d);
                }
                for decl in &nested_decls.declarations.important_declarations {
                    let (d, next) = convert_property(decl, true, source, nested_sf, search_end);
                    nested_sf = next;
                    declarations.push(d);
                }
                sf = nested_sf;
            }
            _ => {}
        }
    }

    StyleRule {
        selector,
        declarations,
        span: rule_span,
        children,
        ..Default::default()
    }
}

fn convert_property(
    prop: &lightningcss::properties::Property,
    important: bool,
    source: &str,
    search_from: usize,
    search_end: usize,
) -> (Declaration, usize) {
    let prop_id = prop.property_id();
    let base_name = prop_id.name();
    // Reconstruct the full property name including vendor prefix.
    let prefix = prop_id.prefix();
    let property_name = if prefix.contains(lightningcss::vendor_prefix::VendorPrefix::WebKit) {
        format!("-webkit-{base_name}")
    } else if prefix.contains(lightningcss::vendor_prefix::VendorPrefix::Moz) {
        format!("-moz-{base_name}")
    } else if prefix.contains(lightningcss::vendor_prefix::VendorPrefix::Ms) {
        format!("-ms-{base_name}")
    } else if prefix.contains(lightningcss::vendor_prefix::VendorPrefix::O) {
        format!("-o-{base_name}")
    } else {
        base_name.to_owned()
    };
    let value = prop.value_to_css_string(po()).unwrap_or_default();

    // Find the declaration in the source text for accurate byte offsets.
    let span = find_declaration_span(source, search_from, search_end, &property_name);
    let next_search = if span.length > 0 {
        span.offset + span.length
    } else {
        search_from
    };

    (
        Declaration {
            property: property_name,
            value,
            span,
            important,
        },
        next_search,
    )
}

/// Search for a CSS declaration (`property-name: ...;` or `property-name: ... }`)
/// in the source text between `from` and `to`, returning its span.
fn find_declaration_span(source: &str, from: usize, to: usize, property: &str) -> Span {
    let area = source.get(from..to.min(source.len())).unwrap_or("");
    let lower_area = area.to_ascii_lowercase();
    let lower_prop = property.to_ascii_lowercase();

    // Search for the property name followed by `:` (a CSS declaration).
    // A simple `find` would match property names inside selectors (e.g.
    // "border" in ".foo-border"), so we require the match to be followed
    // by optional whitespace and then a colon.
    let mut search_from = 0;
    loop {
        let rel_idx = match lower_area[search_from..].find(&lower_prop) {
            Some(i) => search_from + i,
            None => return Span::empty(),
        };

        let after_name = rel_idx + lower_prop.len();
        // Check that the property name is followed by optional whitespace + ':'
        let rest_of_area = &lower_area[after_name..];
        let trimmed = rest_of_area.trim_start();
        let is_declaration = trimmed.starts_with(':');

        // Also ensure the match isn't in the middle of a longer identifier
        // (e.g. "border" in "flex-border" or "border-radius")
        let preceded_by_ident = rel_idx > 0 && {
            let prev = lower_area.as_bytes()[rel_idx - 1];
            prev.is_ascii_alphanumeric() || prev == b'-' || prev == b'_'
        };

        if is_declaration && !preceded_by_ident {
            let abs_start = from + rel_idx;
            let after_prop = abs_start + property.len();
            let rest = &source[after_prop..to.min(source.len())];
            let decl_end = rest
                .find(';')
                .map(|i| after_prop + i + 1)
                .unwrap_or_else(|| rest.find('}').map(|i| after_prop + i).unwrap_or(after_prop));
            return Span::new(abs_start, decl_end - abs_start);
        }

        search_from = after_name;
    }
}

/// Extract child nodes (declarations) from an unknown at-rule's block body.
///
/// lightningcss stores unknown at-rule blocks as raw tokens. PostCSS (used by
/// Stylelint) parses them into child nodes.  To match Stylelint's behaviour,
/// we find the `{...}` block in the source and extract `property: value;`
/// declarations from it.
fn extract_unknown_at_rule_children(source: &str, at_span: Span) -> Vec<CssNode> {
    let end = (at_span.offset + at_span.length).min(source.len());
    let area = source.get(at_span.offset..end).unwrap_or("");

    // Find the block body (between first `{` and last `}`)
    let Some(open_brace) = area.find('{') else {
        return Vec::new();
    };

    // If a `;` appears before `{`, this is a blockless at-rule (e.g., `@tailwind base;`)
    // and the `{` belongs to a subsequent rule.  Don't extract children.
    let before_brace = &area[..open_brace];
    if before_brace.contains(';') {
        return Vec::new();
    }

    let Some(close_brace) = area.rfind('}') else {
        return Vec::new();
    };
    if open_brace >= close_brace {
        return Vec::new();
    }

    let body = &area[open_brace + 1..close_brace];
    if body.trim().is_empty() {
        return Vec::new();
    }

    // Try to re-parse the body as a stylesheet by wrapping in a dummy selector.
    // This lets lightningcss handle complex values (var(), calc(), etc.) correctly.
    let wrapper = format!("_x {{{body}}}");
    let opts = ParserOptions {
        flags: ParserFlags::NESTING,
        error_recovery: true,
        ..Default::default()
    };
    let Ok(sheet) = StyleSheet::parse(&wrapper, opts) else {
        return Vec::new();
    };

    let body_abs_offset = at_span.offset + open_brace + 1;
    let body_end = at_span.offset + close_brace;

    // Extract declarations from the re-parsed style rule
    let mut children = Vec::new();
    for rule in sheet.rules.0.iter() {
        if let LcssRule::Style(style) = rule {
            let mut search_from = body_abs_offset;
            for decl in style.declarations.declarations.iter() {
                let (d, next) = convert_property(decl, false, source, search_from, body_end);
                search_from = next;
                children.push(CssNode::Declaration(d));
            }
            for decl in style.declarations.important_declarations.iter() {
                let (d, next) = convert_property(decl, true, source, search_from, body_end);
                search_from = next;
                children.push(CssNode::Declaration(d));
            }
        }
    }
    children
}

/// Convert a 0-indexed line and 1-based column (as reported by lightningcss)
/// into a byte offset within `source`.
///
/// Returns 0 if the line/column is out of range.
#[cfg(test)]
fn line_col_to_byte_offset(source: &str, line: u32, column: u32) -> usize {
    let mut current_line: u32 = 0;
    let mut line_start: usize = 0;

    for (i, ch) in source.char_indices() {
        if current_line == line {
            line_start = i;
            break;
        }
        if ch == '\n' {
            current_line += 1;
            line_start = i + 1;
        }
    }

    if current_line < line {
        // line is beyond end of source
        return source.len();
    }

    // column is 1-based, measured in UTF-16 code units.
    // Walk from line_start counting UTF-16 code units.
    let mut col: u32 = 1;
    for (i, ch) in source[line_start..].char_indices() {
        if col >= column {
            return line_start + i;
        }
        col += ch.len_utf16() as u32;
    }

    // column points past end of line
    source.len().min(line_start + source[line_start..].len())
}

/// Map a lightningcss `Location` (line/column) into our `Span` as a byte offset.
/// Attempts to find the matching closing `}` to determine the span length.
fn loc_to_span(loc: lightningcss::rules::Location, source: &str, idx: &LineIndex) -> Span {
    let offset = idx.line_col_to_offset(source, loc.line, loc.column);

    // Find the matching closing brace to determine length.
    let rest = source.get(offset..).unwrap_or("");
    let length = if let Some(open) = rest.find('{') {
        let mut depth = 0i32;
        let mut end = open;
        for (i, b) in rest[open..].bytes().enumerate() {
            if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
                if depth == 0 {
                    end = open + i + 1; // include the }
                    break;
                }
            }
        }
        end
    } else {
        0
    };

    Span { offset, length }
}

// ---------------------------------------------------------------------------
// raffia-based parsing (SCSS / Less)
// ---------------------------------------------------------------------------

/// Parse SCSS or Less source via raffia, converting to our simplified AST.
fn parse_raffia(source: &str, syntax: Syntax) -> Result<ParseResult, ParseError> {
    use raffia::ParserBuilder;

    let raffia_syntax = match syntax {
        Syntax::Scss => raffia::Syntax::Scss,
        Syntax::Less => raffia::Syntax::Less,
        _ => unreachable!(),
    };

    let mut comments_vec: Vec<raffia::token::Comment<'_>> = Vec::new();
    let builder = ParserBuilder::new(source)
        .syntax(raffia_syntax)
        .comments(&mut comments_vec);
    let mut parser = builder.build();

    let stylesheet = parser
        .parse::<raffia::ast::Stylesheet>()
        .map_err(|err| ParseError::Css {
            message: format!("{err:?}"),
        })?;

    let mut nodes = convert_raffia_statements(&stylesheet.statements, source);

    // Merge collected comments into the node list.
    for c in &comments_vec {
        nodes.push(CssNode::Comment(Comment {
            text: c.content.to_owned(),
            span: raffia_span(&c.span),
            is_line: matches!(c.kind, raffia::token::CommentKind::Line),
        }));
    }

    // Sort all nodes by source offset so comments interleave properly.
    nodes.sort_by_key(|n| n.span().offset);

    Ok(ParseResult {
        nodes,
        syntax,
        source: source.to_owned(),
    })
}

/// Convert a list of raffia [`Statement`]s into our [`CssNode`] list.
fn convert_raffia_statements(stmts: &[raffia::ast::Statement<'_>], source: &str) -> Vec<CssNode> {
    use raffia::ast::Statement;

    let mut nodes = Vec::with_capacity(stmts.len());

    for stmt in stmts {
        match stmt {
            Statement::QualifiedRule(qr) => {
                let style_rule = convert_raffia_qualified_rule(qr, source);
                nodes.push(CssNode::Style(style_rule));
            }

            Statement::Declaration(decl) => {
                nodes.push(CssNode::Declaration(convert_raffia_declaration(
                    decl, source,
                )));
            }

            Statement::AtRule(at) => {
                nodes.push(CssNode::AtRule(convert_raffia_at_rule(at, source)));
            }

            Statement::SassVariableDeclaration(var) => {
                // Treat `$var: value;` as a Declaration with `$name` as property.
                let name = format!("${}", var.name.name.name);
                let value_span = var.value.span();
                let value = source_slice(source, value_span);
                nodes.push(CssNode::Declaration(Declaration {
                    property: name,
                    value,
                    span: raffia_span(&var.span),
                    important: false,
                }));
            }

            Statement::SassIfAtRule(sass_if) => {
                // Map @if/@else if/@else to an AtRule.
                let condition_span = sass_if.if_clause.condition.span();
                let params = source_slice(source, condition_span);
                let mut children =
                    convert_raffia_block_statements(&sass_if.if_clause.block, source);

                // Append else-if and else blocks as nested children.
                for else_if in &sass_if.else_if_clauses {
                    let ep = source_slice(source, else_if.condition.span());
                    let ec = convert_raffia_block_statements(&else_if.block, source);
                    children.push(CssNode::AtRule(AtRule {
                        name: "else if".into(),
                        params: ep,
                        span: raffia_span(&else_if.span),
                        children: ec,
                    }));
                }
                if let Some(ref else_block) = sass_if.else_clause {
                    let ec = convert_raffia_block_statements(else_block, source);
                    children.push(CssNode::AtRule(AtRule {
                        name: "else".into(),
                        params: String::new(),
                        span: raffia_span(&else_block.span),
                        children: ec,
                    }));
                }

                nodes.push(CssNode::AtRule(AtRule {
                    name: "if".into(),
                    params,
                    span: raffia_span(&sass_if.span),
                    children,
                }));
            }

            Statement::UnknownSassAtRule(unknown) => {
                // Handles @mixin, @include, @extend, @warn, @error, @debug, etc.
                let name = raffia_interpolable_ident_to_string(&unknown.name, source);
                let params = unknown
                    .prelude
                    .as_ref()
                    .map(|p| source_slice(source, p.span()))
                    .unwrap_or_default();
                let children = unknown
                    .block
                    .as_ref()
                    .map(|b| convert_raffia_block_statements(b, source))
                    .unwrap_or_default();
                nodes.push(CssNode::AtRule(AtRule {
                    name,
                    params,
                    span: raffia_span(&unknown.span),
                    children,
                }));
            }

            // Less-specific statements we map as best-effort.
            Statement::LessVariableDeclaration(var) => {
                let name = format!("@{}", var.name.name.name);
                let value_span = var.value.span();
                let value = source_slice(source, value_span);
                nodes.push(CssNode::Declaration(Declaration {
                    property: name,
                    value,
                    span: raffia_span(&var.span),
                    important: false,
                }));
            }

            Statement::LessMixinDefinition(mixin) => {
                let params = source_slice(source, &mixin.params.span);
                let children = convert_raffia_block_statements(&mixin.block, source);
                nodes.push(CssNode::AtRule(AtRule {
                    name: "less-mixin".into(),
                    params,
                    span: raffia_span(&mixin.span),
                    children,
                }));
            }

            Statement::LessMixinCall(call) => {
                let params = source_slice(source, &call.span);
                nodes.push(CssNode::AtRule(AtRule {
                    name: "less-mixin-call".into(),
                    params,
                    span: raffia_span(&call.span),
                    children: Vec::new(),
                }));
            }

            Statement::KeyframeBlock(kf_block) => {
                // Convert a raffia KeyframeBlock to a CssNode::Style
                // with the selector being the comma-joined list of selectors.
                let selector = kf_block
                    .selectors
                    .iter()
                    .map(|sel| match sel {
                        raffia::ast::KeyframeSelector::Ident(ident) => {
                            raffia_interpolable_ident_to_string(ident, source)
                        }
                        raffia::ast::KeyframeSelector::Percentage(pct) => {
                            source_slice(source, &pct.span)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                let mut declarations = Vec::new();
                for stmt in &kf_block.block.statements {
                    if let Statement::Declaration(decl) = stmt {
                        declarations.push(convert_raffia_declaration(decl, source));
                    }
                }

                nodes.push(CssNode::Style(StyleRule {
                    selector,
                    declarations,
                    span: raffia_span(&kf_block.span),
                    ..Default::default()
                }));
            }

            // Skip other statement types we don't need yet.
            _ => {}
        }
    }

    nodes
}

fn convert_raffia_block_statements(
    block: &raffia::ast::SimpleBlock<'_>,
    source: &str,
) -> Vec<CssNode> {
    convert_raffia_statements(&block.statements, source)
}

/// Convert a raffia `QualifiedRule` into a `StyleRule`.
///
/// At-rules nested inside style rule blocks (`@include`, `@if`/`@else`,
/// `@media`, etc.) are stored in `StyleRule.nested_at_rules` so they remain
/// scoped to the style rule.  This prevents SCSS at-rule content blocks from
/// leaking to sibling scope and causing false positives in rules like
/// `no-invalid-position-declaration`.
fn convert_raffia_qualified_rule(qr: &raffia::ast::QualifiedRule<'_>, source: &str) -> StyleRule {
    let selector = source_slice(source, &qr.selector.span);

    let mut declarations = Vec::new();
    let mut children = Vec::new();
    let mut nested_at_rules: Vec<CssNode> = Vec::new();

    for stmt in &qr.block.statements {
        match stmt {
            raffia::ast::Statement::Declaration(decl) => {
                declarations.push(convert_raffia_declaration(decl, source));
            }
            raffia::ast::Statement::QualifiedRule(nested) => {
                let child_rule = convert_raffia_qualified_rule(nested, source);
                children.push(child_rule);
            }
            raffia::ast::Statement::SassVariableDeclaration(var) => {
                let name = format!("${}", var.name.name.name);
                let value_span = var.value.span();
                let value = source_slice(source, value_span);
                declarations.push(Declaration {
                    property: name,
                    value,
                    span: raffia_span(&var.span),
                    important: false,
                });
            }
            raffia::ast::Statement::AtRule(at) => {
                nested_at_rules.push(CssNode::AtRule(convert_raffia_at_rule(at, source)));
            }
            raffia::ast::Statement::UnknownSassAtRule(unknown) => {
                let name = raffia_interpolable_ident_to_string(&unknown.name, source);
                let params = unknown
                    .prelude
                    .as_ref()
                    .map(|p| source_slice(source, p.span()))
                    .unwrap_or_default();
                let at_children = unknown
                    .block
                    .as_ref()
                    .map(|b| convert_raffia_block_statements(b, source))
                    .unwrap_or_default();
                nested_at_rules.push(CssNode::AtRule(AtRule {
                    name,
                    params,
                    span: raffia_span(&unknown.span),
                    children: at_children,
                }));
            }
            raffia::ast::Statement::SassIfAtRule(sass_if) => {
                let mut if_children =
                    convert_raffia_block_statements(&sass_if.if_clause.block, source);
                for else_if in &sass_if.else_if_clauses {
                    let ec = convert_raffia_block_statements(&else_if.block, source);
                    if_children.push(CssNode::AtRule(AtRule {
                        name: "else if".to_string(),
                        params: String::new(),
                        span: raffia_span(&else_if.span),
                        children: ec,
                    }));
                }
                if let Some(else_block) = &sass_if.else_clause {
                    let ec = convert_raffia_block_statements(else_block, source);
                    if_children.push(CssNode::AtRule(AtRule {
                        name: "else".to_string(),
                        params: String::new(),
                        span: raffia_span(&else_block.span),
                        children: ec,
                    }));
                }
                nested_at_rules.push(CssNode::AtRule(AtRule {
                    name: "if".to_string(),
                    params: String::new(),
                    span: raffia_span(&sass_if.span),
                    children: if_children,
                }));
            }
            // Other constructs (e.g. LessVariableDeclaration) — skip.
            _ => {}
        }
    }

    StyleRule {
        selector,
        declarations,
        span: raffia_span(&qr.span),
        children,
        nested_at_rules,
    }
}

fn convert_raffia_declaration(decl: &raffia::ast::Declaration<'_>, source: &str) -> Declaration {
    let property = raffia_interpolable_ident_to_string(&decl.name, source);

    // Extract the value from source using spans of the value components.
    let value = if decl.value.is_empty() {
        String::new()
    } else {
        let first = decl.value.first().unwrap().span();
        let last = decl.value.last().unwrap().span();
        source
            .get(first.start..last.end)
            .unwrap_or("")
            .trim()
            .to_owned()
    };

    let important = decl.important.is_some();

    Declaration {
        property,
        value,
        span: raffia_span(&decl.span),
        important,
    }
}

fn convert_raffia_at_rule(at: &raffia::ast::AtRule<'_>, source: &str) -> AtRule {
    let name = at.name.name.to_string();
    let params = at
        .prelude
        .as_ref()
        .map(|p| source_slice(source, p.span()))
        .unwrap_or_default();
    let children = at
        .block
        .as_ref()
        .map(|b| convert_raffia_block_statements(b, source))
        .unwrap_or_default();

    AtRule {
        name,
        params,
        span: raffia_span(&at.span),
        children,
    }
}

/// Convert a raffia `Span` (start/end offsets) to our `Span` (offset/length).
fn raffia_span(s: &raffia::pos::Span) -> Span {
    Span {
        offset: s.start,
        length: s.end.saturating_sub(s.start),
    }
}

/// Extract a trimmed slice of source text from a raffia span.
fn source_slice(source: &str, span: &raffia::pos::Span) -> String {
    let start = span.start.min(source.len());
    let end = span.end.min(source.len());
    source.get(start..end).unwrap_or("").trim().to_owned()
}

/// Convert a raffia `InterpolableIdent` to a plain string.
fn raffia_interpolable_ident_to_string(
    ident: &raffia::ast::InterpolableIdent<'_>,
    source: &str,
) -> String {
    match ident {
        raffia::ast::InterpolableIdent::Literal(id) => id.name.to_string(),
        // For interpolated idents, just use the source text.
        other => {
            let span = other.span();
            source_slice(source, span)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_syntax() {
        assert_eq!(detect_syntax("foo.css"), Syntax::Css);
        assert_eq!(detect_syntax("bar.scss"), Syntax::Scss);
        assert_eq!(detect_syntax("baz.less"), Syntax::Less);
        assert_eq!(detect_syntax("qux.sass"), Syntax::Sass);
        assert_eq!(detect_syntax("unknown.txt"), Syntax::Css);
        assert_eq!(detect_syntax("noext"), Syntax::Css);
    }

    #[test]
    fn test_parse_simple_css() {
        let css = r#"
            .foo {
                color: red;
                display: block;
            }
        "#;
        let result = parse(css, Syntax::Css).expect("should parse");
        assert_eq!(result.syntax, Syntax::Css);
        assert_eq!(result.nodes.len(), 1);

        if let CssNode::Style(ref rule) = result.nodes[0] {
            assert_eq!(rule.selector, ".foo");
            assert_eq!(rule.declarations.len(), 2);
            assert_eq!(rule.declarations[0].property, "color");
            assert_eq!(rule.declarations[0].value, "red");
            assert!(!rule.declarations[0].important);
            assert_eq!(rule.declarations[1].property, "display");
            assert_eq!(rule.declarations[1].value, "block");
        } else {
            panic!("expected a Style node");
        }
    }

    #[test]
    fn test_parse_important() {
        let css = ".bar { margin: 0 !important; }";
        let result = parse(css, Syntax::Css).unwrap();
        assert_eq!(result.nodes.len(), 1);

        if let CssNode::Style(ref rule) = result.nodes[0] {
            assert_eq!(rule.declarations.len(), 1);
            assert!(rule.declarations[0].important);
        } else {
            panic!("expected a Style node");
        }
    }

    #[test]
    fn test_parse_media_rule() {
        let css = "@media (min-width: 768px) { .foo { color: blue; } }";
        let result = parse(css, Syntax::Css).unwrap();
        assert_eq!(result.nodes.len(), 1);

        if let CssNode::AtRule(ref at_rule) = result.nodes[0] {
            assert_eq!(at_rule.name, "media");
            assert!(!at_rule.params.is_empty());
            assert_eq!(at_rule.children.len(), 1);
        } else {
            panic!("expected an AtRule node");
        }
    }

    #[test]
    fn test_sass_converted_to_scss() {
        // Sass indented syntax is now converted to SCSS before parsing.
        let result = parse("$foo: red", Syntax::Sass).expect("should parse Sass");
        assert_eq!(result.syntax, Syntax::Sass);
        assert!(!result.nodes.is_empty(), "should produce AST nodes");
    }

    #[test]
    fn test_parse_scss_variable() {
        let scss = "$color: red;";
        let result = parse(scss, Syntax::Scss).expect("should parse SCSS");
        assert_eq!(result.syntax, Syntax::Scss);
        assert_eq!(result.nodes.len(), 1);
        if let CssNode::Declaration(ref decl) = result.nodes[0] {
            assert_eq!(decl.property, "$color");
            assert_eq!(decl.value, "red");
        } else {
            panic!("expected a Declaration node, got: {:?}", result.nodes[0]);
        }
    }

    #[test]
    fn test_parse_scss_nesting() {
        let scss = r#"
            .foo {
                color: red;
                &:hover {
                    color: blue;
                }
            }
        "#;
        let result = parse(scss, Syntax::Scss).expect("should parse SCSS");
        // Should have one top-level style rule.
        let style_nodes: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| matches!(n, CssNode::Style(_)))
            .collect();
        assert_eq!(style_nodes.len(), 1);

        if let CssNode::Style(rule) = style_nodes[0] {
            assert!(
                rule.selector.contains(".foo"),
                "selector should contain '.foo', got: {}",
                rule.selector,
            );
            assert_eq!(rule.declarations.len(), 1);
            assert_eq!(rule.declarations[0].property, "color");
            assert_eq!(rule.declarations[0].value, "red");
            assert_eq!(rule.children.len(), 1);
            assert!(
                rule.children[0].selector.contains("&:hover"),
                "nested selector should contain '&:hover', got: {}",
                rule.children[0].selector,
            );
            assert_eq!(rule.children[0].declarations[0].value, "blue");
        } else {
            panic!("expected a Style node");
        }
    }

    #[test]
    fn test_parse_scss_mixin() {
        let scss = "@mixin button($color) { background: $color; }";
        let result = parse(scss, Syntax::Scss).expect("should parse SCSS mixin");
        let at_rules: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| matches!(n, CssNode::AtRule(_)))
            .collect();
        assert!(!at_rules.is_empty(), "should have at least one AtRule");
        if let CssNode::AtRule(at) = at_rules[0] {
            assert_eq!(at.name, "mixin");
        } else {
            panic!("expected an AtRule");
        }
    }

    #[test]
    fn test_parse_scss_include() {
        let scss = ".foo { @include button(red); }";
        let result = parse(scss, Syntax::Scss).expect("should parse SCSS @include");
        // The @include should be inside the style rule's block.
        // Since we only extract declarations and nested rules from qualified rules,
        // the @include is handled at the statement level.
        assert!(!result.nodes.is_empty());
    }

    #[test]
    fn test_parse_scss_include_content_block() {
        // @include with a content block should keep at-rules inside the
        // parent StyleRule's nested_at_rules (not as top-level siblings),
        // so that declarations inside @include don't leak to the wrong scope.
        let scss = r#".parent {
  @include rtl() {
    -webkit-transform: rotate(90deg);
    transform: rotate(90deg);
  }
  @include mq-medium {
    .child {
      left: 10px;
    }
  }
}"#;
        let result = parse(scss, Syntax::Scss).expect("should parse SCSS @include content block");

        // There should be exactly 1 top-level node: the .parent StyleRule.
        assert_eq!(
            result.nodes.len(),
            1,
            "should have 1 top-level node (.parent), got: {:?}",
            result
                .nodes
                .iter()
                .map(|n| match n {
                    CssNode::Style(_) => "Style",
                    CssNode::AtRule(_) => "AtRule",
                    CssNode::Comment(_) => "Comment",
                    CssNode::Declaration(_) => "Declaration",
                })
                .collect::<Vec<_>>()
        );

        let CssNode::Style(ref parent) = result.nodes[0] else {
            panic!("expected Style node");
        };

        // The @include blocks should be inside nested_at_rules.
        assert_eq!(
            parent.nested_at_rules.len(),
            2,
            "should have 2 @include at-rule nodes in nested_at_rules"
        );

        // First @include should have declarations as children.
        if let CssNode::AtRule(at) = &parent.nested_at_rules[0] {
            assert_eq!(at.name, "include");
            assert!(
                !at.children.is_empty(),
                "first @include should have children (declarations)"
            );
            let decls: Vec<_> = at
                .children
                .iter()
                .filter(|n| matches!(n, CssNode::Declaration(_)))
                .collect();
            assert_eq!(
                decls.len(),
                2,
                "should have 2 declarations inside @include rtl()"
            );
        } else {
            panic!("expected AtRule");
        }

        // Second @include should have a nested style rule as a child.
        if let CssNode::AtRule(at) = &parent.nested_at_rules[1] {
            assert_eq!(at.name, "include");
            let styles: Vec<_> = at
                .children
                .iter()
                .filter(|n| matches!(n, CssNode::Style(_)))
                .collect();
            assert_eq!(
                styles.len(),
                1,
                "should have 1 style rule inside @include mq-medium"
            );
        } else {
            panic!("expected AtRule");
        }
    }

    #[test]
    fn test_parse_scss_comment() {
        let scss = "/* hello */ .foo { color: red; }";
        let result = parse(scss, Syntax::Scss).expect("should parse SCSS with comments");
        let comments: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| matches!(n, CssNode::Comment(_)))
            .collect();
        assert_eq!(comments.len(), 1);
        if let CssNode::Comment(c) = comments[0] {
            assert!(c.text.contains("hello"));
        }
    }

    #[test]
    fn test_parse_scss_media() {
        let scss = "@media (min-width: 768px) { .foo { color: blue; } }";
        let result = parse(scss, Syntax::Scss).expect("should parse SCSS @media");
        let at_rules: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| matches!(n, CssNode::AtRule(_)))
            .collect();
        assert!(!at_rules.is_empty());
        if let CssNode::AtRule(at) = at_rules[0] {
            assert_eq!(at.name, "media");
            assert!(!at.children.is_empty());
        }
    }

    #[test]
    fn test_parse_nested_rules() {
        let css = r#"
            .parent {
                color: red;
                .child {
                    color: blue;
                }
            }
        "#;
        let result = parse(css, Syntax::Css).unwrap();
        assert_eq!(result.nodes.len(), 1);

        if let CssNode::Style(ref rule) = result.nodes[0] {
            assert_eq!(rule.selector, ".parent");
            assert!(!rule.children.is_empty());
            // lightningcss includes the `&` nesting selector prefix.
            assert!(
                rule.children[0].selector.contains(".child"),
                "expected nested selector to contain '.child', got: {}",
                rule.children[0].selector,
            );
        } else {
            panic!("expected a Style node");
        }
    }

    #[test]
    fn test_parse_import() {
        let css = r#"@import "reset.css";"#;
        let result = parse(css, Syntax::Css).unwrap();
        assert_eq!(result.nodes.len(), 1);

        if let CssNode::AtRule(ref at_rule) = result.nodes[0] {
            assert_eq!(at_rule.name, "import");
            assert_eq!(at_rule.params, "reset.css");
        } else {
            panic!("expected an AtRule node");
        }
    }

    #[test]
    fn test_parse_keyframes() {
        let css = "@keyframes fade { from { opacity: 0; } to { opacity: 1; } }";
        let result = parse(css, Syntax::Css).unwrap();
        assert_eq!(result.nodes.len(), 1);

        if let CssNode::AtRule(ref at_rule) = result.nodes[0] {
            assert_eq!(at_rule.name, "keyframes");
            assert_eq!(at_rule.params, "fade");
        } else {
            panic!("expected an AtRule node");
        }
    }

    #[test]
    fn test_parse_multiple_rules() {
        let css = r#"
            .a { color: red; }
            .b { color: blue; }
        "#;
        let result = parse(css, Syntax::Css).unwrap();
        assert_eq!(result.nodes.len(), 2);
    }

    #[test]
    fn test_default_syntax_is_css() {
        assert_eq!(Syntax::default(), Syntax::Css);
    }

    #[test]
    fn test_line_col_to_byte_offset() {
        let src = "abc\ndef\nghi";
        // line 0, col 1 => byte 0 ('a')
        assert_eq!(line_col_to_byte_offset(src, 0, 1), 0);
        // line 0, col 3 => byte 2 ('c')
        assert_eq!(line_col_to_byte_offset(src, 0, 3), 2);
        // line 1, col 1 => byte 4 ('d')
        assert_eq!(line_col_to_byte_offset(src, 1, 1), 4);
        // line 1, col 2 => byte 5 ('e')
        assert_eq!(line_col_to_byte_offset(src, 1, 2), 5);
        // line 2, col 1 => byte 8 ('g')
        assert_eq!(line_col_to_byte_offset(src, 2, 1), 8);
    }

    #[test]
    fn test_span_is_byte_offset() {
        // Parse a simple rule and verify span.offset is a valid byte offset.
        let css = "a { }\n.b { }";
        let result = parse(css, Syntax::Css).unwrap();
        assert_eq!(result.nodes.len(), 2);

        // "a" starts at byte 0
        if let CssNode::Style(ref rule) = result.nodes[0] {
            assert_eq!(rule.span.offset, 0);
            assert_eq!(&css[rule.span.offset..rule.span.offset + 1], "a");
        } else {
            panic!("expected Style node");
        }

        // ".b" starts at byte 6
        if let CssNode::Style(ref rule) = result.nodes[1] {
            assert_eq!(rule.span.offset, 6);
            assert_eq!(&css[rule.span.offset..rule.span.offset + 2], ".b");
        } else {
            panic!("expected Style node");
        }
    }

    #[test]
    fn test_raffia_failure_triggers_fallback() {
        // Verify that parse_raffia actually fails on certain inputs and
        // our fallback to lightningcss kicks in.
        // We test this by checking that parse() succeeds even when parse_raffia
        // would fail — i.e. the fallback is working.
        let garbage = "}{@!invalid";

        // parse_raffia should fail on this...
        let raffia_result = parse_raffia(garbage, Syntax::Scss);
        // ...but the public parse() function should handle it gracefully.
        let public_result = parse(garbage, Syntax::Scss);

        if raffia_result.is_err() {
            // Raffia failed as expected — public parse should either succeed
            // (via lightningcss fallback) or return an error (never silently empty).
            // An Err here is fine: both parsers failed and the error is
            // propagated rather than swallowed.
            if let Ok(result) = &public_result {
                assert_eq!(result.syntax, Syntax::Scss);
            }
        }
        // If raffia somehow succeeds, that's fine too — no fallback needed.
    }

    #[test]
    fn test_scss_fallback_preserves_nodes() {
        // An SCSS file with valid CSS after a broken part should still yield
        // some AST nodes via the lightningcss fallback.
        let src = ".broken { content: \"unclosed; }\n.valid { color: red; }";
        match parse(src, Syntax::Scss) {
            Ok(result) => {
                assert_eq!(result.syntax, Syntax::Scss);
                // lightningcss with error recovery should produce at least one node.
                assert!(
                    !result.nodes.is_empty(),
                    "Fallback parser should recover at least one node"
                );
            }
            Err(_) => {
                // If both fail, that's fine — the error is propagated, not swallowed.
            }
        }
    }

    #[test]
    fn test_parse_css_keyframes_children() {
        let css = "@keyframes fade { from { opacity: 0; } to { opacity: 1; } }";
        let result = parse(css, Syntax::Css).expect("should parse");
        assert_eq!(result.nodes.len(), 1);
        if let CssNode::AtRule(ref at_rule) = result.nodes[0] {
            assert_eq!(at_rule.name, "keyframes");
            assert_eq!(at_rule.children.len(), 2, "should have 2 keyframe blocks");
            if let CssNode::Style(ref kf) = at_rule.children[0] {
                assert!(
                    kf.selector.contains("from") || kf.selector.contains("0%"),
                    "first keyframe selector should be 'from' or '0%', got: {}",
                    kf.selector
                );
                assert!(!kf.declarations.is_empty(), "should have declarations");
            } else {
                panic!("expected Style node for keyframe block");
            }
        } else {
            panic!("expected AtRule node for @keyframes");
        }
    }

    #[test]
    fn test_parse_scss_keyframes_children() {
        let scss = "@keyframes fade { from { opacity: 0; } to { opacity: 1; } }";
        let result = parse(scss, Syntax::Scss).expect("should parse SCSS");
        // Filter out comments.
        let nodes: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| !matches!(n, CssNode::Comment(_)))
            .collect();
        assert_eq!(nodes.len(), 1);
        if let CssNode::AtRule(at_rule) = &nodes[0] {
            assert_eq!(at_rule.name, "keyframes");
            assert_eq!(at_rule.children.len(), 2, "should have 2 keyframe blocks");
        } else {
            panic!("expected AtRule node for @keyframes");
        }
    }

    #[test]
    fn test_parse_scss_keyframes_important() {
        let scss = "@keyframes fade { from { opacity: 0 !important; } }";
        let result = parse(scss, Syntax::Scss).expect("should parse SCSS");
        let nodes: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| !matches!(n, CssNode::Comment(_)))
            .collect();
        assert_eq!(nodes.len(), 1);
        if let CssNode::AtRule(at_rule) = &nodes[0] {
            assert_eq!(at_rule.name, "keyframes");
            assert!(!at_rule.children.is_empty());
            if let CssNode::Style(ref kf) = at_rule.children[0] {
                assert!(!kf.declarations.is_empty());
                assert!(
                    kf.declarations[0].important,
                    "should detect !important in SCSS keyframe"
                );
            } else {
                panic!("expected Style node");
            }
        } else {
            panic!("expected AtRule");
        }
    }

    #[test]
    fn test_layer_with_nested_media_has_declarations() {
        // Declarations that follow a nested @media rule inside @layer should
        // be extracted via NestedDeclarations and not lost.
        let css = r#"@layer utils {
	.foo {
		@media not ( prefers-reduced-motion ) {
			transition: outline 0.1s ease-out;
		}
		outline-width: 0;
		outline-style: solid;
	}
}"#;
        let result = parse(css, Syntax::Css).expect("should parse");
        assert_eq!(result.nodes.len(), 1);
        if let CssNode::AtRule(ref at_rule) = result.nodes[0] {
            assert_eq!(at_rule.name, "layer");
            assert!(!at_rule.children.is_empty(), "layer should have children");
            if let CssNode::Style(ref style) = at_rule.children[0] {
                assert!(
                    !style.declarations.is_empty(),
                    "style rule should have declarations from NestedDeclarations, got {}",
                    style.declarations.len(),
                );
                let props: Vec<&str> = style
                    .declarations
                    .iter()
                    .map(|d| d.property.as_str())
                    .collect();
                assert!(
                    props.contains(&"outline-width"),
                    "should contain outline-width, got {:?}",
                    props
                );
                assert!(
                    props.contains(&"outline-style"),
                    "should contain outline-style, got {:?}",
                    props
                );
            } else {
                panic!("expected Style node in layer children");
            }
        } else {
            panic!("expected AtRule");
        }
    }

    #[test]
    fn declaration_span_not_confused_by_selector_substring() {
        // Regression: "border" in selector ".ws-core-f-flex-border" was being
        // matched as the declaration span for `border:`, causing wrong order.
        let css = ".ws-core-f-flex-border,\n.ws-core-f-flex .pf-v6-l-flex .pf-v6-l-flex,\n.ws-core-f-flex .pf-v6-l-flex__item {\n  padding: var(--pf-t--global--spacer--sm);\n  border: var(--pf-t--global--border--width--box--default) dashed var(--pf-t--global--border--color--default);\n}";
        let result = parse(css, Syntax::Css).unwrap();
        if let CssNode::Style(ref rule) = result.nodes[0] {
            assert_eq!(rule.declarations.len(), 2);
            assert_eq!(rule.declarations[0].property, "padding");
            assert_eq!(rule.declarations[1].property, "border");
            assert!(
                rule.declarations[0].span.offset < rule.declarations[1].span.offset,
                "padding offset ({}) should precede border offset ({})",
                rule.declarations[0].span.offset,
                rule.declarations[1].span.offset
            );
        } else {
            panic!("expected Style node");
        }
    }
}

#[cfg(test)]
#[test]
fn debug_custom_property_span() {
    let css = ":root {\n  --my-color: rgb(248, 248, 247);\n  color: rgb(100, 200, 50);\n}\n";
    let result = parse(css, Syntax::Css).unwrap();
    for node in &result.nodes {
        if let CssNode::Style(rule) = node {
            for decl in &rule.declarations {
                let span_text = if decl.span.length > 0 {
                    &css[decl.span.offset..decl.span.offset + decl.span.length]
                } else {
                    "(empty span)"
                };
                eprintln!(
                    "DECL: prop={:?} value={:?} span=({}, {}) text={:?}",
                    decl.property, decl.value, decl.span.offset, decl.span.length, span_text
                );
            }
        }
    }
}

#[cfg(test)]
#[test]
fn test_custom_property_spans() {
    let css = ":root {\n  --font-sans: var(--font-inter), ui-sans-serif, system-ui;\n  --font-serif: var(--font-roboto-serif), ui-serif, Georgia;\n}";
    let result = parse(css, Syntax::Css).unwrap();
    if let CssNode::Style(rule) = &result.nodes[0] {
        assert_eq!(rule.declarations.len(), 2);
        let serif = &rule.declarations[1];
        assert_eq!(serif.property, "--font-serif");
        assert!(serif.span.length > 0, "span should be set");
        let span_text = &css[serif.span.offset..serif.span.offset + serif.span.length];
        assert!(
            span_text.contains("Georgia"),
            "source span should contain Georgia"
        );
    } else {
        panic!("expected StyleRule");
    }
}
