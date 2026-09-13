use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow unspaced operators in SCSS expressions.
///
/// Checks `+`, `-`, `*`, `/`, `%` operators and comparison operators
/// (`==`, `!=`, `<`, `>`, `>=`, `<=`) in declaration values, variable
/// assignments, function defaults, media queries, selectors (inside
/// interpolation), and comments (inside interpolation).
///
/// Equivalent to `scss/operator-no-unspaced`.
pub struct ScssOperatorNoUnspaced;

impl Rule for ScssOperatorNoUnspaced {
  fn name(&self) -> &'static str {
    "scss/operator-no-unspaced"
  }

  fn description(&self) -> &'static str {
    "Disallow unspaced Sass operators"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, context: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(context.syntax, Syntax::Scss | Syntax::Less | Syntax::Sass) {
      return vec![];
    }
    let style = match node {
      CssNode::Style(s) => s,
      _ => return vec![],
    };
    let mut diagnostics = Vec::new();
    for decl in &style.declarations {
      let slash_is_sep = is_shorthand_with_slash(&decl.property);
      // Check interpolations in property name
      check_interpolations_only(
        &decl.property,
        find_prop_offset(context.source, &decl.property, decl.span.offset),
        self,
        &mut diagnostics,
      );
      check_value(
        &decl.value,
        decl.span.offset,
        slash_is_sep,
        false,
        self,
        &mut diagnostics,
      );
    }
    diagnostics
  }

  fn check_root(&self, _nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(context.syntax, Syntax::Scss | Syntax::Less | Syntax::Sass) {
      return vec![];
    }
    let mut diagnostics = Vec::new();
    check_source_level(context.source, self, &mut diagnostics);
    check_calc_multi_space(context.source, self, &mut diagnostics);
    diagnostics
  }
}

fn find_prop_offset(source: &str, prop: &str, val_offset: usize) -> usize {
  if val_offset > prop.len() + 2 {
    let start = val_offset.saturating_sub(prop.len() + 20);
    if let Some(pos) = source[start..val_offset].rfind(prop) {
      return start + pos;
    }
  }
  0
}

fn is_shorthand_with_slash(property: &str) -> bool {
  let prop = property.to_ascii_lowercase();
  let prop = prop
    .strip_prefix("-webkit-")
    .or_else(|| prop.strip_prefix("-moz-"))
    .or_else(|| prop.strip_prefix("-ms-"))
    .or_else(|| prop.strip_prefix("-o-"))
    .unwrap_or(&prop);
  matches!(
    prop,
    "font"
      | "border-radius"
      | "background"
      | "background-size"
      | "grid"
      | "grid-area"
      | "grid-column"
      | "grid-row"
      | "grid-template"
      | "grid-template-columns"
      | "grid-template-rows"
      | "list-style"
      | "mask"
      | "mask-border"
  )
}

// ── Source-level scan (check_root) ──────────────────────────────────────

/// Scan the full source for contexts the per-node check misses:
/// interpolation in selectors/comments, variable declarations,
/// function/mixin defaults, media queries, and @import url() expressions.
fn check_source_level(source: &str, rule: &ScssOperatorNoUnspaced, diags: &mut Vec<Diagnostic>) {
  let b = source.as_bytes();
  let len = b.len();
  let mut i = 0;

  while i < len {
    // Single-line comment: skip (// #{10+ 1} is accept)
    if b[i] == b'/' && i + 1 < len && b[i + 1] == b'/' {
      i += 2;
      while i < len && b[i] != b'\n' {
        i += 1;
      }
      continue;
    }
    // Block comment: check interpolation inside
    if b[i] == b'/' && i + 1 < len && b[i + 1] == b'*' {
      i += 2;
      let cs = i;
      while i + 1 < len && !(b[i] == b'*' && b[i + 1] == b'/') {
        i += 1;
      }
      check_interpolations_only(&source[cs..i], cs, rule, diags);
      if i + 1 < len {
        i += 2;
      }
      continue;
    }
    // String literal: check interpolation inside
    if b[i] == b'"' || b[i] == b'\'' {
      let q = b[i];
      i += 1;
      while i < len {
        if b[i] == b'\\' {
          i += 2;
          continue;
        }
        if b[i] == b'#' && i + 1 < len && b[i + 1] == b'{' {
          let (inner, end) = extract_interpolation(source, i + 2);
          check_value(inner, i + 2, false, true, rule, diags);
          i = end;
          continue;
        }
        if b[i] == q {
          i += 1;
          break;
        }
        i += 1;
      }
      continue;
    }
    // Variable declaration: $var: value;
    if b[i] == b'$' {
      let vs = i;
      i += 1;
      while i < len && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b'_') {
        i += 1;
      }
      skip_ws(b, &mut i);
      if i < len && b[i] == b':' {
        i += 1;
        skip_ws(b, &mut i);
        let val_start = i;
        let val_end = find_stmt_end(source, i);
        check_value_expr(&source[val_start..val_end], val_start, rule, diags);
        i = val_end;
      }
      continue;
    }
    // @-rule
    if b[i] == b'@' {
      i += 1;
      let ks = i;
      while i < len && b[i].is_ascii_alphabetic() {
        i += 1;
      }
      let kw = source[ks..i].to_ascii_lowercase();

      if kw == "function" || kw == "mixin" || kw == "include" {
        // Find `(`
        while i < len && b[i] != b'(' && b[i] != b'{' && b[i] != b';' {
          i += 1;
        }
        if i < len && b[i] == b'(' {
          i += 1;
          let mut depth = 1i32;
          while i < len && depth > 0 {
            if b[i] == b'(' {
              depth += 1;
            } else if b[i] == b')' {
              depth -= 1;
              if depth == 0 {
                break;
              }
            } else if b[i] == b':' && depth == 1 {
              i += 1;
              skip_ws(b, &mut i);
              let ds = i;
              let de = find_param_end(source, i, depth);
              check_value_expr(&source[ds..de], ds, rule, diags);
              i = de;
              continue;
            }
            i += 1;
          }
        }
        continue;
      }
      if kw == "media" {
        skip_ws(b, &mut i);
        let ms = i;
        let mut depth = 0i32;
        while i < len && !(b[i] == b'{' && depth == 0) {
          if b[i] == b'(' {
            depth += 1;
          } else if b[i] == b')' {
            depth -= 1;
          }
          if b[i] == b'"' || b[i] == b'\'' {
            let q = b[i];
            i += 1;
            while i < len && b[i] != q {
              if b[i] == b'\\' {
                i += 1;
              }
              i += 1;
            }
          }
          i += 1;
        }
        let me = i;
        let mt = &source[ms..me];
        check_interpolations_only(mt, ms, rule, diags);
        check_media_values(mt, ms, rule, diags);
        continue;
      }
      if kw == "import" {
        skip_ws(b, &mut i);
        if i + 4 <= len && source[i..].to_ascii_lowercase().starts_with("url(") {
          i += 4;
          let us = i;
          skip_ws(b, &mut i);
          if i < len && (b[i] == b'"' || b[i] == b'\'') {
            // Quoted URL — skip
            let q = b[i];
            i += 1;
            while i < len && b[i] != q {
              if b[i] == b'\\' {
                i += 1;
              }
              i += 1;
            }
          } else if i < len && b[i] == b'/' {
            // Protocol-relative — skip
            while i < len && b[i] != b')' {
              i += 1;
            }
          } else {
            // Expression URL
            let mut depth = 1i32;
            let mut j = us;
            while j < len && depth > 0 {
              if b[j] == b'(' {
                depth += 1;
              } else if b[j] == b')' {
                depth -= 1;
                if depth == 0 {
                  break;
                }
              }
              j += 1;
            }
            let content = &source[us..j];
            if content
              .bytes()
              .next()
              .is_some_and(|c| c.is_ascii_digit() || c == b'$')
            {
              check_value_expr(content, us, rule, diags);
            }
            i = j;
          }
        }
        continue;
      }
      continue;
    }
    // Interpolation at source level (selectors, etc.)
    if b[i] == b'#' && i + 1 < len && b[i + 1] == b'{' {
      let (inner, end) = extract_interpolation(source, i + 2);
      check_value(inner, i + 2, false, true, rule, diags);
      i = end;
      continue;
    }
    i += 1;
  }
}

