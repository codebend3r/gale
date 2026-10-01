//! A port of `postcss-value-parser`, the tokenizer Stylelint rules use to
//! walk a declaration value or at-rule params.
//!
//! Rules that report on single words of a value (a container name, a
//! dashed ident) need to split it exactly the way Stylelint does, or they
//! test different words and report different columns.  [`parse`] follows
//! `postcss-value-parser` 4.2's `parse.js` step by step, quirks included,
//! with byte offsets in place of UTF-16 ones.

/// What a [`ValueNode`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
  /// A run of anything that is no other node: `red`, `10px`, `#fff`,
  /// `$var`, `#{$x}`.
  Word,
  /// A quoted string; [`ValueNode::value`] is its text without the quotes.
  String,
  /// A `/`, `,` or `:` divider.
  Div,
  /// Whitespace between two nodes.
  Space,
  /// A `/* ... */` comment; [`ValueNode::value`] is its text.
  Comment,
  /// `name(...)`, or a bare `(...)` with an empty name; its arguments are
  /// in [`ValueNode::nodes`].
  Function,
  /// A `U+0025-00FF` unicode range.
  UnicodeRange,
}

/// One node of a parsed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueNode<'a> {
  /// The node's kind.
  pub kind: NodeKind,
  /// The node's text: the word, the divider, the string or comment body,
  /// or the function name.
  pub value: &'a str,
  /// Byte offset of the node in the parsed text.  For a divider this is
  /// where the whitespace before it starts, as in `postcss-value-parser`.
  pub source_index: usize,
  /// Byte offset just past the node (`sourceEndIndex`).  A divider's end
  /// includes the whitespace after it.
  pub source_end_index: usize,
  /// The quote a string is written with.
  pub quote: Option<char>,
  /// Whitespace before a divider, or after a function's `(`.
  pub before: &'a str,
  /// Whitespace after a divider, or before a function's `)`.
  pub after: &'a str,
  /// Whether a string, comment or function runs to the end of the input.
  pub unclosed: bool,
  /// A function's arguments; empty for every other kind.
  pub nodes: Vec<ValueNode<'a>>,
}

impl ValueNode<'_> {
  /// Whether this is a word node.
  pub fn is_word(&self) -> bool {
    self.kind == NodeKind::Word
  }

  /// Whether this is a function node.
  pub fn is_function(&self) -> bool {
    self.kind == NodeKind::Function
  }

  /// Whether this is the `/` divider.
  pub fn is_slash(&self) -> bool {
    self.kind == NodeKind::Div && self.value == "/"
  }

  /// Whether this is a function named `name`, ignoring ASCII case.
  pub fn is_function_named(&self, name: &str) -> bool {
    self.is_function() && self.value.eq_ignore_ascii_case(name)
  }

  /// The node as text, as `postcss-value-parser`'s `stringify` prints it.
  pub fn to_css(&self) -> String {
    match self.kind {
      NodeKind::Word | NodeKind::Space | NodeKind::UnicodeRange => self.value.to_string(),
      NodeKind::String => {
        let quote = self.quote.map(String::from).unwrap_or_default();
        let close = if self.unclosed { "" } else { quote.as_str() };
        format!("{quote}{}{close}", self.value)
      }
      NodeKind::Comment => {
        let close = if self.unclosed { "" } else { "*/" };
        format!("/*{}{close}", self.value)
      }
      NodeKind::Div => format!("{}{}{}", self.before, self.value, self.after),
      NodeKind::Function => {
        let close = if self.unclosed { "" } else { ")" };
        format!(
          "{}({}{}{}{close}",
          self.value,
          self.before,
          stringify(&self.nodes),
          self.after
        )
      }
    }
  }
}

/// `nodes` as text, as `postcss-value-parser`'s `stringify` prints them.
pub fn stringify(nodes: &[ValueNode<'_>]) -> String {
  nodes.iter().map(ValueNode::to_css).collect()
}

/// What the parser's `parent` variable holds: nothing before the first
/// `)`, the root after closing a top-level function, or the function whose
/// arguments are being read.  The difference between the first two decides
/// whether whitespace before a top-level `/` becomes a space node.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Parent<'a> {
  /// No parent yet.
  Unset,
  /// The root (back at the top level after a function).
  Root,
  /// Inside the function of this name.
  Function(&'a str),
}

impl Parent<'_> {
  /// Whether the parent is the function `calc`.
  fn is_calc(self) -> bool {
    self == Parent::Function("calc")
  }

  /// Whether the parent is any function.
  fn is_function(self) -> bool {
    matches!(self, Parent::Function(_))
  }
}

