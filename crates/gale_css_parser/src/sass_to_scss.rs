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
/// Source-map accuracy is *not* preserved — byte offsets in the resulting SCSS
/// will differ from the original Sass.  That is acceptable for an initial
/// implementation (diagnostics may point to slightly wrong columns).
pub fn convert_sass_to_scss(input: &str) -> String {
  let lines: Vec<&str> = input.lines().collect();
  let mut out = String::with_capacity(input.len() * 2);

  // We track a stack of indentation levels.  Each entry is the column-width
  // of the indentation at that nesting depth.
  let mut indent_stack: Vec<usize> = Vec::new();

  // Detect whether the file uses tabs or spaces (whichever appears first).
  // We only need this to measure indent width consistently.
  let indent_unit = detect_indent_unit(input);

  let mut inside_block_comment = false;

  for (i, raw_line) in lines.iter().enumerate() {
    // ── Block comments ────────────────────────────────────────────
    if inside_block_comment {
      out.push_str(raw_line);
      out.push('\n');
      if raw_line.contains("*/") {
        inside_block_comment = false;
      }
      continue;
    }
    if raw_line.trim_start().starts_with("/*") && !raw_line.contains("*/") {
      inside_block_comment = true;
      out.push_str(raw_line);
      out.push('\n');
      continue;
    }

    // ── Blank / whitespace-only lines ─────────────────────────────
    let trimmed = raw_line.trim();
    if trimmed.is_empty() {
      out.push('\n');
      continue;
    }

    // ── Measure indentation ───────────────────────────────────────
    let indent = measure_indent(raw_line, indent_unit);

    // Close blocks whose indentation we have left.
    while let Some(&top) = indent_stack.last() {
      if indent <= top {
        indent_stack.pop();
        push_indent(&mut out, indent_stack.len(), indent_unit);
        out.push_str("}\n");
      } else {
        break;
      }
    }

    // ── Line-level transformations ────────────────────────────────

    // Pure line comment — pass through.
    if trimmed.starts_with("//") {
      push_indent(&mut out, indent_stack.len(), indent_unit);
      out.push_str(trimmed);
      out.push('\n');
      continue;
    }

    // Inline `/* ... */` single-line comment — pass through.
    if trimmed.starts_with("/*") && trimmed.contains("*/") {
      push_indent(&mut out, indent_stack.len(), indent_unit);
      out.push_str(trimmed);
      out.push('\n');
      continue;
    }

    // Sass shorthand: `=mixin-name(...)` → `@mixin mixin-name(...)`
    let line = if let Some(rest) = trimmed.strip_prefix('=') {
      format!("@mixin {rest}")
    // Sass shorthand: `+mixin-name` → `@include mixin-name`
    } else if trimmed.starts_with('+') && !trimmed.starts_with("+-") {
      format!("@include {}", &trimmed[1..])
    } else {
      trimmed.to_string()
    };

    // Determine whether this line starts a new block.  A block-opener is
    // any line followed by a more-indented line (unless it is a property
    // declaration, which can also contain nested blocks in Sass, but the
    // most common pattern is selectors / at-rules).
    let next_indent = next_non_empty_indent(&lines, i + 1, indent_unit);
    let opens_block = next_indent.is_some_and(|ni| ni > indent);

    push_indent(&mut out, indent_stack.len(), indent_unit);

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
    push_indent(&mut out, indent_stack.len(), indent_unit);
    out.push_str("}\n");
  }

  out
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

  #[test]
  fn blank_lines_preserved() {
    let sass = ".a\n  color: red\n\n.b\n  color: blue\n";
    let scss = convert_sass_to_scss(sass);
    // Should contain a blank line somewhere between the two rules.
    assert!(scss.contains("\n\n"), "blank line preserved: {scss}");
  }
}
