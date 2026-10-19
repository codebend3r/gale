//! The subjects list rules match: each finder walks the file's
//! [`PostcssTree`] the way the upstream rule walks PostCSS's, and yields
//! what the rule matches against its list with where to report it.
//!
//! Values are split with [`value_parser`] (postcss-value-parser), media
//! queries and raw values with [`css_tokenizer`] (`@csstools/css-tokenizer`)
//! and selectors with [`selector::postcss`] (postcss-selector-parser),
//! whichever the upstream rule uses, so the pieces and their offsets agree
//! with Stylelint's.

use gale_css_parser::Syntax;

use super::{Found, KeyedFound};
use crate::css_tokenizer::{self, ComponentValue, Token, TokenType};
use crate::postcss_tree::{Node, NodeKind, PostcssTree, has_delimited};
use crate::selector::postcss::{self as selector, Kind};
use crate::standard_syntax;
use crate::value_parser::{self, ValueNode};

/// Every node of `kind`, with its index, in document order.
fn nodes_of<'t>(tree: &'t PostcssTree, kind: NodeKind) -> impl Iterator<Item = (usize, &'t Node)> {
  tree
    .nodes
    .iter()
    .enumerate()
    .filter(move |(_, node)| node.kind == kind)
}

/// Every at-rule whose name is `name`, ignoring case (Stylelint's
/// `atRuleRegexes.*Name`).
fn at_rules_named<'t>(
  tree: &'t PostcssTree,
  name: &'static str,
) -> impl Iterator<Item = (usize, &'t Node)> {
  nodes_of(tree, NodeKind::AtRule).filter(move |(_, node)| node.name.eq_ignore_ascii_case(name))
}

/// An at-rule's params or a declaration's value as written, comments
/// included (`getAtRuleParams`, `getDeclarationValue`), and where it
/// starts.
fn raw_params<'t>(tree: &PostcssTree<'t>, node: &Node) -> (&'t str, usize) {
  let span = node.value_span.clone();
  (tree.source().get(span.clone()).unwrap_or(""), span.start)
}

/// Every standard style rule (`isStandardSyntaxRule`), with its selector as
/// written (`getRuleSelector`) and where that starts.
fn standard_rules<'t>(
  tree: &'t PostcssTree,
) -> impl Iterator<Item = (usize, &'t Node, &'t str)> + 't {
  nodes_of(tree, NodeKind::Rule)
    .filter(|&(i, _)| tree.is_standard_syntax_rule(i))
    .map(|(i, node)| {
      let raw = tree.source().get(node.name_span.clone()).unwrap_or("");
      (i, node, raw)
    })
}

/// Whether the rule sits directly in `@keyframes` (`isKeyframeRule`).
fn is_keyframe_rule(tree: &PostcssTree, node: &Node) -> bool {
  node.parent.is_some_and(|parent| {
    let parent = &tree.nodes[parent];
    parent.kind == NodeKind::AtRule && parent.name.eq_ignore_ascii_case("keyframes")
  })
}

/// At-rule names, without the `@`: `at-rule-*-list`.  Non-standard
/// at-rules (`@charset`, Less mixins and variables, SCSS `@content`) are
/// skipped.
pub fn at_rule_names(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::AtRule) {
    if tree.is_standard_syntax_at_rule(i) {
      found.push(Found::new(
        node.name.as_str(),
        i,
        node.start,
        node.start + 1 + node.name.len(),
      ));
    }
  }
}

/// Declaration properties: `property-*-list`.  Sass and Less variables,
/// Less merges and interpolated properties are skipped.
pub fn property_names(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::Decl) {
    if standard_syntax::is_standard_syntax_property(&node.name) {
      found.push(Found::new(
        node.name.as_str(),
        i,
        node.name_span.start,
        node.name_span.end,
      ));
    }
  }
}

/// The text of every comment but a `/*# sourceMappingURL */` one, reported
/// where the text starts: `comment-word-disallowed-list`.
pub fn comment_texts(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::Comment) {
    let raw = tree.text(i);
    if raw.starts_with("/*# ") {
      continue;
    }
    let at = raw.find(node.name.as_str()).unwrap_or(0);
    let start = node.start + at;
    found.push(Found::new(
      node.name.as_str(),
      i,
      start,
      start + node.name.len(),
    ));
  }
}

