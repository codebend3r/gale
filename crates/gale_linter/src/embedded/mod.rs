//! Linting the style sheets embedded in HTML-like files.
//!
//! [`lint_host`] lints a `.vue`, `.svelte`, `.astro` or `.html` file the way
//! Stylelint does with `customSyntax: "postcss-html"`: every `<style>` element
//! and `style="…"` attribute is a root of its own, linted with the syntax its
//! `lang` names, and every problem is reported at its position in the host
//! file.  The caller supplies the function that lints one style sheet, so
//! the same config, overrides and options apply as for a standalone file.
//!
//! A few rules see an embedded root differently from a file, as they do in
//! Stylelint:
//!
//! - `@stylistic/unicode-bom` and `nesting-selector-no-missing-scoping-root`
//!   skip every embedded root.
//! - `@stylistic/no-empty-first-line`,
//!   `@stylistic/no-missing-end-of-source-newline` and
//!   `no-invalid-position-declaration` skip `style` attributes.
//! - `@stylistic/indentation` infers the root's base indent level from the
//!   surrounding markup ([`indentation_context`]).
//! - Configuration comments are read across the whole file, so a
//!   `stylelint-disable` left open at the end of one block still applies in
//!   the blocks after it.
//! - `no-duplicate-selectors` names the host-file line a selector was first
//!   used on.

use gale_css_parser::Syntax;
use gale_diagnostics::{Diagnostic, LintResult, SourceLineIndex};

use crate::disables;
use crate::registry::canonical_name;

mod scanner;

pub use scanner::{HostLanguage, StyleBlock, StyleLang, extract_style_blocks};

/// Rules Stylelint does not apply to any embedded root.
const SKIPPED_IN_EMBEDDED_ROOTS: &[&str] = &[
  "@stylistic/unicode-bom",
  "nesting-selector-no-missing-scoping-root",
];

/// Rules Stylelint does not apply to a `style="…"` attribute.
const SKIPPED_IN_STYLE_ATTRIBUTES: &[&str] = &[
  "@stylistic/no-empty-first-line",
  "@stylistic/no-missing-end-of-source-newline",
  "no-invalid-position-declaration",
];

use crate::disables::DisablePolicy;

/// The parser for a style block, or `None` when gale has none for its
/// language.
///
/// A `style` attribute is a bare declaration list, which only the SCSS
/// parser reads as top-level declarations; plain CSS is valid SCSS.
fn block_syntax(block: &StyleBlock) -> Option<Syntax> {
  if block.inline {
    return Some(Syntax::Scss);
  }
  match block.lang {
    StyleLang::Css => Some(Syntax::Css),
    StyleLang::Scss => Some(Syntax::Scss),
    StyleLang::Less => Some(Syntax::Less),
    StyleLang::Sass => Some(Syntax::Sass),
    StyleLang::Unsupported(_) => None,
  }
}

/// The languages of the `<style>` blocks in `source` that gale cannot lint
/// (`stylus`, say), in document order, for the caller to warn about.
pub fn unsupported_blocks(source: &str, host: HostLanguage) -> Vec<String> {
  extract_style_blocks(source, host)
    .into_iter()
    .filter_map(|block| match block.lang {
      StyleLang::Unsupported(name) => Some(name),
      _ => None,
    })
    .collect()
}

