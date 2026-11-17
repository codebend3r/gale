//! Units of dimensions in values, media queries and declarations.

use super::*;

/// A number with a unit in a value, as Stylelint's `getDimension` reads a
/// postcss-value-parser word.
struct Dimension<'v> {
  /// The word's offset in the value.
  index: usize,
  /// The length of its number.
  number_len: usize,
  /// Its unit, as written.
  unit: &'v str,
}

/// Stylelint's `getDimension`: the number length and unit of a standard
/// word that is not a hex color, ignoring the `\0` and `\9` hacks.
fn dimension<'v>(node: &ValueNode<'v>) -> Option<(usize, &'v str)> {
  let word = node.value;
  if !node.is_word() || !standard_syntax::is_standard_syntax_value(word) || word.starts_with('#') {
    return None;
  }
  let (number, unit) = value_parser::unit(word)?;
  let hack = ["\\0", "\\9"].iter().find_map(|hack| unit.find(hack));
  let unit = hack.map_or(unit, |at| &unit[..at]);
  (!unit.is_empty()).then_some((number.len(), unit))
}

/// Every dimension in `value` (postcss-value-parser words), skipping the
/// arguments of the functions `skip` names.
fn dimensions<'v>(value: &'v str, skip: &dyn Fn(&ValueNode) -> bool) -> Vec<Dimension<'v>> {
  let mut out = Vec::new();
  value_parser::walk(&value_parser::parse(value), &mut |node| {
    if node.is_function() {
      return !skip(node);
    }
    if let Some((number_len, unit)) = dimension(node) {
      out.push(Dimension {
        index: node.source_index,
        number_len,
        unit,
      });
    }
    true
  });
  out
}

/// Whether `value` may hold a dimension (`mayIncludeRegexes.dimension`,
/// `/\d[%\w-]/`).
fn may_include_dimension(value: &str) -> bool {
  value.as_bytes().windows(2).any(|pair| {
    pair[0].is_ascii_digit()
      && (pair[1].is_ascii_alphanumeric() || matches!(pair[1], b'_' | b'%' | b'-'))
  })
}

/// What a unit list rule skips: the arguments of some functions, and units
/// its `ignore*` options exempt for a property or media feature.
pub struct UnitFilter<'f> {
  /// Whether the arguments of the function (lowercased name) are skipped;
  /// `url()` always is.
  pub function: &'f dyn Fn(&str) -> bool,
  /// Whether the unit (lowercased) is exempt in a declaration of the
  /// property.
  pub property: &'f dyn Fn(&str, &str) -> bool,
  /// Whether the unit (lowercased) is exempt in the media feature
  /// (lowercased name).
  pub media_feature: &'f dyn Fn(&str, &str) -> bool,
}

impl UnitFilter<'_> {
  /// Whether the arguments of the function `name` are skipped.
  fn skips(&self, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "url" || (self.function)(&name)
  }
}

/// The units of the dimensions in declaration values and `@media` params,
/// as `unit-allowed-list` finds them: postcss-value-parser words (with `*`
/// read as a separator), reported at the unit.
pub fn value_units(tree: &PostcssTree, filter: &UnitFilter, found: &mut Vec<Found>) {
  for (i, node) in tree.nodes.iter().enumerate() {
    let value = match node.kind {
      NodeKind::Decl => node.value.as_str(),
      NodeKind::AtRule if node.name.eq_ignore_ascii_case("media") => node.params.as_str(),
      _ => continue,
    };
    if !may_include_dimension(value) {
      continue;
    }
    let base = node.value_span.start;
    let value = value.replace('*', ",");
    for dimension in dimensions(&value, &|function| filter.skips(function.value)) {
      let unit = dimension.unit.to_ascii_lowercase();
      if node.kind == NodeKind::Decl && (filter.property)(&unit, &node.name) {
        continue;
      }
      let start = base + dimension.index + dimension.number_len;
      found.push(Found::new(
        dimension.unit,
        i,
        start,
        start + dimension.unit.len(),
      ));
    }
  }
}