/// The name of every function in a declaration value: `function-*-list`.
/// Nameless parentheses and interpolated names are skipped.
pub fn function_names(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::Decl) {
    if !node.value.contains('(') {
      continue;
    }
    let base = node.value_span.start;
    value_parser::walk(&value_parser::parse(&node.value), &mut |value| {
      if value.is_function() && standard_syntax::is_standard_syntax_function(value) {
        let start = base + value.source_index;
        found.push(Found::new(value.value, i, start, start + value.value.len()));
      }
      true
    });
  }
}

/// Stylelint's `hasPrevPropertyDeclaration`: whether a declaration of the
/// same property (ignoring case) comes before declaration `i` in its block,
/// the fallback `function-allowed-list`'s `exceptWithoutPropertyFallback`
/// asks for.  Never for a custom property.
pub fn has_fallback_declaration(tree: &PostcssTree, i: usize) -> bool {
  let prop = tree.nodes[i].name.to_ascii_lowercase();
  if prop.starts_with("--") {
    return false;
  }
  let mut prev = tree.prev(i);
  while let Some(p) = prev {
    let node = &tree.nodes[p];
    if node.kind == NodeKind::Decl && node.name.eq_ignore_ascii_case(&prop) {
      return true;
    }
    prev = tree.prev(p);
  }
  false
}

/// The scheme of every `url()` in a declaration, lowercased as Stylelint
/// lowercases the whole declaration first, reported at the argument:
/// `function-url-scheme-*-list`.
pub fn url_schemes(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, node) in nodes_of(tree, NodeKind::Decl) {
    let text = tree.text(i).trim_end_matches(';').to_ascii_lowercase();
    for (args, index) in function_arguments(&text, "url") {
      let unspaced = args.trim();
      if !is_standard_syntax_url(unspaced) {
        continue;
      }
      let url = unspaced.trim_matches(['\'', '"']);
      if let Some(scheme) = url_scheme(url) {
        let start = node.start + index;
        found.push(Found::new(scheme, i, start, start + args.len()));
      }
    }
  }
}

/// Stylelint's `functionArgumentsSearch`: the argument text of every call
/// to `name` in `source`, with where it starts.
fn function_arguments<'s>(source: &'s str, name: &str) -> Vec<(&'s str, usize)> {
  let mut out = Vec::new();
  value_parser::walk(&value_parser::parse(source), &mut |node| {
    if !node.is_function() || node.value != name {
      return true;
    }
    let open = node.source_index + node.value.len();
    let start = open + 1;
    let end = if node.value.eq_ignore_ascii_case("url") {
      // Re-parse from the `(` as a nameless group, as upstream does, so
      // interpolation inside an unquoted url keeps its extent.
      let group_end = value_parser::parse(source.get(open..).unwrap_or(""))
        .first()
        .map_or(1, |group| group.source_end_index);
      let close = source.as_bytes().get((start + group_end).wrapping_sub(2)) == Some(&b')');
      (start + group_end - 1).saturating_sub(usize::from(close))
    } else {
      let close = source.as_bytes().get(node.source_end_index.wrapping_sub(1)) == Some(&b')');
      node.source_end_index - usize::from(close)
    };
    if let Some(args) = source.get(start..end.max(start)) {
      out.push((args, start));
    }
    true
  });
  out
}

/// Stylelint's `isStandardSyntaxUrl`: no interpolation (only Less's inside
/// quotes), no Less variable as the whole url, no unquoted Sass variable.
fn is_standard_syntax_url(url: &str) -> bool {
  if url.is_empty() {
    return true;
  }
  if has_delimited(url, "#{", '}', true)
    || has_delimited(url, "{", '}', true)
    || has_delimited(url, "$(", ')', false)
  {
    return false;
  }
  let quoted =
    (url.starts_with('\'') && url.ends_with('\'')) || (url.starts_with('"') && url.ends_with('"'));
  if quoted {
    return !has_delimited(url, "@{", '}', false);
  }
  let less_variable = url
    .strip_prefix("@@")
    .or_else(|| url.strip_prefix('@'))
    .is_some_and(|name| {
      !name.is_empty()
        && name
          .bytes()
          .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    });
  if less_variable {
    return false;
  }
  let scss_characters = url.bytes().all(|b| {
    b.is_ascii_alphanumeric() || b.is_ascii_whitespace() || b"$_+-,./*'\"@#?".contains(&b)
  });
  !(url.contains('$') && scss_characters && !url.ends_with('/'))
}