/// Check calc() blocks in the full source for operators with multiple spaces.
/// The per-node check skips calc() content entirely, but
/// `scss/operator-no-unspaced` should still flag multiple-space violations
/// inside calc().
fn check_calc_multi_space(
  source: &str,
  rule: &ScssOperatorNoUnspaced,
  diags: &mut Vec<Diagnostic>,
) {
  let b = source.as_bytes();
  let len = b.len();
  let mut i = 0;

  while i + 4 < len {
    // Skip strings
    if b[i] == b'"' || b[i] == b'\'' {
      let q = b[i];
      i += 1;
      while i < len && b[i] != q {
        if b[i] == b'\\' {
          i += 1;
        }
        i += 1;
      }
      if i < len {
        i += 1;
      }
      continue;
    }
    // Skip comments
    if b[i] == b'/' && i + 1 < len && b[i + 1] == b'*' {
      i += 2;
      while i + 1 < len && !(b[i] == b'*' && b[i + 1] == b'/') {
        i += 1;
      }
      i += 2;
      continue;
    }
    if b[i] == b'/' && i + 1 < len && b[i + 1] == b'/' {
      while i < len && b[i] != b'\n' {
        i += 1;
      }
      continue;
    }

    // Detect calc(
    if i + 5 <= len && b[i..i + 5].eq_ignore_ascii_case(b"calc(") {
      let calc_start = i + 5;
      // Find matching )
      let mut depth = 1i32;
      let mut j = calc_start;
      while j < len && depth > 0 {
        match b[j] {
          b'(' => depth += 1,
          b')' => {
            depth -= 1;
            if depth == 0 {
              break;
            }
          }
          b'"' | b'\'' => {
            let q = b[j];
            j += 1;
            while j < len && b[j] != q {
              if b[j] == b'\\' {
                j += 1;
              }
              j += 1;
            }
          }
          _ => {}
        }
        j += 1;
      }
      let calc_end = j;
      // Scan inside calc for operators with multiple spaces
      let mut k = calc_start;
      while k < calc_end {
        // Skip interpolation
        if b[k] == b'#' && k + 1 < calc_end && b[k + 1] == b'{' {
          k += 2;
          let mut id = 1i32;
          while k < calc_end && id > 0 {
            if b[k] == b'{' {
              id += 1;
            } else if b[k] == b'}' {
              id -= 1;
            }
            if id > 0 {
              k += 1;
            }
          }
          if k < calc_end {
            k += 1;
          }
          continue;
        }
        if (b[k] == b'+' || b[k] == b'-' || b[k] == b'*' || b[k] == b'/')
          && k > calc_start
          && k + 1 < calc_end
        {
          // Skip unary
          if (b[k] == b'+' || b[k] == b'-') && is_unary(b, k) {
            k += 1;
            continue;
          }
          let ws_before = b[k - 1].is_ascii_whitespace();
          let ws_after = b[k + 1].is_ascii_whitespace();
          if ws_before && ws_after {
            let sb = count_ws_before(b, k);
            let sa = count_ws_after(b, k);
            if (sb > 1 || sa > 1) && !has_nl_before(b, k) && !has_nl_after(b, k) {
              let op = b[k] as char;
              let msg = if sb > 1 && sa > 1 {
                format!("Expected single space before and after \"{op}\"")
              } else if sb > 1 {
                format!("Expected single space before \"{op}\"")
              } else {
                format!("Expected single space after \"{op}\"")
              };
              diags.push(
                Diagnostic::new(rule.name(), msg)
                  .severity(rule.default_severity())
                  .span(Span::new(k, 1)),
              );
            }
          }
        }
        k += 1;
      }
      i = calc_end;
      continue;
    }
    i += 1;
  }
}

fn skip_ws(b: &[u8], i: &mut usize) {
  while *i < b.len() && b[*i].is_ascii_whitespace() {
    *i += 1;
  }
}

fn extract_interpolation(source: &str, start: usize) -> (&str, usize) {
  let b = source.as_bytes();
  let len = b.len();
  let mut depth = 1i32;
  let mut j = start;
  while j < len && depth > 0 {
    if b[j] == b'{' {
      depth += 1;
    } else if b[j] == b'}' {
      depth -= 1;
      if depth == 0 {
        break;
      }
    }
    j += 1;
  }
  (&source[start..j], j + 1)
}

