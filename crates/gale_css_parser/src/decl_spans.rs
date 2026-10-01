//! Locating lightningcss declarations in the source text.
//!
//! lightningcss hands back declarations without positions, and splits a
//! block's `!important` declarations from its normal ones.  The parser finds
//! each declaration again by searching the block's source for its property
//! name followed by optional whitespace and a colon.
//!
//! The search is deliberately textual (it does not skip comments or strings),
//! and diagnostics positions depend on it, so every function here reproduces
//! the original search result for result while doing a single forward pass
//! per block instead of one pass per property name per declaration.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::{Declaration, Span};

/// Whether `b` continues an identifier for the purposes of the search: an
/// ASCII letter or digit, `-` or `_`.
///
/// A property name preceded by one of these bytes is part of a longer word
/// (`border` inside `flex-border`) and is not a declaration.
fn is_ident_byte(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// End offset of a declaration whose property name ends at `after_prop`.
///
/// The declaration runs through the next `;`, or up to (not including) the
/// next `}` when there is no `;` before `to`, or is just the name when there
/// is neither.
fn declaration_end(source: &str, after_prop: usize, to: usize) -> usize {
  let rest = &source.as_bytes()[after_prop..to];
  match rest.iter().position(|&b| b == b';') {
    Some(i) => after_prop + i + 1,
    None => rest
      .iter()
      .position(|&b| b == b'}')
      .map_or(after_prop, |i| after_prop + i),
  }
}

/// Search for a CSS declaration (`property-name: ...;` or `property-name: ... }`)
/// in the source text between `from` and `to`, returning its span.
///
/// The name is matched ASCII case-insensitively, must be followed by optional
/// whitespace and a `:`, and must not be preceded by an identifier byte
/// (except at `from` itself, which is never checked).  Returns an empty span
/// when there is no such occurrence or `from..to` does not slice `source`.
pub(crate) fn find_declaration_span(source: &str, from: usize, to: usize, property: &str) -> Span {
  let to = to.min(source.len());
  let Some(area) = source.get(from..to) else {
    return Span::empty();
  };
  let hay = area.as_bytes();
  let needle = property.as_bytes();
  let Some(&first) = needle.first() else {
    return Span::empty();
  };
  let first = first.to_ascii_lowercase();

  let mut search_from = 0;
  while search_from + needle.len() <= hay.len() {
    // The leftmost case-insensitive occurrence at or after `search_from`.
    let last_start = hay.len() - needle.len();
    let Some(rel_idx) = (search_from..=last_start).find(|&i| {
      hay[i].to_ascii_lowercase() == first && hay[i..i + needle.len()].eq_ignore_ascii_case(needle)
    }) else {
      return Span::empty();
    };

    let after_name = rel_idx + needle.len();
    let is_declaration = area[after_name..].trim_start().starts_with(':');
    let preceded_by_ident = rel_idx > 0 && is_ident_byte(hay[rel_idx - 1]);

    if is_declaration && !preceded_by_ident {
      let abs_start = from + rel_idx;
      let decl_end = declaration_end(source, from + after_name, to);
      return Span::new(abs_start, decl_end - abs_start);
    }

    // Resume after this occurrence, not inside it.
    search_from = after_name;
  }
  Span::empty()
}

/// Remembers the next position of one byte so that repeated searches with a
/// non-decreasing start offset scan each byte of the block at most once.
struct NextByte {
  byte: u8,
  /// The start offset of the last scan and what it found.
  cached: Option<(usize, Option<usize>)>,
}

impl NextByte {
  /// A finder for `byte` with nothing cached.
  fn new(byte: u8) -> Self {
    Self { byte, cached: None }
  }

  /// The first position of the byte in `hay[pos..]`, as an index into `hay`.
  ///
  /// Callers must pass non-decreasing `pos` values for the same `hay`.
  fn find(&mut self, hay: &[u8], pos: usize) -> Option<usize> {
    if let Some((from, found)) = self.cached
      && from <= pos
      && found.is_none_or(|at| at >= pos)
    {
      return found;
    }
    let found = hay[pos..]
      .iter()
      .position(|&b| b == self.byte)
      .map(|i| pos + i);
    self.cached = Some((pos, found));
    found
  }
}

/// Whether two occurrences of `name` can overlap: some proper prefix of it
/// is also a suffix of it.
fn can_overlap_itself(name: &[u8]) -> bool {
  // The last entry of the KMP failure table is the longest such border.
  let mut border = vec![0usize; name.len()];
  let mut k = 0;
  for i in 1..name.len() {
    while k > 0 && name[i] != name[k] {
      k = border[k - 1];
    }
    if name[i] == name[k] {
      k += 1;
    }
    border[i] = k;
  }
  border.last().is_some_and(|&k| k > 0)
}

/// Whether [`locate_declarations`] can find `name` (lowercased) in its
/// single pass with exactly the result [`find_declaration_span`] gives.
///
/// The pass looks for names ending just before the whitespace in front of
/// each colon, so a name must not contain whitespace or a colon itself.
/// The per-name search also resumes after a rejected occurrence rather than
/// inside it, which can skip an overlapping occurrence the pass would see;
/// that skipped occurrence can only be accepted when the name contains a
/// non-identifier byte and can overlap itself, so such names are excluded.
fn scannable(name: &str) -> bool {
  !name.is_empty()
    && !name.contains(':')
    && !name.chars().any(char::is_whitespace)
    && (name.bytes().all(is_ident_byte) || !can_overlap_itself(name.as_bytes()))
}

/// Spans of the declarations of `properties` in `from..to`, in source order.
///
/// Repeatedly takes the earliest declaration of any of the names (as
/// [`find_declaration_span`] defines one) and resumes after it, until one
/// span per entry of `properties` is found or nothing is left.  Returns the
/// spans and the offset just after the last one (`from` when none matched).
pub(crate) fn locate_declarations(
  source: &str,
  from: usize,
  to: usize,
  properties: &[String],
) -> (Vec<Span>, usize) {
  let to = to.min(source.len());
  let names: HashSet<String> = properties.iter().map(|p| p.to_ascii_lowercase()).collect();
  if !names.iter().all(|n| scannable(n)) {
    return locate_declarations_by_name(source, from, to, properties);
  }
  let max_len = names.iter().map(String::len).max().unwrap_or(0);
  let mut has_len = vec![false; max_len + 1];
  for name in &names {
    has_len[name.len()] = true;
  }

  let mut spans = Vec::with_capacity(properties.len());
  let mut sf = from;
  let mut semicolons = NextByte::new(b';');
  let mut braces = NextByte::new(b'}');
  let mut candidate = String::new();
  let hay = source.as_bytes();

  'decls: while spans.len() < properties.len() && sf < to {
    let Some(area) = source.get(sf..to) else {
      break;
    };
    let bytes = area.as_bytes();
    let mut colon_from = 0;
    while let Some(i) = bytes[colon_from..].iter().position(|&b| b == b':') {
      let colon = colon_from + i;
      colon_from = colon + 1;

      // A declaration's name ends just before the whitespace in front of
      // its colon, and starts no earlier than the last whitespace or colon
      // before that (names contain neither) or `max_len` bytes back.
      let name_end = area[..colon].trim_end().len();
      let mut lowest = name_end.saturating_sub(max_len);
      while !area.is_char_boundary(lowest) {
        lowest += 1;
      }
      let run_start = area[lowest..name_end]
        .char_indices()
        .rev()
        .find(|&(_, ch)| ch == ':' || ch.is_whitespace())
        .map_or(lowest, |(i, ch)| lowest + i + ch.len_utf8());

      // The earliest start that names a property and is not inside a
      // longer word.  At `sf` itself the byte before is not checked,
      // exactly as in `find_declaration_span`.
      let found = (run_start..name_end).find(|&start| {
        if !has_len[name_end - start]
          || !area.is_char_boundary(start)
          || (start > 0 && is_ident_byte(bytes[start - 1]))
        {
          return false;
        }
        candidate.clear();
        candidate.push_str(&area[start..name_end]);
        candidate.make_ascii_lowercase();
        names.contains(candidate.as_str())
      });
      let Some(name_start) = found else {
        continue;
      };

      let abs_start = sf + name_start;
      let after_prop = sf + name_end;
      let decl_end = match semicolons.find(&hay[..to], after_prop) {
        Some(semi) => semi + 1,
        None => braces.find(&hay[..to], after_prop).unwrap_or(after_prop),
      };
      spans.push(Span::new(abs_start, decl_end - abs_start));
      sf = decl_end;
      continue 'decls;
    }
    break;
  }
  (spans, sf)
}