/// Stylelint's `getSchemeFromUrl`: the scheme of an absolute URL that is
/// followed by `//` (or is `data:`), so `host:port` is not taken for one.
fn url_scheme(url: &str) -> Option<&str> {
  let url = url.trim_start_matches(|c: char| c <= ' ');
  let colon = url.find(':')?;
  let scheme = &url[..colon];
  let mut chars = scheme.chars();
  let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
    && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
  if !valid {
    return None;
  }
  let rest = &url[colon + 1..];
  if scheme.eq_ignore_ascii_case("data") {
    return Some(scheme);
  }
  // `new URL()` rejects a special scheme without a host.
  let special = ["http", "https", "ws", "wss", "ftp"]
    .iter()
    .any(|s| scheme.eq_ignore_ascii_case(s));
  if !rest.starts_with("//") || (special && rest.len() <= 2) {
    return None;
  }
  Some(scheme)
}

/// One media feature of a media query: its name, and the values compared
/// with it (none for a boolean feature, two for `(1px < width < 2px)`).
struct MediaFeature {
  /// The name's ident token; its value is the name, unescaped.
  name: Token,
  /// Each value, without the whitespace and comments around it.
  values: Vec<Vec<ComponentValue>>,
}

impl MediaFeature {
  /// Every dimension token in the feature's values, also inside functions
  /// and blocks.  With `percentages`, percentages count as dimensions too.
  fn dimensions(&self, percentages: bool) -> Vec<&Token> {
    fn collect<'v>(values: &'v [ComponentValue], percentages: bool, out: &mut Vec<&'v Token>) {
      for value in values {
        match value {
          ComponentValue::Token(token)
            if token.kind == TokenType::Dimension
              || (percentages && token.kind == TokenType::Percentage) =>
          {
            out.push(token)
          }
          ComponentValue::Function { value, .. } | ComponentValue::SimpleBlock { value, .. } => {
            collect(value, percentages, out)
          }
          _ => {}
        }
      }
    }
    let mut out = Vec::new();
    for value in &self.values {
      collect(value, percentages, &mut out);
    }
    out
  }
}

/// The media features in `params`, the way `@csstools/media-query-list-parser`
/// finds them: `(name)`, `(name: value)` and the range forms, inside any
/// nesting of `not`, `and`, `or` and parentheses.
fn media_features(params: &str) -> Vec<MediaFeature> {
  let values = css_tokenizer::parse_list_of_component_values(css_tokenizer::tokenize(params));
  let mut out = Vec::new();
  collect_media_features(&values, &mut out);
  out
}

/// The features in the parenthesised blocks among `values`.
fn collect_media_features(values: &[ComponentValue], out: &mut Vec<MediaFeature>) {
  for value in values {
    if let ComponentValue::SimpleBlock { open, value, .. } = value
      && open.kind == TokenType::OpenParen
    {
      match media_feature(value) {
        Some(feature) => out.push(feature),
        None => collect_media_features(value, out),
      }
    }
  }
}

