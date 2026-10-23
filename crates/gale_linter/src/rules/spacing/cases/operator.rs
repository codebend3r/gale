//! The `operator` rules' upstream test suites, ported case for case, then
//! gale's own cases from the rules' earlier tests.  See
//! [`harness`](super::super::harness).

use gale_css_parser::Syntax::*;
use serde_json::json;

use crate::rules::spacing::harness::{bad, check, ok};

#[test]
fn media_feature_range_operator_space_after_always() {
  let expected_after = "Expected single space after range operator";
  check(
    "@stylistic/media-feature-range-operator-space-after",
    json!(["always"]),
    Css,
    &[
      ok("@media (width= 600px) {}"),
      ok("@mEdIa (width= 600px) {}"),
      ok("@MEDIA (width= 600px) {}"),
      ok("@media (width > 600px) {}"),
      ok("@media (width>= 600px) and (width<= 3em) {}"),
      ok("@media /*(width>=600px) and*/ (width <= 3em) {}"),
      ok("@media (width >= 600px) /*and (width<3em)*/ {}"),
      ok("@media (width >= /*>*/ 600px) {}"),
      bad("@media (width<600px) {}")
        .at(1, 15, expected_after)
        .fixed("@media (width< 600px) {}"),
      bad("@mEdIa (width<600px) {}")
        .at(1, 15, expected_after)
        .fixed("@mEdIa (width< 600px) {}"),
      bad("@MEDIA (width<600px) {}")
        .at(1, 15, expected_after)
        .fixed("@MEDIA (width< 600px) {}"),
      bad("@media (width<=  600px) {}")
        .at(1, 16, expected_after)
        .fixed("@media (width<= 600px) {}"),
      bad("@media (width=\t600px) {}")
        .at(1, 15, expected_after)
        .fixed("@media (width= 600px) {}"),
      bad("@media (width>\n600px) {}")
        .at(1, 15, expected_after)
        .fixed("@media (width> 600px) {}"),
      bad("@media (width>\r\n600px) {}")
        .at(1, 15, expected_after)
        .fixed("@media (width> 600px) {}"),
      bad("@media (width>=600px) and (width< 3em) {}")
        .at(1, 16, expected_after)
        .fixed("@media (width>= 600px) and (width< 3em) {}"),
      bad("@media (width> 600px) and (width=3em) {}")
        .at(1, 34, expected_after)
        .fixed("@media (width> 600px) and (width= 3em) {}"),
      bad("@media (width>=600px) and (width<3em) {}")
        .at(1, 16, expected_after)
        .at(1, 34, expected_after)
        .fixed("@media (width>= 600px) and (width< 3em) {}"),
      bad("@media (width</**/600px) {}")
        .at(1, 15, expected_after)
        .fixed("@media (width< /**/600px) {}"),
    ],
  );
}

