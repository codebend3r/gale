//! Style sheets embedded in HTML-like files.
//!
//! Stylelint lints `.vue`, `.svelte`, `.astro` and `.html` files through the
//! `postcss-html` custom syntax.  That syntax tokenizes the file with
//! htmlparser2 and hands the text of every `<style>` element, and of every
//! quoted `style="…"` attribute, to a CSS parser as a separate root.  This
//! module finds the same pieces of text, as byte ranges into the host file,
//! so each can be linted on its own and its problems reported at host
//! positions.
//!
//! The scanner follows htmlparser2's tokenizer where it matters for finding
//! styles: `<script>`, `<style>`, `<textarea>` and `<title>` hold raw text,
//! comments, CDATA sections, declarations and processing instructions hide
//! whatever they contain, and attribute values are skipped whole.  It also
//! honours postcss-html's `<!-- postcss-ignore -->`,
//! `<!-- postcss-disable -->` and `<!-- postcss-enable -->` comments.
//!
//! It departs from postcss-html in three places, each to avoid linting text
//! that is not a style sheet:
//!
//! - Vue `{{ … }}` interpolations and Svelte `{ … }` expressions in text are
//!   skipped, so a `"<style>"` string inside one does not open a style block
//!   (postcss-html reads it as a tag and usually fails with a syntax error).
//! - A self-closing `<style … />` that no later `<style>` element replaces
//!   is empty.  postcss-html leaves it open and swallows the rest of the
//!   file into it.
//! - A `style` attribute whose value holds an entity reference (`&amp;`) is
//!   skipped, where postcss-html works from the decoded value.

use std::ops::Range;
use std::path::Path;

/// The flavour of HTML a host file is written in, which decides how its
/// attribute values and text-level expressions are tokenized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostLanguage {
  /// Plain HTML: `.html`, `.htm`, `.xhtml` and `.php` files.
  Html,
  /// A Vue single-file component.  `{{ … }}` interpolations are skipped.
  Vue,
  /// A Svelte component.  `{ … }` expressions in text and as attribute
  /// values are skipped.
  Svelte,
  /// An Astro component.  The `---` front matter and `{ … }` attribute
  /// values are skipped.
  Astro,
}

impl HostLanguage {
  /// The host language of a file, from its extension, or `None` when the
  /// file is not one gale reads style blocks from.
  pub fn from_path(path: &str) -> Option<Self> {
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
      "vue" => Some(Self::Vue),
      "svelte" => Some(Self::Svelte),
      "astro" => Some(Self::Astro),
      "html" | "htm" | "xhtml" | "php" => Some(Self::Html),
      _ => None,
    }
  }

  /// Whether postcss-html counts the file as a "standard" HTML document,
  /// where a `<style src>` or `<style href>` element with no content is
  /// still linted (as an empty source).
  fn is_standard(self) -> bool {
    self == Self::Html
  }

  /// Whether `{ … }` attribute values are JavaScript expressions, read to
  /// their matching brace (postcss-html's JSX-like tokenizer).
  fn has_expression_attributes(self) -> bool {
    matches!(self, Self::Svelte | Self::Astro)
  }
}

/// The language of a style block, from its `type` or `lang` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleLang {
  /// Plain CSS (no attribute, `lang="css"` or `lang="postcss"`).
  Css,
  /// `lang="scss"`.
  Scss,
  /// `lang="less"`.
  Less,
  /// `lang="sass"` (the indented syntax).
  Sass,
  /// A language gale cannot parse, by postcss-html's name for it:
  /// `stylus`, `sugarss`, or the attribute's own value (`lang="ts"`).
  Unsupported(String),
}

/// One style sheet found in a host file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleBlock {
  /// Byte range of the style sheet's text in the host file.
  ///
  /// For a `<style>` element this is postcss-html's content: a first line
  /// holding only whitespace is dropped, and so are the spaces and tabs that
  /// indent the closing tag.
  pub range: Range<usize>,
  /// The block's language.  Always [`StyleLang::Css`] for an attribute.
  pub lang: StyleLang,
  /// Whether this is a `style="…"` attribute, a bare declaration list,
  /// rather than a `<style>` element.
  pub inline: bool,
  /// Byte ranges, into the host file, of the `{ … }` template expressions
  /// in an attribute's value (`style="width: {size}px"`), each of which
  /// postcss-html reads as a single word.  Empty for elements.
  pub expressions: Vec<Range<usize>>,
}

/// Find every style sheet in `source`, in document order.
pub fn extract_style_blocks(source: &str, host: HostLanguage) -> Vec<StyleBlock> {
  let mut scanner = Scanner {
    src: source,
    bytes: source.as_bytes(),
    host,
    pos: 0,
    blocks: Vec::new(),
    disabled: false,
    ignore_next: false,
    open_html: false,
    open_xsl: false,
    found_front_matter: false,
    pending_style: None,
  };
  scanner.run();
  scanner.blocks.sort_by_key(|block| block.range.start);
  scanner.blocks
}

/// Whether `b` is whitespace to htmlparser2.
fn is_html_space(b: u8) -> bool {
  matches!(b, b' ' | b'\n' | b'\t' | b'\x0c' | b'\r')
}