/// Lint every style sheet embedded in `source`, an HTML-like file at
/// `file_path`, and report the problems at host-file positions.
///
/// `lint_css` lints one style sheet's text with the given syntax as though
/// it were a file at `file_path`, returning spans relative to that text.
/// Fixes are moved into the host file too; one that would touch text
/// outside its block is dropped, so applying the fixes leaves everything
/// outside the style sheets byte-for-byte as it was.
pub fn lint_host(
  source: &str,
  file_path: &str,
  host: HostLanguage,
  options: &DisablePolicy,
  mut lint_css: impl FnMut(&str, Syntax, &EmbeddedRoot) -> LintResult,
) -> LintResult {
  let roots: Vec<(StyleBlock, Syntax)> = extract_style_blocks(source, host)
    .into_iter()
    .filter_map(|block| block_syntax(&block).map(|syntax| (block, syntax)))
    .collect();
  let doc_indent = infer_doc_indent_size(source);

  let mut diagnostics = Vec::new();
  let mut invalid_options: Vec<String> = Vec::new();

  for (index, (block, syntax)) in roots.iter().enumerate() {
    let range = block.range.clone();
    let previous_end = index
      .checked_sub(1)
      .map_or(0, |i| roots[i].0.range.end)
      .min(range.start);
    let code_before = &source[previous_end..range.start];
    let next_code_before = roots
      .get(index + 1)
      .map(|(next, _)| &source[range.end..next.range.start.max(range.end)]);

    let text = sheet_text(source, block);
    let lines_before = source[..range.start].matches('\n').count();
    let root = EmbeddedRoot::new(
      block.inline,
      code_before,
      &text,
      next_code_before,
      doc_indent,
      lines_before,
    );
    let result = lint_css(&text, *syntax, &root);

    diagnostics.extend(result.diagnostics.into_iter().map(|d| to_host(d, block)));
    for warning in result.invalid_option_warnings {
      if !invalid_options.contains(&warning) {
        invalid_options.push(warning);
      }
    }
  }

  if let Some(rejected) =
    apply_document_disables(source, file_path, &roots, options, &mut diagnostics)
  {
    return LintResult::new(file_path, source, vec![rejected]);
  }
  crate::runner::sort_diagnostics(&mut diagnostics);
  let mut result = LintResult::new(file_path, source, diagnostics);
  result.invalid_option_warnings = invalid_options;
  result
}

/// The text gale parses for a block: the host's text, with each template
/// expression in a `style` attribute (`{size}`) turned into an SCSS variable
/// of the same length, which rules leave alone as postcss-html's template
/// syntax leaves the expression alone as one opaque word.
fn sheet_text<'s>(source: &'s str, block: &StyleBlock) -> std::borrow::Cow<'s, str> {
  let text = &source[block.range.clone()];
  if block.expressions.is_empty() {
    return text.into();
  }
  let mut bytes = text.as_bytes().to_vec();
  for expr in &block.expressions {
    let start = expr.start - block.range.start;
    let end = expr.end - block.range.start;
    bytes[start] = b'$';
    bytes[start + 1..end].fill(b'_');
  }
  // Only ASCII went in, at character boundaries.
  String::from_utf8(bytes).map_or_else(|_| text.into(), Into::into)
}

/// Whether Stylelint runs the rule that reported `rule_name` on an embedded
/// root of this kind.
fn applies_to_root(rule_name: &str, inline: bool) -> bool {
  let rule = canonical_name(rule_name);
  !SKIPPED_IN_EMBEDDED_ROOTS.contains(&rule)
    && !(inline && SKIPPED_IN_STYLE_ATTRIBUTES.contains(&rule))
}

/// Move a diagnostic from the block's text, which starts after
/// `lines_before` lines of the host file, into the host file.  A fix is kept
/// only when every edit stays inside the block and clear of template
/// expressions, whose text the linter never saw.
fn to_host(mut diag: Diagnostic, block: &StyleBlock) -> Diagnostic {
  let base = block.range.start;
  diag.span.offset += base;
  for related in &mut diag.related {
    related.span.offset += base;
  }
  if let Some(fix) = diag.fix.as_mut() {
    for edit in &mut fix.edits {
      edit.span.offset += base;
    }
    let inside = |span: &gale_diagnostics::Span| {
      span.offset >= block.range.start
        && span.end() <= block.range.end
        && !block
          .expressions
          .iter()
          .any(|expr| span.offset < expr.end && expr.start < span.end().max(span.offset + 1))
    };
    if !fix.edits.iter().all(|edit| inside(&edit.span)) {
      diag.fix = None;
    }
  }
  diag
}

