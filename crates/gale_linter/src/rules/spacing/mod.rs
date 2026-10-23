//! The @stylistic and stylelint-scss rules about the whitespace next to one
//! character: `*-space-before/after`, `*-newline-before/after`.
//!
//! Each file holds one family: the finder that walks the tree for the
//! family's characters (the commas of a selector list, the colon of a
//! declaration, ...), as the plugin's per-family utilities do, and the rules
//! that run the shared [`Checker`](crate::whitespace::Checker) over what it
//! finds.  A rule is its `declare_rule!` and a [`SpacingRule`] impl: its
//! options, its messages, and which side of the character it checks.

pub mod at_rule;
pub mod bang;
pub mod brace;
pub mod colon;
pub mod combinator;
pub mod comma;
pub mod inside;
pub mod operator;
pub mod semicolon;

#[cfg(test)]
mod harness;

pub use at_rule::*;
pub use bang::*;
pub use brace::*;
pub use colon::*;
pub use combinator::*;
pub use comma::*;
pub use inside::*;
pub use operator::*;
pub use semicolon::*;

use std::borrow::Cow;

use gale_css_parser::postcss_tree::{AtRuleRef, DeclRef, NodeKind, NodeRef};
use gale_diagnostics::{Diagnostic, Edit};

use gale_css_parser::selector::postcss::{Kind, Visit};

use crate::whitespace::{At, Spacing, SpacingRule, fix_space_after, fix_space_before, is_js_space};

/// A text a finder hands its rules: a selector, a declaration or an
/// at-rule's params, as the plugin's utility reads it.
#[derive(Debug, Clone, Copy)]
pub struct Text<'a> {
  /// The text.
  pub text: &'a str,
  /// Where the plugin's report indexes into `text` count from: the node's
  /// start, plus where `text` starts in the node's string.
  pub report_base: usize,
  /// Where `text` starts in the source, when it is the source as written,
  /// so that a fix computed in it applies to the source.  `None` when
  /// postcss-scss rewrote a `//` comment in it.
  pub fix_base: Option<usize>,
}

impl Text<'_> {
  /// The fix `edits` computes for a text starting at the given offset, or
  /// none when the text is not the source as written.
  pub fn edits(&self, edits: impl FnOnce(usize) -> Vec<Edit>) -> Vec<Edit> {
    self.fix_base.map(edits).unwrap_or_default()
  }

  /// Where the plugin reports index `index` of the text (see [`index_in`]).
  pub fn report(&self, source: &str, index: usize) -> usize {
    index_in(source, self.report_base, self.text, index)
  }
}

/// Check the whitespace after `text[index]` and fix it to one space or
/// none, as the `*-space-after` rules do.
pub fn space_after<R: SpacingRule>(
  rule: &Spacing<R>,
  source: &str,
  text: &Text,
  index: usize,
) -> Option<Diagnostic> {
  let message = rule.checker.after(At::new(text.text, index))?;
  let edits = text.edits(|base| fix_space_after(rule.expectation(), base, text.text, index));
  Some(rule.problem_fixed_by(source, message, text.report(source, index), edits))
}

/// Check the whitespace before `text[index]` and fix it to one space or
/// none, as the `*-space-before` rules do.
pub fn space_before<R: SpacingRule>(
  rule: &Spacing<R>,
  source: &str,
  text: &Text,
  index: usize,
) -> Option<Diagnostic> {
  let message = rule.checker.before(At::new(text.text, index))?;
  let edits = text.edits(|base| fix_space_before(rule.expectation(), base, text.text, index));
  Some(rule.problem_fixed_by(source, message, text.report(source, index), edits))
}