/// Whether `b` ends a tag or attribute name.
fn ends_tag_section(b: u8) -> bool {
  b == b'/' || b == b'>' || is_html_space(b)
}

/// Find `needle` in `haystack` at or after `from`.
fn find_from(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
  if from > haystack.len() {
    return None;
  }
  haystack[from..]
    .windows(needle.len())
    .position(|window| window == needle)
    .map(|i| from + i)
}

/// The attributes of an open tag that postcss-html looks at.
#[derive(Debug, Default)]
struct TagAttributes {
  lang: Option<String>,
  type_: Option<String>,
  src: Option<String>,
  href: Option<String>,
}

impl TagAttributes {
  /// Record an attribute, keeping the first value of a repeated name as
  /// htmlparser2 does.
  fn record(&mut self, name: &str, value: &str) {
    let slot = match name {
      "lang" => &mut self.lang,
      "type" => &mut self.type_,
      "src" => &mut self.src,
      "href" => &mut self.href,
      _ => return,
    };
    if slot.is_none() {
      *slot = Some(value.to_string());
    }
  }

  /// postcss-html's `getLang`: the subtype of `type="text/x-scss"`, else
  /// the `lang` attribute up to any `?query`, else `css`, lowercased.
  fn lang(&self) -> String {
    if let Some(sub) = self.type_.as_deref().and_then(type_subtype) {
      return sub.to_ascii_lowercase();
    }
    if let Some(lang) = self.lang.as_deref().and_then(lang_word) {
      return lang.to_ascii_lowercase();
    }
    "css".to_string()
  }
}

/// The `scss` in `text/scss` or `text/x-scss` (`/^\w+\/(?:x-)?(\w+)$/i`).
fn type_subtype(value: &str) -> Option<&str> {
  let (major, minor) = value.split_once('/')?;
  if major.is_empty() || !major.bytes().all(is_word_byte) {
    return None;
  }
  let minor = match minor.get(..2) {
    Some(prefix) if prefix.eq_ignore_ascii_case("x-") && minor.len() > 2 => &minor[2..],
    _ => minor,
  };
  (!minor.is_empty() && minor.bytes().all(is_word_byte)).then_some(minor)
}

/// The `scss` in `scss` or `scss?inline` (`/^(\w+)(?:\?.+)?$/`).
fn lang_word(value: &str) -> Option<&str> {
  let (word, query) = match value.split_once('?') {
    Some((word, query)) => (word, Some(query)),
    None => (value, None),
  };
  if word.is_empty() || !word.bytes().all(is_word_byte) || query.is_some_and(str::is_empty) {
    return None;
  }
  Some(word)
}

/// Whether `b` matches the regex class `\w`.
fn is_word_byte(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'_'
}

/// The style language postcss-html resolves a `lang` name to.
fn resolve_lang(lang: &str) -> StyleLang {
  match lang {
    "css" | "postcss" => StyleLang::Css,
    "scss" => StyleLang::Scss,
    "less" => StyleLang::Less,
    "sass" => StyleLang::Sass,
    "sss" | "sugarss" => StyleLang::Unsupported("sugarss".to_string()),
    "styl" | "stylus" => StyleLang::Unsupported("stylus".to_string()),
    other => StyleLang::Unsupported(other.to_string()),
  }
}

/// The postcss-html directive in an HTML comment, if any: the word after
/// the first `postcss-` that stands alone (`/(?:^|\s+)postcss-(\w+)(?:\s+|$)/i`).
fn comment_directive(data: &str) -> Option<String> {
  let bytes = data.as_bytes();
  let lower = data.to_ascii_lowercase();
  let mut from = 0;
  while let Some(at) = find_from(lower.as_bytes(), from, b"postcss-") {
    from = at + 1;
    let after_space = at == 0 || bytes[at - 1].is_ascii_whitespace();
    let word_start = at + "postcss-".len();
    let word_end = bytes[word_start..]
      .iter()
      .position(|&b| !is_word_byte(b))
      .map_or(bytes.len(), |n| word_start + n);
    let stands_alone = word_end == bytes.len() || bytes[word_end].is_ascii_whitespace();
    if after_space && word_end > word_start && stands_alone {
      return Some(lower[word_start..word_end].to_string());
    }
  }
  None
}

/// The tokenizer state machine.  `pos` always points at the next byte to
/// read.
struct Scanner<'a> {
  src: &'a str,
  bytes: &'a [u8],
  host: HostLanguage,
  pos: usize,
  blocks: Vec<StyleBlock>,
  /// Between `<!-- postcss-disable -->` and `<!-- postcss-enable -->`.
  disabled: bool,
  /// Set by `<!-- postcss-ignore -->`, consumed by the next tag.
  ignore_next: bool,
  /// Inside an `<html>` element.
  open_html: bool,
  /// Inside an `<xsl:stylesheet>` or `<xsl:template>` element.
  open_xsl: bool,
  /// Astro: the front matter has been skipped.
  found_front_matter: bool,
  /// A self-closing `<style … />`: htmlparser2 leaves it open, so it ends
  /// at a later `</style>`, or is replaced by the next `<style>` element.
  pending_style: Option<PendingStyle>,
}