/// Parse `input` into nodes, as `postcss-value-parser`'s `parse` does.
pub fn parse(input: &str) -> Vec<ValueNode<'_>> {
  let bytes = input.as_bytes();
  let max = bytes.len();
  let at = |i: usize| bytes.get(i).copied();
  // `stack[0]` holds the root's nodes; each open function pushes its own
  // node, whose arguments collect in `nodes`.
  let mut stack: Vec<ValueNode<'_>> = vec![leaf(NodeKind::Function, "", 0, 0)];
  let mut parent = Parent::Unset;
  let mut name = "";
  let mut before_len = 0usize;
  // Whitespace before the `)` about to close a function.
  let mut after = "";
  let mut pos = 0usize;

  while pos < max {
    let code = bytes[pos];
    let balanced = stack.len() > 1;
    if code <= b' ' {
      // Whitespace.
      let mut next = pos;
      while next < max && bytes[next] <= b' ' {
        next += 1;
      }
      let following = at(next);
      let after_div = stack
        .last()
        .and_then(|frame| frame.nodes.last())
        .is_some_and(|n| n.kind == NodeKind::Div);
      if following == Some(b')') && balanced {
        // Kept as the function's `after`.
        after = &input[pos..next];
      } else if after_div {
        // Kept as the divider's `after`.
        if let Some(div) = stack.last_mut().and_then(|frame| frame.nodes.last_mut()) {
          div.after = &input[pos..next];
          div.source_end_index = next;
        }
      } else if following == Some(b',')
        || following == Some(b':')
        || (following == Some(b'/')
          && at(next + 1) != Some(b'*')
          && (parent == Parent::Unset || (parent.is_function() && !parent.is_calc())))
      {
        before_len = next - pos;
      } else {
        push(
          &mut stack,
          leaf(NodeKind::Space, &input[pos..next], pos, next),
        );
      }
      pos = next;
    } else if code == b'\'' || code == b'"' {
      // A string, up to the first unescaped matching quote.
      let mut next = pos;
      let close = loop {
        match input[next + 1..].find(code as char) {
          Some(offset) => {
            next = next + 1 + offset;
            let backslashes = bytes[pos + 1..next]
              .iter()
              .rev()
              .take_while(|&&b| b == b'\\')
              .count();
            if backslashes % 2 == 0 {
              break Some(next);
            }
          }
          None => break None,
        }
      };
      let end = close.unwrap_or(max);
      let mut string = leaf(
        NodeKind::String,
        &input[pos + 1..end],
        pos,
        close.map_or(max, |c| c + 1),
      );
      string.quote = Some(code as char);
      string.unclosed = close.is_none();
      push(&mut stack, string);
      pos = close.map_or(max, |c| c + 1);
    } else if code == b'/' && at(pos + 1) == Some(b'*') {
      // A comment.  The search for `*/` starts at the `*`, so `/*/`
      // closes at once, as in `postcss-value-parser`.
      let close = input[pos + 1..].find("*/").map(|offset| pos + 1 + offset);
      let end = close.unwrap_or(max);
      let text = input.get(pos + 2..end).unwrap_or("");
      let mut comment = leaf(NodeKind::Comment, text, pos, close.map_or(max, |c| c + 2));
      comment.unclosed = close.is_none();
      push(&mut stack, comment);
      pos = close.map_or(max, |c| c + 2);
    } else if (code == b'/' || code == b'*') && parent.is_calc() {
      // An operator inside `calc()`.
      push(
        &mut stack,
        leaf(
          NodeKind::Word,
          &input[pos..pos + 1],
          pos - before_len,
          pos + 1,
        ),
      );
      pos += 1;
    } else if code == b'/' || code == b',' || code == b':' {
      // A divider; whitespace before it was saved in `before_len`.
      let mut div = leaf(
        NodeKind::Div,
        &input[pos..pos + 1],
        pos - before_len,
        pos + 1,
      );
      div.before = &input[pos - before_len..pos];
      push(&mut stack, div);
      before_len = 0;
      pos += 1;
    } else if code == b'(' {
      let open = pos;
      let mut next = pos + 1;
      while next < max && bytes[next] <= b' ' {
        next += 1;
      }
      let function_start = open - name.len();
      pos = next;
      if name == "url" && at(next) != Some(b'\'') && at(next) != Some(b'"') {
        // `url(` with an unquoted argument: everything up to the first
        // unescaped `)` is one word.
        let mut close = None;
        let mut search = next.saturating_sub(1);
        while let Some(offset) = input.get(search + 1..).and_then(|rest| rest.find(')')) {
          let found = search + 1 + offset;
          let backslashes = bytes[..found]
            .iter()
            .rev()
            .take_while(|&&b| b == b'\\')
            .count();
          if backslashes % 2 == 0 {
            close = Some(found);
            break;
          }
          search = found;
        }
        let end = close.unwrap_or(max);
        let mut content_end = end;
        while content_end > open + 1 && bytes[content_end - 1] <= b' ' {
          content_end -= 1;
        }
        let mut function = leaf(
          NodeKind::Function,
          name,
          function_start,
          close.map_or(max, |c| c + 1),
        );
        function.before = &input[open + 1..next];
        function.unclosed = close.is_none();
        if content_end > pos {
          function.nodes.push(leaf(
            NodeKind::Word,
            &input[pos..content_end],
            pos,
            content_end,
          ));
        }
        if close.is_none() && content_end < end {
          function.nodes.push(leaf(
            NodeKind::Space,
            &input[content_end..end],
            content_end,
            end,
          ));
        } else if content_end > open + 1 {
          function.after = &input[content_end..end];
        }
        push(&mut stack, function);
        pos = close.map_or(max, |c| c + 1);
      } else {
        let mut function = leaf(NodeKind::Function, name, function_start, open + 1);
        function.before = &input[open + 1..next];
        stack.push(function);
        parent = Parent::Function(name);
      }
      name = "";
    } else if code == b')' && balanced {
      pos += 1;
      let mut done = stack.pop().expect("open function");
      done.source_end_index = pos;
      done.after = std::mem::take(&mut after);
      stack.last_mut().expect("root frame").nodes.push(done);
      parent = match stack.last() {
        Some(frame) if stack.len() > 1 => Parent::Function(frame.value),
        _ => Parent::Root,
      };
    } else {
      // A word: up to whitespace, a quote, a divider, `(`, or a `)` that
      // closes a function.
      let mut next = pos;
      loop {
        if bytes[next] == b'\\' {
          next += 1;
        }
        next += 1;
        if next >= max {
          break;
        }
        let c = bytes[next];
        let ends = c <= b' '
          || c == b'\''
          || c == b'"'
          || c == b','
          || c == b':'
          || c == b'/'
          || c == b'('
          || (c == b'*' && parent.is_calc())
          || (c == b')' && balanced);
        if ends {
          break;
        }
      }
      let next = next.min(max);
      let end = floor_boundary(input, next);
      let token = &input[pos..end];
      if at(next) == Some(b'(') {
        name = token;
      } else if is_unicode_range(token) {
        push(&mut stack, leaf(NodeKind::UnicodeRange, token, pos, end));
      } else {
        push(&mut stack, leaf(NodeKind::Word, token, pos, end));
      }
      pos = end.max(pos + 1);
    }
  }

  // Close any functions left open.
  while stack.len() > 1 {
    let mut done = stack.pop().expect("open function");
    done.unclosed = true;
    done.source_end_index = max;
    stack.last_mut().expect("root frame").nodes.push(done);
  }
  stack.pop().map(|root| root.nodes).unwrap_or_default()
}

