//! How `@stylistic/indentation` indents an embedded root: the base indent
//! level the plugin infers from the markup around the sheet, and the
//! indent width it infers from the whole host document.

use super::{EmbeddedRoot, leading_blanks};

/// How `@stylistic/indentation` indents an embedded root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedIndentation {
  /// The indent level of the root's top-level nodes.
  pub base_level: usize,
  /// Whether the first line is checked too: only when the sheet starts on a
  /// line of its own, after the line holding its `<style>` tag.
  pub check_first_line: bool,
}

/// The indentation `@stylistic/indentation` expects of `root`, the
/// embedded root being linted.
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
pub(super) fn infer_doc_indent_size(source: &str) -> Option<usize> {
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