/// A `<style … />` waiting for its content to end.
struct PendingStyle {
  content_start: usize,
  attributes: TagAttributes,
}

impl Scanner<'_> {
  /// Read the whole document.
  fn run(&mut self) {
    let len = self.bytes.len();
    while self.pos < len {
      let b = self.bytes[self.pos];
      if b == b'<' {
        self.tag_open();
        continue;
      }
      if self.host == HostLanguage::Astro && !self.found_front_matter && self.skip_front_matter() {
        continue;
      }
      if self.host == HostLanguage::Vue && self.bytes[self.pos..].starts_with(b"{{") {
        // Vue's own parser ends an interpolation at the first `}}`.
        match find_from(self.bytes, self.pos + 2, b"}}") {
          Some(close) => self.pos = close + 2,
          None => self.pos += 2,
        }
        continue;
      }
      if self.host == HostLanguage::Svelte && b == b'{' {
        match js_expression_end(self.bytes, self.pos + 1) {
          Some(close) => self.pos = close + 1,
          None => self.pos += 1,
        }
        continue;
      }
      self.pos += 1;
    }
    if let Some(pending) = self.pending_style.take() {
      // Left open to the end of the file.  Whatever markup follows is not
      // a style sheet, so only blank text counts as its content.
      let end = if self.src[pending.content_start..].trim().is_empty() {
        len
      } else {
        pending.content_start
      };
      if !self.disabled {
        self.style_element(pending.content_start..end, &pending.attributes);
      }
    }
  }

  /// Astro's front matter: a `---` fence at the very start, or after a
  /// newline before any front matter was found, up to the next `\n---`.
  /// Returns whether it was skipped.
  fn skip_front_matter(&mut self) -> bool {
    let rest = &self.bytes[self.pos..];
    let search_from = if self.pos == 0 && rest.starts_with(b"---") {
      self.pos + 3
    } else if rest.starts_with(b"\n---") {
      self.pos + 4
    } else {
      return false;
    };
    match find_from(self.bytes, search_from, b"\n---") {
      Some(close) => {
        self.pos = close + 4;
        self.found_front_matter = true;
        true
      }
      None => false,
    }
  }

  /// Handle a `<`: a tag, comment, declaration, processing instruction or
  /// plain text.
  fn tag_open(&mut self) {
    let lt = self.pos;
    let Some(&next) = self.bytes.get(lt + 1) else {
      self.pos = lt + 1;
      return;
    };
    match next {
      b'!' => self.markup_declaration(lt),
      b'?' => self.skip_to_gt(lt + 2),
      b'/' => self.close_tag(lt),
      b if b.is_ascii_alphabetic() => self.open_tag(lt),
      _ => self.pos = lt + 1,
    }
  }

  /// Move past the next `>` at or after `from`, or to the end.
  fn skip_to_gt(&mut self, from: usize) {
    self.pos = find_from(self.bytes, from, b">").map_or(self.bytes.len(), |gt| gt + 1);
  }

  /// `<!…`: a comment, a CDATA section or a declaration such as `<!DOCTYPE>`.
  fn markup_declaration(&mut self, lt: usize) {
    let after_bang = lt + 2;
    match self.bytes.get(after_bang) {
      Some(b'-') if self.bytes.get(after_bang + 1) == Some(&b'-') => {
        // The comment's opening dashes may share the closing `-->`, so
        // `<!-->` and `<!--->` are complete, empty comments.
        let data_start = after_bang + 2;
        match find_from(self.bytes, after_bang, b"-->") {
          Some(end) => {
            let data_end = end.max(data_start);
            self.comment(&self.src[data_start..data_end]);
            self.pos = end + 3;
          }
          None => {
            self.comment(&self.src[data_start.min(self.src.len())..]);
            self.pos = self.bytes.len();
          }
        }
      }
      // `<!-x`: the dash and the character after it are consumed before
      // the declaration looks for its `>`.
      Some(b'-') => self.skip_to_gt(after_bang + 2),
      Some(b'[') => {
        let cdata_start = after_bang + 1;
        if self.bytes[cdata_start..].starts_with(b"CDATA[") {
          let data_start = cdata_start + "CDATA[".len();
          match find_from(self.bytes, data_start, b"]]>") {
            Some(end) => {
              self.comment(&self.src[lt + 2..end + 2]);
              self.pos = end + 3;
            }
            None => self.pos = self.bytes.len(),
          }
        } else {
          // The first character that breaks `CDATA[` is read again as
          // part of the declaration.
          let mismatch = (cdata_start..self.bytes.len())
            .zip(b"CDATA[".iter())
            .find(|&(i, &want)| self.bytes[i] != want)
            .map_or(self.bytes.len(), |(i, _)| i);
          self.skip_to_gt(mismatch);
        }
      }
      // The character after `<!` is consumed before looking for `>`, so
      // `<!>` runs on to the next `>`.
      Some(_) => self.skip_to_gt(after_bang + 1),
      None => self.pos = self.bytes.len(),
    }
  }

  /// A comment's text: it resets `postcss-ignore` and may carry a
  /// postcss-html directive.
  fn comment(&mut self, data: &str) {
    self.ignore_next = false;
    match comment_directive(data).as_deref() {
      Some("enable") => self.disabled = false,
      Some("disable") => self.disabled = true,
      Some("ignore") => self.ignore_next = true,
      _ => {}
    }
  }

  /// `</…`: a closing tag, or a "special comment" when no name follows.
  fn close_tag(&mut self, lt: usize) {
    let len = self.bytes.len();
    let mut i = lt + 2;
    while i < len && is_html_space(self.bytes[i]) {
      i += 1;
    }
    if i >= len {
      self.pos = len;
      return;
    }
    let b = self.bytes[i];
    if b == b'>' {
      self.pos = i + 1;
      return;
    }
    if !b.is_ascii_alphabetic() {
      let end = find_from(self.bytes, i, b">").unwrap_or(len);
      let data = &self.src[i..end];
      self.comment(data);
      self.pos = (end + 1).min(len);
      return;
    }
    let name_start = i;
    while i < len && self.bytes[i] != b'>' && !is_html_space(self.bytes[i]) {
      i += 1;
    }
    let name = self.src[name_start..i].to_ascii_lowercase();
    self.ignore_next = false;
    match name.as_str() {
      "html" => self.open_html = false,
      "xsl:stylesheet" | "xsl:template" => self.open_xsl = false,
      "style" => {
        if let Some(pending) = self.pending_style.take()
          && !self.disabled
        {
          self.style_element(pending.content_start..lt, &pending.attributes);
        }
      }
      _ => {}
    }
    self.skip_to_gt(i);
  }

  /// `<name …>`: read the attributes, then the raw text of the elements
  /// that hold it.
  fn open_tag(&mut self, lt: usize) {
    let len = self.bytes.len();
    let name_start = lt + 1;
    let mut i = name_start;
    while i < len && !ends_tag_section(self.bytes[i]) {
      i += 1;
    }
    let name = self.src[name_start..i].to_ascii_lowercase();
    let mut attributes = TagAttributes::default();
    let mut self_closing = false;

    // Attributes, up to the `>` that ends the tag.
    let tag_end = loop {
      while i < len && is_html_space(self.bytes[i]) {
        i += 1;
      }
      if i >= len {
        break None;
      }
      match self.bytes[i] {
        b'>' => break Some(i),
        b'/' => {
          // `/` then `>` closes the tag; anything else after the slash is
          // read as the start of another attribute.
          i += 1;
          while i < len && is_html_space(self.bytes[i]) {
            i += 1;
          }
          if self.bytes.get(i) == Some(&b'>') {
            self_closing = true;
            break Some(i);
          }
          continue;
        }
        _ => {}
      }
      // An attribute name runs to `=`, whitespace, `/` or `>`; its first
      // character is taken whatever it is.
      let attr_start = i;
      i += 1;
      while i < len && self.bytes[i] != b'=' && !ends_tag_section(self.bytes[i]) {
        i += 1;
      }
      let attr_name = self.src[attr_start..i].to_ascii_lowercase();
      while i < len && is_html_space(self.bytes[i]) {
        i += 1;
      }
      if self.bytes.get(i) != Some(&b'=') {
        attributes.record(&attr_name, "");
        continue;
      }
      i += 1;
      while i < len && is_html_space(self.bytes[i]) {
        i += 1;
      }
      if i >= len {
        break None;
      }
      match self.bytes[i] {
        quote @ (b'"' | b'\'') => {
          let value_start = i + 1;
          let value_end = find_from(self.bytes, value_start, &[quote]).unwrap_or(len);
          let value = &self.src[value_start..value_end];
          attributes.record(&attr_name, value);
          if attr_name == "style" && value_end < len {
            self.style_attribute(value_start..value_end);
          }
          i = (value_end + 1).min(len);
        }
        b'{' if self.host.has_expression_attributes() => match js_expression_end(self.bytes, i + 1)
        {
          Some(close) => i = close + 1,
          None => i = self.unquoted_value_end(i),
        },
        _ => {
          let value_end = self.unquoted_value_end(i);
          attributes.record(&attr_name, &self.src[i..value_end]);
          i = value_end;
        }
      }
    };

    let Some(gt) = tag_end else {
      self.pos = len;
      return;
    };
    self.pos = gt + 1;

    let ignored = std::mem::take(&mut self.ignore_next);
    match name.as_str() {
      "html" => self.open_html = true,
      "xsl:stylesheet" | "xsl:template" => self.open_xsl = true,
      _ => {}
    }

    let raw_text_end: &[u8] = match name.as_str() {
      "style" => b"</style",
      "script" => b"</script",
      "textarea" => b"</textarea",
      "title" => b"</title",
      _ => return,
    };
    let content_start = gt + 1;
    if self_closing {
      // Not raw text after all.  An ignored `<style />` is never opened.
      if name == "style" && !ignored {
        self.pending_style = Some(PendingStyle {
          content_start,
          attributes,
        });
      }
      return;
    }
    let content_end = self.raw_text_end(content_start, raw_text_end);
    // The closing tag runs to its `>`.
    self.skip_to_gt(content_end + raw_text_end.len().min(len - content_end));
    if name == "style" {
      self.pending_style = None;
      // `postcss-disable` is checked when the element closes.
      if !ignored && !self.disabled {
        self.style_element(content_start..content_end, &attributes);
      }
    }
  }

  /// The end of an unquoted attribute value starting at `from`.
  fn unquoted_value_end(&self, from: usize) -> usize {
    let len = self.bytes.len();
    let mut i = from;
    while i < len && self.bytes[i] != b'>' && !is_html_space(self.bytes[i]) {
      i += 1;
    }
    i
  }

  /// Where the raw text that started at `from` ends: the `<` of the first
  /// `</name` (any case) followed by whitespace or `>`, or the end of the
  /// file.
  fn raw_text_end(&self, from: usize, end_tag: &[u8]) -> usize {
    let len = self.bytes.len();
    let mut i = from;
    while let Some(lt) = find_from(self.bytes, i, b"</") {
      let candidate = &self.bytes[lt..];
      let matches = candidate.len() > end_tag.len()
        && candidate[..end_tag.len()].eq_ignore_ascii_case(end_tag)
        && matches!(
          candidate[end_tag.len()],
          b'>' | b' ' | b'\n' | b'\t' | b'\x0c' | b'\r'
        );
      if matches {
        return lt;
      }
      i = lt + 1;
    }
    len
  }

  /// Record a `<style>` element's text, trimmed the way postcss-html trims
  /// it, unless postcss-html would skip the element.
  fn style_element(&mut self, content: Range<usize>, attributes: &TagAttributes) {
    let mut start = content.start;
    let mut end = content.end;
    let text = &self.bytes[start..end];

    // A first line holding only spaces and tabs is not part of the sheet.
    let leading = text
      .iter()
      .take_while(|&&b| b == b' ' || b == b'\t')
      .count();
    if text[leading..].starts_with(b"\n") {
      start += leading + 1;
    } else if text[leading..].starts_with(b"\r\n") {
      start += leading + 2;
    }
    // Nor is the indentation before the closing tag.
    while end > start && matches!(self.bytes[end - 1], b' ' | b'\t') {
      end -= 1;
    }

    let has_source_attr = attributes.src.as_deref().is_some_and(|v| !v.is_empty())
      || attributes.href.as_deref().is_some_and(|v| !v.is_empty());
    let in_document = self.open_html || self.open_xsl || self.host.is_standard();
    if !in_document && has_source_attr && self.src[start..end].trim().is_empty() {
      return;
    }

    self.blocks.push(StyleBlock {
      range: start..end,
      lang: resolve_lang(&attributes.lang()),
      inline: false,
      expressions: Vec::new(),
    });
  }

  /// Record a quoted `style="…"` attribute value.
  fn style_attribute(&mut self, value: Range<usize>) {
    if self.disabled || self.ignore_next {
      return;
    }
    let text = &self.src[value.clone()];
    // htmlparser2 hands postcss-html the decoded value; when decoding
    // changes its length the positions no longer line up, so leave it.
    let has_entity = text
      .as_bytes()
      .windows(2)
      .any(|w| w[0] == b'&' && (w[1].is_ascii_alphanumeric() || w[1] == b'#'));
    if has_entity {
      return;
    }
    let expressions = if has_template_braces(text) {
      template_expressions(text)
        .into_iter()
        .map(|r| r.start + value.start..r.end + value.start)
        .collect()
    } else {
      Vec::new()
    };
    self.blocks.push(StyleBlock {
      range: value,
      lang: StyleLang::Css,
      inline: true,
      expressions,
    });
  }
}