/// The units of the dimension and percentage tokens in declaration values
/// and in the media features of `@media` params, as `unit-disallowed-list`
/// finds them with the CSS tokenizer, reported at the unit.  `unicode-range`
/// in `@font-face` is skipped.
pub fn token_units(tree: &PostcssTree, filter: &UnitFilter, found: &mut Vec<Found>) {
  for (i, node) in tree.nodes.iter().enumerate() {
    let is_media = node.kind == NodeKind::AtRule && node.name.eq_ignore_ascii_case("media");
    let is_decl = node.kind == NodeKind::Decl && !is_unicode_range_descriptor(tree, i);
    if !is_media && !is_decl {
      continue;
    }
    let (raw, base) = raw_params(tree, node);
    if !may_include_dimension(raw) {
      continue;
    }
    let mut report = |token: &Token, exempt: bool| {
      if !exempt {
        let unit = token_unit(token).unwrap_or("");
        let start = base + token.end - unit.len();
        found.push(Found::new(unit, i, start, start + unit.len()));
      }
    };
    if is_media {
      for feature in media_features(raw) {
        let name = feature.name.value.to_ascii_lowercase();
        for token in feature.dimensions(true) {
          let unit = token_unit(token).unwrap_or("").to_ascii_lowercase();
          report(token, (filter.media_feature)(&unit, &name));
        }
      }
    } else {
      let values = css_tokenizer::parse_list_of_component_values(css_tokenizer::tokenize(raw));
      let mut tokens = Vec::new();
      unit_tokens(&values, filter, &mut tokens);
      for token in tokens {
        let unit = token_unit(token).unwrap_or("").to_ascii_lowercase();
        report(token, (filter.property)(&unit, &node.name));
      }
    }
  }
}

/// The dimension and percentage tokens among `values`, outside the
/// functions `filter` skips.
fn unit_tokens<'v>(values: &'v [ComponentValue], filter: &UnitFilter, out: &mut Vec<&'v Token>) {
  for value in values {
    match value {
      ComponentValue::Token(token) if token_unit(token).is_some() => out.push(token),
      ComponentValue::Function { name, value, .. } if !filter.skips(&name.value) => {
        unit_tokens(value, filter, out)
      }
      ComponentValue::SimpleBlock { value, .. } => unit_tokens(value, filter, out),
      _ => {}
    }
  }
}

/// The unit of a dimension token, or `%` for a percentage token (Stylelint
/// reads percentages as dimensions of unit `%`).
fn token_unit(token: &Token) -> Option<&str> {
  match token.kind {
    TokenType::Dimension => Some(token.unit.as_str()),
    TokenType::Percentage => Some("%"),
    _ => None,
  }
}

/// Whether the declaration is `unicode-range` in `@font-face`
/// (`isUnicodeRangeDescriptor`).
fn is_unicode_range_descriptor(tree: &PostcssTree, i: usize) -> bool {
  let node = &tree.nodes[i];
  node.name.eq_ignore_ascii_case("unicode-range")
    && node.parent.is_some_and(|parent| {
      let parent = &tree.nodes[parent];
      parent.kind == NodeKind::AtRule && parent.name.eq_ignore_ascii_case("font-face")
    })
}

/// Every declaration with its property and value as PostCSS reads them,
/// and where the value starts: for the rules keyed by property.
fn declarations<'t>(tree: &'t PostcssTree) -> impl Iterator<Item = (usize, &'t Node, usize)> + 't {
  nodes_of(tree, NodeKind::Decl).map(|(i, node)| (i, node, node.value_span.start))
}

/// The value of every declaration, keyed by its property:
/// `declaration-property-value-*-list`.
pub fn declaration_values(tree: &PostcssTree, found: &mut Vec<KeyedFound>) {
  for (_, node, start) in declarations(tree) {
    found.push(KeyedFound::new(
      node.name.as_str(),
      node.value.as_str(),
      start,
      start + node.value.len(),
    ));
  }
}

/// The unit of every dimension in a declaration value outside `url()`,
/// keyed by the property and reported at the number:
/// `declaration-property-unit-*-list`.  With `skip_functions`, the
/// arguments of every function are skipped too.
pub fn declaration_units(tree: &PostcssTree, skip_functions: bool, found: &mut Vec<KeyedFound>) {
  for (_, node, start) in declarations(tree) {
    let skip = |function: &ValueNode| skip_functions || function.is_function_named("url");
    for dimension in dimensions(&node.value, &skip) {
      let at = start + dimension.index;
      found.push(KeyedFound::new(
        node.name.as_str(),
        dimension.unit,
        at,
        at + dimension.number_len + dimension.unit.len(),
      ));
    }
  }
}
