//! Finding the configuration comments of a style sheet, in document
//! order, the way Stylelint's walk meets them.

use gale_css_parser::Syntax;
use gale_css_parser::postcss_tree::{NodeKind, PostcssTree};

use super::{Directive, DirectiveNode, command, may_have_directives};

/// The configuration comments of the style sheet `tree` was parsed from,
/// as `syntax`, in document order, the way Stylelint's walk meets them.
pub(super) fn directives(
  tree: &PostcssTree,
  syntax: Syntax,
  base: usize,
  line_of: &dyn Fn(usize) -> usize,
) -> Vec<Directive> {
  let text = tree.source();
  let line = |offset: usize| line_of(base + offset);
  // The last end of a run of `//` comments that has been merged into the
  // command before it (Stylelint's `inlineEnd`).
  let mut merged_until: Option<usize> = None;
  let mut found = Vec::new();
  for i in 0..tree.nodes.len() {
    let node = &tree.nodes[i];
    match node.kind {
      NodeKind::Comment => {
        if let Some(last) = merged_until {
          if last == i {
            merged_until = None;
          }
          continue;
        }
        let (directive, merged) = comment_directive(tree, i, base, &line);
        merged_until = merged;
        found.push(directive);
      }
      NodeKind::Rule | NodeKind::AtRule | NodeKind::Decl => {
        let header_end = node.block_open.unwrap_or(node.end);
        let header_start = match node.kind {
          NodeKind::AtRule => node.start + 1 + node.name.len(),
          NodeKind::Decl if text[node.start..].starts_with(node.name.as_str()) => {
            node.start + node.name.len()
          }
          _ => node.start,
        };
        let owner = DirectiveNode::new(text, node.start, node.end, base);
        let header = header_start.min(header_end)..header_end;
        comments_in_node(tree, header, owner, syntax, &line, &mut found);
      }
    }
  }
  found
}

/// The comment node `i` as a directive, merged with the `//` comments on
/// the lines after it that carry on its description, and the last of
/// those merged comments, if any.
///
/// Stylelint works around postcss-scss#109 this way: a `//` command whose
/// text holds `--`, or whose next comment starts with `--`, takes in every
/// following `//` comment on the very next line that is not a command.
fn comment_directive(
  tree: &PostcssTree,
  i: usize,
  base: usize,
  line: &dyn Fn(usize) -> usize,
) -> (Directive, Option<usize>) {
  let node = &tree.nodes[i];
  let next_comment = tree
    .next(i)
    .filter(|&n| tree.nodes[n].kind == NodeKind::Comment);
  let merges = node.inline
    && command(&node.name).is_some()
    && next_comment
      .is_some_and(|n| node.name.contains("--") || tree.nodes[n].name.starts_with("--"));

  let mut text = node.name.clone();
  let mut end = node.end;
  let mut merged = None;
  if let Some(mut current) = next_comment.filter(|_| merges) {
    let mut last_line = line(last_char(tree.source(), node.start, end));
    while tree.nodes[current].inline && command(&tree.nodes[current].name).is_none() {
      let current_node = &tree.nodes[current];
      let current_line = line(last_char(
        tree.source(),
        current_node.start,
        current_node.end,
      ));
      if last_line + 1 != current_line {
        break;
      }
      text.push('\n');
      text.push_str(&current_node.name);
      end = current_node.end;
      merged = Some(current);
      match tree
        .next(current)
        .filter(|&n| tree.nodes[n].kind == NodeKind::Comment)
      {
        Some(next) => {
          current = next;
          last_line = current_line;
        }
        None => break,
      }
    }
  }
  let directive = Directive {
    text,
    node: DirectiveNode::new(tree.source(), node.start, end, base),
    start_line: line(node.start),
    end_line: line(last_char(tree.source(), node.start, end)),
  };
  (directive, merged)
}

/// The offset of the last character of `start..end` (PostCSS's
/// `source.end`), or `start` when it is empty.
pub(super) fn last_char(text: &str, start: usize, end: usize) -> usize {
  text
    .get(start..end)
    .and_then(|s| s.char_indices().next_back())
    .map_or(start, |(at, _)| start + at)
}

/// Stylelint's `checkCommentsInNode`: the comments in `header` of the
/// tree's source, a node's selector, params or value, each belonging to
/// `owner`, the node that holds them.  Block comments count everywhere,
/// `//` ones in SCSS only; strings and `url()` are skipped.
fn comments_in_node(
  tree: &PostcssTree,
  header: std::ops::Range<usize>,
  owner: DirectiveNode,
  syntax: Syntax,
  line: &dyn Fn(usize) -> usize,
  found: &mut Vec<Directive>,
) {
  let source = tree.source();
  let start = header.start;
  let Some(part) = source.get(header) else {
    return;
  };
  let inline_comments = matches!(syntax, Syntax::Scss | Syntax::Sass);
  if !may_have_directives(part) {
    return;
  }
  let bytes = part.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'"' | b'\'' => i = string_end(bytes, i),
      b'\\' => i += 2,
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        let Some(close) = part[i + 2..].find("*/") else {
          return;
        };
        let comment_end = i + 2 + close + 2;
        found.push(Directive {
          text: part[i + 2..comment_end - 2].trim().to_string(),
          node: owner,
          start_line: line(start + i),
          end_line: line(start + comment_end - 1),
        });
        i = comment_end;
      }
      b'/' if inline_comments && bytes.get(i + 1) == Some(&b'/') => {
        let comment_end = part[i..]
          .find(['\n', '\r', '\x0c'])
          .map_or(part.len(), |n| i + n);
        found.push(Directive {
          text: part[i + 2..comment_end].trim().to_string(),
          node: owner,
          start_line: line(start + i),
          end_line: line(last_char(part, i, comment_end) + start),
        });
        i = comment_end;
      }
      b'u' | b'U'
        if part[i..]
          .get(..4)
          .is_some_and(|s| s.eq_ignore_ascii_case("url(")) =>
      {
        let open = i + 4;
        let first = part[open..].trim_start();
        if first.starts_with(['"', '\'']) {
          i = open;
        } else {
          i = part[open..].find(')').map_or(part.len(), |n| open + n + 1);
        }
      }
      _ => i += 1,
    }
  }
}

/// The offset just past the string that opens at `start` (its closing
/// quote, or the line break or end of text that cuts it short).
fn string_end(bytes: &[u8], start: usize) -> usize {
  let quote = bytes[start];
  let mut i = start + 1;
  while i < bytes.len() {
    match bytes[i] {
      b'\\' => i += 2,
      b'\n' | b'\r' | b'\x0c' => return i,
      b if b == quote => return i + 1,
      _ => i += 1,
    }
  }
  bytes.len()
}
