//! Regression test: no rule may panic on multibyte input.
//!
//! Rules work on byte offsets, and an offset computed from the wrong text
//! (the parser's re-serialised selector, a value without its comments, the
//! converted SCSS of a Sass file) lands inside `é` or `😀` sooner or later.
//! The runner turns such a panic into an "Internal error" problem instead of
//! aborting the run, so this test asserts that none ever shows up: every
//! registered rule is enabled and the snippets below are linted with a
//! multibyte character inserted at many positions.
//!
//! It is deterministic (fixed snippets and positions) and keeps to a few
//! seconds by sampling: every few characters plus around every punctuation
//! mark and line end, cycling through the inserted characters and the two
//! option sets rather than trying every combination.  Insertions at the very
//! end, where unterminated comments and blocks meet the end of the file, try
//! every character with both option sets.

use std::collections::HashMap;

use gale_css_parser::Syntax;
use gale_linter::{LintRunner, RuleRegistry};

/// Characters of two, three and four bytes.
const MULTIBYTE: &[&str] = &["é", "中", "😀"];

/// Insert at every `STRIDE`th character boundary, plus next to every byte
/// that tends to anchor offset arithmetic (punctuation and line ends).
const STRIDE: usize = 7;

const CSS: &str = r#"@charset "utf-8";
@import url("a.css") screen;
/* comment */
:root { --main-color: #FFF; --gap: 4px }
a , b:hover::before, .c > .d ~ .e + .f[data-x="1"] {
  color: RED;
  font: italic bold 12px/30px Georgia, serif;
  font-weight: bold;
  margin: 0px 0 0 0;
  background: url(img.png) no-repeat , linear-gradient( to right , #fff 0% , #000 100% );
  transition: opacity .3s ease-in-out,transform 0.3s;
  content: "\201C";
}
@media (min-width:100px) and (max-width : 200px) {
  .g { width: calc(100% - 10px); color: rgba(0,0,0,.5) }

}
@keyframes spin { from { transform: rotate(0deg) } to { transform: rotate(360deg) } }
@font-face { font-family: "X"; font-weight: 400 700; src: url(x.woff2) }
@supports (display:grid) { .h { display: grid; grid-template-areas: "a b" "c d"; } }
.i{}
/* unterminated"#;

const SCSS: &str = r#"@use "sass:math";
$map: (key1: value1, key2: value2);
$var: 10px !default;
// line comment
@mixin m($a, $b: 2) { width: $a * $b; }
@function f($x) { @return $x+1; }
.a {
  &__b, &--c { color: red; }
  #{$sel} > .d { margin: math.div($var, 2) }
  @include m(1px, 2);
  @extend %placeholder;
  font-weight: normal;
  .e { @if $var == 10px { color: blue } @else { color: green } }
  @each $k, $v in $map { .#{$k} { width: $v } }
}
%placeholder { padding: 0 }
/* unterminated"#;

const LESS: &str = r#"@color: #4D926F;
@import (reference) "foo.less";
.mixin(@a; @b: 2) { width: @a }
.a {
  .mixin(1px; 2);
  color: darken(@color, 10%);
  &:extend(.b all);
  @media (min-width: 768px) { float: left }
  font-weight: 700;
}
@plugin "plugin";
.guard when (@mode = huge) { width: 100% }
// line"#;

/// A block that never closes, so scans for its `}` run off the end.
const UNCLOSED: &str = "a { color: red; }\n.z { color: red";

const SASS: &str = "// héllo wörld\n=mixin($a)\n  width: $a\n\n.foo\n  content: \"→ ✓\"\n  color: RED\n  +mixin(1px)\n  .bar, .baz\n    margin: 0px 0 0 0\n    font: bold 12px serif\n\n@media (min-width: 1px)\n  .q\n    color: #FFF\n";

/// Every registered rule, enabled with its default options.
fn runner_with_every_rule() -> LintRunner {
  let registry = RuleRegistry::default();
  let names = registry
    .all()
    .iter()
    .map(|rule| rule.name().to_string())
    .collect();
  LintRunner::new(registry, names)
}

/// Every registered rule, with the options that switch on the code paths
/// the defaults leave alone.
fn runner_with_options() -> LintRunner {
  let registry = RuleRegistry::default();
  let names: Vec<String> = registry
    .all()
    .iter()
    .map(|rule| rule.name().to_string())
    .collect();
  let mut options: HashMap<String, serde_json::Value> = HashMap::new();
  for name in &names {
    if name.contains("newline") || name.contains("space") {
      options.insert(name.clone(), serde_json::json!("always"));
    }
  }
  options.insert(
    "font-weight-notation".into(),
    serde_json::json!("named-where-possible"),
  );
  options.insert("color-hex-case".into(), serde_json::json!("lower"));
  options.insert("max-line-length".into(), serde_json::json!(20));
  options.insert(
    "rule-empty-line-before".into(),
    serde_json::json!(["always", { "except": ["after-single-line-comment"] }]),
  );
  LintRunner::with_options(registry, names, options)
}

/// Byte offsets at which to insert a character into `source`.
fn insertion_points(source: &str) -> Vec<usize> {
  let mut points: Vec<usize> = source
    .char_indices()
    .map(|(offset, _)| offset)
    .chain(std::iter::once(source.len()))
    .enumerate()
    .filter(|(index, _)| index % STRIDE == 0)
    .map(|(_, offset)| offset)
    .collect();
  for (offset, ch) in source.char_indices() {
    if ",;:{}()[]/*\"'@#$&>+~=.!\n".contains(ch) {
      points.push(offset);
      points.push(offset + ch.len_utf8());
    }
  }
  points.sort_unstable();
  points.dedup();
  points
}

/// The internal errors from linting `source` with `ch` inserted at byte
/// `point`.
fn crashes_at(
  runner: &LintRunner,
  source: &str,
  (file, syntax): (&str, Syntax),
  point: usize,
  ch: &str,
) -> Vec<String> {
  let mut text = String::with_capacity(source.len() + 4);
  text.push_str(&source[..point]);
  text.push_str(ch);
  text.push_str(&source[point..]);

  runner
    .lint_source(&text, file, syntax)
    .diagnostics
    .iter()
    .filter(|diag| diag.message.starts_with("Internal error"))
    .map(|diag| {
      format!(
        "{file}: {ch:?} inserted at byte {point}: {} ({})",
        diag.message, diag.rule_name
      )
    })
    .collect()
}

/// Lint the sampled variants of `source` and collect the internal errors.
fn crashes(runners: &[LintRunner], source: &str, file: &str, syntax: Syntax) -> Vec<String> {
  let mut found = Vec::new();
  for (index, &point) in insertion_points(source).iter().enumerate() {
    let ch = MULTIBYTE[index % MULTIBYTE.len()];
    let runner = &runners[(index / MULTIBYTE.len()) % runners.len()];
    found.extend(crashes_at(runner, source, (file, syntax), point, ch));
  }
  for runner in runners {
    for ch in MULTIBYTE {
      found.extend(crashes_at(runner, source, (file, syntax), source.len(), ch));
    }
  }
  found
}

#[test]
fn no_rule_panics_on_multibyte_input() {
  let runners = [runner_with_every_rule(), runner_with_options()];
  let snippets = [
    (CSS, "test.css", Syntax::Css),
    (SCSS, "test.scss", Syntax::Scss),
    (LESS, "test.less", Syntax::Less),
    (SASS, "test.sass", Syntax::Sass),
    (UNCLOSED, "unclosed.css", Syntax::Css),
    (UNCLOSED, "unclosed.scss", Syntax::Scss),
  ];

  let mut found = Vec::new();
  for (source, file, syntax) in snippets {
    found.extend(crashes(&runners, source, file, syntax));
  }

  found.sort();
  found.dedup();
  assert!(
    found.is_empty(),
    "{} rule crash(es) on multibyte input:\n{}",
    found.len(),
    found.join("\n")
  );
}
