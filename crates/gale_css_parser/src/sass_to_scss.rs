/// Converts Sass indented syntax to SCSS so that the existing raffia parser can
/// handle it.  This is intentionally a "good-enough" mechanical transformation:
///
/// * Indentation changes → `{` / `}`
/// * Property lines (containing `:`) get a trailing `;`
/// * `=name` → `@mixin name`
/// * `+name` → `@include name`
/// * Comments (`//` and `/* */`) are preserved as-is
/// * Blank lines are passed through
///
/// Byte offsets in the resulting SCSS differ from the original Sass; use
/// [`convert_sass_to_scss_with_map`] to translate them back.
#[cfg(test)]
pub fn convert_sass_to_scss(input: &str) -> String {
  convert_sass_to_scss_with_map(input).0
}

/// [`convert_sass_to_scss`], plus a [`SourceMap`] from offsets in the SCSS
/// back to offsets in `input`.
///
/// Rules lint the SCSS, so every span they report points into it.  The map
/// lets the runner report those spans against the file the author wrote.
pub fn convert_sass_to_scss_with_map(input: &str) -> (String, SourceMap) {
  let lines: Vec<&str> = input.lines().collect();
  let mut out = String::with_capacity(input.len() * 2);
  let mut map = SourceMap::default();

  // Byte offset in `input` where each of `lines` starts.  `lines()` drops
  // the `\n` (and a `\r` before it), so walk the newline-inclusive pieces.
  let line_starts: Vec<usize> = input
    .split_inclusive('\n')
    .scan(0usize, |offset, piece| {
      let start = *offset;
      *offset += piece.len();
      Some(start)
    })
    .collect();

  // Where generated closing braces point: the end of the last line that
  // carried real content.
  let mut last_content_end = 0usize;

  // We track a stack of indentation levels.  Each entry is the column-width
  // of the indentation at that nesting depth.
  let mut indent_stack: Vec<usize> = Vec::new();

  // Detect whether the file uses tabs or spaces (whichever appears first).
  // We only need this to measure indent width consistently.
  let indent_unit = detect_indent_unit(input);

  let mut inside_block_comment = false;

  for (i, raw_line) in lines.iter().enumerate() {
    let line_start = line_starts.get(i).copied().unwrap_or(input.len());
    // ── Block comments ────────────────────────────────────────────
    if inside_block_comment {
      map.copied(out.len(), out.len(), line_start, raw_line.len(), 0, 0);
      out.push_str(raw_line);
      out.push('\n');
      last_content_end = line_start + raw_line.len();
      if raw_line.contains("*/") {
        inside_block_comment = false;
      }
      continue;
    }
    if raw_line.trim_start().starts_with("/*") && !raw_line.contains("*/") {
      inside_block_comment = true;
      map.copied(out.len(), out.len(), line_start, raw_line.len(), 0, 0);
      out.push_str(raw_line);
      out.push('\n');
      last_content_end = line_start + raw_line.len();
      continue;
    }

    // ── Blank / whitespace-only lines ─────────────────────────────
    let trimmed = raw_line.trim();
    if trimmed.is_empty() {
      map.copied(out.len(), out.len(), line_start, 0, 0, 0);
      out.push('\n');
      continue;
    }
    // Where `trimmed` starts and ends in `input`.
    let text_start = line_start + (raw_line.len() - raw_line.trim_start().len());
    let text_end = text_start + trimmed.len();

    // ── Measure indentation ───────────────────────────────────────
    let indent = measure_indent(raw_line, indent_unit);

    // Close blocks whose indentation we have left.
    while let Some(&top) = indent_stack.last() {
      if indent <= top {
        indent_stack.pop();
        map.generated(out.len(), last_content_end);
        push_indent(&mut out, indent_stack.len(), indent_unit);
        out.push_str("}\n");
      } else {
        break;
      }
    }
    last_content_end = text_end;

    // ── Line-level transformations ────────────────────────────────

    // Pure line comment — pass through.  Inline `/* ... */` single-line
    // comment — pass through.
    if trimmed.starts_with("//") || (trimmed.starts_with("/*") && trimmed.contains("*/")) {
      let line_out = out.len();
      push_indent(&mut out, indent_stack.len(), indent_unit);
      map.copied(line_out, out.len(), text_start, trimmed.len(), 0, 0);
      out.push_str(trimmed);
      out.push('\n');
      continue;
    }

    // Sass shorthand: `=mixin-name(...)` → `@mixin mixin-name(...)`, and
    // `+mixin-name` → `@include mixin-name`.  `prefix` is how many bytes
    // of the original the rewritten prefix replaces.
    let (line, prefix) = if let Some(rest) = trimmed.strip_prefix('=') {
      (format!("@mixin {rest}"), 1)
    } else if let Some(rest) = trimmed.strip_prefix('+')
      && !trimmed.starts_with("+-")
    {
      (format!("@include {rest}"), 1)
    } else {
      (trimmed.to_string(), 0)
    };
    let prefix_out = line.len() - (trimmed.len() - prefix);

    // Determine whether this line starts a new block.  A block-opener is
    // any line followed by a more-indented line (unless it is a property
    // declaration, which can also contain nested blocks in Sass, but the
    // most common pattern is selectors / at-rules).
    let next_indent = next_non_empty_indent(&lines, i + 1, indent_unit);
    let opens_block = next_indent.is_some_and(|ni| ni > indent);

    let line_out = out.len();
    push_indent(&mut out, indent_stack.len(), indent_unit);
    map.copied(
      line_out,
      out.len(),
      text_start,
      trimmed.len(),
      prefix_out,
      prefix,
    );

    if opens_block {
      out.push_str(&line);
      // If the line is *also* a declaration (e.g. `font:` in Sass can
      // open a value block) we do NOT add a semicolon — we add a brace.
      out.push_str(" {\n");
      indent_stack.push(indent);
    } else {
      out.push_str(&line);
      // Add semicolons for declarations / @-rules that don't open blocks.
      if needs_semicolon(&line) {
        out.push(';');
      }
      out.push('\n');
    }
  }

  // Close any remaining open blocks.
  while indent_stack.pop().is_some() {
    map.generated(out.len(), last_content_end);
    push_indent(&mut out, indent_stack.len(), indent_unit);
    out.push_str("}\n");
  }

  (out, map)
}

