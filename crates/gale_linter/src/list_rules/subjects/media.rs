//! Media features of `@media` and `@custom-media` queries.

use super::*;

/// One media feature of a media query: its name, and the values compared
/// with it (none for a boolean feature, two for `(1px < width < 2px)`).
pub(super) struct MediaFeature {
  /// The name's ident token; its value is the name, unescaped.
  pub(super) name: Token,
  /// Each value, without the whitespace and comments around it.
  values: Vec<Vec<ComponentValue>>,
}

impl MediaFeature {
  /// Every dimension token in the feature's values, also inside functions
  /// and blocks.  With `percentages`, percentages count as dimensions too.
  pub(super) fn dimensions(&self, percentages: bool) -> Vec<&Token> {
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
pub(super) fn media_features(params: &str) -> Vec<MediaFeature> {
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