/// Call `visit` on every node, depth first.  Like `postcss-value-parser`'s
/// `walk`, a function's arguments are skipped when `visit` returns `false`
/// for it.
pub fn walk<'a>(nodes: &[ValueNode<'a>], visit: &mut impl FnMut(&ValueNode<'a>) -> bool) {
  for node in nodes {
    if visit(node) && node.is_function() {
      walk(&node.nodes, visit);
    }
  }
}

/// Add `node` to the innermost open function, or the root.
fn push<'a>(stack: &mut [ValueNode<'a>], node: ValueNode<'a>) {
  if let Some(frame) = stack.last_mut() {
    frame.nodes.push(node);
  }
}

/// A node spanning `source_index..source_end_index`, without children or
/// any of the optional parts.
fn leaf(
  kind: NodeKind,
  value: &str,
  source_index: usize,
  source_end_index: usize,
) -> ValueNode<'_> {
  ValueNode {
    kind,
    value,
    source_index,
    source_end_index,
    quote: None,
    before: "",
    after: "",
    unclosed: false,
    nodes: Vec::new(),
  }
}

/// `index`, moved back to a character boundary of `text` (an escape can
/// step into a multibyte character).
fn floor_boundary(text: &str, index: usize) -> usize {
  let mut index = index.min(text.len());
  while !text.is_char_boundary(index) {
    index -= 1;
  }
  index
}

/// Whether `token` is a unicode range: `u+` or `U+` and then hex digits,
/// `?` or `-`.
fn is_unicode_range(token: &str) -> bool {
  let bytes = token.as_bytes();
  bytes.len() > 2
    && (bytes[0] == b'u' || bytes[0] == b'U')
    && bytes[1] == b'+'
    && bytes[2..]
      .iter()
      .all(|b| b.is_ascii_hexdigit() || *b == b'?' || *b == b'-')
}

#[cfg(test)]
mod tests {
  use super::*;