/// Whether an attribute value holds a `{` with a `}` after it, which makes
/// postcss-html parse it with its template syntax.
fn has_template_braces(text: &str) -> bool {
  text
    .find('{')
    .is_some_and(|open| text[open + 1..].contains('}'))
}

/// The `{ … }` runs in a template-syntax attribute value, each balanced to
/// its matching brace with quoted strings and comments skipped, as
/// postcss-html's template tokenizer merges them into single words.
fn template_expressions(text: &str) -> Vec<Range<usize>> {
  let bytes = text.as_bytes();
  let mut runs = Vec::new();
  let mut depth = 0usize;
  let mut run_start = 0;
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      quote @ (b'"' | b'\'') => {
        i += 1;
        while i < bytes.len() && bytes[i] != quote {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
      }
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        i = find_from(bytes, i + 2, b"*/").map_or(bytes.len(), |end| end + 1);
      }
      b'{' => {
        if depth == 0 {
          run_start = i;
        }
        depth += 1;
      }
      b'}' if depth > 0 => {
        depth -= 1;
        if depth == 0 {
          runs.push(run_start..i + 1);
        }
      }
      _ => {}
    }
    i += 1;
  }
  runs
}

/// The index of the `}` that closes a JavaScript expression whose `{` sits
/// just before `from`, or `None` when it never closes.
///
/// Strings, template literals (with their `${ … }` holes), comments and
/// regular expression literals are skipped, so braces inside them do not
/// count.
fn js_expression_end(bytes: &[u8], from: usize) -> Option<usize> {
  let len = bytes.len();
  let mut depth = 0usize;
  let mut i = from;
  // The last significant byte, to tell a regex literal from a division.
  let mut previous: u8 = b'{';
  while i < len {
    let b = bytes[i];
    match b {
      b'"' | b'\'' => {
        i += 1;
        while i < len && bytes[i] != b && bytes[i] != b'\n' {
          if bytes[i] == b'\\' {
            i += 1;
          }
          i += 1;
        }
      }
      b'`' => i = template_literal_end(bytes, i + 1)?,
      b'/' if bytes.get(i + 1) == Some(&b'/') => {
        while i < len && bytes[i] != b'\n' {
          i += 1;
        }
        continue;
      }
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        i = find_from(bytes, i + 2, b"*/").map_or(len, |end| end + 2);
        continue;
      }
      b'/' if b"(,=:[!&|?{};+-*%<>~^".contains(&previous) => {
        // A regular expression literal.
        i += 1;
        let mut in_class = false;
        while i < len && bytes[i] != b'\n' {
          match bytes[i] {
            b'\\' => i += 1,
            b'[' => in_class = true,
            b']' => in_class = false,
            b'/' if !in_class => break,
            _ => {}
          }
          i += 1;
        }
      }
      b'{' => depth += 1,
      b'}' => {
        if depth == 0 {
          return Some(i);
        }
        depth -= 1;
      }
      _ => {}
    }
    if !b.is_ascii_whitespace() {
      previous = b;
    }
    i += 1;
  }
  None
}

