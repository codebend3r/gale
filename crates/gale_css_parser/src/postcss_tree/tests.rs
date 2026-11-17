use super::*;

/// The kinds and texts of the top-level nodes.
fn top(tree: &PostcssTree) -> Vec<(NodeKind, String)> {
  tree
    .root
    .iter()
    .map(|&i| (tree.nodes[i].kind, tree.text(i).to_string()))
    .collect()
}

#[test]
fn css_statements_keep_comments_and_unknown_at_rules() {
  let tree = PostcssTree::parse(
    "/* a */\n@foo bar;\na {\n  color: red; /* b */\n\n  @mixin x;\n}",
    Syntax::Css,
  );
  assert_eq!(
    top(&tree),
    vec![
      (NodeKind::Comment, "/* a */".to_string()),
      (NodeKind::AtRule, "@foo bar;".to_string()),
      (
        NodeKind::Rule,
        "a {\n  color: red; /* b */\n\n  @mixin x;\n}".to_string()
      ),
    ]
  );
  let rule = tree.root[2];
  let kids = tree.nodes[rule].children.clone().unwrap();
  assert_eq!(kids.len(), 3);
  assert_eq!(tree.text(kids[0]), "color: red;");
  assert_eq!(tree.before(kids[0]), "\n  ");
  assert_eq!(tree.before(kids[1]), " ");
  assert_eq!(tree.before(kids[2]), "\n\n  ");
  assert_eq!(tree.nodes[kids[2]].name, "mixin");
  assert_eq!(tree.nodes[kids[2]].params, "x");
}

#[test]
fn free_semicolons_join_the_next_before_unless_a_rule_takes_one() {
  let tree = PostcssTree::parse("@media {};\n@import 'x';\na {};  b {}", Syntax::Css);
  let [media, import, a, b] = tree.root[..] else {
    panic!("{:?}", tree.root);
  };
  assert_eq!(tree.text(media), "@media {}");
  assert_eq!(tree.before(import), ";\n");
  assert_eq!(tree.text(a), "a {};");
  assert_eq!(tree.before(b), "  ");
}

#[test]
fn scss_line_comments_and_interpolation() {
  let tree = PostcssTree::parse(
    "a {\n  // note\n  #{$p}-x: 1;\n  b: c; // trailing\n}",
    Syntax::Scss,
  );
  let rule = tree.root[0];
  let kids = tree.nodes[rule].children.clone().unwrap();
  let kinds: Vec<_> = kids.iter().map(|&k| tree.nodes[k].kind).collect();
  assert_eq!(
    kinds,
    vec![
      NodeKind::Comment,
      NodeKind::Decl,
      NodeKind::Decl,
      NodeKind::Comment
    ]
  );
  assert!(tree.nodes[kids[0]].inline);
  assert_eq!(tree.text(kids[0]), "// note");
  assert_eq!(tree.nodes[kids[1]].name, "#{$p}-x");
  assert_eq!(tree.end_line(kids[2]), tree.start_line(kids[3]));
}

#[test]
fn less_mixins_variables_and_comments() {
  let tree = PostcssTree::parse(
    "@var: 1px;\n.a {\n  .mixin();\n  // c\n  color: red;\n}",
    Syntax::Less,
  );
  let var = tree.root[0];
  assert!(tree.nodes[var].variable);
  assert_eq!(tree.nodes[var].name, "var");
  let kids = tree.nodes[tree.root[1]].children.clone().unwrap();
  assert!(tree.nodes[kids[0]].mixin);
  assert_eq!(tree.nodes[kids[1]].kind, NodeKind::Comment);
  assert!(tree.nodes[kids[1]].inline);
  assert_eq!(tree.text(kids[1]), "// c");
  assert_eq!(tree.nodes[kids[2]].name, "color");
}

#[test]
fn removal_ranges_keep_the_semicolons_postcss_prints() {
  let source = "a { color: red; top: 0; left: 1px }";
  let tree = PostcssTree::parse(source, Syntax::Css);
  let kids = tree.nodes[tree.root[0]].children.clone().unwrap();
  let removed = |i: usize| {
    let mut text = source.to_string();
    for range in tree.removal_ranges(i).into_iter().rev() {
      text.replace_range(range, "");
    }
    text
  };
  assert_eq!(removed(kids[0]), "a { top: 0; left: 1px }");
  assert_eq!(removed(kids[2]), "a { color: red; top: 0 }");
  let source = "a { color: red; /* c */ top: 0; }";
  let tree = PostcssTree::parse(source, Syntax::Css);
  let kids = tree.nodes[tree.root[0]].children.clone().unwrap();
  assert_eq!(tree.removal_ranges(kids[2]), vec![23..31]);
}