/// A declaration as `decl.toString()` gives it: from the property to the
/// end of the value, `!important` included, the `;` (and a nested
/// property's block) left out.  postcss-scss writes the `//` comments of
/// the value as `/* */` ones, and leaves those before the value alone.
pub fn declaration_string<'t>(decl: DeclRef<'t>) -> Cow<'t, str> {
  let source = decl.tree().source();
  // PostCSS writes a Sass nested property's declaration without its block.
  let end = if decl.has_block() {
    decl.value_span.end
  } else {
    statement_end(decl.node())
  };
  let string = match decl.raw_value() {
    Cow::Borrowed(_) => Cow::Borrowed(&source[decl.prop_start()..end]),
    Cow::Owned(value) => Cow::Owned(format!(
      "{}{value}{}",
      &source[decl.prop_start()..decl.value_start()],
      &source[decl.value_span.end..end]
    )),
  };
  escape_html_in_css(string)
}

/// PostCSS's `escapeHTMLInCSS`, which its stringifier runs over what it
/// writes: `<style`, `</style` (in any case) and `<!--` get their `<`
/// written as `\3c `, so that the CSS cannot end an HTML `<style>`.
pub fn escape_html_in_css(text: Cow<'_, str>) -> Cow<'_, str> {
  let escaped = |at: usize| {
    let rest = &text.as_bytes()[at + 1..];
    if rest.starts_with(b"!--") {
      return true;
    }
    let tag = rest.strip_prefix(b"/").unwrap_or(rest);
    tag.len() >= 5
      && tag[..5].eq_ignore_ascii_case(b"style")
      && !tag
        .get(5)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
  };
  let opens: Vec<usize> = text
    .match_indices('<')
    .map(|(at, _)| at)
    .filter(|&at| escaped(at))
    .collect();
  if opens.is_empty() {
    return text;
  }
  let mut out = String::with_capacity(text.len() + 3 * opens.len());
  let mut last = 0;
  for at in opens {
    out.push_str(&text[last..at]);
    out.push_str("\\3c ");
    last = at + 1;
  }
  out.push_str(&text[last..]);
  Cow::Owned(out)
}

/// The plugin's `declarationValueIndex`: where the value starts in the
/// declaration's string.
pub fn declaration_value_index(decl: DeclRef) -> usize {
  decl.value_start().saturating_sub(decl.prop_start())
}

/// A node as `toString()` gives it: its text without the `;` that ends a
/// declaration or an at-rule without a block (the parent writes that).
/// postcss-scss keeps a `//` comment in a selector, params or value as a
/// `/* */` one, and PostCSS writes a `//` comment node in a block as one
/// too, while what separates the name from the params and the selector or
/// params from the block (`raws.afterName`, `raws.between`) stays as
/// written.  A Sass nested property's block is left out, as PostCSS
/// writes one, and PostCSS escapes what could end an HTML `<style>` in all
/// of it ([`escape_html_in_css`]).
pub fn node_string(node: NodeRef<'_>) -> Cow<'_, str> {
  let tree = node.tree();
  let end = statement_end(node);
  match node.kind {
    NodeKind::Decl => return declaration_string(node.as_decl().expect("a declaration")),
    // PostCSS writes a `//` comment (postcss-scss's or postcss-less's) as a
    // `/* */` one.
    NodeKind::Comment => {
      let text = tree.raw_text(node.start..end);
      return escape_html_in_css(match text {
        Cow::Borrowed(text) if node.inline => match text.strip_prefix("//") {
          Some(body) => Cow::Owned(format!("/*{body}*/")),
          None => Cow::Borrowed(text),
        },
        text => text,
      });
    }
    NodeKind::Rule | NodeKind::AtRule => {}
  }
  let text = &tree.source()[node.start..end];
  let nested = || {
    node
      .walk()
      .skip(1)
      .any(|n| n.kind == NodeKind::Decl && n.has_block())
  };
  if !text.contains("//") && !nested() {
    return escape_html_in_css(Cow::Borrowed(text));
  }
  let mut out = String::with_capacity(text.len() + 16);
  write_statement(node, &mut out);
  escape_html_in_css(Cow::Owned(out))
}

/// Write a rule's or at-rule's `toString()` to `out`: the selector or
/// params as postcss-scss keeps them, each child as it writes it, and the
/// rest as written.
fn write_statement(node: NodeRef<'_>, out: &mut String) {
  let tree = node.tree();
  let source = tree.source();
  let end = statement_end(node);
  let raw = match node.kind {
    NodeKind::Rule => node.name_span.clone(),
    _ => node.value_span.clone(),
  };
  let block = node.block_open.unwrap_or(end).max(raw.end).min(end);
  out.push_str(&source[node.start..raw.start]);
  out.push_str(&tree.raw_text(raw.clone()));
  out.push_str(&source[raw.end..block]);
  let mut cursor = block;
  for child in node.children() {
    out.push_str(&source[cursor.min(child.start)..child.start]);
    match child.kind {
      NodeKind::Rule | NodeKind::AtRule => write_statement(child, out),
      _ => out.push_str(&node_string(child)),
    }
    cursor = statement_end(child);
    // A Sass nested property's block is left out, with the `;` its parent
    // writes after any declaration but a last one without.
    if child.kind == NodeKind::Decl && child.has_block() {
      let last = node.children().rev().find(|n| n.kind != NodeKind::Comment);
      if last != Some(child) || node.semicolon {
        out.push(';');
      }
      cursor = child.end;
    }
  }
  out.push_str(&source[cursor.min(end)..end]);
}

/// Where a node's string ends: before the `;` that ends a declaration or
/// an at-rule without a block, past the whitespace and comments a custom
/// property's value takes in, and for an at-rule that the parent's
/// `}` ends instead, at that `}`, since PostCSS keeps the whitespace and
/// comments before it in the at-rule's `raws.between`.
pub fn statement_end(node: NodeRef<'_>) -> usize {
  if node.has_block() {
    return node.end;
  }
  if node.source_text().ends_with(';') {
    return node.end - 1;
  }
  // A custom property's value keeps the whitespace and comments after it.
  if node.kind == NodeKind::Decl {
    return node.end.max(node.value_span.end);
  }
  match node.parent().and_then(closing_brace) {
    Some(close) if node.kind == NodeKind::AtRule && node.next().is_none() => close,
    _ => node.end,
  }
}

/// Where the `}` that closes a statement's block is: last in its text but
/// for a `;` after it (`raws.ownSemicolon`).  `None` without a block, or
/// for one the file ends inside.
pub fn closing_brace(node: NodeRef<'_>) -> Option<usize> {
  if !node.has_block() {
    return None;
  }
  let text = node
    .source_text()
    .trim_end_matches(|c: char| c == ';' || c.is_ascii_whitespace());
  text.ends_with('}').then(|| node.start + text.len() - 1)
}

/// The plugin's `isStandardSyntaxAtRule`, which unlike Stylelint's own
/// keeps `@charset`: not a blockless at-rule without params, a Less mixin
/// or variable, or a blockless `@name(...)`.
pub fn is_standard_at_rule(at: AtRuleRef) -> bool {
  let blockless = !at.has_block();
  !(blockless && at.params().is_empty()
    || at.mixin
    || at.variable
    || blockless && at.after_name.is_empty() && at.params().starts_with('('))
}

/// The plugin's `beforeBlockString(statement, { noRawBefore: true })`: a
/// rule's selector or an at-rule's `@name`, `raws.afterName` and params, as
/// PostCSS stores them (comments removed), then `raws.between`, up to the
/// `{`.  `None` without a block.
pub fn before_block_string(node: NodeRef<'_>) -> Option<String> {
  let open = node.block_open?;
  let source = node.tree().source();
  match node.kind {
    NodeKind::Rule => Some(format!(
      "{}{}",
      node.name,
      source.get(node.name_span.end..open)?
    )),
    NodeKind::AtRule => {
      let params_end = if node.params.is_empty() {
        node.name_span.end + node.after_name.len()
      } else {
        node.value_span.end
      };
      Some(format!(
        "@{}{}{}{}",
        node.name,
        node.after_name,
        node.params,
        source.get(params_end..open)?
      ))
    }
    _ => None,
  }
}

/// The plugin's `blockString`: the statement's string from the length of
/// its `beforeBlockString` on, which is its block, `{` to `}`, unless
/// comments were taken out of the selector or params before it.
pub fn block_string(node: NodeRef<'_>) -> Cow<'_, str> {
  let Some(head) = before_block_string(node).map(|head| utf16_len(&head)) else {
    return Cow::Borrowed("");
  };
  match node_string(node) {
    Cow::Borrowed(text) => Cow::Borrowed(&text[utf16_offset(text, 0, head)..]),
    Cow::Owned(text) => Cow::Owned(text[utf16_offset(&text, 0, head)..].to_string()),
  }
}

/// A selector node as postcss-selector-parser's `toString()` gives it,
/// and where that starts in `selector`: the first node of a selector, or
/// one after a comment, takes the whitespace before it (`spaces.before`),
/// which its `sourceIndex` leaves out.
pub fn selector_node_string<'s>(selector: &'s str, visit: &Visit) -> (usize, &'s str) {
  let node = visit.node;
  let after_comment = visit.prev().is_some_and(|prev| prev.kind == Kind::Comment);
  let start = if visit.index == 0 || after_comment {
    selector[..node.start].trim_end_matches(is_js_space).len()
  } else {
    node.start
  };
  (start, &selector[start..node.end])
}

/// Where Stylelint reports index `index` of `text`, a string it built for
/// a node starting at source offset `start` (a selector, a declaration, a
/// node's `toString()`): PostCSS walks the source from `start` for as many
/// UTF-16 units as `text` has up to `index`.  Where `text` is the source as
/// written that is `start + index`; where it is not (comments rewritten or
/// removed) and characters outside ASCII follow, it is not.
pub fn index_in(source: &str, start: usize, text: &str, index: usize) -> usize {
  let within = index.min(text.len());
  let units = utf16_len(text.get(..within).unwrap_or(text)) + (index - within);
  utf16_offset(source, start, units)
}

/// Where Stylelint reports `back` UTF-16 units before the end of `text`
/// (`string.length - back`), a string it built for a node starting at
/// source offset `start` (see [`index_in`]).
pub fn from_end(source: &str, start: usize, text: &str, back: usize) -> usize {
  utf16_offset(source, start, utf16_len(text).saturating_sub(back))
}

/// How many UTF-16 units `text` takes: its length to JavaScript.
pub fn utf16_len(text: &str) -> usize {
  text.chars().map(char::len_utf16).sum()
}

/// The byte offset `units` UTF-16 units past byte `from` of `text`, as
/// JavaScript counts them: where an index into a string that is not the
/// source (a selector with its comments removed, say) lands when PostCSS
/// walks the source with it.  Past the end, each unit is one byte.
pub fn utf16_offset(text: &str, from: usize, units: usize) -> usize {
  let mut left = units;
  let mut offset = from;
  for c in text.get(from..).unwrap_or("").chars() {
    if left == 0 {
      return offset;
    }
    left = left.saturating_sub(c.len_utf16());
    offset += c.len_utf8();
  }
  offset + left
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn strings_escape_what_could_end_a_style_element() {
    let escaped = |text: &'static str| escape_html_in_css(Cow::Borrowed(text)).into_owned();
    assert_eq!(
      escaped("a </STYLE> <style2 <!-- b"),
      "a \\3c /STYLE> <style2 \\3c !-- b"
    );
    assert_eq!(escaped("a < b"), "a < b");
  }

  #[test]
  fn utf16_offsets_walk_the_text_as_javascript_counts_it() {
    let text = "a — b";
    assert_eq!(utf16_len(text), 5);
    assert_eq!(utf16_offset(text, 0, 4), "a — ".len());
    assert_eq!(utf16_offset(text, 2, 1), "a —".len());
    // Past the end, one byte a unit.
    assert_eq!(utf16_offset(text, 0, 7), text.len() + 2);
  }
}