/// The index of the backtick that closes a template literal whose body
/// starts at `from`, skipping its `${ … }` holes.
fn template_literal_end(bytes: &[u8], from: usize) -> Option<usize> {
  let mut i = from;
  while i < bytes.len() {
    match bytes[i] {
      b'\\' => i += 1,
      b'`' => return Some(i),
      b'$' if bytes.get(i + 1) == Some(&b'{') => {
        i = js_expression_end(bytes, i + 2)?;
      }
      _ => {}
    }
    i += 1;
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The text of each block, with its language and whether it is inline.
  fn blocks(source: &str, host: HostLanguage) -> Vec<(&str, StyleLang, bool)> {
    extract_style_blocks(source, host)
      .into_iter()
      .map(|b| (&source[b.range], b.lang, b.inline))
      .collect()
  }

  /// Just the text of each `<style>` element.
  fn texts(source: &str, host: HostLanguage) -> Vec<&str> {
    blocks(source, host)
      .into_iter()
      .filter(|(_, _, inline)| !inline)
      .map(|(text, _, _)| text)
      .collect()
  }

  #[test]
  fn host_language_comes_from_the_extension() {
    assert_eq!(HostLanguage::from_path("a/B.Vue"), Some(HostLanguage::Vue));
    assert_eq!(
      HostLanguage::from_path("x.svelte"),
      Some(HostLanguage::Svelte)
    );
    assert_eq!(
      HostLanguage::from_path("x.astro"),
      Some(HostLanguage::Astro)
    );
    for html in ["x.html", "x.htm", "x.xhtml", "x.php"] {
      assert_eq!(HostLanguage::from_path(html), Some(HostLanguage::Html));
    }
    assert_eq!(HostLanguage::from_path("x.css"), None);
    assert_eq!(HostLanguage::from_path("x.md"), None);
    assert_eq!(HostLanguage::from_path("vue"), None);
  }

  #[test]
  fn style_text_drops_a_blank_first_line_and_the_closing_indent() {
    let src = "<template><div/></template>\n<style scoped>\n.a { color: red; }\n  </style>\n";
    let found = extract_style_blocks(src, HostLanguage::Vue);
    assert_eq!(found.len(), 1);
    assert_eq!(&src[found[0].range.clone()], ".a { color: red; }\n");
    assert_eq!(found[0].range.start, src.find(".a").unwrap());

    assert_eq!(
      texts("<style>\r\n.a{}\r\n</style>", HostLanguage::Html),
      vec![".a{}\r\n"]
    );
    assert_eq!(
      texts("<style>   \n.b{}\n   </style>", HostLanguage::Html),
      vec![".b{}\n"]
    );
    assert_eq!(
      texts("<style>\t\n\t.c{}\n\t</style>", HostLanguage::Html),
      vec!["\t.c{}\n"]
    );
    assert_eq!(
      texts("<style>  .d{}  </style>", HostLanguage::Html),
      vec!["  .d{}"]
    );
    assert_eq!(texts("<style></style>", HostLanguage::Html), vec![""]);
    assert_eq!(texts("<style>\n</style>", HostLanguage::Html), vec![""]);
  }

  #[test]
  fn lang_and_type_attributes_pick_the_syntax() {
    let src = "<style lang=\"scss\">a{}</style>\
               <style lang=less>b{}</style>\
               <style type=\"text/x-scss\">c{}</style>\
               <style lang=\"postcss\">d{}</style>\
               <style lang=\"SASS\">e\n  color: red\n</style>\
               <style lang=\"scss?inline\">f{}</style>\
               <style type=\"text/css\" lang=\"scss\">g{}</style>\
               <style lang=\"stylus\">h</style>\
               <style lang=\"styl\">i</style>\
               <style lang=\"ts\">j</style>";
    let langs: Vec<StyleLang> = extract_style_blocks(src, HostLanguage::Vue)
      .into_iter()
      .map(|b| b.lang)
      .collect();
    assert_eq!(
      langs,
      vec![
        StyleLang::Scss,
        StyleLang::Less,
        StyleLang::Scss,
        StyleLang::Css,
        StyleLang::Sass,
        StyleLang::Scss,
        StyleLang::Css,
        StyleLang::Unsupported("stylus".into()),
        StyleLang::Unsupported("stylus".into()),
        StyleLang::Unsupported("ts".into()),
      ]
    );
  }

  #[test]
  fn other_style_attributes_do_not_matter() {
    let src = "<style scoped>a{}</style><style module>b{}</style>\
               <style is:global>c{}</style><style global>d{}</style>";
    assert_eq!(
      texts(src, HostLanguage::Astro),
      vec!["a{}", "b{}", "c{}", "d{}"]
    );
    assert_eq!(texts("<STYLE>e{}</STYLE>", HostLanguage::Html), vec!["e{}"]);
    assert_eq!(
      texts("<style\n  lang=\"scss\"\n>f{}</style >", HostLanguage::Vue),
      vec!["f{}"]
    );
  }

  #[test]
  fn raw_text_elements_and_comments_hide_style_tags() {
    let src = "<script setup>\nconst a = '<style>.x{}</style>'\n</script>\n\
               <script>var b = \"<style>.y{}</style>\"</script>\n\
               <textarea><style>.z{}</style></textarea>\n\
               <title><style>.t{}</style></title>\n\
               <!-- <style>.c{}</style> -->\n\
               <![CDATA[<style>.cd{}</style>]]>\n\
               <?php echo '<style>'; ?>\n\
               <style>.real{}</style>";
    assert_eq!(texts(src, HostLanguage::Vue), vec![".real{}"]);
  }

  #[test]
  fn other_elements_do_not_hide_style_tags() {
    let src = "<template><style>.tpl{}</style></template>\
               <noscript><style>.ns{}</style></noscript>\
               <svg><style>.svg{}</style></svg>";
    assert_eq!(
      texts(src, HostLanguage::Html),
      vec![".tpl{}", ".ns{}", ".svg{}"]
    );
  }

  #[test]
  fn comment_edge_cases_follow_htmlparser2() {
    // `<!-->` and `<!--->` are complete comments.
    assert_eq!(
      texts(
        "<!--><style>a{}</style><!---><style>b{}</style>",
        HostLanguage::Html
      ),
      vec!["a{}", "b{}"]
    );
    // An unterminated comment hides everything after it.
    assert!(texts("<!-- <style>a{}</style>", HostLanguage::Html).is_empty());
    // `</ style>` does not end raw text.
    assert_eq!(
      texts("<style>a{}</ style><style>b{}</style>", HostLanguage::Html),
      vec!["a{}</ style><style>b{}"]
    );
  }

  #[test]
  fn postcss_comment_directives_skip_blocks() {
    let src = "<!-- postcss-ignore -->\n<style>.ignored{}</style>\n\
               <style>.kept{}</style>\n\
               <!-- postcss-disable -->\n<style>.off{}</style>\n<p style=\"color: red\"></p>\n\
               <!-- postcss-enable -->\n<style>.on{}</style>";
    assert_eq!(texts(src, HostLanguage::Vue), vec![".kept{}", ".on{}"]);
    assert!(
      !blocks(src, HostLanguage::Vue)
        .iter()
        .any(|(_, _, inline)| *inline)
    );
    // `postcss-ignore` only applies to the very next tag.
    assert_eq!(
      texts(
        "<!-- postcss-ignore --><div></div><style>a{}</style>",
        HostLanguage::Html
      ),
      vec!["a{}"]
    );
    assert_eq!(comment_directive(" postcss-ignore "), Some("ignore".into()));
    assert_eq!(comment_directive("POSTCSS-Disable"), Some("disable".into()));
    assert_eq!(comment_directive("xpostcss-ignore"), None);
    assert_eq!(comment_directive("postcss-ignore-me"), None);
  }

  #[test]
  fn external_style_elements_are_skipped_outside_html_documents() {
    // Vue: an empty `<style src>` is an external sheet, not an empty one.
    assert!(texts("<style src=\"./a.css\"></style>", HostLanguage::Vue).is_empty());
    assert!(
      texts(
        "<style src=\"./a.css\" />\n<script></script>",
        HostLanguage::Vue
      )
      .is_empty()
    );
    // With content it is linted.
    assert_eq!(
      texts("<style src=\"./a.css\">a{}</style>", HostLanguage::Vue),
      vec!["a{}"]
    );
    // An HTML file, or an `<html>` element, lints it as an empty sheet.
    assert_eq!(
      texts("<style src=\"a.css\"></style>", HostLanguage::Html),
      vec![""]
    );
    assert_eq!(
      texts(
        "<html><style href=\"a.css\"></style></html>",
        HostLanguage::Vue
      ),
      vec![""]
    );
  }

  #[test]
  fn self_closing_style_waits_for_a_later_close_or_element() {
    // Replaced by the next `<style>` element.
    let src = "<style />\n<style scoped>\n.a {}\n</style>\n";
    assert_eq!(texts(src, HostLanguage::Vue), vec![".a {}\n"]);
    // Left open to the end: empty, or its blank tail.
    assert_eq!(
      texts("<template/>\n<style />\n", HostLanguage::Vue),
      vec![""]
    );
    assert_eq!(
      texts("<style />\n<script>x</script>\n", HostLanguage::Vue),
      vec![""]
    );
    // Closed by a later `</style>`.
    assert_eq!(
      texts("<style/>\n.a{}\n</style>", HostLanguage::Html),
      vec![".a{}\n"]
    );
  }

  #[test]
  fn an_unterminated_style_runs_to_the_end() {
    assert_eq!(
      texts(
        "<template/>\n<style>\n.a { color: red; }\n",
        HostLanguage::Vue
      ),
      vec![".a { color: red; }\n"]
    );
  }

  #[test]
  fn style_attributes_are_inline_blocks() {
    let src = "<div style=\"color: red; margin: 0\" STYLE='x: y'></div>\
               <p style=color:red></p><p style=\"\"></p><p :style=\"{a: 1}\"></p>\
               <p style=\"a: b &amp; c\"></p>";
    assert_eq!(
      blocks(src, HostLanguage::Vue),
      vec![
        ("color: red; margin: 0", StyleLang::Css, true),
        ("x: y", StyleLang::Css, true),
        ("", StyleLang::Css, true),
      ]
    );
  }

  #[test]
  fn template_expressions_in_style_attributes_are_marked() {
    let src = "<div style=\"width: {size}px; color: {c ? '}' : 'a'}\"></div>";
    let found = extract_style_blocks(src, HostLanguage::Svelte);
    assert_eq!(found.len(), 1);
    let exprs: Vec<&str> = found[0]
      .expressions
      .iter()
      .map(|r| &src[r.clone()])
      .collect();
    assert_eq!(exprs, vec!["{size}", "{c ? '}' : 'a'}"]);
  }

  #[test]
  fn vue_interpolations_and_svelte_expressions_hide_style_text() {
    let vue = "<template><p>{{ '<style>' }}</p></template>\n<style>.real{}</style>";
    assert_eq!(texts(vue, HostLanguage::Vue), vec![".real{}"]);

    let svelte =
      "<p>{x ? \"<style>\" : `${'}'}`}</p>\n{#if a < b}<i/>{/if}\n<style>.real{}</style>";
    assert_eq!(texts(svelte, HostLanguage::Svelte), vec![".real{}"]);

    // Attribute expressions may hold `>` and quotes.
    let attrs = "<button on:click={() => count > 1 ? go('<style>') : 0} class={a}>x</button>\
                 <style>.real{}</style>";
    assert_eq!(texts(attrs, HostLanguage::Svelte), vec![".real{}"]);
    assert_eq!(texts(attrs, HostLanguage::Astro), vec![".real{}"]);
  }

  #[test]
  fn astro_front_matter_is_skipped() {
    let src =
      "---\nconst css = '<style>.front{}</style>';\n---\n<h1>x</h1>\n<style>.real{}</style>";
    assert_eq!(texts(src, HostLanguage::Astro), vec![".real{}"]);
    // Other hosts read the same text as HTML.
    assert_eq!(texts(src, HostLanguage::Html), vec![".front{}", ".real{}"]);
  }

  #[test]
  fn ranges_are_byte_offsets_into_multibyte_hosts() {
    let src = "<p>日本語 🎉</p>\n<style>\n.ü { content: \"日本\"; }\n</style>";
    let found = extract_style_blocks(src, HostLanguage::Vue);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].range.start, src.find(".ü").unwrap());
    assert_eq!(&src[found[0].range.clone()], ".ü { content: \"日本\"; }\n");
  }

  #[test]
  fn js_expressions_balance_braces_outside_strings_and_comments() {
    let end = |s: &str| js_expression_end(s.as_bytes(), 1);
    assert_eq!(end("{a}"), Some(2));
    assert_eq!(end("{ {b: 1}.b }"), Some(11));
    assert_eq!(end("{'}'}"), Some(4));
    assert_eq!(end("{`${'}'}`}"), Some(9));
    assert_eq!(end("{/* } */ a}"), Some(10));
    assert_eq!(end("{a / 2 }"), Some(7));
    assert_eq!(end("{x.replace(/}/g, '')}"), Some(20));
    assert_eq!(end("{never"), None);
  }

  #[test]
  fn lang_attribute_patterns() {
    assert_eq!(type_subtype("text/x-scss"), Some("scss"));
    assert_eq!(type_subtype("text/less"), Some("less"));
    assert_eq!(type_subtype("css"), None);
    assert_eq!(type_subtype("text/x-"), None);
    assert_eq!(lang_word("scss?inline"), Some("scss"));
    assert_eq!(lang_word("scss?"), None);
    assert_eq!(lang_word("s-css"), None);
  }
}