  /// `(kind, value, source_index)` for each top-level node.
  fn top(input: &str) -> Vec<(NodeKind, &str, usize)> {
    parse(input)
      .into_iter()
      .map(|n| (n.kind, n.value, n.source_index))
      .collect()
  }

  #[test]
  fn splits_words_spaces_and_dividers() {
    use NodeKind::*;
    assert_eq!(
      top("a b, c / d"),
      vec![
        (Word, "a", 0),
        (Space, " ", 1),
        (Word, "b", 2),
        (Div, ",", 3),
        (Word, "c", 5),
        (Div, "/", 6),
        (Word, "d", 9),
      ]
    );
  }

  #[test]
  fn functions_hold_their_arguments() {
    let nodes = parse("var(--x, 1px) rgba(0 0 0 / 50%)");
    assert_eq!(nodes[0].kind, NodeKind::Function);
    assert_eq!(nodes[0].value, "var");
    assert_eq!(nodes[0].source_index, 0);
    let args: Vec<&str> = nodes[0].nodes.iter().map(|n| n.value).collect();
    assert_eq!(args, vec!["--x", ",", "1px"]);
    assert_eq!(nodes[0].nodes[0].source_index, 4);
    assert_eq!(nodes[2].value, "rgba");
    assert_eq!(nodes[2].source_index, 14);
  }

  #[test]
  fn strings_comments_urls_and_ranges() {
    use NodeKind::*;
    assert_eq!(
      top("'a b' /* c */ url(x y.png) U+0-7F"),
      vec![
        (String, "a b", 0),
        (Space, " ", 5),
        (Comment, " c ", 6),
        (Space, " ", 13),
        (Function, "url", 14),
        (Space, " ", 26),
        (UnicodeRange, "U+0-7F", 27),
      ]
    );
    assert_eq!(parse("url(x y.png)")[0].nodes[0].value, "x y.png");
  }

  #[test]
  fn interpolation_stays_in_one_word() {
    use NodeKind::*;
    assert_eq!(
      top("#{$p}contain-viewport #{$p}contain-table"),
      vec![
        (Word, "#{$p}contain-viewport", 0),
        (Space, " ", 21),
        (Word, "#{$p}contain-table", 22),
      ]
    );
  }

  #[test]
  fn calc_operators_are_words() {
    let nodes = parse("calc(1px*2)");
    let args: Vec<(NodeKind, &str)> = nodes[0].nodes.iter().map(|n| (n.kind, n.value)).collect();
    assert_eq!(
      args,
      vec![
        (NodeKind::Word, "1px"),
        (NodeKind::Word, "*"),
        (NodeKind::Word, "2")
      ]
    );
  }

  #[test]
  fn walk_skips_arguments_when_told_to() {
    let nodes = parse("a f(b) c");
    let mut all = Vec::new();
    walk(&nodes, &mut |n| {
      all.push(n.value);
      true
    });
    assert_eq!(all, vec!["a", " ", "f", "b", " ", "c"]);
    let mut words = Vec::new();
    walk(&nodes, &mut |n| {
      words.push(n.value);
      !n.is_function()
    });
    assert_eq!(words, vec!["a", " ", "f", " ", "c"]);
  }

  #[test]
  fn records_ends_quotes_and_spacing() {
    let nodes = parse("'a' , f( x ) url( y )");
    assert_eq!((nodes[0].source_index, nodes[0].source_end_index), (0, 3));
    assert_eq!(nodes[0].quote, Some('\''));
    assert_eq!(nodes[1].kind, NodeKind::Div);
    assert_eq!((nodes[1].before, nodes[1].after), (" ", " "));
    assert_eq!((nodes[1].source_index, nodes[1].source_end_index), (3, 6));
    let f = &nodes[2];
    assert_eq!((f.before, f.after), (" ", " "));
    assert_eq!((f.source_index, f.source_end_index), (6, 12));
    let url = &nodes[4];
    assert_eq!((url.before, url.after), (" ", " "));
    assert_eq!(url.source_end_index, 21);
    assert!(parse("f(a")[0].unclosed);
    assert!(parse("'a")[0].unclosed);
  }

  #[test]
  fn stringify_round_trips() {
    for input in [
      "1px solid red",
      "a , b / c",
      "url( x.png )",
      "fn( a, \"b\" ) /* c */",
      "calc(1px + 2px)",
      "\"unclosed",
      "f(unclosed",
      "url(unclosed ",
    ] {
      assert_eq!(stringify(&parse(input)), input, "{input}");
    }
  }

  #[test]
  fn unclosed_and_multibyte_input_does_not_panic() {
    for input in ["'abc", "f(a", "url(a", "/* x", "a\\é b", "é(ü) 中", "a\\"] {
      let _ = parse(input);
    }
  }
}
