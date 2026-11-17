//! SCSS function calls, `@import` extensions and `@include`d mixins.

use super::*;

/// The functions called in declaration values and at-rule params, without
/// any Sass module namespace (`math.random` is `random`), skipping native
/// CSS functions and the mixin or function an `@include`, `@mixin` or
/// `@function` names: `scss/function-disallowed-list`.  Each is reported
/// where its name first occurs in the statement, as Stylelint's `word`
/// option places it.
pub fn scss_function_names(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in tree.nodes.iter().enumerate() {
    let (value, skip_first) = match node.kind {
      NodeKind::Decl => (node.value.as_str(), false),
      NodeKind::AtRule => (
        node.params.as_str(),
        ["function", "include", "mixin"]
          .iter()
          .any(|name| node.name.eq_ignore_ascii_case(name)),
      ),
      _ => continue,
    };
    let mut skipped = !skip_first;
    let text = tree.text(i);
    value_parser::walk(&value_parser::parse(value), &mut |function| {
      if !function.is_function() {
        return true;
      }
      if !skipped {
        skipped = true;
        return true;
      }
      if function.value.is_empty() || data::is_native_css_function(function.value) {
        return true;
      }
      let name = function
        .value
        .find('.')
        .map_or(function.value, |dot| &function.value[dot + 1..]);
      let start = node.start + text.find(name).unwrap_or(0);
      found.push(Found::new(name, i, start, start + name.len()));
      true
    });
  }
}

/// The extension of every partial an `@import` names, as written and
/// without the dot, skipping CSS imports, URLs and media queries:
/// `scss/at-import-partial-extension-disallowed-list`.  Each is reported
/// where it first occurs in the at-rule.
pub fn import_extensions(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::AtRule) {
    if node.name != "import" {
      continue;
    }
    for path in node.params.split(',') {
      let Some(extension) = partial_extension(path) else {
        continue;
      };
      let start = node.start + tree.text(i).find(extension).unwrap_or(0);
      let partial = strip_quote(strip_quote(path, true), false);
      found.push(Found::new(extension, i, start, start + extension.len()).with_detail(partial));
    }
  }
}

/// The extension of one `@import` path, unless it is a CSS import, a URL or
/// carries a media query.
pub(super) fn partial_extension(path: &str) -> Option<&str> {
  // `/^\s*(["'])\s*/` and `/\s*(["'])\s*$/`: a quote and the whitespace
  // around it, at each end.
  let path = strip_quote(path, true);
  let path = strip_quote(path, false);
  let extension = extension_of(path)?;
  // `/[\s,)"']\w+$/`: a media query after the path.
  let media_query = path
    .rfind(|c: char| c.is_whitespace() || matches!(c, ',' | ')' | '"' | '\''))
    .is_some_and(|at| {
      let word = &path[at + 1..];
      !word.is_empty() && word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    });
  if path.starts_with("url(") || path.ends_with(".css") || path.contains("//") || media_query {
    return None;
  }
  Some(extension)
}

/// `text` without a quote and the whitespace around it at its start (or
/// end); `text` itself when there is no quote there.
fn strip_quote(text: &str, start: bool) -> &str {
  if start {
    match text.trim_start().strip_prefix(['"', '\'']) {
      Some(rest) => rest.trim_start(),
      None => text,
    }
  } else {
    match text.trim_end().strip_suffix(['"', '\'']) {
      Some(rest) => rest.trim_end(),
      None => text,
    }
  }
}

/// Node's `path.extname(path).slice(1)`: what follows the last `.` of the
/// last path segment, unless that `.` starts the segment; `None` when
/// that is empty.
fn extension_of(path: &str) -> Option<&str> {
  let base = path.rsplit('/').next().unwrap_or(path);
  let dot = base.rfind('.')?;
  let extension = &base[dot + 1..];
  (dot > 0 && !extension.is_empty()).then_some(extension)
}

/// The mixin every SCSS `@include` names: `scss/at-mixin-disallowed-list`.
pub fn included_mixins(tree: &PostcssTree, syntax: Syntax, found: &mut Vec<Found>) {
  if !matches!(syntax, Syntax::Scss | Syntax::Sass) {
    return;
  }
  for (i, node) in nodes_of(tree, NodeKind::AtRule) {
    if node.name != "include" {
      continue;
    }
    let params = node.params.trim();
    let name = params
      .find('(')
      .map_or(params, |open| params[..open].trim());
    if !name.is_empty() {
      found.push(Found::new(name, i, node.start, node.end));
    }
  }
}