/// Maps byte offsets in converted SCSS back to the Sass it was converted
/// from.
///
/// The conversion is line by line: each output line either copies one input
/// line's text (re-indented, perhaps with a rewritten `=`/`+` prefix and a
/// trailing `;` or ` {`), or is a generated closing brace.  Offsets inside
/// copied text map to the same character in the input; offsets in added
/// indentation, prefixes or punctuation map to the nearest edge of the copied
/// text; generated lines map to the end of the content they close.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMap {
  /// One entry per output line, in output order.
  lines: Vec<LineMapping>,
}

/// How one output line relates to the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineMapping {
  /// Output offset where the line starts.
  out_start: usize,
  /// Output offset where the copied text starts, after the indentation.
  out_text: usize,
  /// Input offset of the copied text.
  in_text: usize,
  /// Length in bytes of the copied text in the input.
  in_len: usize,
  /// Bytes of output the rewritten prefix takes up (`@mixin `, say).
  prefix_out: usize,
  /// Bytes of input the rewritten prefix replaced (`=`).
  prefix_in: usize,
}

impl SourceMap {
  /// Record an output line, starting at `out_start`, whose text from
  /// `out_text` copies `in_len` bytes of input starting at `in_text`.
  fn copied(
    &mut self,
    out_start: usize,
    out_text: usize,
    in_text: usize,
    in_len: usize,
    prefix_out: usize,
    prefix_in: usize,
  ) {
    self.lines.push(LineMapping {
      out_start,
      out_text,
      in_text,
      in_len,
      prefix_out,
      prefix_in,
    });
  }

  /// Record a generated output line starting at `out_start` whose every
  /// offset maps to `anchor` in the input.
  fn generated(&mut self, out_start: usize, anchor: usize) {
    self.copied(out_start, out_start, anchor, 0, 0, 0);
  }

