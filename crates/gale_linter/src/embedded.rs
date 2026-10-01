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

use std::cell::RefCell;

use gale_css_parser::Syntax;
use gale_css_parser::embedded::{HostLanguage, StyleBlock, StyleLang, extract_style_blocks};
use gale_diagnostics::{Diagnostic, LintResult, SourceLineIndex};

use crate::disables;
use crate::registry::resolve_deprecated_alias;

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

/// How [`lint_host`] treats the file.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostOptions {
  /// Stylelint's `ignoreDisables`: a `stylelint-disable` comment left open
  /// in one block does not carry into the next.
  pub ignore_disables: bool,
}

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
  options: HostOptions,
  mut lint_css: impl FnMut(&str, Syntax) -> LintResult,
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
    let root = EmbeddedRoot::new(
      block.inline,
      code_before,
      &text,
      next_code_before,
      doc_indent,
    );
    let result = {
      let _context = RootContext::enter(root);
      lint_css(&text, *syntax)
    };

    let lines_before = source[..range.start].matches('\n').count();
    diagnostics.extend(
      result
        .diagnostics
        .into_iter()
        .filter(|d| applies_to_root(&d.rule_name, block.inline))
        .map(|d| to_host(d, block, lines_before)),
    );
    for warning in result.invalid_option_warnings {
      if !invalid_options.contains(&warning) {
        invalid_options.push(warning);
      }
    }
  }

  if !options.ignore_disables {
    apply_document_disables(source, &roots, &mut diagnostics);
  }
  diagnostics.sort_by(|a, b| {
    a.span
      .offset
      .cmp(&b.span.offset)
      .then_with(|| a.rule_name.cmp(&b.rule_name))
  });
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

/// The canonical name of a rule, resolving Stylelint's pre-`@stylistic`
/// aliases (the runner reports a rule under the name the config used).
fn canonical(name: &str) -> &str {
  resolve_deprecated_alias(name).unwrap_or(name)
}

/// Whether Stylelint runs the rule that reported `rule_name` on an embedded
/// root of this kind.
fn applies_to_root(rule_name: &str, inline: bool) -> bool {
  let rule = canonical(rule_name);
  !SKIPPED_IN_EMBEDDED_ROOTS.contains(&rule)
    && !(inline && SKIPPED_IN_STYLE_ATTRIBUTES.contains(&rule))
}

