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

  let svelte = "<p>{x ? \"<style>\" : `${'}'}`}</p>\n{#if a < b}<i/>{/if}\n<style>.real{}</style>";
  assert_eq!(texts(svelte, HostLanguage::Svelte), vec![".real{}"]);

  // Attribute expressions may hold `>` and quotes.
  let attrs = "<button on:click={() => count > 1 ? go('<style>') : 0} class={a}>x</button>\
                 <style>.real{}</style>";
  assert_eq!(texts(attrs, HostLanguage::Svelte), vec![".real{}"]);
  assert_eq!(texts(attrs, HostLanguage::Astro), vec![".real{}"]);
}

#[test]
fn astro_front_matter_is_skipped() {
  let src = "---\nconst css = '<style>.front{}</style>';\n---\n<h1>x</h1>\n<style>.real{}</style>";
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