// ---------------------------------------------------------------------------
// `stylelint-disable` across blocks
// ---------------------------------------------------------------------------

/// Drop the problems the document's configuration comments disable.
///
/// Stylelint reads the comments of every root of the document in order, as
/// one sequence with host-file line numbers, so a `stylelint-disable` left
/// open in one `<style>` block reaches into the blocks after it until an
/// `enable` in a later one.  The runner leaves suppression inside an
/// embedded sheet to this pass.
///
/// Returns Stylelint's `CssSyntaxError` for a comment it rejects, which
/// replaces every other problem in the file, `ignoreDisables` or not.
fn apply_document_disables(
  source: &str,
  file_path: &str,
  roots: &[(StyleBlock, Syntax)],
  options: &DisablePolicy,
  diagnostics: &mut Vec<Diagnostic>,
) -> Option<Diagnostic> {
  if !disables::may_have_directives(source) {
    return None;
  }
  let lines = SourceLineIndex::build(source);
  let line_of = |offset: usize| lines.line(offset);
  options.run(
    diagnostics,
    &line_of,
    |collector| {
      for (block, syntax) in roots {
        collector.scan(&sheet_text(source, block), *syntax, block.range.start);
      }
    },
    file_path,
  )
}

/// The spaces and tabs `line` starts with.
fn leading_blanks(line: &str) -> &str {
  &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

// ---------------------------------------------------------------------------
// The root being linted, for `@stylistic/indentation`
// ---------------------------------------------------------------------------

/// What `@stylistic/indentation` needs to know about the embedded root it
/// is checking, taken from the markup around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedRoot {
  lines_before: usize,
  /// A `style` attribute rather than a `<style>` element.
  inline: bool,
  /// The sheet starts on a fresh line (postcss-html's `codeBefore` ends
  /// with a newline), so its first line is indented like any other.
  starts_on_new_line: bool,
  /// The indentation of the last line before the sheet whose first
  /// character is `<`: the `<style>` tag's line.
  tag_indent: Option<String>,
  /// The indentation of the text after the sheet's last line: the closing
  /// tag's line.
  after_indent: Option<String>,
  /// The document's indent size, inferred from the whole host file, or
  /// `None` to fall back on the rule's own.
  doc_indent: Option<usize>,
}

impl EmbeddedRoot {
  /// Host lines preceding this sheet, for rule messages naming a line.
  pub fn lines_before(&self) -> usize {
    self.lines_before
  }

  /// Whether Stylelint runs this rule on this kind of embedded root.
  pub(crate) fn allows(&self, name: &str) -> bool {
    applies_to_root(name, self.inline)
  }

  /// Describe the root whose text is `text`, between `code_before` and the
  /// next root's `next_code_before` (or the end of the file).
  fn new(
    inline: bool,
    code_before: &str,
    text: &str,
    next_code_before: Option<&str>,
    doc_indent: Option<usize>,
    lines_before: usize,
  ) -> Self {
    // The plugin's `/(?:^|\n)([\t ]*)\S/gm`, last match first, until one
    // whose first non-blank character is `<`.
    let tag_indent = code_before.split('\n').rev().find_map(|line| {
      let indent = leading_blanks(line);
      line[indent.len()..]
        .starts_with('<')
        .then(|| indent.to_string())
    });
    // postcss's `root.raws.after`: the whitespace after the last node.
    let after = &text[text.trim_end_matches([' ', '\t', '\r', '\n']).len()..];
    let after_end = if after.is_empty() {
      None
    } else if after.ends_with('\n') {
      next_code_before
    } else {
      Some(after)
    };
    let after_indent = after_end
      .filter(|s| !s.is_empty())
      .map(|s| leading_blanks(s).to_string());
    Self {
      lines_before,
      inline,
      starts_on_new_line: code_before.ends_with('\n'),
      tag_indent,
      after_indent,
      doc_indent,
    }
  }
}