/// The feature a parenthesised block holds, if it holds one rather than a
/// condition.
fn media_feature(block: &[ComponentValue]) -> Option<MediaFeature> {
  let significant: Vec<(usize, &ComponentValue)> = block
    .iter()
    .enumerate()
    .filter(|(_, value)| !value.is_whitespace_or_comment())
    .collect();
  let ident = |value: &ComponentValue| match value {
    ComponentValue::Token(token) if token.kind == TokenType::Ident => Some(token.clone()),
    _ => None,
  };
  let is_token = |value: &ComponentValue, kind: TokenType| matches!(value, ComponentValue::Token(token) if token.kind == kind);
  if let Some(name) = significant.first().and_then(|(_, value)| ident(value)) {
    match significant.get(1) {
      None => {
        return Some(MediaFeature {
          name,
          values: Vec::new(),
        });
      }
      Some(&(colon, value)) if is_token(value, TokenType::Colon) => {
        return Some(MediaFeature {
          name,
          values: vec![trimmed(&block[colon + 1..])],
        });
      }
      _ => {}
    }
  }
  // A range: parts separated by `<`, `>` and `=` delims, one of which is
  // the name.
  let is_operator = |value: &ComponentValue| {
    matches!(value, ComponentValue::Token(token)
      if token.is_delim('<') || token.is_delim('>') || token.is_delim('='))
  };
  let mut parts: Vec<&[ComponentValue]> = Vec::new();
  let mut start = 0;
  let mut previous_operator = false;
  for (index, value) in block.iter().enumerate() {
    if is_operator(value) {
      if !previous_operator {
        parts.push(&block[start..index]);
      }
      start = index + 1;
      previous_operator = true;
    } else if !value.is_whitespace_or_comment() {
      previous_operator = false;
    }
  }
  if parts.is_empty() {
    return None;
  }
  parts.push(&block[start..]);
  let parts: Vec<Vec<ComponentValue>> = parts.into_iter().map(trimmed).collect();
  let name_at = parts
    .iter()
    .position(|part| part.len() == 1 && ident(&part[0]).is_some())?;
  let name = ident(&parts[name_at][0])?;
  let values = parts
    .into_iter()
    .enumerate()
    .filter(|&(index, ref part)| index != name_at && !part.is_empty())
    .map(|(_, part)| part)
    .collect();
  Some(MediaFeature { name, values })
}

/// `values` without the whitespace and comments at either end.
fn trimmed(values: &[ComponentValue]) -> Vec<ComponentValue> {
  let first = values.iter().position(|v| !v.is_whitespace_or_comment());
  let last = values.iter().rposition(|v| !v.is_whitespace_or_comment());
  match (first, last) {
    (Some(first), Some(last)) => values[first..=last].to_vec(),
    _ => Vec::new(),
  }
}

/// Every `@media` at-rule with its media features and where its params
/// start.
fn media_queries<'t>(
  tree: &'t PostcssTree,
) -> impl Iterator<Item = (usize, Vec<MediaFeature>, usize)> + 't {
  at_rules_named(tree, "media").map(|(i, node)| {
    let (params, base) = raw_params(tree, node);
    (i, media_features(params), base)
  })
}

/// The name of every media feature in `@media` params, except custom media
/// (`--name`): `media-feature-name-*-list`.
pub fn media_feature_names(tree: &PostcssTree, found: &mut Vec<Found>) {
  for (i, features, base) in media_queries(tree) {
    for feature in features {
      let name = &feature.name;
      if !name.value.starts_with("--") {
        found.push(Found::new(
          name.value.as_str(),
          i,
          base + name.start,
          base + name.end,
        ));
      }
    }
  }
}

/// The unit of every dimension in a media feature, keyed by the feature's
/// name as written: `media-feature-name-unit-allowed-list`.
pub fn media_feature_units(tree: &PostcssTree, found: &mut Vec<KeyedFound>) {
  for (_, features, base) in media_queries(tree) {
    for feature in &features {
      for token in feature.dimensions(false) {
        found.push(KeyedFound::new(
          feature.name.value.as_str(),
          token.unit.as_str(),
          base + token.start,
          base + token.end,
        ));
      }
    }
  }
}

/// Every value compared with a media feature, keyed by the feature's name
/// as written: `media-feature-name-value-allowed-list`.
pub fn media_feature_values(tree: &PostcssTree, found: &mut Vec<KeyedFound>) {
  for (_, features, base) in media_queries(tree) {
    for feature in &features {
      for value in &feature.values {
        let (Some(first), Some(last)) = (value.first(), value.last()) else {
          continue;
        };
        let text: String = value.iter().map(ComponentValue::to_css).collect();
        found.push(KeyedFound::new(
          feature.name.value.as_str(),
          text,
          base + first.start(),
          base + last.end(),
        ));
      }
    }
  }
}

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

