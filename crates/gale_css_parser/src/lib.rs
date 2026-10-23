//! What gale parses a style sheet into: the PostCSS-shaped statement tree
//! every rule reads ([`postcss_tree`]), the value and selector parsers the
//! rules use on its text, and the Sass to SCSS conversion that lets Sass be
//! linted as SCSS.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

pub mod css_tokenizer;
pub mod postcss_tree;
mod sass_to_scss;
pub mod selector;
pub mod value_parser;

pub use sass_to_scss::SourceMap;

/// The kind of CSS dialect we are parsing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Syntax {
  #[default]
  Css,
  Scss,
  Less,
  Sass,
}

/// Infer the [`Syntax`] from a file extension.
///
/// Falls back to [`Syntax::Css`] for unrecognised extensions.
pub fn detect_syntax(file_path: &str) -> Syntax {
  match file_path.rsplit('.').next() {
    Some(ext) => match ext.to_ascii_lowercase().as_str() {
      "scss" => Syntax::Scss,
      "less" => Syntax::Less,
      "sass" => Syntax::Sass,
      _ => Syntax::Css,
    },
    None => Syntax::Css,
  }
}

/// The text the rules lint for `source`, written in `syntax`: `source`
/// itself, or for Sass the SCSS it converts to, with a [`SourceMap`] from
/// offsets in that SCSS back to the Sass.
pub fn lintable_source(source: &str, syntax: Syntax) -> (Cow<'_, str>, Option<SourceMap>) {
  match syntax {
    Syntax::Sass => {
      let (scss, map) = sass_to_scss::convert_sass_to_scss_with_map(source);
      (Cow::Owned(scss), Some(map))
    }
    Syntax::Css | Syntax::Scss | Syntax::Less => (Cow::Borrowed(source), None),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_detect_syntax() {
    assert_eq!(detect_syntax("foo.css"), Syntax::Css);
    assert_eq!(detect_syntax("bar.scss"), Syntax::Scss);
    assert_eq!(detect_syntax("baz.less"), Syntax::Less);
    assert_eq!(detect_syntax("qux.sass"), Syntax::Sass);
    assert_eq!(detect_syntax("unknown.txt"), Syntax::Css);
    assert_eq!(detect_syntax("noext"), Syntax::Css);
  }

  #[test]
  fn sass_is_linted_as_the_scss_it_converts_to() {
    let (text, map) = lintable_source("a\n  color: red\n", Syntax::Sass);
    assert_eq!(text, "a {\n  color: red;\n}\n");
    let map = map.expect("a map back to the Sass");
    assert_eq!(map.to_original(text.find("color").unwrap()), 4);

    let (text, map) = lintable_source("a { color: red }", Syntax::Scss);
    assert!(matches!(text, Cow::Borrowed("a { color: red }")));
    assert!(map.is_none());
  }
}