/// Move a diagnostic from the block's text, which starts after
/// `lines_before` lines of the host file, into the host file.  A fix is kept
/// only when every edit stays inside the block and clear of template
/// expressions, whose text the linter never saw.
fn to_host(mut diag: Diagnostic, block: &StyleBlock, lines_before: usize) -> Diagnostic {
  let base = block.range.start;
  diag.span.offset += base;
  if canonical(&diag.rule_name) == "no-duplicate-selectors" {
    diag.message = shift_line_reference(&diag.message, lines_before);
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

/// `message` with the line number after its last "first used at line "
/// moved down by `lines_before` lines.  A custom message without that
/// phrase is left alone.
fn shift_line_reference(message: &str, lines_before: usize) -> String {
  const PHRASE: &str = "first used at line ";
  let Some(at) = message.rfind(PHRASE) else {
    return message.to_string();
  };
  let digits_start = at + PHRASE.len();
  let digits_len = message[digits_start..]
    .bytes()
    .take_while(u8::is_ascii_digit)
    .count();
  let Ok(line) = message[digits_start..digits_start + digits_len].parse::<usize>() else {
    return message.to_string();
  };
  format!(
    "{}{}{}",
    &message[..digits_start],
    line + lines_before,
    &message[digits_start + digits_len..]
  )
}

// ---------------------------------------------------------------------------
// `stylelint-disable` across blocks
// ---------------------------------------------------------------------------

/// Whether the problem is about the file or the config rather than a rule's
/// finding, which configuration comments never disable.
fn ignores_disables(d: &Diagnostic) -> bool {
  d.is_comment_problem()
    || d.is_unknown_rule()
    || d.is_invalid_option()
    || d.message.starts_with("Internal error")
    || d.rule_name == "parse-error"
}

/// Drop the problems the document's configuration comments disable.
///
/// Stylelint reads the comments of every root of the document in order, as
/// one sequence with host-file line numbers, so a `stylelint-disable` left
/// open in one `<style>` block reaches into the blocks after it until an
/// `enable` in a later one.  The runner leaves suppression inside an
/// embedded sheet to this pass.
fn apply_document_disables(
  source: &str,
  roots: &[(StyleBlock, Syntax)],
  diagnostics: &mut Vec<Diagnostic>,
) {
  if !disables::may_have_directives(source) {
    return;
  }
  let lines = SourceLineIndex::build(source);
  let line_of = |offset: usize| lines.line(offset);
  let mut collector = disables::Collector::new(&line_of);
  for (block, syntax) in roots {
    collector.scan(&sheet_text(source, block), *syntax, block.range.start);
  }
  let (ranges, _rejected) = collector.finish();
  if ranges.is_empty() {
    return;
  }
  diagnostics
    .retain(|d| ignores_disables(d) || !ranges.covers(&d.rule_name, line_of(d.span.offset)));
}

// ---------------------------------------------------------------------------
// The root being linted, for `@stylistic/indentation`
// ---------------------------------------------------------------------------

/// What `@stylistic/indentation` needs to know about the embedded root it
/// is checking, taken from the markup around it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EmbeddedRoot {
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
  /// Describe the root whose text is `text`, between `code_before` and the
  /// next root's `next_code_before` (or the end of the file).
  fn new(
    inline: bool,
    code_before: &str,
    text: &str,
    next_code_before: Option<&str>,
    doc_indent: Option<usize>,
  ) -> Self {
    // The plugin's `/(?:^|\n)([\t ]*)\S/gm`, last match first, until one
    // whose first non-blank character is `<`.
    let tag_indent = code_before.split('\n').rev().find_map(|line| {
      let rest = line.trim_start_matches([' ', '\t']);
      rest
        .starts_with('<')
        .then(|| line[..line.len() - rest.len()].to_string())
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
      .map(|s| s[..s.len() - s.trim_start_matches([' ', '\t']).len()].to_string());
    Self {
      inline,
      starts_on_new_line: code_before.ends_with('\n'),
      tag_indent,
      after_indent,
      doc_indent,
    }
  }
}

thread_local! {
  /// The embedded root being linted on this thread, if any.
  static CURRENT_ROOT: RefCell<Option<EmbeddedRoot>> = const { RefCell::new(None) };
}

/// Whether a style sheet embedded in an HTML-like file is being linted on
/// this thread.  The runner then leaves suppressing what configuration
/// comments disable to [`lint_host`], which reads them for the whole file.
pub(crate) fn in_embedded_root() -> bool {
  CURRENT_ROOT.with(|current| current.borrow().is_some())
}

/// Marks a root as the one being linted for as long as it lives.
struct RootContext;

impl RootContext {
  /// Make `root` current until the returned guard is dropped.
  fn enter(root: EmbeddedRoot) -> Self {
    CURRENT_ROOT.with(|current| *current.borrow_mut() = Some(root));
    RootContext
  }
}

impl Drop for RootContext {
  /// Clear the current root, even when linting panicked.
  fn drop(&mut self) {
    CURRENT_ROOT.with(|current| *current.borrow_mut() = None);
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
  text: &str,
  space: Option<usize>,
  base_indent_level: Option<i64>,
) -> Option<EmbeddedIndentation> {
  CURRENT_ROOT.with(|current| {
    let current = current.borrow();
    let root = current.as_ref()?;
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
            let rest = line.trim_start_matches([' ', '\t']);
            let has_content = rest.chars().next().is_some_and(|c| !c.is_whitespace());
            has_content.then(|| level(&line[..line.len() - rest.len()]))
          })
          .min()
          .unwrap_or(1)
      }
    };
    Some(EmbeddedIndentation {
      base_level,
      check_first_line: root.starts_on_new_line && !root.inline,
    })
  })
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
mod tests {
  use super::*;
  use gale_diagnostics::{Edit, Fix, Severity, Span, apply_fixes};

  /// A stand-in linter: one `fake` problem at every `!`, with a fix
  /// replacing it by `?`, plus whatever `extra` adds.
  fn fake_lint(text: &str, _syntax: Syntax) -> LintResult {
    let diagnostics = text
      .match_indices('!')
      .map(|(at, _)| {
        Diagnostic::new("fake", "bang")
          .severity(Severity::Error)
          .span(Span::new(at, 1))
          .fix(Fix::new("unbang", vec![Edit::new(Span::new(at, 1), "?")]))
      })
      .collect();
    LintResult::new("x.vue", text, diagnostics)
  }

  #[test]
  fn problems_land_at_host_offsets() {
    let source = "<template><p>!</p></template>\n<style>\n.a { b: c! }\n</style>\n<style lang=\"scss\">!</style>";
    let result = lint_host(
      source,
      "x.vue",
      HostLanguage::Vue,
      HostOptions::default(),
      fake_lint,
    );
    let offsets: Vec<usize> = result.diagnostics.iter().map(|d| d.span.offset).collect();
    let bangs: Vec<usize> = source
      .match_indices('!')
      .map(|(at, _)| at)
      .skip(1)
      .collect();
    assert_eq!(offsets, bangs);
    assert_eq!(result.source, source);
    assert_eq!(result.file_path, "x.vue");
  }

  #[test]
  fn fixes_rewrite_only_the_style_blocks() {
    let source = "<template><p>!</p></template>\n<style>.a{b:c!}</style><div style=\"x:y!\"></div>";
    let result = lint_host(
      source,
      "x.vue",
      HostLanguage::Vue,
      HostOptions::default(),
      fake_lint,
    );
    let (fixed, count) = apply_fixes(source, &result.diagnostics);
    assert_eq!(count, 2);
    assert_eq!(
      fixed,
      "<template><p>!</p></template>\n<style>.a{b:c?}</style><div style=\"x:y?\"></div>"
    );
  }

  #[test]
  fn fixes_that_leave_the_block_are_dropped() {
    let source = "<style>a!</style>";
    let result = lint_host(
      source,
      "x.html",
      HostLanguage::Html,
      HostOptions::default(),
      |text, _| {
        let reach = Diagnostic::new("reach", "too far")
          .span(Span::new(0, 1))
          .fix(Fix::new(
            "eat",
            vec![Edit::new(Span::new(0, text.len() + 3), "")],
          ));
        LintResult::new("x.html", text, vec![reach])
      },
    );
    assert!(result.diagnostics[0].fix.is_none());
  }

  #[test]
  fn blocks_get_their_own_syntax_and_unsupported_ones_are_skipped() {
    let source = "<style>a</style><style lang=\"less\">b</style><style lang=\"sass\">c</style>\
                  <style lang=\"stylus\">d</style><p style=\"e: f\"></p>";
    let mut seen = Vec::new();
    lint_host(
      source,
      "x.vue",
      HostLanguage::Vue,
      HostOptions::default(),
      |text, syntax| {
        seen.push((text.to_string(), syntax));
        LintResult::new("x.vue", text, Vec::new())
      },
    );
    assert_eq!(
      seen,
      vec![
        ("a".to_string(), Syntax::Css),
        ("b".to_string(), Syntax::Less),
        ("c".to_string(), Syntax::Sass),
        ("e: f".to_string(), Syntax::Scss),
      ]
    );
    assert_eq!(
      unsupported_blocks(source, HostLanguage::Vue),
      vec!["stylus"]
    );
  }

  #[test]
  fn template_expressions_become_placeholders_of_the_same_length() {
    let source = "<div style=\"width: {size}px; color: {c ? 'é' : 'b'}\"></div>";
    let mut seen = String::new();
    lint_host(
      source,
      "x.svelte",
      HostLanguage::Svelte,
      HostOptions::default(),
      |text, _| {
        seen = text.to_string();
        LintResult::new("x.svelte", text, Vec::new())
      },
    );
    assert_eq!(seen, "width: $_____px; color: $_______________");
    assert_eq!(seen.len(), "width: {size}px; color: {c ? 'é' : 'b'}".len());
  }

  #[test]
  fn some_rules_skip_embedded_roots() {
    let source = "<style>a</style><p style=\"b\"></p>";
    let rules = [
      "@stylistic/unicode-bom",
      "unicode-bom",
      "nesting-selector-no-missing-scoping-root",
      "@stylistic/no-empty-first-line",
      "no-missing-end-of-source-newline",
      "no-invalid-position-declaration",
      "color-named",
    ];
    let result = lint_host(
      source,
      "x.vue",
      HostLanguage::Vue,
      HostOptions::default(),
      |text, _| {
        let diagnostics = rules.iter().map(|r| Diagnostic::new(*r, "x")).collect();
        LintResult::new("x.vue", text, diagnostics)
      },
    );
    let mut reported: Vec<(usize, &str)> = result
      .diagnostics
      .iter()
      .map(|d| (d.span.offset, d.rule_name.as_str()))
      .collect();
    reported.sort();
    assert_eq!(
      reported,
      vec![
        (7, "@stylistic/no-empty-first-line"),
        (7, "color-named"),
        (7, "no-invalid-position-declaration"),
        (7, "no-missing-end-of-source-newline"),
        (26, "color-named"),
      ]
    );
  }

  #[test]
  fn duplicate_selector_messages_name_the_host_line() {
    let source = "<template/>\n<style>\n.a {}\n.a {}\n</style>";
    let result = lint_host(
      source,
      "x.vue",
      HostLanguage::Vue,
      HostOptions::default(),
      |text, _| {
        let dup = Diagnostic::new(
          "no-duplicate-selectors",
          "Unexpected duplicate selector \".a\", first used at line 1",
        )
        .span(Span::new(6, 2));
        LintResult::new("x.vue", text, vec![dup])
      },
    );
    assert_eq!(
      result.diagnostics[0].message,
      "Unexpected duplicate selector \".a\", first used at line 3"
    );
    assert_eq!(shift_line_reference("custom", 4), "custom");
  }

  #[test]
  fn open_disables_carry_into_later_blocks() {
    let source = "<style>\n/* stylelint-disable fake */\na{!}\n</style>\n\
                  <style>\na{!}\n/* stylelint-enable fake */\na{!}\n</style>\n\
                  <style>/* stylelint-disable */</style><style>a{!}</style>";
    // Lines 2 to 7 are off for `fake`, and everything from the last line,
    // the line of the blanket disable, on.
    let result = lint_host(
      source,
      "x.vue",
      HostLanguage::Vue,
      HostOptions::default(),
      fake_lint,
    );
    let offsets: Vec<usize> = result.diagnostics.iter().map(|d| d.span.offset).collect();
    let bangs: Vec<usize> = source.match_indices('!').map(|(at, _)| at).collect();
    assert_eq!(offsets, vec![bangs[2]]);

    let ignored = lint_host(
      source,
      "x.vue",
      HostLanguage::Vue,
      HostOptions {
        ignore_disables: true,
      },
      fake_lint,
    );
    assert_eq!(ignored.diagnostics.len(), 4);
  }

  #[test]
  fn invalid_options_are_reported_once() {
    let source = "<style>a</style><style>b</style>";
    let result = lint_host(
      source,
      "x.vue",
      HostLanguage::Vue,
      HostOptions::default(),
      |text, _| {
        let mut r = LintResult::new("x.vue", text, Vec::new());
        r.invalid_option_warnings.push("bad option".to_string());
        r
      },
    );
    assert_eq!(result.invalid_option_warnings, vec!["bad option"]);
  }

  #[test]
  fn a_file_without_styles_has_no_problems() {
    let result = lint_host(
      "<template><div/></template>",
      "x.vue",
      HostLanguage::Vue,
      HostOptions::default(),
      fake_lint,
    );
    assert!(result.diagnostics.is_empty());
  }

  /// The indentation context seen while linting each root of `source`.
  fn indentation_seen(
    source: &str,
    space: Option<usize>,
    base: Option<i64>,
  ) -> Vec<Option<EmbeddedIndentation>> {
    let mut seen = Vec::new();
    lint_host(
      source,
      "x.html",
      HostLanguage::Html,
      HostOptions::default(),
      |text, _| {
        seen.push(indentation_context(text, space, base));
        LintResult::new("x.html", text, Vec::new())
      },
    );
    seen
  }

  #[test]
  fn indentation_base_level_is_inferred_from_the_markup() {
    let html = "<html>\n  <head>\n    <style>\n      a {\n        color: red;\n      }\n    </style>\n  </head>\n</html>\n";
    // The shallowest line is 6 spaces: three 2-space levels.
    assert_eq!(
      indentation_seen(html, Some(2), None),
      vec![Some(EmbeddedIndentation {
        base_level: 3,
        check_first_line: true,
      })]
    );
    // With `baseIndentLevel: 1`, one level past the `<style>` tag's two.
    assert_eq!(
      indentation_seen(html, Some(2), Some(1))[0]
        .unwrap()
        .base_level,
      3
    );
    assert_eq!(
      indentation_seen(html, Some(2), Some(0))[0]
        .unwrap()
        .base_level,
      2
    );

    // Unindented Vue-style blocks sit at level 0.
    let vue = "<template>\n  <div />\n</template>\n\n<style>\n.a {\n  color: red;\n}\n</style>\n";
    assert_eq!(
      indentation_seen(vue, Some(2), None)[0].unwrap().base_level,
      0
    );

    // A sheet on the tag's own line does not check (or count) that line.
    let inline = indentation_seen("<style>  a {}\n  b {}</style>", Some(2), None);
    assert_eq!(
      inline,
      vec![Some(EmbeddedIndentation {
        base_level: 1,
        check_first_line: false,
      })]
    );

    // Outside an embedded root there is no context.
    assert_eq!(indentation_context("a {}", Some(2), None), None);
  }

  #[test]
  fn doc_indent_size_votes_on_indent_steps() {
    assert_eq!(infer_doc_indent_size("a\n  b\n    c\n  d\n"), Some(2));
    assert_eq!(infer_doc_indent_size("a\n    b\n        c\n"), Some(4));
    assert_eq!(infer_doc_indent_size("a\nb\n"), None);
    assert_eq!(infer_doc_indent_size(" a\n"), Some(1));
    assert_eq!(infer_doc_indent_size("\ta\n\t\tb\n"), None);
  }
}