/// Every declaration directly in a standard style rule (not in one nested
/// inside it), keyed by the rule's selector:
/// `rule-selector-property-disallowed-list`.
pub fn rule_properties(tree: &PostcssTree, ignore_keyframes: bool, found: &mut Vec<KeyedFound>) {
  for (rule, node, _) in standard_rules(tree) {
    if ignore_keyframes && is_keyframe_rule(tree, node) {
      continue;
    }
    for (i, decl) in nodes_of(tree, NodeKind::Decl) {
      if nearest_rule(tree, i) == Some(rule) {
        found.push(KeyedFound::new(
          node.name.as_str(),
          decl.name.as_str(),
          decl.name_span.start,
          decl.name_span.end,
        ));
      }
    }
  }
}

/// The closest style rule enclosing node `i`.
fn nearest_rule(tree: &PostcssTree, i: usize) -> Option<usize> {
  let mut parent = tree.nodes[i].parent;
  while let Some(p) = parent {
    if tree.nodes[p].kind == NodeKind::Rule {
      return Some(p);
    }
    parent = tree.nodes[p].parent;
  }
  None
}

/// What `selector-disallowed-list` skips and how it splits selectors.
#[derive(Debug, Clone, Copy, Default)]
pub struct SelectorOptions {
  /// `ignore: ["inside-block"]`: skip rules nested in another block.
  pub ignore_inside_block: bool,
  /// `ignore: ["keyframe-selectors"]`: skip the selectors in `@keyframes`.
  pub ignore_keyframes: bool,
  /// `splitList`: match each selector of a list on its own.
  pub split_list: bool,
}

/// The selector of every standard style rule, whole or split at its
/// commas: `selector-disallowed-list`.
pub fn selectors(tree: &PostcssTree, options: SelectorOptions, found: &mut Vec<Found>) {
  for (i, node, raw) in standard_rules(tree) {
    if (options.ignore_keyframes && is_keyframe_rule(tree, node))
      || (options.ignore_inside_block && node.parent.is_some())
    {
      continue;
    }
    let base = node.name_span.start;
    if !options.split_list {
      found.push(Found::new(raw, i, base, base + raw.len()));
      continue;
    }
    for item in selector::parse(raw, base).unwrap_or_default() {
      let first = item.nodes.iter().find(|n| n.kind != Kind::Comment);
      let last = item.nodes.iter().rev().find(|n| n.kind != Kind::Comment);
      let (start, end) = match (first, last) {
        (Some(first), Some(last)) => (first.start, last.end),
        _ => (item.start, item.start),
      };
      let text = tree.source().get(start..end).unwrap_or("").trim();
      found.push(Found::new(text, i, start, start + text.len()));
    }
  }
}

/// Call `visit` with every node of `selector`, which starts at `base` in
/// the source and belongs to statement `i`.  A selector that does not parse
/// is skipped, as Stylelint skips one postcss-selector-parser rejects.
fn walk_selector(
  i: usize,
  selector: &str,
  base: usize,
  visit: &mut dyn FnMut(usize, &selector::Visit),
) {
  if let Some(parsed) = selector::parse(selector, base) {
    selector::walk(&parsed, &mut |node| visit(i, node));
  }
}

/// Call `visit` with every selector node of the standard style rules whose
/// selector passes `marker` (Stylelint's `mayIncludeRegexes` filter).
fn walk_rule_selectors(
  tree: &PostcssTree,
  marker: fn(&str) -> bool,
  visit: &mut dyn FnMut(usize, &selector::Visit),
) {
  for (i, node, raw) in standard_rules(tree) {
    if marker(raw) {
      walk_selector(i, raw, node.name_span.start, visit);
    }
  }
}

/// Pseudo-class names, without the colon, in style rule selectors and
/// `@page` params: `selector-pseudo-class-*-list`.
pub fn pseudo_classes(tree: &PostcssTree, found: &mut Vec<Found>) {
  let mut visit = |i: usize, visit: &selector::Visit| {
    let node = visit.node;
    if node.kind == Kind::Pseudo && !node.value.starts_with("::") {
      let name = node.value.get(1..).unwrap_or("");
      found.push(Found::new(
        name,
        i,
        node.start,
        node.start + node.value.len(),
      ));
    }
  };
  walk_rule_selectors(tree, |s| s.contains(':'), &mut visit);
  for (i, node) in at_rules_named(tree, "page") {
    let (params, base) = raw_params(tree, node);
    if node.params.contains(':') && tree.is_standard_syntax_at_rule(i) {
      walk_selector(i, params, base, &mut visit);
    }
  }
}