/// How `@stylistic/indentation` indents an embedded root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedIndentation {
  /// The indent level of the root's top-level nodes.
  pub base_level: usize,
  /// Whether the first line is checked too: only when the sheet starts on a
  /// line of its own, after the line holding its `<style>` tag.
  pub check_first_line: bool,
}

/// The indentation `@stylistic/indentation` expects of the embedded root
/// being linted, or `None` outside one.
///
/// `text` is the root's text, `space` the rule's indent width (`None` for
/// tabs) and `base_indent_level` its `baseIndentLevel` option when that is
/// an integer.  Without one the base level is the shallowest indentation in
/// the sheet; with one it counts from the `<style>` tag's indentation.  This
/// is the plugin's own inference, quirks included.
pub fn indentation_context(
  root: &EmbeddedRoot,
  text: &str,
  space: Option<usize>,
  base_indent_level: Option<i64>,
) -> EmbeddedIndentation {
  let indent_size = root
    .doc_indent
    .unwrap_or_else(|| space.filter(|&n| n > 0).unwrap_or(2));
  let level = |indent: &str| {
    let tabs = indent.bytes().filter(|&b| b == b'\t').count();
    let spaces = indent.bytes().filter(|&b| b == b' ').count();
    tabs + js_round(spaces as f64 / indent_size as f64)
  };

  let base_level = match base_indent_level {
    Some(n) => {
      let mut indents = Vec::new();
      if let Some(tag) = &root.tag_indent {
        indents.push(level(&"  ".repeat(level(tag))));
      }
      if let Some(after) = &root.after_indent {
        indents.push(level(after));
      }
      let n = usize::try_from(n).unwrap_or(0);
      indents.into_iter().max().map_or(n, |max| max + n)
    }
    None => {
      // The first line counts only when the sheet starts on a line of
      // its own.
      let lines = text.split('\n').skip(usize::from(!root.starts_on_new_line));
      lines
        .filter_map(|line| {
          let indent = leading_blanks(line);
          let has_content = line[indent.len()..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace());
          has_content.then(|| level(indent))
        })
        .min()
        .unwrap_or(1)
    }
  };
  EmbeddedIndentation {
    base_level,
    check_first_line: root.starts_on_new_line && !root.inline,
  }
}

/// JavaScript's `Math.round` for a non-negative number.
fn js_round(value: f64) -> usize {
  (value + 0.5).floor() as usize
}

/// The plugin's guess at the indent width the host document uses: the
/// difference between successive space indents that occurs most often
/// (ignoring single spaces), or 1 when there is none but the first indented
/// line starts with spaces, or `None` to fall back on the rule's width.
fn infer_doc_indent_size(source: &str) -> Option<usize> {
  let indents: Vec<usize> = source
    .split('\n')
    .filter_map(|line| {
      let rest = line.trim_start_matches(' ');
      let has_content = rest.chars().next().is_some_and(|c| !c.is_whitespace());
      has_content.then_some(line.len() - rest.len())
    })
    .collect();

  // Scores in first-seen order, so ties go to the earlier size as with a
  // JavaScript Map.
  let mut scores: Vec<(usize, usize)> = Vec::new();
  let mut last_size = 0usize;
  let mut last_leading = 0usize;
  for &leading in &indents {
    if leading > 0 {
      let diff = leading.abs_diff(last_leading);
      if diff != 0 {
        last_size = diff;
      }
      if last_size > 1 {
        match scores.iter_mut().find(|(size, _)| *size == last_size) {
          Some((_, score)) => *score += 1,
          None => scores.push((last_size, 1)),
        }
      }
    } else {
      last_size = 0;
    }
    last_leading = leading;
  }
  let mut best: Option<(usize, usize)> = None;
  for &(size, score) in &scores {
    if best.is_none_or(|(_, best_score)| score > best_score) {
      best = Some((size, score));
    }
  }
  match best {
    Some((size, _)) => Some(size),
    None if indents.first().is_some_and(|&n| n > 0) => Some(1),
    None => None,
  }
}

#[cfg(test)]
mod tests;