  /// The input offset that output offset `offset` came from.
  ///
  /// Always lands within the copied text of the line (or on its anchor), so
  /// the result is a character boundary in the input whenever `offset` is
  /// one in the output.
  pub fn to_original(&self, offset: usize) -> usize {
    let index = match self.lines.binary_search_by(|l| l.out_start.cmp(&offset)) {
      Ok(exact) => exact,
      Err(0) => return 0,
      Err(after) => after - 1,
    };
    let line = self.lines[index];
    let Some(into_text) = offset.checked_sub(line.out_text) else {
      return line.in_text;
    };
    let Some(past_prefix) = into_text.checked_sub(line.prefix_out) else {
      return line.in_text;
    };
    line.in_text + (line.prefix_in + past_prefix).min(line.in_len)
  }
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Detect whether the file predominantly uses tabs or spaces for indentation.
/// Returns the width of one indent unit in columns (1 for tabs, N for spaces).
fn detect_indent_unit(source: &str) -> IndentUnit {
  for line in source.lines() {
    if line.starts_with('\t') {
      return IndentUnit::Tab;
    }
    let spaces = line.len() - line.trim_start_matches(' ').len();
    if spaces > 0 {
      return IndentUnit::Spaces(spaces);
    }
  }
  IndentUnit::Spaces(2) // default
}

#[derive(Debug, Clone, Copy)]
enum IndentUnit {
  Tab,
  Spaces(usize),
}

/// Measure indentation depth (0-based nesting level) of a line.
fn measure_indent(line: &str, unit: IndentUnit) -> usize {
  match unit {
    IndentUnit::Tab => line.len() - line.trim_start_matches('\t').len(),
    IndentUnit::Spaces(w) => {
      let spaces = line.len() - line.trim_start_matches(' ').len();
      spaces.checked_div(w).unwrap_or(0)
    }
  }
}

/// Find the indentation depth of the next non-empty line starting from `start`.
fn next_non_empty_indent(lines: &[&str], start: usize, unit: IndentUnit) -> Option<usize> {
  for line in lines.iter().skip(start) {
    if !line.trim().is_empty() {
      return Some(measure_indent(line, unit));
    }
  }
  None
}

/// Emit `depth` levels of indentation.
fn push_indent(out: &mut String, depth: usize, unit: IndentUnit) {
  match unit {
    IndentUnit::Tab => {
      for _ in 0..depth {
        out.push('\t');
      }
    }
    IndentUnit::Spaces(w) => {
      for _ in 0..depth * w {
        out.push(' ');
      }
    }
  }
}

/// Determine whether a (trimmed) line should receive a trailing `;`.
fn needs_semicolon(line: &str) -> bool {
  // Skip pure comments.
  if line.starts_with("//") || line.starts_with("/*") {
    return false;
  }
  // Skip blank.
  if line.is_empty() {
    return false;
  }
  // At-rules that don't take a block but ARE complete statements.
  if line.starts_with("@import")
    || line.starts_with("@use")
    || line.starts_with("@forward")
    || line.starts_with("@include")
    || line.starts_with("@extend")
    || line.starts_with("@warn")
    || line.starts_with("@debug")
    || line.starts_with("@error")
    || line.starts_with("@return")
  {
    return true;
  }
  // At-rules that open blocks (@media, @mixin, @if, etc.) — no semicolon.
  if line.starts_with('@') {
    return false;
  }
  // Property declarations contain `:` (but selectors like `&:hover` do too).
  // Heuristic: if it contains `:` and what comes after `:` looks like a value
  // (not a pseudo-class), it's a declaration.
  if let Some(colon_pos) = line.find(':') {
    let after = &line[colon_pos + 1..];
    // Pseudo-selectors: `:hover`, `:focus`, `::before`, `:nth-child(…)`
    // These start immediately with an alphabetic character or another `:`.
    let first_non_space = after.trim_start().chars().next();
    if let Some(ch) = first_non_space {
      // If the character after the colon (ignoring spaces) is alphabetic
      // and the part before the colon looks like a property name (no
      // selector-specific chars like `.`, `#`, `&`, `>`), treat it as a
      // declaration.
      let before = &line[..colon_pos];
      let looks_like_selector = before.contains('&')
        || before.contains('.')
        || before.contains('#')
        || before.contains('>')
        || before.contains('~')
        || before.contains('+')
        || before.contains('[');
      if looks_like_selector {
        // It's a selector with a pseudo-class, no semicolon.
        return false;
      }
      // If the value side starts with `:` it's `::before` etc.
      if ch == ':' {
        return false;
      }
      return true;
    }
  }
  false
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn basic_rule() {
    let sass = "\
.container
  display: flex
  color: red
";
    let scss = convert_sass_to_scss(sass);
    assert!(scss.contains(".container {"), "should open block: {scss}");
    assert!(
      scss.contains("display: flex;"),
      "should add semicolon: {scss}"
    );
    assert!(scss.contains("color: red;"), "should add semicolon: {scss}");
    assert!(scss.contains('}'), "should close block: {scss}");
  }

  #[test]
  fn nested_rules() {
    let sass = "\
.parent
  color: blue
  .child
    color: red
";
    let scss = convert_sass_to_scss(sass);
    assert!(scss.contains(".parent {"), "parent opens block: {scss}");
    assert!(scss.contains(".child {"), "child opens block: {scss}");
    // Should have two closing braces.
    assert_eq!(scss.matches('}').count(), 2, "two closing braces: {scss}");
  }

  #[test]
  fn mixin_shorthand() {
    let sass = "\
=border-radius($r)
  border-radius: $r

.box
  +border-radius(5px)
";
    let scss = convert_sass_to_scss(sass);
    assert!(
      scss.contains("@mixin border-radius($r)"),
      "= → @mixin: {scss}"
    );
    assert!(
      scss.contains("@include border-radius(5px)"),
      "+ → @include: {scss}"
    );
  }

  #[test]
  fn comments_preserved() {
    let sass = "\
// line comment
.a
  /* block comment */
  color: red
";
    let scss = convert_sass_to_scss(sass);
    assert!(
      scss.contains("// line comment"),
      "line comment kept: {scss}"
    );
    assert!(
      scss.contains("/* block comment */"),
      "block comment kept: {scss}"
    );
  }

  #[test]
  fn at_rules() {
    let sass = "\
@import 'variables'

@media (min-width: 768px)
  .container
    width: 750px
";
    let scss = convert_sass_to_scss(sass);
    assert!(
      scss.contains("@import 'variables';"),
      "@import gets semicolon: {scss}"
    );
    assert!(
      scss.contains("@media (min-width: 768px) {"),
      "@media opens block: {scss}"
    );
  }

  #[test]
  fn empty_input() {
    assert_eq!(convert_sass_to_scss(""), "");
  }

  /// Every char-boundary offset of `scss` that falls inside copied text maps
  /// to the same character in `sass`.
  fn assert_copied_text_maps_back(sass: &str) {
    let (scss, map) = convert_sass_to_scss_with_map(sass);
    for (offset, ch) in scss.char_indices() {
      let back = map.to_original(offset);
      assert!(back <= sass.len(), "{offset} -> {back} past the end");
      assert!(
        sass.is_char_boundary(back),
        "{offset} -> {back} splits a char"
      );
      if ch.is_alphanumeric() && !"mixinclude".contains(ch) {
        assert_eq!(
          sass[back..].chars().next(),
          Some(ch),
          "offset {offset} ({ch:?}) maps to {back} in {sass:?} / {scss:?}"
        );
      }
    }
  }

  #[test]
  fn offsets_map_back_to_the_sass_source() {
    let sass = "// héllo\n.foo\n  content: \"→ ✓\"\n  color: red\n  .bar\n    font-family: Ärial\n\n.baz\n  color: #FFF\n";
    assert_copied_text_maps_back(sass);

    let (scss, map) = convert_sass_to_scss_with_map(sass);
    let color = scss.find("color: red").unwrap();
    assert_eq!(map.to_original(color), sass.find("color: red").unwrap());
    // The `;` the converter adds maps to the end of the declaration.
    let semi = scss[color..].find(';').unwrap() + color;
    assert_eq!(
      map.to_original(semi),
      sass.find("color: red").unwrap() + "color: red".len()
    );
  }

  #[test]
  fn offsets_map_back_through_rewritten_prefixes_and_crlf() {
    let sass = "=border-radius($r)\r\n  border-radius: $r\r\n\r\n.box\r\n  +border-radius(5px)\r\n";
    assert_copied_text_maps_back(sass);

    let (scss, map) = convert_sass_to_scss_with_map(sass);
    let include = scss.find("@include").unwrap();
    let plus = sass.find('+').unwrap();
    assert_eq!(map.to_original(include), plus, "prefix maps to its start");
    let name = scss.find("border-radius(5px)").unwrap();
    assert_eq!(map.to_original(name), plus + 1);
  }

  #[test]
  fn generated_braces_map_to_the_end_of_the_block_they_close() {
    let sass = ".a\n  color: red\n.b\n  color: blue";
    let (scss, map) = convert_sass_to_scss_with_map(sass);
    let first_close = scss.find('}').unwrap();
    assert_eq!(
      map.to_original(first_close),
      sass.find('\n').unwrap() + "\n  color: red".len()
    );
    let last_close = scss.rfind('}').unwrap();
    assert_eq!(map.to_original(last_close), sass.len());
    assert_eq!(map.to_original(scss.len() + 10), sass.len());
  }

  #[test]
  fn blank_lines_preserved() {
    let sass = ".a\n  color: red\n\n.b\n  color: blue\n";
    let scss = convert_sass_to_scss(sass);
    // Should contain a blank line somewhere between the two rules.
    assert!(scss.contains("\n\n"), "blank line preserved: {scss}");
  }
}