#[test]
fn media_feature_range_operator_space_after_never() {
  let rejected_after = "Unexpected whitespace after range operator";
  check(
    "@stylistic/media-feature-range-operator-space-after",
    json!(["never"]),
    Css,
    &[
      ok("@media (width =600px) {}"),
      ok("@mEdIa (width =600px) {}"),
      ok("@MEDIA (width =600px) {}"),
      ok("@media (width>600px) {}"),
      ok("@media (width >=600px) and (width <=3em) {}"),
      ok("@media /*(width >= 600px) and*/ (width<=3em) {}"),
      ok("@media (width>=600px) /*and (width < 3em)*/ {}"),
      ok("@media (width>=/* > */ 600px) {}"),
      bad("@media (width < 600px) {}")
        .at(1, 16, rejected_after)
        .fixed("@media (width <600px) {}"),
      bad("@mEdIa (width < 600px) {}")
        .at(1, 16, rejected_after)
        .fixed("@mEdIa (width <600px) {}"),
      bad("@MEDIA (width < 600px) {}")
        .at(1, 16, rejected_after)
        .fixed("@MEDIA (width <600px) {}"),
      bad("@media (width <=  600px) {}")
        .at(1, 17, rejected_after)
        .fixed("@media (width <=600px) {}"),
      bad("@media (width =\t600px) {}")
        .at(1, 16, rejected_after)
        .fixed("@media (width =600px) {}"),
      bad("@media (width >\n600px) {}")
        .at(1, 16, rejected_after)
        .fixed("@media (width >600px) {}"),
      bad("@media (width >\r\n600px) {}")
        .at(1, 16, rejected_after)
        .fixed("@media (width >600px) {}"),
      bad("@media (width >= 600px) and (width <3em) {}")
        .at(1, 17, rejected_after)
        .fixed("@media (width >=600px) and (width <3em) {}"),
      bad("@media (width >600px) and (width = 3em) {}")
        .at(1, 35, rejected_after)
        .fixed("@media (width >600px) and (width =3em) {}"),
      bad("@media (width >= 600px) and (width < 3em) {}")
        .at(1, 17, rejected_after)
        .at(1, 37, rejected_after)
        .fixed("@media (width >=600px) and (width <3em) {}"),
      bad("@media (width = /**/ 600px) {}")
        .at(1, 16, rejected_after)
        .fixed("@media (width =/**/ 600px) {}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_feature_range_operator_space_after_always_gale_cases() {
  check(
    "@stylistic/media-feature-range-operator-space-after",
    json!("always"),
    Css,
    &[
      ok("@media (width >= 600px) { }"),
      bad("@media (width >=600px) { }")
        .at(1, 17, "Expected single space after range operator")
        .fixed("@media (width >= 600px) { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_feature_range_operator_space_after_never_gale_cases() {
  check(
    "@stylistic/media-feature-range-operator-space-after",
    json!("never"),
    Css,
    &[bad("@media (width >= 600px) { }")
      .at(1, 17, "Unexpected whitespace after range operator")
      .fixed("@media (width >=600px) { }")],
  );
}

#[test]
fn media_feature_range_operator_space_before_always() {
  let expected_before = "Expected single space before range operator";
  check(
    "@stylistic/media-feature-range-operator-space-before",
    json!(["always"]),
    Css,
    &[
      ok("@media (width = 600px) {}"),
      ok("@mEdIa (width = 600px) {}"),
      ok("@MEDIA (width = 600px) {}"),
      ok("@media (width >600px) {}"),
      ok("@media (width >= 600px) and (width <= 3em) {}"),
      ok("@media /*(width>=600px) and*/ (width <= 3em) {}"),
      ok("@media (width >= 600px) /*and (width<=3em)*/ {}"),
      ok("@media (width >= /*>*/ 600px) {}"),
      bad("@media (width< 600px) {}")
        .at(1, 13, expected_before)
        .fixed("@media (width < 600px) {}"),
      bad("@mEdIa (width< 600px) {}")
        .at(1, 13, expected_before)
        .fixed("@mEdIa (width < 600px) {}"),
      bad("@MEDIA (width< 600px) {}")
        .at(1, 13, expected_before)
        .fixed("@MEDIA (width < 600px) {}"),
      bad("@media (width  <= 600px) {}")
        .at(1, 15, expected_before)
        .fixed("@media (width <= 600px) {}"),
      bad("@media (width\t= 600px) {}")
        .at(1, 14, expected_before)
        .fixed("@media (width = 600px) {}"),
      bad("@media (width\n> 600px) {}")
        .at(1, 14, expected_before)
        .fixed("@media (width > 600px) {}"),
      bad("@media (width\r\n> 600px) {}")
        .at(1, 15, expected_before)
        .fixed("@media (width > 600px) {}"),
      bad("@media (width>= 600px) and (width < 3em) {}")
        .at(1, 13, expected_before)
        .fixed("@media (width >= 600px) and (width < 3em) {}"),
      bad("@media (width > 600px) and (width= 3em) {}")
        .at(1, 33, expected_before)
        .fixed("@media (width > 600px) and (width = 3em) {}"),
      bad("@media (width> 600px) and (width= 3em) {}")
        .at(1, 13, expected_before)
        .at(1, 32, expected_before)
        .fixed("@media (width > 600px) and (width = 3em) {}"),
      bad("@media (width/**/< 600px) {}")
        .at(1, 17, expected_before)
        .fixed("@media (width/**/ < 600px) {}"),
    ],
  );
}

#[test]
fn media_feature_range_operator_space_before_never() {
  let rejected_before = "Unexpected whitespace before range operator";
  check(
    "@stylistic/media-feature-range-operator-space-before",
    json!(["never"]),
    Css,
    &[
      ok("@media (width= 600px) {}"),
      ok("@mEdIa (width= 600px) {}"),
      ok("@MEDIA (width= 600px) {}"),
      ok("@media (width>600px) {}"),
      ok("@media (width>= 600px) and (width<= 3em) {}"),
      ok("@media /*(width >= 600px) and*/ (width<= 3em) {}"),
      ok("@media (width>= 600px) /*and (width <= 3em)*/ {}"),
      ok("@media (width>= /* > */ 600px) {}"),
      bad("@media (width < 600px) {}")
        .at(1, 14, rejected_before)
        .fixed("@media (width< 600px) {}"),
      bad("@mEdIa (width < 600px) {}")
        .at(1, 14, rejected_before)
        .fixed("@mEdIa (width< 600px) {}"),
      bad("@MEDIA (width < 600px) {}")
        .at(1, 14, rejected_before)
        .fixed("@MEDIA (width< 600px) {}"),
      bad("@media (width  <= 600px) {}")
        .at(1, 15, rejected_before)
        .fixed("@media (width<= 600px) {}"),
      bad("@media (width\t= 600px) {}")
        .at(1, 14, rejected_before)
        .fixed("@media (width= 600px) {}"),
      bad("@media (width\n> 600px) {}")
        .at(1, 14, rejected_before)
        .fixed("@media (width> 600px) {}"),
      bad("@media (width\r\n> 600px) {}")
        .at(1, 15, rejected_before)
        .fixed("@media (width> 600px) {}"),
      bad("@media (width>= 600px) and (width < 3em) {}")
        .at(1, 34, rejected_before)
        .fixed("@media (width>= 600px) and (width< 3em) {}"),
      bad("@media (width > 600px) and (width= 3em) {}")
        .at(1, 14, rejected_before)
        .fixed("@media (width> 600px) and (width= 3em) {}"),
      bad("@media (width >= 600px) and (width < 3em) {}")
        .at(1, 14, rejected_before)
        .at(1, 35, rejected_before)
        .fixed("@media (width>= 600px) and (width< 3em) {}"),
      bad("@media (width /**/ = 600px) {}")
        .at(1, 19, rejected_before)
        .fixed("@media (width /**/= 600px) {}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_feature_range_operator_space_before_always_gale_cases() {
  check(
    "@stylistic/media-feature-range-operator-space-before",
    json!("always"),
    Css,
    &[
      ok("@media (width >= 600px) { }"),
      bad("@media (width>= 600px) { }")
        .at(1, 13, "Expected single space before range operator")
        .fixed("@media (width >= 600px) { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn media_feature_range_operator_space_before_never_gale_cases() {
  check(
    "@stylistic/media-feature-range-operator-space-before",
    json!("never"),
    Css,
    &[
      bad("@media (width >= 600px) { }")
        .at(1, 14, "Unexpected whitespace before range operator")
        .fixed("@media (width>= 600px) { }"),
      ok("@media (width>=600px) { }"),
    ],
  );
}

#[test]
fn selector_attribute_operator_space_after_always() {
  check(
    "@stylistic/selector-attribute-operator-space-after",
    json!(["always"]),
    Css,
    &[
      ok(".foo {}"),
      ok("[target] {}"),
      ok("[ target] {}"),
      ok("[target ] {}"),
      ok("[ target ] {}"),
      ok("[target= _blank] {}"),
      ok("[target = _blank] {}"),
      ok("[target= _blank ] {}"),
      ok("[target = _blank ] {}"),
      ok("[ target= _blank] {}"),
      ok("[ target = _blank] {}"),
      ok("[ target= _blank ] {}"),
      ok("[ target = _blank ] {}"),
      ok("[target= '_blank'] {}"),
      ok("[target = '_blank'] {}"),
      ok("[target= ' _blank'] {}"),
      ok("[target= '_blank' ] {}"),
      ok("[target= '_blank '] {}"),
      ok("[target = '_blank' ] {}"),
      ok("[target= ' _blank '] {}"),
      ok("[ target= '_blank'] {}"),
      ok("[ target = '_blank'] {}"),
      ok("[ target= ' _blank'] {}"),
      ok("[ target= '_blank' ] {}"),
      ok("[ target= '_blank '] {}"),
      ok("[ target = '_blank' ] {}"),
      ok("[ target= ' _blank '] {}"),
      ok("[target= \"_blank\"] {}"),
      ok("[target = \"_blank\"] {}"),
      ok("[target= \" _blank\"] {}"),
      ok("[target= \"_blank\" ] {}"),
      ok("[target= \"_blank \"] {}"),
      ok("[target = \"_blank\" ] {}"),
      ok("[target= \" _blank \"] {}"),
      ok("[ target= \"_blank\"] {}"),
      ok("[ target = \"_blank\"] {}"),
      ok("[ target= \" _blank\"] {}"),
      ok("[ target= \"_blank\" ] {}"),
      ok("[ target= \"_blank \"] {}"),
      ok("[ target = \"_blank\" ] {}"),
      ok("[ target= \" _blank \"] {}"),
      ok("[title~= flower] { }"),
      ok("[title ~= flower] { }"),
      ok("[title|= flower] { }"),
      ok("[title |= flower] { }"),
      ok("[title^= flower] { }"),
      ok("[title ^= flower] { }"),
      ok("[title$= flower] { }"),
      ok("[title $= flower] { }"),
      ok("a[href*= w3schools] { }"),
      ok("a[href *= w3schools] { }"),
      ok("img[alt~= person][src*= lorem] { }"),
      ok("img[alt ~= person][src*= lorem] { }"),
      ok("img[alt~= person][src *= lorem] { }"),
      ok("img[alt ~= person][src *= lorem] { }"),
      ok("[target= _blank]:hover { }"),
      ok("[target = _blank]:hover { }"),
      ok("[target= _blank]::before { }"),
      ok("[target = _blank]::before { }"),
      ok("option[data-hidden= true] { }"),
      ok("option[data-hidden = true] { }"),
      ok("option[dataHidden] { }"),
      ok("@media screen and (max-width: 480px) { img[align= right] {} }"),
      ok("@media screen and (max-width: 480px) { img[align = right] {} }"),
      ok("[target= _blank] .foo { }"),
      ok("[target = _blank] .foo { }"),
      ok("li[aria-hidden= false]:nth-child(1) { }"),
      ok("li[aria-hidden= false]:nth-child( 1) { }"),
      ok("li[aria-hidden= false]:nth-child(1 ) { }"),
      ok("li[aria-hidden= false]:nth-child( 1 ) { }"),
      ok("li[aria-hidden = false]:nth-child(1) { }"),
      ok("li[aria-hidden = false]:nth-child( 1) { }"),
      ok("li[aria-hidden = false]:nth-child(1 ) { }"),
      ok("li[aria-hidden = false]:nth-child( 1 ) { }"),
      ok("ul li[aria-hidden= false] + li[aria-hidden= false] a { }"),
      ok("ul li[aria-hidden = false] + li[aria-hidden= false] a { }"),
      ok("ul li[aria-hidden= false] + li[aria-hidden = false] a { }"),
      ok("ul li[aria-hidden = false] + li[aria-hidden = false] a { }"),
      ok("[target] { content: \" \" }"),
      ok("[target ] { content: \" \" }"),
      ok("[ target] { content: \" \" }"),
      ok("[ target ] { content: \" \" }"),
      ok(":root { --foo: 1px; }"),
      ok("html { --foo: 1px; }"),
      ok(":root { --custom-property-set: {} }"),
      ok("html { --custom-property-set: {} }"),
      ok("a[b=#{c}] { }"),
      bad("[target=_blank] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= _blank] { }"),
      bad("[target =_blank] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[target = _blank] { }"),
      bad("[target=_blank ] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= _blank ] { }"),
      bad("[target =_blank ] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[target = _blank ] { }"),
      bad("[ target=_blank] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= _blank] { }"),
      bad("[ target =_blank] { }")
        .at(1, 10, "Expected single space after \"=\"")
        .fixed("[ target = _blank] { }"),
      bad("[ target=_blank ] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= _blank ] { }"),
      bad("[ target =_blank ] { }")
        .at(1, 10, "Expected single space after \"=\"")
        .fixed("[ target = _blank ] { }"),
      bad("[target='_blank'] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= '_blank'] { }"),
      bad("[target ='_blank'] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[target = '_blank'] { }"),
      bad("[target=' _blank'] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= ' _blank'] { }"),
      bad("[target='_blank' ] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= '_blank' ] { }"),
      bad("[target='_blank '] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= '_blank '] { }"),
      bad("[target ='_blank' ] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[target = '_blank' ] { }"),
      bad("[target=' _blank '] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= ' _blank '] { }"),
      bad("[ target='_blank'] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= '_blank'] { }"),
      bad("[ target ='_blank'] { }")
        .at(1, 10, "Expected single space after \"=\"")
        .fixed("[ target = '_blank'] { }"),
      bad("[ target=' _blank'] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= ' _blank'] { }"),
      bad("[ target='_blank' ] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= '_blank' ] { }"),
      bad("[ target='_blank '] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= '_blank '] { }"),
      bad("[ target ='_blank' ] { }")
        .at(1, 10, "Expected single space after \"=\"")
        .fixed("[ target = '_blank' ] { }"),
      bad("[ target=' _blank '] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= ' _blank '] { }"),
      bad("[target=\"_blank\"] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= \"_blank\"] { }"),
      bad("[target =\"_blank\"] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[target = \"_blank\"] { }"),
      bad("[target=\" _blank\"] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= \" _blank\"] { }"),
      bad("[target=\"_blank\" ] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= \"_blank\" ] { }"),
      bad("[target=\"_blank \"] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= \"_blank \"] { }"),
      bad("[target =\"_blank\" ] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[target = \"_blank\" ] { }"),
      bad("[target=\" _blank \"] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= \" _blank \"] { }"),
      bad("[ target=\"_blank\"] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= \"_blank\"] { }"),
      bad("[ target =\"_blank\"] { }")
        .at(1, 10, "Expected single space after \"=\"")
        .fixed("[ target = \"_blank\"] { }"),
      bad("[ target=\" _blank\"] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= \" _blank\"] { }"),
      bad("[ target=\"_blank\" ] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= \"_blank\" ] { }"),
      bad("[ target=\"_blank \"] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= \"_blank \"] { }"),
      bad("[ target =\"_blank\" ] { }")
        .at(1, 10, "Expected single space after \"=\"")
        .fixed("[ target = \"_blank\" ] { }"),
      bad("[ target=\" _blank \"] { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[ target= \" _blank \"] { }"),
      bad("[title~=flower] { }")
        .at(1, 8, "Expected single space after \"~=\"")
        .fixed("[title~= flower] { }"),
      bad("[title ~=flower] { }")
        .at(1, 9, "Expected single space after \"~=\"")
        .fixed("[title ~= flower] { }"),
      bad("[title|=flower] { }")
        .at(1, 8, "Expected single space after \"|=\"")
        .fixed("[title|= flower] { }"),
      bad("[title |=flower] { }")
        .at(1, 9, "Expected single space after \"|=\"")
        .fixed("[title |= flower] { }"),
      bad("[title^=flower] { }")
        .at(1, 8, "Expected single space after \"^=\"")
        .fixed("[title^= flower] { }"),
      bad("[title ^=flower] { }")
        .at(1, 9, "Expected single space after \"^=\"")
        .fixed("[title ^= flower] { }"),
      bad("[title$=flower] { }")
        .at(1, 8, "Expected single space after \"$=\"")
        .fixed("[title$= flower] { }"),
      bad("[title $=flower] { }")
        .at(1, 9, "Expected single space after \"$=\"")
        .fixed("[title $= flower] { }"),
      bad("a[href*=w3schools] { }")
        .at(1, 8, "Expected single space after \"*=\"")
        .fixed("a[href*= w3schools] { }"),
      bad("a[href *=w3schools] { }")
        .at(1, 9, "Expected single space after \"*=\"")
        .fixed("a[href *= w3schools] { }"),
      bad("img[alt~=person][src*= lorem] { }")
        .at(1, 9, "Expected single space after \"~=\"")
        .fixed("img[alt~= person][src*= lorem] { }"),
      bad("img[alt ~=person][src*= lorem] { }")
        .at(1, 10, "Expected single space after \"~=\"")
        .fixed("img[alt ~= person][src*= lorem] { }"),
      bad("img[alt~= person][src *=lorem] { }")
        .at(1, 24, "Expected single space after \"*=\"")
        .fixed("img[alt~= person][src *= lorem] { }"),
      bad("img[alt ~= person][src *=lorem] { }")
        .at(1, 25, "Expected single space after \"*=\"")
        .fixed("img[alt ~= person][src *= lorem] { }"),
      bad("[target=_blank]:hover { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= _blank]:hover { }"),
      bad("[target =_blank]:hover { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[target = _blank]:hover { }"),
      bad("[target=_blank]::before { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= _blank]::before { }"),
      bad("[target =_blank]::before { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[target = _blank]::before { }"),
      bad("option[data-hidden=true] { }")
        .at(1, 19, "Expected single space after \"=\"")
        .fixed("option[data-hidden= true] { }"),
      bad("option[data-hidden =true] { }")
        .at(1, 20, "Expected single space after \"=\"")
        .fixed("option[data-hidden = true] { }"),
      bad("@media screen and (max-width: 480px) { img[align=right] {} }")
        .at(1, 49, "Expected single space after \"=\"")
        .fixed("@media screen and (max-width: 480px) { img[align= right] {} }"),
      bad("@media screen and (max-width: 480px) { img[align =right] {} }")
        .at(1, 50, "Expected single space after \"=\"")
        .fixed("@media screen and (max-width: 480px) { img[align = right] {} }"),
      bad("[target=_blank] .foo { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= _blank] .foo { }"),
      bad("[target =_blank] .foo { }")
        .at(1, 9, "Expected single space after \"=\"")
        .fixed("[target = _blank] .foo { }"),
      bad("li[aria-hidden=false]:nth-child(1) { }")
        .at(1, 15, "Expected single space after \"=\"")
        .fixed("li[aria-hidden= false]:nth-child(1) { }"),
      bad("li[aria-hidden=false]:nth-child( 1) { }")
        .at(1, 15, "Expected single space after \"=\"")
        .fixed("li[aria-hidden= false]:nth-child( 1) { }"),
      bad("li[aria-hidden=false]:nth-child(1 ) { }")
        .at(1, 15, "Expected single space after \"=\"")
        .fixed("li[aria-hidden= false]:nth-child(1 ) { }"),
      bad("li[aria-hidden=false]:nth-child( 1 ) { }")
        .at(1, 15, "Expected single space after \"=\"")
        .fixed("li[aria-hidden= false]:nth-child( 1 ) { }"),
      bad("li[aria-hidden =false]:nth-child(1) { }")
        .at(1, 16, "Expected single space after \"=\"")
        .fixed("li[aria-hidden = false]:nth-child(1) { }"),
      bad("li[aria-hidden =false]:nth-child( 1) { }")
        .at(1, 16, "Expected single space after \"=\"")
        .fixed("li[aria-hidden = false]:nth-child( 1) { }"),
      bad("li[aria-hidden =false]:nth-child(1 ) { }")
        .at(1, 16, "Expected single space after \"=\"")
        .fixed("li[aria-hidden = false]:nth-child(1 ) { }"),
      bad("li[aria-hidden =false]:nth-child( 1 ) { }")
        .at(1, 16, "Expected single space after \"=\"")
        .fixed("li[aria-hidden = false]:nth-child( 1 ) { }"),
      bad("ul li[aria-hidden=false] + li[aria-hidden= false] a { }")
        .at(1, 18, "Expected single space after \"=\"")
        .fixed("ul li[aria-hidden= false] + li[aria-hidden= false] a { }"),
      bad("ul li[aria-hidden= false] + li[aria-hidden=false] a { }")
        .at(1, 43, "Expected single space after \"=\"")
        .fixed("ul li[aria-hidden= false] + li[aria-hidden= false] a { }"),
      bad("ul li[aria-hidden =false] + li[aria-hidden= false] a { }")
        .at(1, 19, "Expected single space after \"=\"")
        .fixed("ul li[aria-hidden = false] + li[aria-hidden= false] a { }"),
      bad("ul li[aria-hidden = false] + li[aria-hidden=false] a { }")
        .at(1, 44, "Expected single space after \"=\"")
        .fixed("ul li[aria-hidden = false] + li[aria-hidden= false] a { }"),
      bad("ul li[aria-hidden=false] + li[aria-hidden = false] a { }")
        .at(1, 18, "Expected single space after \"=\"")
        .fixed("ul li[aria-hidden= false] + li[aria-hidden = false] a { }"),
      bad("ul li[aria-hidden= false] + li[aria-hidden =false] a { }")
        .at(1, 44, "Expected single space after \"=\"")
        .fixed("ul li[aria-hidden= false] + li[aria-hidden = false] a { }"),
      bad("ul li[aria-hidden =false] + li[aria-hidden = false] a { }")
        .at(1, 19, "Expected single space after \"=\"")
        .fixed("ul li[aria-hidden = false] + li[aria-hidden = false] a { }"),
      bad("ul li[aria-hidden = false] + li[aria-hidden =false] a { }")
        .at(1, 45, "Expected single space after \"=\"")
        .fixed("ul li[aria-hidden = false] + li[aria-hidden = false] a { }"),
      bad("ul li[aria-hidden=false] + li[aria-hidden=false] a { }")
        .at(1, 18, "Expected single space after \"=\"")
        .at(1, 42, "Expected single space after \"=\"")
        .fixed("ul li[aria-hidden= false] + li[aria-hidden= false] a { }"),
      bad("[target=/**/\"_blank\"] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= /**/\"_blank\"] { }"),
      bad("[target=/**/ \"_blank\"] { }")
        .at(1, 8, "Expected single space after \"=\"")
        .fixed("[target= /**/ \"_blank\"] { }"),
    ],
  );
}

#[test]
fn selector_attribute_operator_space_after_never() {
  check(
    "@stylistic/selector-attribute-operator-space-after",
    json!(["never"]),
    Css,
    &[
      ok(".foo { }"),
      ok("[target] { }"),
      ok("[ target] { }"),
      ok("[target ] { }"),
      ok("[ target ] { }"),
      ok("[target=_blank] { }"),
      ok("[target =_blank] { }"),
      ok("[target=_blank ] { }"),
      ok("[target =_blank ] { }"),
      ok("[ target=_blank] { }"),
      ok("[ target =_blank] { }"),
      ok("[ target=_blank ] { }"),
      ok("[ target =_blank ] { }"),
      ok("[target='_blank'] { }"),
      ok("[target ='_blank'] { }"),
      ok("[target=' _blank'] { }"),
      ok("[target='_blank' ] { }"),
      ok("[target='_blank '] { }"),
      ok("[target ='_blank' ] { }"),
      ok("[target=' _blank '] { }"),
      ok("[ target='_blank'] { }"),
      ok("[ target ='_blank'] { }"),
      ok("[ target=' _blank'] { }"),
      ok("[ target='_blank' ] { }"),
      ok("[ target='_blank '] { }"),
      ok("[ target ='_blank' ] { }"),
      ok("[ target=' _blank '] { }"),
      ok("[target=\"_blank\"] { }"),
      ok("[target =\"_blank\"] { }"),
      ok("[target=\" _blank\"] { }"),
      ok("[target=\"_blank\" ] { }"),
      ok("[target=\"_blank \"] { }"),
      ok("[target =\"_blank\" ] { }"),
      ok("[target=\" _blank \"] { }"),
      ok("[ target=\"_blank\"] { }"),
      ok("[ target =\"_blank\"] { }"),
      ok("[ target=\" _blank\"] { }"),
      ok("[ target=\"_blank\" ] { }"),
      ok("[ target=\"_blank \"] { }"),
      ok("[ target =\"_blank\" ] { }"),
      ok("[ target=\" _blank \"] { }"),
      ok("[title~=flower] { }"),
      ok("[title ~=flower] { }"),
      ok("[title|=flower] { }"),
      ok("[title |=flower] { }"),
      ok("[title^=flower] { }"),
      ok("[title ^=flower] { }"),
      ok("[title$=flower] { }"),
      ok("[title $=flower] { }"),
      ok("a[href*=w3schools] { }"),
      ok("a[href *=w3schools] { }"),
      ok("img[alt~=person][src*=lorem] { }"),
      ok("img[alt ~=person][src*=lorem] { }"),
      ok("img[alt~=person][src *=lorem] { }"),
      ok("img[alt ~=person][src *=lorem] { }"),
      ok("[target=_blank]:hover { }"),
      ok("[target =_blank]:hover { }"),
      ok("[target=_blank]::before { }"),
      ok("[target =_blank]::before { }"),
      ok("option[data-hidden=true] { }"),
      ok("option[data-hidden =true] { }"),
      ok("option[dataHidden] { }"),
      ok("@media screen and (max-width: 480px) { img[align=right] {} }"),
      ok("@media screen and (max-width: 480px) { img[align =right] {} }"),
      ok("[target=_blank] .foo { }"),
      ok("[target =_blank] .foo { }"),
      ok("li[aria-hidden=false]:nth-child(1) { }"),
      ok("li[aria-hidden=false]:nth-child( 1) { }"),
      ok("li[aria-hidden=false]:nth-child(1 ) { }"),
      ok("li[aria-hidden=false]:nth-child( 1 ) { }"),
      ok("li[aria-hidden =false]:nth-child(1) { }"),
      ok("li[aria-hidden =false]:nth-child( 1) { }"),
      ok("li[aria-hidden =false]:nth-child(1 ) { }"),
      ok("li[aria-hidden =false]:nth-child( 1 ) { }"),
      ok("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      ok("ul li[aria-hidden =false] + li[aria-hidden=false] a { }"),
      ok("ul li[aria-hidden=false] + li[aria-hidden =false] a { }"),
      ok("ul li[aria-hidden =false] + li[aria-hidden =false] a { }"),
      ok("[target] { content: \" \" }"),
      ok("[target ] { content: \" \" }"),
      ok("[ target] { content: \" \" }"),
      ok("[ target ] { content: \" \" }"),
      bad("[target= _blank] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=_blank] { }"),
      bad("[target = _blank] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[target =_blank] { }"),
      bad("[target= _blank ] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=_blank ] { }"),
      bad("[target = _blank ] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[target =_blank ] { }"),
      bad("[ target= _blank] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target=_blank] { }"),
      bad("[ target = _blank] { }")
        .at(1, 10, "Unexpected whitespace after \"=\"")
        .fixed("[ target =_blank] { }"),
      bad("[ target= _blank ] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target=_blank ] { }"),
      bad("[ target = _blank ] { }")
        .at(1, 10, "Unexpected whitespace after \"=\"")
        .fixed("[ target =_blank ] { }"),
      bad("[target= '_blank'] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target='_blank'] { }"),
      bad("[target = '_blank'] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[target ='_blank'] { }"),
      bad("[target= ' _blank'] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=' _blank'] { }"),
      bad("[target= '_blank' ] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target='_blank' ] { }"),
      bad("[target= '_blank '] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target='_blank '] { }"),
      bad("[target = '_blank' ] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[target ='_blank' ] { }"),
      bad("[target= ' _blank '] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=' _blank '] { }"),
      bad("[ target= '_blank'] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target='_blank'] { }"),
      bad("[ target = '_blank'] { }")
        .at(1, 10, "Unexpected whitespace after \"=\"")
        .fixed("[ target ='_blank'] { }"),
      bad("[ target= ' _blank'] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target=' _blank'] { }"),
      bad("[ target= '_blank' ] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target='_blank' ] { }"),
      bad("[ target= '_blank '] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target='_blank '] { }"),
      bad("[ target = '_blank' ] { }")
        .at(1, 10, "Unexpected whitespace after \"=\"")
        .fixed("[ target ='_blank' ] { }"),
      bad("[ target= ' _blank '] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target=' _blank '] { }"),
      bad("[target= \"_blank\"] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=\"_blank\"] { }"),
      bad("[target = \"_blank\"] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[target =\"_blank\"] { }"),
      bad("[target= \" _blank\"] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=\" _blank\"] { }"),
      bad("[target= \"_blank\" ] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=\"_blank\" ] { }"),
      bad("[target= \"_blank \"] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=\"_blank \"] { }"),
      bad("[target = \"_blank\" ] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[target =\"_blank\" ] { }"),
      bad("[target= \" _blank \"] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=\" _blank \"] { }"),
      bad("[ target= \"_blank\"] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target=\"_blank\"] { }"),
      bad("[ target = \"_blank\"] { }")
        .at(1, 10, "Unexpected whitespace after \"=\"")
        .fixed("[ target =\"_blank\"] { }"),
      bad("[ target= \" _blank\"] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target=\" _blank\"] { }"),
      bad("[ target= \"_blank\" ] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target=\"_blank\" ] { }"),
      bad("[ target= \"_blank \"] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target=\"_blank \"] { }"),
      bad("[ target = \"_blank\" ] { }")
        .at(1, 10, "Unexpected whitespace after \"=\"")
        .fixed("[ target =\"_blank\" ] { }"),
      bad("[ target= \" _blank \"] { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[ target=\" _blank \"] { }"),
      bad("[title~= flower] { }")
        .at(1, 8, "Unexpected whitespace after \"~=\"")
        .fixed("[title~=flower] { }"),
      bad("[title ~= flower] { }")
        .at(1, 9, "Unexpected whitespace after \"~=\"")
        .fixed("[title ~=flower] { }"),
      bad("[title|= flower] { }")
        .at(1, 8, "Unexpected whitespace after \"|=\"")
        .fixed("[title|=flower] { }"),
      bad("[title |= flower] { }")
        .at(1, 9, "Unexpected whitespace after \"|=\"")
        .fixed("[title |=flower] { }"),
      bad("[title^= flower] { }")
        .at(1, 8, "Unexpected whitespace after \"^=\"")
        .fixed("[title^=flower] { }"),
      bad("[title ^= flower] { }")
        .at(1, 9, "Unexpected whitespace after \"^=\"")
        .fixed("[title ^=flower] { }"),
      bad("[title$= flower] { }")
        .at(1, 8, "Unexpected whitespace after \"$=\"")
        .fixed("[title$=flower] { }"),
      bad("[title $= flower] { }")
        .at(1, 9, "Unexpected whitespace after \"$=\"")
        .fixed("[title $=flower] { }"),
      bad("a[href*= w3schools] { }")
        .at(1, 8, "Unexpected whitespace after \"*=\"")
        .fixed("a[href*=w3schools] { }"),
      bad("a[href *= w3schools] { }")
        .at(1, 9, "Unexpected whitespace after \"*=\"")
        .fixed("a[href *=w3schools] { }"),
      bad("img[alt~= person][src*=lorem] { }")
        .at(1, 9, "Unexpected whitespace after \"~=\"")
        .fixed("img[alt~=person][src*=lorem] { }"),
      bad("img[alt ~= person][src*=lorem] { }")
        .at(1, 10, "Unexpected whitespace after \"~=\"")
        .fixed("img[alt ~=person][src*=lorem] { }"),
      bad("img[alt~=person][src *= lorem] { }")
        .at(1, 23, "Unexpected whitespace after \"*=\"")
        .fixed("img[alt~=person][src *=lorem] { }"),
      bad("img[alt ~=person][src *= lorem] { }")
        .at(1, 24, "Unexpected whitespace after \"*=\"")
        .fixed("img[alt ~=person][src *=lorem] { }"),
      bad("[target= _blank]:hover { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=_blank]:hover { }"),
      bad("[target = _blank]:hover { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[target =_blank]:hover { }"),
      bad("[target= _blank]::before { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=_blank]::before { }"),
      bad("[target = _blank]::before { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[target =_blank]::before { }"),
      bad("option[data-hidden= true] { }")
        .at(1, 19, "Unexpected whitespace after \"=\"")
        .fixed("option[data-hidden=true] { }"),
      bad("option[data-hidden = true] { }")
        .at(1, 20, "Unexpected whitespace after \"=\"")
        .fixed("option[data-hidden =true] { }"),
      bad("@media screen and (max-width: 480px) { img[align= right] {} }")
        .at(1, 49, "Unexpected whitespace after \"=\"")
        .fixed("@media screen and (max-width: 480px) { img[align=right] {} }"),
      bad("@media screen and (max-width: 480px) { img[align = right] {} }")
        .at(1, 50, "Unexpected whitespace after \"=\"")
        .fixed("@media screen and (max-width: 480px) { img[align =right] {} }"),
      bad("[target= _blank] .foo { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=_blank] .foo { }"),
      bad("[target = _blank] .foo { }")
        .at(1, 9, "Unexpected whitespace after \"=\"")
        .fixed("[target =_blank] .foo { }"),
      bad("li[aria-hidden= false]:nth-child(1) { }")
        .at(1, 15, "Unexpected whitespace after \"=\"")
        .fixed("li[aria-hidden=false]:nth-child(1) { }"),
      bad("li[aria-hidden= false]:nth-child( 1) { }")
        .at(1, 15, "Unexpected whitespace after \"=\"")
        .fixed("li[aria-hidden=false]:nth-child( 1) { }"),
      bad("li[aria-hidden= false]:nth-child(1 ) { }")
        .at(1, 15, "Unexpected whitespace after \"=\"")
        .fixed("li[aria-hidden=false]:nth-child(1 ) { }"),
      bad("li[aria-hidden= false]:nth-child( 1 ) { }")
        .at(1, 15, "Unexpected whitespace after \"=\"")
        .fixed("li[aria-hidden=false]:nth-child( 1 ) { }"),
      bad("li[aria-hidden = false]:nth-child(1) { }")
        .at(1, 16, "Unexpected whitespace after \"=\"")
        .fixed("li[aria-hidden =false]:nth-child(1) { }"),
      bad("li[aria-hidden = false]:nth-child( 1) { }")
        .at(1, 16, "Unexpected whitespace after \"=\"")
        .fixed("li[aria-hidden =false]:nth-child( 1) { }"),
      bad("li[aria-hidden = false]:nth-child(1 ) { }")
        .at(1, 16, "Unexpected whitespace after \"=\"")
        .fixed("li[aria-hidden =false]:nth-child(1 ) { }"),
      bad("li[aria-hidden = false]:nth-child( 1 ) { }")
        .at(1, 16, "Unexpected whitespace after \"=\"")
        .fixed("li[aria-hidden =false]:nth-child( 1 ) { }"),
      bad("ul li[aria-hidden= false] + li[aria-hidden=false] a { }")
        .at(1, 18, "Unexpected whitespace after \"=\"")
        .fixed("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden=false] + li[aria-hidden= false] a { }")
        .at(1, 42, "Unexpected whitespace after \"=\"")
        .fixed("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden = false] + li[aria-hidden=false] a { }")
        .at(1, 19, "Unexpected whitespace after \"=\"")
        .fixed("ul li[aria-hidden =false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden =false] + li[aria-hidden= false] a { }")
        .at(1, 43, "Unexpected whitespace after \"=\"")
        .fixed("ul li[aria-hidden =false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden= false] + li[aria-hidden =false] a { }")
        .at(1, 18, "Unexpected whitespace after \"=\"")
        .fixed("ul li[aria-hidden=false] + li[aria-hidden =false] a { }"),
      bad("ul li[aria-hidden=false] + li[aria-hidden = false] a { }")
        .at(1, 43, "Unexpected whitespace after \"=\"")
        .fixed("ul li[aria-hidden=false] + li[aria-hidden =false] a { }"),
      bad("ul li[aria-hidden = false] + li[aria-hidden =false] a { }")
        .at(1, 19, "Unexpected whitespace after \"=\"")
        .fixed("ul li[aria-hidden =false] + li[aria-hidden =false] a { }"),
      bad("ul li[aria-hidden =false] + li[aria-hidden = false] a { }")
        .at(1, 44, "Unexpected whitespace after \"=\"")
        .fixed("ul li[aria-hidden =false] + li[aria-hidden =false] a { }"),
      bad("ul li[aria-hidden = false] + li[aria-hidden = false] a { }")
        .at(1, 19, "Unexpected whitespace after \"=\"")
        .at(1, 45, "Unexpected whitespace after \"=\"")
        .fixed("ul li[aria-hidden =false] + li[aria-hidden =false] a { }"),
      bad("[target= /**/\"_blank\"] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=/**/\"_blank\"] { }"),
      bad("[target= /**/ \"_blank\"] { }")
        .at(1, 8, "Unexpected whitespace after \"=\"")
        .fixed("[target=/**/ \"_blank\"] { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_attribute_operator_space_after_always_gale_cases() {
  check(
    "@stylistic/selector-attribute-operator-space-after",
    json!("always"),
    Css,
    &[
      ok("[attr= value] { }"),
      bad("[attr=value] { }")
        .at(1, 6, "Expected single space after \"=\"")
        .fixed("[attr= value] { }"),
      bad("[class^=foo] { }")
        .at(1, 8, "Expected single space after \"^=\"")
        .fixed("[class^= foo] { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_attribute_operator_space_after_never_gale_cases() {
  check(
    "@stylistic/selector-attribute-operator-space-after",
    json!("never"),
    Css,
    &[
      ok("[attr=value] { }"),
      bad("[attr= value] { }")
        .at(1, 6, "Unexpected whitespace after \"=\"")
        .fixed("[attr=value] { }"),
    ],
  );
}

#[test]
fn selector_attribute_operator_space_before_always() {
  check(
    "@stylistic/selector-attribute-operator-space-before",
    json!(["always"]),
    Css,
    &[
      ok(".foo { }"),
      ok("[target] { }"),
      ok("[ target] { }"),
      ok("[target ] { }"),
      ok("[ target ] { }"),
      ok("[target =_blank] { }"),
      ok("[target = _blank] { }"),
      ok("[target =_blank ] { }"),
      ok("[target = _blank ] { }"),
      ok("[ target =_blank] { }"),
      ok("[ target = _blank] { }"),
      ok("[ target =_blank ] { }"),
      ok("[ target = _blank ] { }"),
      ok("[target ='_blank'] { }"),
      ok("[target = '_blank'] { }"),
      ok("[target =' _blank'] { }"),
      ok("[target ='_blank' ] { }"),
      ok("[target ='_blank '] { }"),
      ok("[target = '_blank' ] { }"),
      ok("[target =' _blank '] { }"),
      ok("[ target ='_blank'] { }"),
      ok("[ target = '_blank'] { }"),
      ok("[ target =' _blank'] { }"),
      ok("[ target ='_blank' ] { }"),
      ok("[ target ='_blank '] { }"),
      ok("[ target = '_blank' ] { }"),
      ok("[ target =' _blank '] { }"),
      ok("[target =\"_blank\"] { }"),
      ok("[target = \"_blank\"] { }"),
      ok("[target =\" _blank\"] { }"),
      ok("[target =\"_blank\" ] { }"),
      ok("[target =\"_blank \"] { }"),
      ok("[target = \"_blank\" ] { }"),
      ok("[target =\" _blank \"] { }"),
      ok("[ target =\"_blank\"] { }"),
      ok("[ target = \"_blank\"] { }"),
      ok("[ target =\" _blank\"] { }"),
      ok("[ target =\"_blank\" ] { }"),
      ok("[ target =\"_blank \"] { }"),
      ok("[ target = \"_blank\" ] { }"),
      ok("[ target =\" _blank \"] { }"),
      ok("[title ~=flower] { }"),
      ok("[title ~= flower] { }"),
      ok("[title |=flower] { }"),
      ok("[title |= flower] { }"),
      ok("[title ^=flower] { }"),
      ok("[title ^= flower] { }"),
      ok("[title $=flower] { }"),
      ok("[title $= flower] { }"),
      ok("a[href *=w3schools] { }"),
      ok("a[href *= w3schools] { }"),
      ok("img[alt ~=person][src *=lorem] { }"),
      ok("img[alt ~= person][src *=lorem] { }"),
      ok("img[alt ~=person][src *= lorem] { }"),
      ok("img[alt ~= person][src *= lorem] { }"),
      ok("[target =_blank]:hover { }"),
      ok("[target = _blank]:hover { }"),
      ok("[target =_blank]::before { }"),
      ok("[target = _blank]::before { }"),
      ok("option[data-hidden =true] { }"),
      ok("option[data-hidden = true] { }"),
      ok("option[dataHidden] { }"),
      ok("@media screen and (max-width: 480px) { img[align =right] {} }"),
      ok("@media screen and (max-width: 480px) { img[align = right] {} }"),
      ok("[target =_blank] .foo { }"),
      ok("[target = _blank] .foo { }"),
      ok("li[aria-hidden =false]:nth-child(1) { }"),
      ok("li[aria-hidden =false]:nth-child( 1) { }"),
      ok("li[aria-hidden =false]:nth-child(1 ) { }"),
      ok("li[aria-hidden =false]:nth-child( 1 ) { }"),
      ok("li[aria-hidden = false]:nth-child(1) { }"),
      ok("li[aria-hidden = false]:nth-child( 1) { }"),
      ok("li[aria-hidden = false]:nth-child(1 ) { }"),
      ok("li[aria-hidden = false]:nth-child( 1 ) { }"),
      ok("ul li[aria-hidden =false] + li[aria-hidden =false] a { }"),
      ok("ul li[aria-hidden = false] + li[aria-hidden =false] a { }"),
      ok("ul li[aria-hidden =false] + li[aria-hidden = false] a { }"),
      ok("ul li[aria-hidden = false] + li[aria-hidden = false] a { }"),
      ok("[target] { content: \" \" }"),
      ok("[target ] { content: \" \" }"),
      ok("[ target] { content: \" \" }"),
      ok("[ target ] { content: \" \" }"),
      ok(":root { --foo: 1px; }"),
      ok("html { --foo: 1px; }"),
      ok(":root { --custom-property-set: {} }"),
      ok("html { --custom-property-set: {} }"),
      ok("a[b=#{c}] { }"),
      bad("[target=_blank] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =_blank] { }"),
      bad("[target= _blank] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target = _blank] { }"),
      bad("[target=_blank ] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =_blank ] { }"),
      bad("[target= _blank ] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target = _blank ] { }"),
      bad("[ target=_blank] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target =_blank] { }"),
      bad("[ target= _blank] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target = _blank] { }"),
      bad("[ target=_blank ] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target =_blank ] { }"),
      bad("[ target= _blank ] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target = _blank ] { }"),
      bad("[target='_blank'] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target ='_blank'] { }"),
      bad("[target= '_blank'] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target = '_blank'] { }"),
      bad("[target=' _blank'] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =' _blank'] { }"),
      bad("[target='_blank' ] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target ='_blank' ] { }"),
      bad("[target='_blank '] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target ='_blank '] { }"),
      bad("[target= '_blank' ] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target = '_blank' ] { }"),
      bad("[target=' _blank '] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =' _blank '] { }"),
      bad("[ target='_blank'] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target ='_blank'] { }"),
      bad("[ target= '_blank'] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target = '_blank'] { }"),
      bad("[ target=' _blank'] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target =' _blank'] { }"),
      bad("[ target='_blank' ] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target ='_blank' ] { }"),
      bad("[ target='_blank '] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target ='_blank '] { }"),
      bad("[ target= '_blank' ] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target = '_blank' ] { }"),
      bad("[ target=' _blank '] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target =' _blank '] { }"),
      bad("[target=\"_blank\"] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =\"_blank\"] { }"),
      bad("[target= \"_blank\"] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target = \"_blank\"] { }"),
      bad("[target=\" _blank\"] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =\" _blank\"] { }"),
      bad("[target=\"_blank\" ] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =\"_blank\" ] { }"),
      bad("[target=\"_blank \"] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =\"_blank \"] { }"),
      bad("[target= \"_blank\" ] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target = \"_blank\" ] { }"),
      bad("[target=\" _blank \"] { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =\" _blank \"] { }"),
      bad("[ target=\"_blank\"] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target =\"_blank\"] { }"),
      bad("[ target= \"_blank\"] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target = \"_blank\"] { }"),
      bad("[ target=\" _blank\"] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target =\" _blank\"] { }"),
      bad("[ target=\"_blank\" ] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target =\"_blank\" ] { }"),
      bad("[ target=\"_blank \"] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target =\"_blank \"] { }"),
      bad("[ target= \"_blank\" ] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target = \"_blank\" ] { }"),
      bad("[ target=\" _blank \"] { }")
        .at(1, 9, "Expected single space before \"=\"")
        .fixed("[ target =\" _blank \"] { }"),
      bad("[title~=flower] { }")
        .at(1, 7, "Expected single space before \"~=\"")
        .fixed("[title ~=flower] { }"),
      bad("[title~= flower] { }")
        .at(1, 7, "Expected single space before \"~=\"")
        .fixed("[title ~= flower] { }"),
      bad("[title|=flower] { }")
        .at(1, 7, "Expected single space before \"|=\"")
        .fixed("[title |=flower] { }"),
      bad("[title|= flower] { }")
        .at(1, 7, "Expected single space before \"|=\"")
        .fixed("[title |= flower] { }"),
      bad("[title^=flower] { }")
        .at(1, 7, "Expected single space before \"^=\"")
        .fixed("[title ^=flower] { }"),
      bad("[title^= flower] { }")
        .at(1, 7, "Expected single space before \"^=\"")
        .fixed("[title ^= flower] { }"),
      bad("[title$=flower] { }")
        .at(1, 7, "Expected single space before \"$=\"")
        .fixed("[title $=flower] { }"),
      bad("[title$= flower] { }")
        .at(1, 7, "Expected single space before \"$=\"")
        .fixed("[title $= flower] { }"),
      bad("a[href*=w3schools] { }")
        .at(1, 7, "Expected single space before \"*=\"")
        .fixed("a[href *=w3schools] { }"),
      bad("a[href*= w3schools] { }")
        .at(1, 7, "Expected single space before \"*=\"")
        .fixed("a[href *= w3schools] { }"),
      bad("img[alt~=person][src *=lorem] { }")
        .at(1, 8, "Expected single space before \"~=\"")
        .fixed("img[alt ~=person][src *=lorem] { }"),
      bad("img[alt~= person][src *=lorem] { }")
        .at(1, 8, "Expected single space before \"~=\"")
        .fixed("img[alt ~= person][src *=lorem] { }"),
      bad("img[alt ~=person][src*= lorem] { }")
        .at(1, 22, "Expected single space before \"*=\"")
        .fixed("img[alt ~=person][src *= lorem] { }"),
      bad("img[alt ~= person][src*= lorem] { }")
        .at(1, 23, "Expected single space before \"*=\"")
        .fixed("img[alt ~= person][src *= lorem] { }"),
      bad("[target=_blank]:hover { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =_blank]:hover { }"),
      bad("[target= _blank]:hover { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target = _blank]:hover { }"),
      bad("[target=_blank]::before { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =_blank]::before { }"),
      bad("[target= _blank]::before { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target = _blank]::before { }"),
      bad("option[data-hidden=true] { }")
        .at(1, 19, "Expected single space before \"=\"")
        .fixed("option[data-hidden =true] { }"),
      bad("option[data-hidden= true] { }")
        .at(1, 19, "Expected single space before \"=\"")
        .fixed("option[data-hidden = true] { }"),
      bad("@media screen and (max-width: 480px) { img[align=right] {} }")
        .at(1, 49, "Expected single space before \"=\"")
        .fixed("@media screen and (max-width: 480px) { img[align =right] {} }"),
      bad("@media screen and (max-width: 480px) { img[align= right] {} }")
        .at(1, 49, "Expected single space before \"=\"")
        .fixed("@media screen and (max-width: 480px) { img[align = right] {} }"),
      bad("[target=_blank] .foo { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target =_blank] .foo { }"),
      bad("[target= _blank] .foo { }")
        .at(1, 8, "Expected single space before \"=\"")
        .fixed("[target = _blank] .foo { }"),
      bad("li[aria-hidden=false]:nth-child(1) { }")
        .at(1, 15, "Expected single space before \"=\"")
        .fixed("li[aria-hidden =false]:nth-child(1) { }"),
      bad("li[aria-hidden=false]:nth-child( 1) { }")
        .at(1, 15, "Expected single space before \"=\"")
        .fixed("li[aria-hidden =false]:nth-child( 1) { }"),
      bad("li[aria-hidden=false]:nth-child(1 ) { }")
        .at(1, 15, "Expected single space before \"=\"")
        .fixed("li[aria-hidden =false]:nth-child(1 ) { }"),
      bad("li[aria-hidden=false]:nth-child( 1 ) { }")
        .at(1, 15, "Expected single space before \"=\"")
        .fixed("li[aria-hidden =false]:nth-child( 1 ) { }"),
      bad("li[aria-hidden= false]:nth-child(1) { }")
        .at(1, 15, "Expected single space before \"=\"")
        .fixed("li[aria-hidden = false]:nth-child(1) { }"),
      bad("li[aria-hidden= false]:nth-child( 1) { }")
        .at(1, 15, "Expected single space before \"=\"")
        .fixed("li[aria-hidden = false]:nth-child( 1) { }"),
      bad("li[aria-hidden= false]:nth-child(1 ) { }")
        .at(1, 15, "Expected single space before \"=\"")
        .fixed("li[aria-hidden = false]:nth-child(1 ) { }"),
      bad("li[aria-hidden= false]:nth-child( 1 ) { }")
        .at(1, 15, "Expected single space before \"=\"")
        .fixed("li[aria-hidden = false]:nth-child( 1 ) { }"),
      bad("ul li[aria-hidden=false] + li[aria-hidden =false] a { }")
        .at(1, 18, "Expected single space before \"=\"")
        .fixed("ul li[aria-hidden =false] + li[aria-hidden =false] a { }"),
      bad("ul li[aria-hidden =false] + li[aria-hidden=false] a { }")
        .at(1, 43, "Expected single space before \"=\"")
        .fixed("ul li[aria-hidden =false] + li[aria-hidden =false] a { }"),
      bad("ul li[aria-hidden= false] + li[aria-hidden =false] a { }")
        .at(1, 18, "Expected single space before \"=\"")
        .fixed("ul li[aria-hidden = false] + li[aria-hidden =false] a { }"),
      bad("ul li[aria-hidden = false] + li[aria-hidden=false] a { }")
        .at(1, 44, "Expected single space before \"=\"")
        .fixed("ul li[aria-hidden = false] + li[aria-hidden =false] a { }"),
      bad("ul li[aria-hidden=false] + li[aria-hidden = false] a { }")
        .at(1, 18, "Expected single space before \"=\"")
        .fixed("ul li[aria-hidden =false] + li[aria-hidden = false] a { }"),
      bad("ul li[aria-hidden =false] + li[aria-hidden= false] a { }")
        .at(1, 43, "Expected single space before \"=\"")
        .fixed("ul li[aria-hidden =false] + li[aria-hidden = false] a { }"),
      bad("ul li[aria-hidden= false] + li[aria-hidden = false] a { }")
        .at(1, 18, "Expected single space before \"=\"")
        .fixed("ul li[aria-hidden = false] + li[aria-hidden = false] a { }"),
      bad("ul li[aria-hidden = false] + li[aria-hidden= false] a { }")
        .at(1, 44, "Expected single space before \"=\"")
        .fixed("ul li[aria-hidden = false] + li[aria-hidden = false] a { }"),
      bad("ul li[aria-hidden= false] + li[aria-hidden= false] a { }")
        .at(1, 18, "Expected single space before \"=\"")
        .at(1, 43, "Expected single space before \"=\"")
        .fixed("ul li[aria-hidden = false] + li[aria-hidden = false] a { }"),
      bad("[target/**/=\"_blank\"] { }")
        .at(1, 12, "Expected single space before \"=\"")
        .fixed("[target/**/ =\"_blank\"] { }"),
      bad("[target /**/=\"_blank\"] { }")
        .at(1, 13, "Expected single space before \"=\"")
        .fixed("[target /**/ =\"_blank\"] { }"),
    ],
  );
}

#[test]
fn selector_attribute_operator_space_before_never() {
  check(
    "@stylistic/selector-attribute-operator-space-before",
    json!(["never"]),
    Css,
    &[
      ok(".foo { }"),
      ok("[target] { }"),
      ok("[ target] { }"),
      ok("[target ] { }"),
      ok("[ target ] { }"),
      ok("[target=_blank] { }"),
      ok("[target= _blank] { }"),
      ok("[target=_blank ] { }"),
      ok("[target= _blank ] { }"),
      ok("[ target=_blank] { }"),
      ok("[ target= _blank] { }"),
      ok("[ target=_blank ] { }"),
      ok("[ target= _blank ] { }"),
      ok("[target='_blank'] { }"),
      ok("[target= '_blank'] { }"),
      ok("[target=' _blank'] { }"),
      ok("[target='_blank' ] { }"),
      ok("[target='_blank '] { }"),
      ok("[target= '_blank' ] { }"),
      ok("[target=' _blank '] { }"),
      ok("[ target='_blank'] { }"),
      ok("[ target= '_blank'] { }"),
      ok("[ target=' _blank'] { }"),
      ok("[ target='_blank' ] { }"),
      ok("[ target='_blank '] { }"),
      ok("[ target= '_blank' ] { }"),
      ok("[ target=' _blank '] { }"),
      ok("[target=\"_blank\"] { }"),
      ok("[target= \"_blank\"] { }"),
      ok("[target=\" _blank\"] { }"),
      ok("[target=\"_blank\" ] { }"),
      ok("[target=\"_blank \"] { }"),
      ok("[target= \"_blank\" ] { }"),
      ok("[target=\" _blank \"] { }"),
      ok("[ target=\"_blank\"] { }"),
      ok("[ target= \"_blank\"] { }"),
      ok("[ target=\" _blank\"] { }"),
      ok("[ target=\"_blank\" ] { }"),
      ok("[ target=\"_blank \"] { }"),
      ok("[ target= \"_blank\" ] { }"),
      ok("[ target=\" _blank \"] { }"),
      ok("[title~=flower] { }"),
      ok("[title~= flower] { }"),
      ok("[title|=flower] { }"),
      ok("[title|= flower] { }"),
      ok("[title^=flower] { }"),
      ok("[title^= flower] { }"),
      ok("[title$=flower] { }"),
      ok("[title$= flower] { }"),
      ok("a[href*=w3schools] { }"),
      ok("a[href*= w3schools] { }"),
      ok("img[alt~=person][src*=lorem] { }"),
      ok("img[alt~= person][src*=lorem] { }"),
      ok("img[alt~=person][src*= lorem] { }"),
      ok("img[alt~= person][src*= lorem] { }"),
      ok("[target=_blank]:hover { }"),
      ok("[target= _blank]:hover { }"),
      ok("[target=_blank]::before { }"),
      ok("[target= _blank]::before { }"),
      ok("option[data-hidden=true] { }"),
      ok("option[data-hidden= true] { }"),
      ok("option[dataHidden] { }"),
      ok("@media screen and (max-width: 480px) { img[align=right] {} }"),
      ok("@media screen and (max-width: 480px) { img[align= right] {} }"),
      ok("[target=_blank] .foo { }"),
      ok("[target= _blank] .foo { }"),
      ok("li[aria-hidden=false]:nth-child(1) { }"),
      ok("li[aria-hidden=false]:nth-child( 1) { }"),
      ok("li[aria-hidden=false]:nth-child(1 ) { }"),
      ok("li[aria-hidden=false]:nth-child( 1 ) { }"),
      ok("li[aria-hidden= false]:nth-child(1) { }"),
      ok("li[aria-hidden= false]:nth-child( 1) { }"),
      ok("li[aria-hidden= false]:nth-child(1 ) { }"),
      ok("li[aria-hidden= false]:nth-child( 1 ) { }"),
      ok("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      ok("ul li[aria-hidden= false] + li[aria-hidden=false] a { }"),
      ok("ul li[aria-hidden=false] + li[aria-hidden= false] a { }"),
      ok("ul li[aria-hidden= false] + li[aria-hidden= false] a { }"),
      ok("[target] { content: \" \" }"),
      ok("[target ] { content: \" \" }"),
      ok("[ target] { content: \" \" }"),
      ok("[ target ] { content: \" \" }"),
      bad("[target =_blank] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=_blank] { }"),
      bad("[target = _blank] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target= _blank] { }"),
      bad("[target =_blank ] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=_blank ] { }"),
      bad("[target = _blank ] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target= _blank ] { }"),
      bad("[ target =_blank] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target=_blank] { }"),
      bad("[ target = _blank] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target= _blank] { }"),
      bad("[ target =_blank ] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target=_blank ] { }"),
      bad("[ target = _blank ] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target= _blank ] { }"),
      bad("[target ='_blank'] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target='_blank'] { }"),
      bad("[target = '_blank'] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target= '_blank'] { }"),
      bad("[target =' _blank'] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=' _blank'] { }"),
      bad("[target ='_blank' ] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target='_blank' ] { }"),
      bad("[target ='_blank '] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target='_blank '] { }"),
      bad("[target = '_blank' ] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target= '_blank' ] { }"),
      bad("[target =' _blank '] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=' _blank '] { }"),
      bad("[ target ='_blank'] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target='_blank'] { }"),
      bad("[ target = '_blank'] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target= '_blank'] { }"),
      bad("[ target =' _blank'] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target=' _blank'] { }"),
      bad("[ target ='_blank' ] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target='_blank' ] { }"),
      bad("[ target ='_blank '] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target='_blank '] { }"),
      bad("[ target = '_blank' ] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target= '_blank' ] { }"),
      bad("[ target =' _blank '] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target=' _blank '] { }"),
      bad("[target =\"_blank\"] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=\"_blank\"] { }"),
      bad("[target = \"_blank\"] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target= \"_blank\"] { }"),
      bad("[target =\" _blank\"] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=\" _blank\"] { }"),
      bad("[target =\"_blank\" ] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=\"_blank\" ] { }"),
      bad("[target =\"_blank \"] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=\"_blank \"] { }"),
      bad("[target = \"_blank\" ] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target= \"_blank\" ] { }"),
      bad("[target =\" _blank \"] { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=\" _blank \"] { }"),
      bad("[ target =\"_blank\"] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target=\"_blank\"] { }"),
      bad("[ target = \"_blank\"] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target= \"_blank\"] { }"),
      bad("[ target =\" _blank\"] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target=\" _blank\"] { }"),
      bad("[ target =\"_blank\" ] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target=\"_blank\" ] { }"),
      bad("[ target =\"_blank \"] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target=\"_blank \"] { }"),
      bad("[ target = \"_blank\" ] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target= \"_blank\" ] { }"),
      bad("[ target =\" _blank \"] { }")
        .at(1, 10, "Unexpected whitespace before \"=\"")
        .fixed("[ target=\" _blank \"] { }"),
      bad("[title ~=flower] { }")
        .at(1, 8, "Unexpected whitespace before \"~=\"")
        .fixed("[title~=flower] { }"),
      bad("[title ~= flower] { }")
        .at(1, 8, "Unexpected whitespace before \"~=\"")
        .fixed("[title~= flower] { }"),
      bad("[title |=flower] { }")
        .at(1, 8, "Unexpected whitespace before \"|=\"")
        .fixed("[title|=flower] { }"),
      bad("[title |= flower] { }")
        .at(1, 8, "Unexpected whitespace before \"|=\"")
        .fixed("[title|= flower] { }"),
      bad("[title ^=flower] { }")
        .at(1, 8, "Unexpected whitespace before \"^=\"")
        .fixed("[title^=flower] { }"),
      bad("[title ^= flower] { }")
        .at(1, 8, "Unexpected whitespace before \"^=\"")
        .fixed("[title^= flower] { }"),
      bad("[title $=flower] { }")
        .at(1, 8, "Unexpected whitespace before \"$=\"")
        .fixed("[title$=flower] { }"),
      bad("[title $= flower] { }")
        .at(1, 8, "Unexpected whitespace before \"$=\"")
        .fixed("[title$= flower] { }"),
      bad("a[href *=w3schools] { }")
        .at(1, 8, "Unexpected whitespace before \"*=\"")
        .fixed("a[href*=w3schools] { }"),
      bad("a[href *= w3schools] { }")
        .at(1, 8, "Unexpected whitespace before \"*=\"")
        .fixed("a[href*= w3schools] { }"),
      bad("img[alt ~=person][src*=lorem] { }")
        .at(1, 9, "Unexpected whitespace before \"~=\"")
        .fixed("img[alt~=person][src*=lorem] { }"),
      bad("img[alt ~= person][src*=lorem] { }")
        .at(1, 9, "Unexpected whitespace before \"~=\"")
        .fixed("img[alt~= person][src*=lorem] { }"),
      bad("img[alt~=person][src *= lorem] { }")
        .at(1, 22, "Unexpected whitespace before \"*=\"")
        .fixed("img[alt~=person][src*= lorem] { }"),
      bad("img[alt~= person][src *= lorem] { }")
        .at(1, 23, "Unexpected whitespace before \"*=\"")
        .fixed("img[alt~= person][src*= lorem] { }"),
      bad("[target =_blank]:hover { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=_blank]:hover { }"),
      bad("[target = _blank]:hover { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target= _blank]:hover { }"),
      bad("[target =_blank]::before { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=_blank]::before { }"),
      bad("[target = _blank]::before { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target= _blank]::before { }"),
      bad("option[data-hidden =true] { }")
        .at(1, 20, "Unexpected whitespace before \"=\"")
        .fixed("option[data-hidden=true] { }"),
      bad("option[data-hidden = true] { }")
        .at(1, 20, "Unexpected whitespace before \"=\"")
        .fixed("option[data-hidden= true] { }"),
      bad("@media screen and (max-width: 480px) { img[align =right] {} }")
        .at(1, 50, "Unexpected whitespace before \"=\"")
        .fixed("@media screen and (max-width: 480px) { img[align=right] {} }"),
      bad("@media screen and (max-width: 480px) { img[align = right] {} }")
        .at(1, 50, "Unexpected whitespace before \"=\"")
        .fixed("@media screen and (max-width: 480px) { img[align= right] {} }"),
      bad("[target =_blank] .foo { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target=_blank] .foo { }"),
      bad("[target = _blank] .foo { }")
        .at(1, 9, "Unexpected whitespace before \"=\"")
        .fixed("[target= _blank] .foo { }"),
      bad("li[aria-hidden =false]:nth-child(1) { }")
        .at(1, 16, "Unexpected whitespace before \"=\"")
        .fixed("li[aria-hidden=false]:nth-child(1) { }"),
      bad("li[aria-hidden =false]:nth-child( 1) { }")
        .at(1, 16, "Unexpected whitespace before \"=\"")
        .fixed("li[aria-hidden=false]:nth-child( 1) { }"),
      bad("li[aria-hidden =false]:nth-child(1 ) { }")
        .at(1, 16, "Unexpected whitespace before \"=\"")
        .fixed("li[aria-hidden=false]:nth-child(1 ) { }"),
      bad("li[aria-hidden =false]:nth-child( 1 ) { }")
        .at(1, 16, "Unexpected whitespace before \"=\"")
        .fixed("li[aria-hidden=false]:nth-child( 1 ) { }"),
      bad("li[aria-hidden = false]:nth-child(1) { }")
        .at(1, 16, "Unexpected whitespace before \"=\"")
        .fixed("li[aria-hidden= false]:nth-child(1) { }"),
      bad("li[aria-hidden = false]:nth-child( 1) { }")
        .at(1, 16, "Unexpected whitespace before \"=\"")
        .fixed("li[aria-hidden= false]:nth-child( 1) { }"),
      bad("li[aria-hidden = false]:nth-child(1 ) { }")
        .at(1, 16, "Unexpected whitespace before \"=\"")
        .fixed("li[aria-hidden= false]:nth-child(1 ) { }"),
      bad("li[aria-hidden = false]:nth-child( 1 ) { }")
        .at(1, 16, "Unexpected whitespace before \"=\"")
        .fixed("li[aria-hidden= false]:nth-child( 1 ) { }"),
      bad("ul li[aria-hidden =false] + li[aria-hidden=false] a { }")
        .at(1, 19, "Unexpected whitespace before \"=\"")
        .fixed("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden=false] + li[aria-hidden =false] a { }")
        .at(1, 43, "Unexpected whitespace before \"=\"")
        .fixed("ul li[aria-hidden=false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden = false] + li[aria-hidden=false] a { }")
        .at(1, 19, "Unexpected whitespace before \"=\"")
        .fixed("ul li[aria-hidden= false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden= false] + li[aria-hidden =false] a { }")
        .at(1, 44, "Unexpected whitespace before \"=\"")
        .fixed("ul li[aria-hidden= false] + li[aria-hidden=false] a { }"),
      bad("ul li[aria-hidden =false] + li[aria-hidden= false] a { }")
        .at(1, 19, "Unexpected whitespace before \"=\"")
        .fixed("ul li[aria-hidden=false] + li[aria-hidden= false] a { }"),
      bad("ul li[aria-hidden=false] + li[aria-hidden = false] a { }")
        .at(1, 43, "Unexpected whitespace before \"=\"")
        .fixed("ul li[aria-hidden=false] + li[aria-hidden= false] a { }"),
      bad("ul li[aria-hidden = false] + li[aria-hidden= false] a { }")
        .at(1, 19, "Unexpected whitespace before \"=\"")
        .fixed("ul li[aria-hidden= false] + li[aria-hidden= false] a { }"),
      bad("ul li[aria-hidden= false] + li[aria-hidden = false] a { }")
        .at(1, 44, "Unexpected whitespace before \"=\"")
        .fixed("ul li[aria-hidden= false] + li[aria-hidden= false] a { }"),
      bad("ul li[aria-hidden = false] + li[aria-hidden = false] a { }")
        .at(1, 19, "Unexpected whitespace before \"=\"")
        .at(1, 45, "Unexpected whitespace before \"=\"")
        .fixed("ul li[aria-hidden= false] + li[aria-hidden= false] a { }"),
      bad("[target/**/ =\"_blank\"] { }")
        .at(1, 13, "Unexpected whitespace before \"=\"")
        .fixed("[target/**/=\"_blank\"] { }"),
      bad("[target /**/ =\"_blank\"] { }")
        .at(1, 14, "Unexpected whitespace before \"=\"")
        .fixed("[target /**/=\"_blank\"] { }"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_attribute_operator_space_before_never_gale_cases() {
  check(
    "@stylistic/selector-attribute-operator-space-before",
    json!("never"),
    Css,
    &[
      bad("[attr =value] {}")
        .at(1, 7, "Unexpected whitespace before \"=\"")
        .fixed("[attr=value] {}"),
      ok("[attr=value] {}"),
      ok("[disabled] {}"),
    ],
  );
}

// Gale's own cases, from the rule's earlier tests; Stylelint agrees.
#[test]
fn selector_attribute_operator_space_before_always_gale_cases() {
  check(
    "@stylistic/selector-attribute-operator-space-before",
    json!("always"),
    Css,
    &[bad("a,\n  b[x  ^=\"y\"] {}")
      .at(2, 8, "Expected single space before \"^=\"")
      .fixed("a,\n  b[x ^=\"y\"] {}")],
  );
}