/// The original declaration search: for each step, look up every name with
/// [`find_declaration_span`] and keep the earliest hit.
///
/// Only used for names [`locate_declarations`] cannot scan for in one pass.
fn locate_declarations_by_name(
  source: &str,
  from: usize,
  to: usize,
  properties: &[String],
) -> (Vec<Span>, usize) {
  let mut sorted: Vec<&String> = properties.iter().collect();
  sorted.sort();
  sorted.dedup();
  let mut spans = Vec::with_capacity(properties.len());
  let mut sf = from;
  while spans.len() < properties.len() && sf < to {
    let mut best = Span::empty();
    for name in &sorted {
      let span = find_declaration_span(source, sf, to, name);
      if span.length > 0 && (best.length == 0 || span.offset < best.offset) {
        best = span;
      }
    }
    if best.length == 0 {
      break;
    }
    spans.push(best);
    sf = best.offset + best.length;
  }
  (spans, sf)
}

/// A declaration lowered from lightningcss before its span is known.
pub(crate) struct ProtoDeclaration {
  /// Property name with any vendor prefix restored.
  pub property: String,
  /// Serialized value.
  pub value: String,
  /// Whether lightningcss filed it under the `!important` declarations.
  pub important: bool,
}

/// Pair the spans found in the source with the lowered declarations.
///
/// Each span goes to the first unused declaration with the same property
/// (compared ASCII case-insensitively) whose importance matches the span's
/// text, falling back to the first unused one with that property.  Spans
/// that match nothing are dropped; declarations left without a span are
/// appended at the end with an empty span.
pub(crate) fn assign_spans(
  source: &str,
  spans: &[Span],
  protos: Vec<ProtoDeclaration>,
) -> Vec<Declaration> {
  // Unused declaration indices per lowercase property, split by importance
  // and kept in source order.
  let mut queues: HashMap<String, [VecDeque<usize>; 2]> = HashMap::new();
  for (i, proto) in protos.iter().enumerate() {
    queues
      .entry(proto.property.to_ascii_lowercase())
      .or_default()[usize::from(proto.important)]
    .push_back(i);
  }

  let mut slots: Vec<Option<ProtoDeclaration>> = protos.into_iter().map(Some).collect();
  let mut declarations = Vec::with_capacity(slots.len());
  for span in spans {
    let span_text = source
      .get(span.offset..(span.offset + span.length).min(source.len()))
      .unwrap_or("");
    let important = span_text.contains("!important") || span_text.contains("! important");
    let span_prop = span_text
      .split(':')
      .next()
      .unwrap_or("")
      .to_ascii_lowercase();
    let Some(queue) = queues.get_mut(span_prop.trim()) else {
      continue;
    };
    let [normal, bang] = queue;
    let (same, other) = if important {
      (bang, normal)
    } else {
      (normal, bang)
    };
    let Some(idx) = same.pop_front().or_else(|| other.pop_front()) else {
      continue;
    };
    if let Some(proto) = slots[idx].take() {
      declarations.push(Declaration {
        property: proto.property,
        value: proto.value,
        span: *span,
        important: proto.important,
      });
    }
  }

  declarations.extend(slots.into_iter().flatten().map(|proto| Declaration {
    property: proto.property,
    value: proto.value,
    span: Span::empty(),
    important: proto.important,
  }));
  declarations
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The declaration search as it was before the single-pass rewrite,
  /// kept verbatim as the oracle the new code must agree with.
  fn legacy_find(source: &str, from: usize, to: usize, property: &str) -> Span {
    let area = source.get(from..to.min(source.len())).unwrap_or("");
    let lower_area = area.to_ascii_lowercase();
    let lower_prop = property.to_ascii_lowercase();
    let mut search_from = 0;
    loop {
      let rel_idx = match lower_area[search_from..].find(&lower_prop) {
        Some(i) => search_from + i,
        None => return Span::empty(),
      };
      let after_name = rel_idx + lower_prop.len();
      let rest_of_area = &lower_area[after_name..];
      let trimmed = rest_of_area.trim_start();
      let is_declaration = trimmed.starts_with(':');
      let preceded_by_ident = rel_idx > 0 && {
        let prev = lower_area.as_bytes()[rel_idx - 1];
        prev.is_ascii_alphanumeric() || prev == b'-' || prev == b'_'
      };
      if is_declaration && !preceded_by_ident {
        let abs_start = from + rel_idx;
        let after_prop = abs_start + property.len();
        let rest = &source[after_prop..to.min(source.len())];
        let decl_end = rest
          .find(';')
          .map(|i| after_prop + i + 1)
          .unwrap_or_else(|| rest.find('}').map(|i| after_prop + i).unwrap_or(after_prop));
        return Span::new(abs_start, decl_end - abs_start);
      }
      search_from = after_name;
    }
  }

  /// The original multi-name loop from `convert_style_rule`, verbatim.
  fn legacy_locate(
    source: &str,
    from: usize,
    to: usize,
    properties: &[String],
  ) -> (Vec<Span>, usize) {
    let total_decls = properties.len();
    let mut spans_in_order: Vec<Span> = Vec::with_capacity(total_decls);
    let mut sf = from;
    let mut prop_names: Vec<String> = properties.to_vec();
    prop_names.sort();
    prop_names.dedup();
    let mut found_count = 0;
    while found_count < total_decls && sf < to {
      let mut best_span = Span::empty();
      let mut best_offset = usize::MAX;
      for pname in &prop_names {
        let span = legacy_find(source, sf, to, pname);
        if span.length > 0 && span.offset < best_offset {
          best_offset = span.offset;
          best_span = span;
        }
      }
      if best_span.length == 0 {
        break;
      }
      spans_in_order.push(best_span);
      sf = best_span.offset + best_span.length;
      found_count += 1;
    }
    (spans_in_order, sf)
  }

  /// Owned property names from string literals.
  fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
  }

  /// Inputs the textual search has to cope with: comments and strings that
  /// mention property names, nested blocks and selectors with colons,
  /// escapes, `!important`, odd whitespace and case, and missing semicolons.
  const TRICKY: &[(&str, &[&str])] = &[
    (
      "a { color: red; background: blue; }",
      &["color", "background"],
    ),
    ("a { COLOR : red; Color:blue }", &["color", "color"]),
    ("a { /* color: green; */ color: red; }", &["color"]),
    (
      "a { content: \"color: x; y\"; color: red; }",
      &["content", "color"],
    ),
    ("a { content: 'a;b'; color: red }", &["content", "color"]),
    (
      "a { color: red !important; color: blue; }",
      &["color", "color"],
    ),
    (
      "a { color: red ! important; margin: 0 }",
      &["color", "margin"],
    ),
    (
      "a { --x: { color: red; }; color: blue; }",
      &["--x", "color"],
    ),
    ("a { --Foo: 1; --foo: 2; }", &["--Foo", "--foo"]),
    ("a { --a:b:c; --b: url(http://x.y/z); }", &["--a", "--b"]),
    ("a { &:hover { color: red; } color: blue; }", &["color"]),
    ("a { .color:hover { x: y } color: blue; }", &["color"]),
    ("a { b{color:red} color: blue }", &["color", "color"]),
    (
      "a { border-color: red; border: 0; }",
      &["border-color", "border"],
    ),
    ("a { flex-border: 1; border: 0; }", &["border"]),
    ("a { _zoom: 1; zoom: 1; }", &["zoom"]),
    ("a { \\color: red; color: blue; }", &["color"]),
    ("a { co\\lor: red; color: blue; }", &["color"]),
    ("a {\n\tcolor\n\t:\n red\n}", &["color"]),
    (
      "a { color\u{a0}: red; color\u{2003}:blue }",
      &["color", "color"],
    ),
    (
      "a { width: calc(1px + 2px); height: 0 }",
      &["width", "height"],
    ),
    ("a { color: red; }", &["background"]),
    ("a { color: red", &["color"]),
    ("a { color", &["color"]),
    ("a { color: red; color: blue; color: green; }", &["color"]),
    (
      "a{color:red;margin:0;padding:0;}",
      &["padding", "margin", "color"],
    ),
    ("a { --é: 1; color: red; }", &["--é", "color"]),
    ("a { --a€--a€: 1; --a€: 2; }", &["--a€", "--a€--a€"]),
    ("a { color:red;; ;color:blue }", &["color", "color"]),
    (
      "a { grid-template-areas: \"a:b\"; grid-area: a }",
      &["grid-template-areas", "grid-area"],
    ),
    ("@media (min-width: 1px) { a { color: red } }", &["color"]),
    (
      "a { background: url(data:image/png;base64,xyz); color: red; }",
      &["background", "color"],
    ),
    // lightningcss unescapes names, so these never appear verbatim.
    ("a { --a\\.b: 1; --c: 2; }", &["--a.b", "--c"]),
    ("a { --a.b: 1; x.--a.b: 2; .--a.b:3 }", &["--a.b"]),
    (
      "a { --x.y-: 1; --x.y--x.y-: 2 }",
      &["--x.y-", "--x.y--x.y-"],
    ),
    ("a { --a\\:b: 1; --a: 2 }", &["--a:b", "--a"]),
    ("a { --a\\ b: 1; --a b: 2 }", &["--a b", "--a"]),
    ("a{--é.x:1;--é.x:2}", &["--é.x", "--é.x"]),
  ];

  #[test]
  fn find_declaration_span_matches_the_original_search() {
    for (css, props) in TRICKY {
      for from in 0..=css.len() {
        for prop in *props {
          assert_eq!(
            find_declaration_span(css, from, css.len(), prop),
            legacy_find(css, from, css.len(), prop),
            "{css:?} from {from} for {prop:?}"
          );
        }
      }
    }
  }

  #[test]
  fn locate_declarations_matches_the_original_search() {
    for (css, props) in TRICKY {
      let props = names(props);
      for from in 0..=css.len() {
        for to in [css.len(), css.len().saturating_sub(1), css.len() / 2] {
          if from > to {
            continue;
          }
          assert_eq!(
            locate_declarations(css, from, to, &props),
            legacy_locate(css, from, to, &props),
            "{css:?} in {from}..{to} for {props:?}"
          );
        }
      }
    }
  }

  /// A deterministic xorshift generator, so the randomized comparison needs
  /// no extra dependency and fails reproducibly.
  struct XorShift(u64);

  impl XorShift {
    /// The next pseudo-random index below `n`.
    fn below(&mut self, n: usize) -> usize {
      self.0 ^= self.0 << 13;
      self.0 ^= self.0 >> 7;
      self.0 ^= self.0 << 17;
      (self.0 % n as u64) as usize
    }
  }

  #[test]
  fn locate_declarations_matches_the_original_search_on_random_blocks() {
    const PIECES: &[&str] = &[
      "color",
      "Color",
      "border",
      "border-color",
      "--x",
      "--X",
      "a",
      ":",
      ": ",
      " : ",
      ";",
      " ",
      "\n",
      "{",
      "}",
      "/* color: red */",
      "\"c;o:l\"",
      "'}'",
      "!important",
      "! important",
      "red",
      "&:hover",
      ".color",
      "-",
      "_",
      "\\",
      "url(a:b)",
      "é",
      "\u{a0}",
      "--x.y",
      ".",
      "c;o",
      "--é",
      "-}-",
      "--x-",
    ];
    const PROPS: &[&str] = &[
      "color",
      "border",
      "border-color",
      "--x",
      "--X",
      "a",
      "--x.y",
      "c;o",
      "--é",
      "-}-",
      "--x-",
    ];
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    for _ in 0..4000 {
      let mut css = String::from("x {");
      for _ in 0..rng.below(24) {
        css.push_str(PIECES[rng.below(PIECES.len())]);
      }
      css.push('}');
      let props: Vec<String> = (0..1 + rng.below(4))
        .map(|_| PROPS[rng.below(PROPS.len())].to_string())
        .collect();
      for from in [0, 1, 2, 3] {
        assert_eq!(
          locate_declarations(&css, from, css.len(), &props),
          legacy_locate(&css, from, css.len(), &props),
          "{css:?} from {from} for {props:?}"
        );
      }
    }
  }

  /// The original span-to-declaration pairing from `convert_style_rule`,
  /// verbatim apart from taking its inputs as parameters.
  fn legacy_assign(
    source: &str,
    spans_in_order: &[Span],
    proto_decls: &[(String, String, bool)],
  ) -> Vec<Declaration> {
    let mut matched: Vec<bool> = vec![false; proto_decls.len()];
    let mut declarations = Vec::new();
    for span in spans_in_order {
      let span_text = source
        .get(span.offset..(span.offset + span.length).min(source.len()))
        .unwrap_or("");
      let is_important_in_source =
        span_text.contains("!important") || span_text.contains("! important");
      let span_lower = span_text.to_ascii_lowercase();
      let span_prop = span_lower.split(':').next().unwrap_or("").trim();
      let mut found_idx = None;
      for (i, (prop, _, important)) in proto_decls.iter().enumerate() {
        if !matched[i]
          && prop.to_ascii_lowercase() == span_prop
          && *important == is_important_in_source
        {
          found_idx = Some(i);
          break;
        }
      }
      if found_idx.is_none() {
        for (i, (prop, _, _)) in proto_decls.iter().enumerate() {
          if !matched[i] && prop.to_ascii_lowercase() == span_prop {
            found_idx = Some(i);
            break;
          }
        }
      }
      if let Some(idx) = found_idx {
        matched[idx] = true;
        let (ref prop, ref value, important) = proto_decls[idx];
        declarations.push(Declaration {
          property: prop.clone(),
          value: value.clone(),
          span: *span,
          important,
        });
      }
    }
    for (i, (prop, value, important)) in proto_decls.iter().enumerate() {
      if !matched[i] {
        declarations.push(Declaration {
          property: prop.clone(),
          value: value.clone(),
          span: Span::empty(),
          important: *important,
        });
      }
    }
    declarations
  }

  #[test]
  fn assign_spans_matches_the_original_pairing() {
    for (css, props) in TRICKY {
      // Every importance pattern for the declarations, normal ones first as
      // lightningcss lists them.
      for mask in 0..(1u32 << props.len()) {
        let mut protos: Vec<(String, String, bool)> = props
          .iter()
          .enumerate()
          .map(|(i, p)| (p.to_string(), format!("v{i}"), mask & (1 << i) != 0))
          .collect();
        protos.sort_by_key(|(_, _, important)| *important);
        let names: Vec<String> = protos.iter().map(|(p, _, _)| p.clone()).collect();
        let (spans, _) = legacy_locate(css, 0, css.len(), &names);
        let ours = assign_spans(
          css,
          &spans,
          protos
            .iter()
            .map(|(property, value, important)| ProtoDeclaration {
              property: property.clone(),
              value: value.clone(),
              important: *important,
            })
            .collect(),
        );
        assert_eq!(
          ours,
          legacy_assign(css, &spans, &protos),
          "{css:?} with {protos:?}"
        );
      }
    }
  }

  #[test]
  fn assign_spans_pairs_importance_and_keeps_leftovers() {
    let css = "a { color: red !important; color: blue; margin: 0 }";
    let protos = vec![
      ProtoDeclaration {
        property: "color".into(),
        value: "blue".into(),
        important: false,
      },
      ProtoDeclaration {
        property: "margin".into(),
        value: "0".into(),
        important: false,
      },
      ProtoDeclaration {
        property: "color".into(),
        value: "red".into(),
        important: true,
      },
      ProtoDeclaration {
        property: "padding".into(),
        value: "0".into(),
        important: false,
      },
    ];
    let props: Vec<String> = protos.iter().map(|p| p.property.clone()).collect();
    let (spans, _) = locate_declarations(css, 0, css.len(), &props);
    let decls = assign_spans(css, &spans, protos);
    let summary: Vec<(&str, &str, bool, &str)> = decls
      .iter()
      .map(|d| {
        (
          d.property.as_str(),
          d.value.as_str(),
          d.important,
          &css[d.span.offset..d.span.offset + d.span.length],
        )
      })
      .collect();
    assert_eq!(
      summary,
      vec![
        ("color", "red", true, "color: red !important;"),
        ("color", "blue", false, "color: blue;"),
        ("margin", "0", false, "margin: 0 "),
        ("padding", "0", false, ""),
      ]
    );
  }
}