#[test]
fn first_nested_skips_comments_on_the_brace_line() {
  let tree = PostcssTree::parse("a { /* x */\n  b: c;\n  d: e; }", Syntax::Css);
  let kids = tree.nodes[tree.root[0]].children.clone().unwrap();
  assert!(tree.is_first_nested(kids[1]));
  assert!(!tree.is_first_nested(kids[2]));
  assert!(
    !tree.is_after_comment(kids[1]),
    "the comment shares the brace line"
  );
}

/// `(prop, raw value, important, clean value)` for every declaration.
fn decls(source: &str, syntax: Syntax) -> Vec<(String, String, bool, String)> {
  let tree = PostcssTree::parse(source, syntax);
  tree
    .walk_decls()
    .map(|d| {
      (
        source[d.name_span.clone()].to_string(),
        d.raw_value().to_string(),
        d.important(),
        d.value().to_string(),
      )
    })
    .collect()
}

#[test]
fn declarations_keep_their_raw_value_and_important() {
  assert_eq!(
    decls(
      "a { color: red ; margin : 0 /* c */ 1px !important; top: 1px ! important }",
      Syntax::Css
    ),
    vec![
      ("color".into(), "red ".into(), false, "red".into()),
      (
        "margin".into(),
        "0 /* c */ 1px".into(),
        true,
        "0  1px".into()
      ),
      ("top".into(), "1px".into(), true, "1px".into()),
    ]
  );
  assert_eq!(
    decls("a { *zoom: 1; color: /* x */; }", Syntax::Css),
    vec![
      ("zoom".into(), "1".into(), false, "1".into()),
      ("color".into(), " /* x */".into(), false, " ".into()),
    ]
  );
  assert_eq!(
    decls("$a: rgb(0 0 0 / 50%); .b { #{$p}: 1px; }", Syntax::Scss),
    vec![
      (
        "$a".into(),
        "rgb(0 0 0 / 50%)".into(),
        false,
        "rgb(0 0 0 / 50%)".into()
      ),
      ("#{$p}".into(), "1px".into(), false, "1px".into()),
    ]
  );
}

#[test]
fn at_rule_params_and_selectors_are_located() {
  let source =
    "@import url(a;b.css) /* c */ screen;\n@media (x: 1) { a:hover /* c */ { top: 0; } }";
  let tree = PostcssTree::parse(source, Syntax::Css);
  let import = &tree.nodes[tree.root[0]];
  assert_eq!(&source[import.name_span.clone()], "import");
  assert_eq!(
    &source[import.value_span.clone()],
    "url(a;b.css) /* c */ screen"
  );
  assert_eq!(import.params, "url(a;b.css)  screen");
  let media = &tree.nodes[tree.root[1]];
  assert_eq!(&source[media.value_span.clone()], "(x: 1)");
  let rule = &tree.nodes[tree.children_of(Some(tree.root[1]))[0]];
  assert_eq!(&source[rule.name_span.clone()], "a:hover");
  assert!(rule.semicolon);
  assert!(!media.semicolon, "its last child is a rule");
}

#[test]
fn raws_semicolon_follows_the_last_non_comment_node() {
  let tree = PostcssTree::parse(
    "a { color: red; /* c */ } b { color: red /* c */ } top: 0;",
    Syntax::Scss,
  );
  let a = &tree.nodes[tree.root[0]];
  let b = &tree.nodes[tree.root[1]];
  assert!(a.semicolon);
  assert!(!b.semicolon);
  assert!(tree.root_semicolon);
}

#[test]
fn never_panics_on_broken_or_multibyte_input() {
  for source in [
    "a { color: red",
    "}}} a {",
    "@",
    "a { b: \"unclosed",
    "/* unclosed",
    "a\\",
    "a { b: url(é",
    "é { ü: ö; } /* ß */ @ä ü;",
    "\\é",
    "#{",
    "a { b: (c; }",
  ] {
    for syntax in [Syntax::Css, Syntax::Scss, Syntax::Less] {
      let tree = PostcssTree::parse(source, syntax);
      for i in 0..tree.nodes.len() {
        let _ = (tree.text(i), tree.before(i), tree.end_line(i));
      }
    }
  }
}