fn find_stmt_end(source: &str, start: usize) -> usize {
  let b = source.as_bytes();
  let len = b.len();
  let mut i = start;
  let mut depth = 0i32;
  while i < len {
    match b[i] {
      b'(' => depth += 1,
      b')' => depth -= 1,
      b';' if depth <= 0 => return i,
      b'}' if depth <= 0 => return i,
      b'"' | b'\'' => {
        let q = b[i];
        i += 1;
        while i < len && b[i] != q {
          if b[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
      }
      _ => {}
    }
    i += 1;
  }
  len
}

fn find_param_end(source: &str, start: usize, init_depth: i32) -> usize {
  let b = source.as_bytes();
  let len = b.len();
  let mut i = start;
  let mut depth = init_depth;
  while i < len {
    match b[i] {
      b'(' => depth += 1,
      b')' => {
        if depth <= init_depth {
          return i;
        }
        depth -= 1;
      }
      b',' if depth == init_depth => return i,
      b'"' | b'\'' => {
        let q = b[i];
        i += 1;
        while i < len && b[i] != q {
          if b[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
      }
      _ => {}
    }
    i += 1;
  }
  len
}

fn check_media_values(
  mt: &str,
  base: usize,
  rule: &ScssOperatorNoUnspaced,
  diags: &mut Vec<Diagnostic>,
) {
  let b = mt.as_bytes();
  let len = b.len();
  let mut i = 0;
  while i < len {
    if b[i] == b'(' {
      i += 1;
      let mut depth = 1i32;
      while i < len && depth > 0 {
        if b[i] == b'(' {
          depth += 1;
        } else if b[i] == b')' {
          depth -= 1;
          if depth == 0 {
            break;
          }
        } else if b[i] == b':' && depth == 1 {
          i += 1;
          while i < len && b[i] == b' ' {
            i += 1;
          }
          let vs = i;
          let mut d = 1i32;
          let mut end = vs;
          while end < len {
            if b[end] == b'(' {
              d += 1;
            } else if b[end] == b')' {
              d -= 1;
              if d == 0 {
                break;
              }
            }
            end += 1;
          }
          check_value_expr(&mt[vs..end], base + vs, rule, diags);
          i = end;
          break;
        }
        i += 1;
      }
    } else {
      i += 1;
    }
  }
}

/// Check only interpolation blocks inside a text for unspaced operators.
fn check_interpolations_only(
  text: &str,
  base: usize,
  rule: &ScssOperatorNoUnspaced,
  diags: &mut Vec<Diagnostic>,
) {
  let b = text.as_bytes();
  let len = b.len();
  let mut i = 0;
  while i + 1 < len {
    if b[i] == b'#' && b[i + 1] == b'{' {
      let (inner, end_rel) = extract_interpolation(text, i + 2);
      check_value(inner, base + i + 2, false, true, rule, diags);
      i = end_rel;
    } else {
      i += 1;
    }
  }
}

// ── Core value checking ────────────────────────────────────────────────

fn check_value(
  value: &str,
  base_offset: usize,
  slash_is_sep: bool,
  in_interp: bool,
  rule: &ScssOperatorNoUnspaced,
  diags: &mut Vec<Diagnostic>,
)
// Default: expr_ctx = in_interp
{
  check_value_inner(
    value,
    base_offset,
    slash_is_sep,
    in_interp,
    in_interp,
    rule,
    diags,
  );
}

fn check_value_expr(
  value: &str,
  base_offset: usize,
  rule: &ScssOperatorNoUnspaced,
  diags: &mut Vec<Diagnostic>,
) {
  check_value_inner(value, base_offset, false, false, true, rule, diags);
}

fn check_value_inner(
  value: &str,
  base_offset: usize,
  slash_is_sep: bool,
  in_interp: bool,
  expr_ctx: bool,
  rule: &ScssOperatorNoUnspaced,
  diags: &mut Vec<Diagnostic>,
) {
  let b = value.as_bytes();
  let len = b.len();
  let mut i = 0;
  let mut paren_depth: i32 = 0;
  let mut in_calc = false;
  let mut calc_depth: i32 = 0;
  let mut in_url = false;
  let mut url_depth: i32 = 0;
  let mut in_color_fn = false;
  let mut color_fn_depth: i32 = 0;

  while i < len {
    let ch = b[i];

    // Skip SCSS single-line comments
    if ch == b'/' && i + 1 < len && b[i + 1] == b'/' {
      i += 2;
      while i < len && b[i] != b'\n' {
        i += 1;
      }
      continue;
    }
    // Skip block comments /* ... */
    if ch == b'/' && i + 1 < len && b[i + 1] == b'*' {
      i += 2;
      while i + 1 < len && !(b[i] == b'*' && b[i + 1] == b'/') {
        i += 1;
      }
      if i + 1 < len {
        i += 2;
      }
      continue;
    }
    // Skip string literals but check interpolation inside
    if ch == b'"' || ch == b'\'' {
      let q = ch;
      i += 1;
      while i < len {
        if b[i] == b'\\' {
          i += 2;
          continue;
        }
        if b[i] == b'#' && i + 1 < len && b[i + 1] == b'{' {
          let (inner, end) = extract_interp_local(value, i + 2);
          check_value(inner, base_offset + i + 2, false, true, rule, diags);
          i = end;
          continue;
        }
        if b[i] == q {
          i += 1;
          break;
        }
        i += 1;
      }
      continue;
    }
    // SCSS interpolation: check operators INSIDE
    if ch == b'#' && i + 1 < len && b[i + 1] == b'{' {
      let (inner, end) = extract_interp_local(value, i + 2);
      check_value(inner, base_offset + i + 2, false, true, rule, diags);
      i = end;
      continue;
    }
    // calc()
    if i + 5 <= len && b[i..i + 5].eq_ignore_ascii_case(b"calc(") {
      in_calc = true;
      calc_depth = paren_depth + 1;
      paren_depth += 1;
      i += 5;
      continue;
    }
    // url() — skip content but check interpolation inside
    if i + 4 <= len && b[i..i + 4].eq_ignore_ascii_case(b"url(") {
      in_url = true;
      url_depth = paren_depth + 1;
      paren_depth += 1;
      i += 4;
      continue;
    }
    // Color functions — `/` is the alpha separator, not division
    if ch.is_ascii_alphabetic()
      && let Some(fn_len) = is_color_function_at(value, i)
    {
      in_color_fn = true;
      color_fn_depth = paren_depth + 1;
      paren_depth += 1;
      i += fn_len;
      continue;
    }
    if ch == b'(' {
      paren_depth += 1;
      i += 1;
      continue;
    }
    if ch == b')' {
      if in_calc && paren_depth == calc_depth {
        in_calc = false;
      }
      if in_url && paren_depth == url_depth {
        in_url = false;
      }
      if in_color_fn && paren_depth == color_fn_depth {
        in_color_fn = false;
      }
      paren_depth -= 1;
      i += 1;
      continue;
    }
    if in_calc && paren_depth >= calc_depth {
      i += 1;
      continue;
    }
    if in_url && paren_depth >= url_depth {
      // Still check interpolation inside url()
      if b[i] == b'#' && i + 1 < len && b[i + 1] == b'{' {
        let (inner, end) = extract_interp_local(value, i + 2);
        check_value(inner, base_offset + i + 2, false, true, rule, diags);
        i = end;
        continue;
      }
      i += 1;
      continue;
    }

    // Backslash-escaped characters: skip next char
    if ch == b'\\' {
      i += 2;
      continue;
    }

    // ── Comparison operators: ==, !=, >=, <=, <, > ──
    // Treat newline as equivalent to a space (Stylelint allows operators at end of line).
    if ch == b'=' && i + 1 < len && b[i + 1] == b'=' {
      let ok = (i > 0 && is_space_or_nl(b[i - 1])) && (i + 2 < len && is_space_or_nl(b[i + 2]));
      if !ok {
        emit(rule, diags, base_offset + i, 2, "==");
      }
      i += 2;
      continue;
    }
    if ch == b'!' && i + 1 < len && b[i + 1] == b'=' {
      let ok = (i > 0 && is_space_or_nl(b[i - 1])) && (i + 2 < len && is_space_or_nl(b[i + 2]));
      if !ok {
        emit(rule, diags, base_offset + i, 2, "!=");
      }
      i += 2;
      continue;
    }
    if ch == b'>' && i + 1 < len && b[i + 1] == b'=' {
      let ok = (i > 0 && is_space_or_nl(b[i - 1])) && (i + 2 < len && is_space_or_nl(b[i + 2]));
      if !ok {
        emit(rule, diags, base_offset + i, 2, ">=");
      }
      i += 2;
      continue;
    }
    if ch == b'<' && i + 1 < len && b[i + 1] == b'=' {
      let ok = (i > 0 && is_space_or_nl(b[i - 1])) && (i + 2 < len && is_space_or_nl(b[i + 2]));
      if !ok {
        emit(rule, diags, base_offset + i, 2, "<=");
      }
      i += 2;
      continue;
    }
    if ch == b'<' && is_comparison_ctx(b, i) {
      let ok = (i > 0 && is_space_or_nl(b[i - 1])) && (i + 1 < len && is_space_or_nl(b[i + 1]));
      if !ok {
        emit(rule, diags, base_offset + i, 1, "<");
      }
      i += 1;
      continue;
    }
    if ch == b'>' && is_comparison_ctx(b, i) {
      let ok = (i > 0 && is_space_or_nl(b[i - 1])) && (i + 1 < len && is_space_or_nl(b[i + 1]));
      if !ok {
        emit(rule, diags, base_offset + i, 1, ">");
      }
      i += 1;
      continue;
    }

    // ── Modulo % ──
    if ch == b'%' {
      if is_modulo(b, i) {
        let ok = (i > 0 && is_space_or_nl(b[i - 1])) && (i + 1 < len && is_space_or_nl(b[i + 1]));
        if !ok {
          emit(rule, diags, base_offset + i, 1, "%");
        }
      }
      i += 1;
      continue;
    }

    // ── Arithmetic: +, -, *, / ──
    if ch == b'+' || ch == b'-' || ch == b'*' || ch == b'/' {
      // Scientific notation
      if (ch == b'+' || ch == b'-')
        && i > 0
        && (b[i - 1] == b'e' || b[i - 1] == b'E')
        && i >= 2
        && b[i - 2].is_ascii_digit()
      {
        i += 1;
        continue;
      }
      // `--` prefix (CSS custom property value)
      if ch == b'-' && i + 1 < len && b[i + 1] == b'-' {
        i += 2;
        while i < len && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b'_') {
          i += 1;
        }
        continue;
      }
      // Hyphen in identifier
      if ch == b'-' && is_ident_hyphen(b, i) {
        i += 1;
        continue;
      }
      // Trailing/leading operator at edge of value is not an operator
      if i == 0 || i + 1 >= len {
        i += 1;
        continue;
      }
      // Slash in shorthand properties
      if ch == b'/' && slash_is_sep {
        i += 1;
        continue;
      }
      // Slash as alpha separator in CSS color functions
      if ch == b'/' && in_color_fn && paren_depth >= color_fn_depth {
        i += 1;
        continue;
      }
      // Unary
      if (ch == b'+' || ch == b'-') && is_unary(b, i) {
        i += 1;
        continue;
      }

      // Treat newlines as spaces so that operators at end-of-line are not flagged.
      // Stylelint allows `4 *\n  $y` as a valid multi-line expression.
      let sp_before = i > 0 && is_space_or_nl(b[i - 1]);
      let sp_after = i + 1 < len && is_space_or_nl(b[i + 1]);
      let ws_before = i > 0 && b[i - 1].is_ascii_whitespace();
      let ws_after = i + 1 < len && b[i + 1].is_ascii_whitespace();
      let next = if i + 1 < len { b[i + 1] } else { 0 };

      if ch == b'+' || ch == b'-' {
        // No space before, space after: `X+ Y` or `X- Y`
        if !ws_before && ws_after {
          // `1px- Y` or `ss- Y`: if previous token ends with unit → list, not operator
          if ch == b'-' && prev_ends_unit(b, i) {
            // But inside CSS slash context: `8px/2px- 5px` → list
            // (already handled by prev_ends_unit returning true for `2px-`)
            i += 1;
            continue;
          }
          emit(rule, diags, base_offset + i, 1, &(ch as char).to_string());
          i += 1;
          continue;
        }
        // Space before, no space after: `X +Y` or `X -Y`
        if ws_before && !ws_after {
          // CSS slash context takes priority (even inside parens).
          // In a CSS slash context like `8px/2px`, subsequent
          // `+Y` and `-Y` are all list items, not operators.
          // Exceptions: `$var` and `fn()` targets are still operators.
          if in_css_slash_ctx(b, i) {
            if next == b'$' || is_fn_call_next(b, i + 1) {
              emit(rule, diags, base_offset + i, 1, &(ch as char).to_string());
            }
            i += 1;
            continue;
          }
          // Inside interpolation: always operator
          if in_interp {
            emit(rule, diags, base_offset + i, 1, &(ch as char).to_string());
            i += 1;
            continue;
          }
          // Inside parens: operator for most things, but
          // Inside parens: negative numbers are still lists: `(1 -1)`, `(1px -1px)`
          // But `+digit` IS an operator: `(1px +1px)` is reject
          if paren_depth > 0 {
            if ch == b'-' && (next.is_ascii_digit() || next == b'.') {
              // `-1`, `-.5` → negative number → list even in parens
              i += 1;
              continue;
            }
            emit(rule, diags, base_offset + i, 1, &(ch as char).to_string());
            i += 1;
            continue;
          }
          if ch == b'+' {
            if !expr_ctx && (next.is_ascii_digit() || next == b'.') {
              i += 1;
              continue; // `1 +1` → list (only in non-expression context)
            }
            emit(rule, diags, base_offset + i, 1, "+");
            i += 1;
            continue;
          }
          // ch == b'-'
          if !expr_ctx && (next.is_ascii_digit() || next == b'.') {
            i += 1;
            continue;
          } // negative number → list
          if next == b'$' {
            emit(rule, diags, base_offset + i, 1, "-");
            i += 1;
            continue;
          }
          if next == b'#' {
            if i + 2 < len && b[i + 2] == b'{' {
              i += 1;
              continue;
            } // -#{} → sign
            // -#hex: check if prev is hex color
            if prev_is_hex(b, i) {
              emit(rule, diags, base_offset + i, 1, "-");
              i += 1;
              continue;
            }
            i += 1;
            continue; // -#ffc → list
          }
          if next == b'(' {
            emit(rule, diags, base_offset + i, 1, "-");
            i += 1;
            continue;
          }
          if next.is_ascii_alphabetic() || next == b'_' {
            i += 1;
            continue;
          } // -ident → list
          emit(rule, diags, base_offset + i, 1, "-");
          i += 1;
          continue;
        }
        // No spaces at all
        if !ws_before && !ws_after {
          // In CSS slash context: no-space is operator if RHS is
          // numeric, otherwise concatenation.
          // `8px/2px-5px` → operator (REJECT)
          // `8px/2px-$var` → concatenation (ACCEPT)
          // `8px/2px-fn()` → concatenation (ACCEPT)
          if in_css_slash_ctx(b, i) && !(next.is_ascii_digit() || next == b'.') {
            i += 1;
            continue;
          }
          emit(rule, diags, base_offset + i, 1, &(ch as char).to_string());
          i += 1;
          continue;
        }
        // Both spaces: check for multiple spaces
        if ws_before && ws_after {
          let sb = count_ws_before(b, i);
          let sa = count_ws_after(b, i);
          if (sb > 1 || sa > 1) && !has_nl_before(b, i) && !has_nl_after(b, i) {
            emit(rule, diags, base_offset + i, 1, &(ch as char).to_string());
          }
          i += 1;
          continue;
        }
        i += 1;
        continue;
      }
      // * or /
      if ch == b'/' && !expr_ctx && is_css_slash(b, i, paren_depth, in_interp) {
        i += 1;
        continue;
      }
      if !sp_before || !sp_after {
        emit(rule, diags, base_offset + i, 1, &(ch as char).to_string());
      }
    }
    i += 1;
  }
}

fn extract_interp_local(value: &str, start: usize) -> (&str, usize) {
  let b = value.as_bytes();
  let len = b.len();
  let mut depth = 1i32;
  let mut j = start;
  while j < len && depth > 0 {
    if b[j] == b'{' {
      depth += 1;
    } else if b[j] == b'}' {
      depth -= 1;
      if depth == 0 {
        break;
      }
    }
    j += 1;
  }
  (&value[start..j], j + 1)
}

fn emit(
  rule: &ScssOperatorNoUnspaced,
  diags: &mut Vec<Diagnostic>,
  offset: usize,
  len: usize,
  op: &str,
) {
  diags.push(
    Diagnostic::new(
      rule.name(),
      format!("Expected spaces around operator '{op}'"),
    )
    .severity(rule.default_severity())
    .span(Span::new(offset, len)),
  );
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Returns true if the byte is a space, tab, newline, or carriage return.
/// Stylelint treats a newline adjacent to an operator (e.g. `4 *\n  $y`) as
/// "spaced", so we must do the same to avoid false positives.
#[inline]
fn is_space_or_nl(b: u8) -> bool {
  b == b' ' || b == b'\t' || b == b'\n' || b == b'\r'
}

/// Hyphen inside an identifier: `sans-serif`, `border-top`, `#{$var}-suffix`, `$var-name`.
fn is_ident_hyphen(b: &[u8], i: usize) -> bool {
  let len = b.len();
  if i == 0 || i + 1 >= len {
    return false;
  }
  let prev = b[i - 1];
  let next = b[i + 1];

  let prev_ok = prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'}' || prev == b'%';
  let next_ok = next.is_ascii_alphanumeric() || next == b'_' || next == b'#' || next == b'-';
  if !prev_ok || !next_ok {
    return false;
  }

  if prev == b')' {
    return false;
  } // fn()-1 → operator
  if prev == b'}' {
    return true;
  } // #{...}-1 → identifier

  // Numeric+unit minus numeric: `5px-3px`, `.1px-1px`, `s.1px-1` → operator
  if next.is_ascii_digit() {
    let mut ts = i - 1;
    while ts > 0
      && (b[ts - 1].is_ascii_alphanumeric()
        || b[ts - 1] == b'.'
        || b[ts - 1] == b'_'
        || b[ts - 1] == b'-')
    {
      ts -= 1;
    }
    // $var-1 → variable name → identifier
    if ts > 0 && b[ts - 1] == b'$' {
      return true;
    }
    let token = &b[ts..i];
    // Check if token contains a numeric start or `.digit` pattern
    if !token.is_empty() {
      if token[0].is_ascii_digit()
        || (token[0] == b'.' && token.len() > 1 && token[1].is_ascii_digit())
      {
        return false; // operator
      }
      if token
        .windows(2)
        .any(|w| w[0] == b'.' && w[1].is_ascii_digit())
      {
        return false; // operator: s.1px-1
      }
    }
  }
  true
}

/// Previous token ends with unit/letters/digit-in-identifier (not hex color, not bare digit).
/// Returns true for tokens like `1px-`, `ss-`, `s1-`, `abc-` which form
/// identifier-like values where the trailing `-` is not an operator.
/// Returns false for `1-` (bare digit), `#0f0-` (hex color).
fn prev_ends_unit(b: &[u8], i: usize) -> bool {
  if i == 0 {
    return false;
  }
  let p = b[i - 1];
  if p.is_ascii_alphabetic() || p == b'_' || p == b'%' {
    let mut j = i - 1;
    while j > 0
      && (b[j - 1].is_ascii_alphanumeric()
        || b[j - 1] == b'_'
        || b[j - 1] == b'-'
        || b[j - 1] == b'.')
    {
      j -= 1;
    }
    if j > 0 && b[j - 1] == b'#' {
      return false;
    } // #0f0- → hex color
    return true;
  }
  // Check for `s1-` pattern: the char before `-` is a digit, but the
  // full token starts with a letter (making it an identifier like `s1`).
  if p.is_ascii_digit() {
    let mut j = i - 1;
    while j > 0
      && (b[j - 1].is_ascii_alphanumeric()
        || b[j - 1] == b'_'
        || b[j - 1] == b'-'
        || b[j - 1] == b'.')
    {
      j -= 1;
    }
    if j > 0 && b[j - 1] == b'#' {
      return false;
    } // hex color
    if j > 0 && b[j - 1] == b'$' {
      return false;
    } // variable
    // Check if the token starts with a letter
    if j < i && b[j].is_ascii_alphabetic() {
      return true;
    }
  }
  false
}

/// Previous non-whitespace token is a hex color.
fn prev_is_hex(b: &[u8], i: usize) -> bool {
  let mut j = i;
  while j > 0 && b[j - 1].is_ascii_whitespace() {
    j -= 1;
  }
  if j == 0 {
    return false;
  }
  let te = j;
  while j > 0 && b[j - 1].is_ascii_alphanumeric() {
    j -= 1;
  }
  if j > 0 && b[j - 1] == b'#' {
    let tok = &b[j..te];
    if tok.iter().all(|c| c.is_ascii_hexdigit()) && matches!(tok.len(), 3 | 4 | 6 | 8) {
      return true;
    }
  }
  false
}

/// Is the token at position j a function call (identifier followed by `(`)?
fn is_fn_call_next(b: &[u8], j: usize) -> bool {
  let len = b.len();
  if j >= len {
    return false;
  }
  let c = b[j];
  if !c.is_ascii_alphabetic() && c != b'_' {
    return false;
  }
  let mut k = j;
  while k < len && (b[k].is_ascii_alphanumeric() || b[k] == b'-' || b[k] == b'_' || b[k] == b'.') {
    k += 1;
  }
  k < len && b[k] == b'('
}

/// Check if position i is after a CSS slash context (`N/M`).
fn in_css_slash_ctx(b: &[u8], i: usize) -> bool {
  let mut j = i;
  while j > 0 && b[j - 1].is_ascii_whitespace() {
    j -= 1;
  }
  if j == 0 {
    return false;
  }
  // Scan past the token
  let te = j;
  while j > 0
    && (b[j - 1].is_ascii_alphanumeric()
      || b[j - 1] == b'.'
      || b[j - 1] == b'_'
      || b[j - 1] == b'-'
      || b[j - 1] == b'$'
      || b[j - 1] == b'}'
      || b[j - 1] == b')')
  {
    j -= 1;
  }
  // Check for `/` (possibly with spaces)
  let mut k = j;
  while k > 0 && b[k - 1] == b' ' {
    k -= 1;
  }
  if k > 0 && b[k - 1] == b'/' {
    let sp = k - 1;
    if sp > 0 && (b[sp - 1].is_ascii_alphanumeric() || b[sp - 1] == b'.' || b[sp - 1] == b'%') {
      return true;
    }
    let mut m = sp;
    while m > 0 && b[m - 1] == b' ' {
      m -= 1;
    }
    if m > 0 && (b[m - 1].is_ascii_alphanumeric() || b[m - 1] == b'.' || b[m - 1] == b'%') {
      return true;
    }
  }
  // Direct `/` before the token (no spaces between token start and /)
  if j > 0 && b[j - 1] == b'/' {
    let sp = j - 1;
    if sp > 0 && (b[sp - 1].is_ascii_alphanumeric() || b[sp - 1] == b'.' || b[sp - 1] == b'%') {
      return true;
    }
  }
  false
}

#[derive(Debug, PartialEq)]
enum TokType {
  Numeric,
  Variable,
  Ident,
  Interp,
  FnCall,
  Parens,
  Hex,
  Signed,
  Unknown,
}

fn classify_before(b: &[u8], i: usize) -> TokType {
  if i == 0 {
    return TokType::Unknown;
  }
  let mut j = i;
  while j > 0 && b[j - 1] == b' ' {
    j -= 1;
  }
  if j == 0 {
    return TokType::Unknown;
  }
  let last = b[j - 1];
  if last == b')' {
    let mut d = 1i32;
    j -= 1;
    while j > 0 && d > 0 {
      j -= 1;
      if b[j] == b')' {
        d += 1;
      } else if b[j] == b'(' {
        d -= 1;
      }
    }
    if j > 0 && (b[j - 1].is_ascii_alphanumeric() || b[j - 1] == b'_' || b[j - 1] == b'.') {
      return TokType::FnCall;
    }
    return TokType::Parens;
  }
  if last == b'}' {
    return TokType::Interp;
  }
  if last == b'%' {
    return TokType::Numeric;
  }
  let te = j;
  while j > 0
    && (b[j - 1].is_ascii_alphanumeric()
      || b[j - 1] == b'.'
      || b[j - 1] == b'_'
      || b[j - 1] == b'-')
  {
    j -= 1;
  }
  let tok = &b[j..te];
  if tok.is_empty() {
    return TokType::Unknown;
  }
  if j > 0 && b[j - 1] == b'#' {
    return TokType::Hex;
  }
  if j > 0 && b[j - 1] == b'$' {
    return TokType::Variable;
  }
  if tok[0].is_ascii_digit() || (tok[0] == b'.' && tok.len() > 1 && tok[1].is_ascii_digit()) {
    return TokType::Numeric;
  }
  if tok[0].is_ascii_alphabetic() {
    return TokType::Ident;
  }
  TokType::Numeric
}

fn classify_after(b: &[u8], i: usize) -> TokType {
  let len = b.len();
  let mut j = i + 1;
  while j < len && b[j] == b' ' {
    j += 1;
  }
  if j >= len {
    return TokType::Unknown;
  }
  let f = b[j];
  if f == b'+' || f == b'-' {
    return TokType::Signed;
  }
  if f == b'#' && j + 1 < len && b[j + 1] == b'{' {
    return TokType::Interp;
  }
  if f == b'(' {
    return TokType::Parens;
  }
  if f == b'$' {
    return TokType::Variable;
  }
  if f == b'#' {
    return TokType::Hex;
  }
  if f.is_ascii_digit() || f == b'.' {
    return TokType::Numeric;
  }
  if f.is_ascii_alphabetic() || f == b'_' {
    let mut k = j;
    while k < len && (b[k].is_ascii_alphanumeric() || b[k] == b'-' || b[k] == b'_' || b[k] == b'.')
    {
      k += 1;
    }
    if k < len && b[k] == b'(' {
      return TokType::FnCall;
    }
    return TokType::Ident;
  }
  TokType::Unknown
}

/// Is `/` a CSS slash (not division)?
fn is_css_slash(b: &[u8], i: usize, pd: i32, in_interp: bool) -> bool {
  // Inside interpolation: `/` is always division
  if in_interp {
    return false;
  }

  let lt = classify_before(b, i);
  let rt = classify_after(b, i);

  // Inside parens: `/` is CSS slash only when BOTH sides are numeric
  // AND there are NO spaces (i.e., `8px/2px` not `1px/ 1px`).
  if pd > 0 {
    let no_sp = (i > 0 && b[i - 1] != b' ') && (i + 1 < b.len() && b[i + 1] != b' ');
    if no_sp && lt == TokType::Numeric && (rt == TokType::Numeric || rt == TokType::Ident) {
      return true;
    }
    return false;
  }
  if lt == TokType::Variable || rt == TokType::Variable {
    if (lt == TokType::Variable && rt == TokType::Ident)
      || (lt == TokType::Ident && rt == TokType::Variable)
    {
      return true;
    }
    return false;
  }
  if lt == TokType::FnCall || rt == TokType::FnCall {
    return false;
  }
  if rt == TokType::Signed {
    return false;
  }
  if lt == TokType::Ident || rt == TokType::Ident {
    return true;
  }
  if lt == TokType::Interp || rt == TokType::Interp {
    return true;
  }
  if lt == TokType::Parens {
    return rt == TokType::Interp || rt == TokType::Ident;
  }
  if rt == TokType::Parens {
    return lt == TokType::Ident;
  }
  // Both sides numeric → CSS slash, UNLESS there's an arithmetic
  // operator before the left operand (e.g., `5px - 8px/2` → the
  // `-` makes `/` a division operator).
  if has_operator_before_token(b, i) {
    return false;
  }
  true // numeric/numeric → CSS slash
}

/// Check if there's an arithmetic operator before the left operand of `/`.
fn has_operator_before_token(b: &[u8], slash_pos: usize) -> bool {
  // Scan back past the left operand token
  let mut j = slash_pos;
  while j > 0 && b[j - 1] == b' ' {
    j -= 1;
  }
  // Scan back past the token
  while j > 0
    && (b[j - 1].is_ascii_alphanumeric()
      || b[j - 1] == b'.'
      || b[j - 1] == b'_'
      || b[j - 1] == b'-'
      || b[j - 1] == b'%')
  {
    j -= 1;
  }
  // Skip whitespace before the token
  while j > 0 && b[j - 1] == b' ' {
    j -= 1;
  }
  if j == 0 {
    return false;
  }
  let c = b[j - 1];
  c == b'+' || c == b'-' || c == b'*'
}

/// Check if position `i` in `value` is the start of a CSS color function name
/// followed by `(`. Returns `Some(len)` where `len` includes the `(` character,
/// or `None` if not a color function.
fn is_color_function_at(value: &str, i: usize) -> Option<usize> {
  const COLOR_FNS: &[&[u8]] = &[
    b"rgb(", b"rgba(", b"hsl(", b"hsla(", b"hwb(", b"lab(", b"lch(", b"oklch(", b"oklab(",
    b"color(",
  ];
  let rest = &value.as_bytes()[i..];
  for &f in COLOR_FNS {
    if rest.len() >= f.len() && rest[..f.len()].eq_ignore_ascii_case(f) {
      return Some(f.len());
    }
  }
  None
}

fn is_modulo(b: &[u8], i: usize) -> bool {
  let len = b.len();
  let mut ap = i + 1;
  while ap < len && b[ap] == b' ' {
    ap += 1;
  }
  if ap >= len {
    return false;
  }
  let sp_before = i > 0 && b[i - 1] == b' ';

  if sp_before {
    let ac = b[ap];
    // RHS is interpolation → not modulo
    if ac == b'#' && ap + 1 < len && b[ap + 1] == b'{' {
      return false;
    }
    // LHS is interpolation → not modulo
    let mut bj = i;
    while bj > 0 && b[bj - 1] == b' ' {
      bj -= 1;
    }
    if bj > 0 && b[bj - 1] == b'}' {
      return false;
    }
    return ac.is_ascii_digit() || ac == b'$' || ac == b'(';
  }
  if i == 0 {
    return false;
  }
  let before = b[i - 1];
  if before.is_ascii_digit() || before == b'.' {
    // 10% -(expr) → modulo, but 10% - (expr) → not modulo
    let sp_after = i + 1 < len && b[i + 1] == b' ';
    if sp_after && b[ap] == b'-' && ap + 1 < len && b[ap + 1] == b'(' {
      return true;
    }
    return false;
  }
  if before == b')' {
    return true;
  }
  if before == b'}' {
    return false;
  }
  // $var%
  let mut j = i - 1;
  while j > 0 && (b[j].is_ascii_alphanumeric() || b[j] == b'-' || b[j] == b'_') {
    j -= 1;
  }
  if j < i && b[j] == b'$' {
    return true;
  }
  false
}

fn is_unary(b: &[u8], i: usize) -> bool {
  if i == 0 {
    return true;
  }
  let p = b[i - 1];
  if p == b'(' || p == b',' || p == b':' {
    return true;
  }
  // Negative number literal: `-` preceded by whitespace and followed by a
  // digit or decimal point (e.g. `inset -0.375rem`). This is a sign, not
  // a subtraction operator.
  if p.is_ascii_whitespace() && i + 1 < b.len() && (b[i + 1].is_ascii_digit() || b[i + 1] == b'.') {
    return true;
  }
  let mut j = i;
  while j > 0 && b[j - 1].is_ascii_whitespace() {
    j -= 1;
  }
  if j == 0 {
    return true;
  }
  let pnw = b[j - 1];
  matches!(
    pnw,
    b'+' | b'*' | b'/' | b',' | b'(' | b':' | b'=' | b'<' | b'>'
  )
}

fn is_comparison_ctx(b: &[u8], i: usize) -> bool {
  let mut j = i;
  while j > 0 && b[j - 1] == b' ' {
    j -= 1;
  }
  if j == 0 {
    return false;
  }
  let p = b[j - 1];
  p.is_ascii_alphanumeric() || p == b')' || p == b'}' || p == b'$'
}

fn count_ws_before(b: &[u8], i: usize) -> usize {
  let mut c = 0;
  let mut j = i;
  while j > 0 && b[j - 1].is_ascii_whitespace() {
    c += 1;
    j -= 1;
  }
  c
}
fn count_ws_after(b: &[u8], i: usize) -> usize {
  let mut c = 0;
  let mut j = i + 1;
  while j < b.len() && b[j].is_ascii_whitespace() {
    c += 1;
    j += 1;
  }
  c
}
fn has_nl_before(b: &[u8], i: usize) -> bool {
  let mut j = i;
  while j > 0 && b[j - 1].is_ascii_whitespace() {
    if b[j - 1] == b'\n' {
      return true;
    }
    j -= 1;
  }
  false
}
fn has_nl_after(b: &[u8], i: usize) -> bool {
  let mut j = i + 1;
  while j < b.len() && b[j].is_ascii_whitespace() {
    if b[j] == b'\n' {
      return true;
    }
    j += 1;
  }
  false
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule};

  fn scss_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.scss",
      source: "",
      syntax: Syntax::Scss,
      options: None,
    }
  }
  fn css_context() -> RuleContext<'static> {
    RuleContext {
      file_path: "test.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }
  fn make_node(property: &str, value: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: property.to_string(),
        value: value.to_string(),
        span: ParserSpan::new(4, value.len()),
        important: false,
      }],
      span: ParserSpan::new(0, value.len() + 20),
      ..Default::default()
    })
  }

  #[test]
  fn skips_css_files() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("width", "10px+5px");
    assert!(rule.check(&node, &css_context()).is_empty());
  }
  #[test]
  fn reports_unspaced_plus() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("width", "$a+$b");
    assert!(!rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn reports_unspaced_multiply() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("width", "$a*2");
    assert!(!rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn allows_spaced_operators() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("width", "$a + $b");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_unary_minus() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("margin", "-10px");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_calc_operators() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("width", "calc(100%+20px)");
    assert!(
      rule.check(&node, &scss_context()).is_empty(),
      "calc operators should be skipped"
    );
  }
  #[test]
  fn skips_font_shorthand_slash() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("font", "14px/1.5 sans-serif");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_unary_after_operator() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("width", "$a * -1");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_border_radius_shorthand_slash() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("border-radius", "10px 5px / 20px 15px");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_border_radius_unspaced_slash() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("border-radius", "10px/5px");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_grid_shorthand_slash() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("grid", "auto-flow / 1fr 1fr 1fr");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_grid_column_slash() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("grid-column", "1 / 3");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_grid_row_slash() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("grid-row", "1/3");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_grid_area_slash() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("grid-area", "1 / 1 / 3 / 3");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_grid_template_slash() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("grid-template", "'a a' 100px / 1fr 1fr");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_background_shorthand_slash() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("background", "url(img.png) center/cover no-repeat");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_list_style_shorthand_slash() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("list-style", "disc outside/inside");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_in_url() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node(
      "background-image",
      "url(https://example.com/path/to/image.png)",
    );
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_in_url_with_quotes() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node(
      "background-image",
      "url('https://example.com/path/to/image.png')",
    );
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_vendor_prefixed_border_radius() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("-webkit-border-radius", "10px/5px");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn still_reports_unspaced_slash_in_math() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("width", "$a/2");
    assert!(!rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_inline_comment() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("box-shadow", "0 0 0 3px blue, // comment");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_negative_value_after_space() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("margin", "-10px");
    assert!(rule.check(&node, &scss_context()).is_empty());
    let node = make_node("top", "-2px");
    assert!(rule.check(&node, &scss_context()).is_empty());
    let node = make_node("box-shadow", "0 -2px red");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn flags_negative_variable_as_subtraction() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("box-shadow", "0 -$offset red");
    assert!(!rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn still_reports_unspaced_subtraction() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("width", "$a-$b");
    assert!(!rule.check(&node, &scss_context()).is_empty());
    let node = make_node("width", "$a -$b");
    assert!(!rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn still_reports_unspaced_operators_in_shorthand() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("border-radius", "$a+$b");
    assert!(!rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_alpha_separator_in_rgb() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("color", "rgb(255 0 0 / 0.5)");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_alpha_separator_in_rgb_unspaced() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("color", "rgb(255 0 0/0.5)");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_alpha_separator_in_rgba() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("color", "rgba(255 0 0 / 50%)");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_alpha_separator_in_hsl() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("color", "hsl(120 100% 50% / .5)");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_alpha_separator_in_hsla() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("color", "hsla(120 100% 50% / 0.5)");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_alpha_separator_in_hwb() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("color", "hwb(120 0% 0% / 0.5)");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_alpha_separator_in_oklch() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("color", "oklch(0.5 0.2 240 / 0.5)");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_alpha_separator_in_rgb_with_var() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("color", "rgb(from var(--color) r g b / 50%)");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_alpha_separator_in_color_function() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("color", "color(srgb 1 0 0 / 0.5)");
    assert!(rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn still_reports_unspaced_division_outside_color_fn() {
    let rule = ScssOperatorNoUnspaced;
    let node = make_node("width", "$a/2");
    assert!(!rule.check(&node, &scss_context()).is_empty());
  }
  #[test]
  fn skips_slash_alpha_in_rgb_variable_assignment() {
    let rule = ScssOperatorNoUnspaced;
    let source = "$color: rgb(255 0 0 / 0.5);\n";
    let ctx = RuleContext {
      file_path: "test.scss",
      source,
      syntax: Syntax::Scss,
      options: None,
    };
    let diags = rule.check_root(&[], &ctx);
    assert!(
      diags.is_empty(),
      "Expected no diagnostics for rgb alpha separator in variable assignment, got: {:?}",
      diags
    );
  }

  #[test]
  fn skips_block_comments_in_values() {
    let rule = ScssOperatorNoUnspaced;
    // Block comment inside a value should not trigger on * or /
    let node = make_node("color", "red /* fallback */ blue");
    assert!(
      rule.check(&node, &scss_context()).is_empty(),
      "Block comment content should not trigger operator spacing"
    );
  }

  #[test]
  fn skips_jsdoc_style_comments_at_source_level() {
    let rule = ScssOperatorNoUnspaced;
    let source =
      "/**\n * @summary Focus state\n * @selector .slds-has-focus\n */\n.foo { color: red; }\n";
    let ctx = RuleContext {
      file_path: "test.scss",
      source,
      syntax: Syntax::Scss,
      options: None,
    };
    let diags = rule.check_root(&[], &ctx);
    assert!(
      diags.is_empty(),
      "JSDoc-style block comments should not trigger operator spacing, got: {:?}",
      diags
    );
  }

  #[test]
  fn skips_block_comment_with_asterisks_in_value() {
    let rule = ScssOperatorNoUnspaced;
    // Simulates a value containing a block comment with star-prefixed lines
    let node = make_node("color", "/* * * * */ red");
    assert!(
      rule.check(&node, &scss_context()).is_empty(),
      "Stars inside block comments should not be flagged"
    );
  }

  #[test]
  fn accepts_operator_at_end_of_line() {
    // Operator at end of line with value on next line — Stylelint allows this.
    // This pattern appears in multi-line SCSS expressions.
    let scss = "$x: 4 *\n  $y;";
    let rule = ScssOperatorNoUnspaced;
    let result = gale_css_parser::parse(scss, Syntax::Scss).unwrap();
    let ctx = RuleContext {
      file_path: "test.scss",
      source: scss,
      syntax: Syntax::Scss,
      options: None,
    };
    let diags: Vec<_> = rule.check_root(&result.nodes, &ctx);
    assert!(
      diags.is_empty(),
      "operator at end of line should not be flagged, got: {:?}",
      diags
    );
  }

  #[test]
  fn accepts_comparison_operator_followed_by_newline() {
    // `== \n    'string'` pattern from _density.scss — the space before `==` is present,
    // and a newline follows — Stylelint does not flag this.
    let scss = "$x: $a ==\n  'string';";
    let rule = ScssOperatorNoUnspaced;
    let result = gale_css_parser::parse(scss, Syntax::Scss).unwrap();
    let ctx = RuleContext {
      file_path: "test.scss",
      source: scss,
      syntax: Syntax::Scss,
      options: None,
    };
    let diags: Vec<_> = rule.check_root(&result.nodes, &ctx);
    assert!(
      diags.is_empty(),
      "== followed by newline should not be flagged, got: {:?}",
      diags
    );
  }
}