/// Pseudo-element names, without the colons, in style rule selectors; only
/// the `::` form counts: `selector-pseudo-element-*-list`.
pub fn pseudo_elements(tree: &PostcssTree, found: &mut Vec<Found>) {
  walk_rule_selectors(tree, |s| s.contains("::"), &mut |i, visit| {
    let node = visit.node;
    if node.kind == Kind::Pseudo && node.value.as_bytes().get(1) == Some(&b':') {
      let name = node.value.get(2..).unwrap_or("");
      found.push(Found::new(
        name,
        i,
        node.start,
        node.start + node.value.len(),
      ));
    }
  });
}

/// Where the attribute name of `[ns|name ...]` starts: past the `[`, any
/// whitespace and the namespace.
fn attribute_name_start(node: &selector::Node) -> usize {
  let inner = node.value.get(1..).unwrap_or("");
  let mut at = inner.len() - inner.trim_start().len();
  if let Some(namespace) = &node.namespace {
    at += namespace.len() + 1;
  }
  node.start + 1 + at
}

/// Attribute names, with any namespace (`ns|name`), in style rule
/// selectors: `selector-attribute-name-disallowed-list`.
pub fn attribute_names(tree: &PostcssTree, found: &mut Vec<Found>) {
  walk_rule_selectors(tree, |s| s.contains('['), &mut |i, visit| {
    let node = visit.node;
    let Some(attr) = node.attr.as_ref().filter(|_| node.kind == Kind::Attribute) else {
      return;
    };
    let name = match &node.namespace {
      Some(namespace) => format!("{namespace}|{}", attr.name),
      None => attr.name.clone(),
    };
    let start = attribute_name_start(node);
    found.push(Found::new(name, i, start, start + attr.name.len()));
  });
}

/// Attribute operators (`=`, `~=`, `^=`, ...) in style rule selectors:
/// `selector-attribute-operator-*-list`.
pub fn attribute_operators(tree: &PostcssTree, found: &mut Vec<Found>) {
  // `mayIncludeRegexes.attributeSelectorWithOperator`: `/\[.*=/`.
  let has_operator = |s: &str| s.find('[').is_some_and(|open| s[open..].contains('='));
  walk_rule_selectors(tree, has_operator, &mut |i, visit| {
    let node = visit.node;
    let Some(attr) = node.attr.as_ref().filter(|_| node.kind == Kind::Attribute) else {
      return;
    };
    let Some(operator) = &attr.operator else {
      return;
    };
    let after_name = attribute_name_start(node) + attr.name.len() - node.start;
    let offset = node
      .value
      .get(after_name..)
      .and_then(|rest| rest.find(operator.as_str()))
      .unwrap_or(0);
    let start = node.start + after_name + offset;
    found.push(Found::new(
      operator.as_str(),
      i,
      start,
      start + operator.len(),
    ));
  });
}

/// Combinators in style rule selectors, whitespace collapsed to one space
/// (`normalizeCombinator`).  Reference combinators (`/deep/`) and those at
/// the start or end of a selector are skipped (`isStandardSyntaxCombinator`):
/// `selector-combinator-*-list`.
pub fn combinators(tree: &PostcssTree, found: &mut Vec<Found>) {
  walk_rule_selectors(tree, |_| true, &mut |i, visit| {
    let node = visit.node;
    if node.kind != Kind::Combinator
      || node.value.starts_with('/')
      || node.value.ends_with('/')
      || visit.index == 0
      || visit.index + 1 == visit.siblings.len()
    {
      return;
    }
    let mut name = String::new();
    for c in node.value.chars() {
      if !c.is_whitespace() {
        name.push(c);
      } else if !name.ends_with(' ') {
        name.push(' ');
      }
    }
    found.push(Found::new(name, i, node.start, node.end));
  });
}

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
      if function.value.is_empty() || is_native_css_function(function.value) {
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

/// Whether `name` is a function CSS itself defines, which
/// `scss/function-disallowed-list` never reports (stylelint-scss's
/// `isNativeCssFunction`).
fn is_native_css_function(name: &str) -> bool {
  NATIVE_CSS_FUNCTIONS.binary_search(&name).is_ok()
}

/// stylelint-scss's `nativeCssFunctions`, in byte order for the binary
/// search.
const NATIVE_CSS_FUNCTIONS: &[&str] = &[
  "abs",
  "acos",
  "anchor",
  "anchor-size",
  "annotation",
  "asin",
  "atan",
  "atan2",
  "attr",
  "blur",
  "brightness",
  "calc",
  "calc-size",
  "character-variant",
  "circle",
  "clamp",
  "color",
  "color-contrast",
  "color-mix",
  "conic-gradient",
  "contrast",
  "cos",
  "counter",
  "counters",
  "cross-fade",
  "cubic-bezier",
  "device-cmyk",
  "drop-shadow",
  "element",
  "ellipse",
  "env",
  "exp",
  "fit-content",
  "format",
  "frames",
  "grayscale",
  "hsl",
  "hsla",
  "hue-rotate",
  "hwb",
  "hypot",
  "image",
  "image-set",
  "inset",
  "invert",
  "lab",
  "layer",
  "lch",
  "leader",
  "light-dark",
  "linear",
  "linear-gradient",
  "local",
  "log",
  "matrix",
  "matrix3d",
  "max",
  "min",
  "minmax",
  "mod",
  "oklab",
  "oklch",
  "opacity",
  "ornaments",
  "paint",
  "palette-mix",
  "path",
  "perspective",
  "polygon",
  "pow",
  "radial-gradient",
  "ray",
  "rect",
  "rem",
  "repeat",
  "repeating-conic-gradient",
  "repeating-linear-gradient",
  "repeating-radial-gradient",
  "rgb",
  "rgba",
  "rotate",
  "rotate3d",
  "rotateX",
  "rotateY",
  "rotateZ",
  "rotatex",
  "rotatey",
  "rotatez",
  "round",
  "saturate",
  "scale",
  "scale3d",
  "scaleX",
  "scaleY",
  "scaleZ",
  "scalex",
  "scaley",
  "scalez",
  "scroll",
  "sepia",
  "shape",
  "sign",
  "sin",
  "skew",
  "skewX",
  "skewY",
  "skewx",
  "skewy",
  "sqrt",
  "steps",
  "styleset",
  "stylistic",
  "swash",
  "symbols",
  "tan",
  "target-counter",
  "target-counters",
  "target-text",
  "translate",
  "translate3d",
  "translateX",
  "translateY",
  "translateZ",
  "translatex",
  "translatey",
  "translatez",
  "url",
  "var",
  "view",
  "xywh",
];

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
fn partial_extension(path: &str) -> Option<&str> {
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn native_css_functions_are_sorted_for_the_binary_search() {
    assert!(
      NATIVE_CSS_FUNCTIONS
        .windows(2)
        .all(|pair| pair[0] < pair[1])
    );
    assert!(is_native_css_function("translateX"));
    assert!(!is_native_css_function("darken"));
  }

  #[test]
  fn a_scheme_needs_slashes_unless_it_is_data() {
    assert_eq!(url_scheme("https://a.b/c"), Some("https"));
    assert_eq!(url_scheme("data:image/png;base64,AA"), Some("data"));
    assert_eq!(url_scheme("localhost:8080/a"), None);
    assert_eq!(url_scheme("http://"), None);
    assert_eq!(url_scheme("//cdn/a.png"), None);
    assert_eq!(url_scheme("a/b.png"), None);
  }

  #[test]
  fn partial_extensions_skip_css_imports() {
    assert_eq!(partial_extension(" \"a/b.scss\""), Some("scss"));
    assert_eq!(partial_extension("'.scss'"), None);
    assert_eq!(partial_extension("\"a.css\""), None);
    assert_eq!(partial_extension("url(a.scss)"), None);
    assert_eq!(partial_extension("\"a.scss\" screen"), None);
  }

  #[test]
  fn media_features_are_found_in_every_form() {
    let names = |params: &str| -> Vec<String> {
      media_features(params)
        .into_iter()
        .map(|feature| feature.name.value)
        .collect()
    };
    assert_eq!(
      names("screen and (min-width: 1px), not ((color) or (10px < width < 20px))"),
      ["min-width", "color", "width"]
    );
    assert_eq!(names("(1px >= height)"), ["height"]);
    assert!(names("print").is_empty());
  }
}
