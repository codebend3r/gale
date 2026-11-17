use super::*;

#[test]
fn strips_vendor_prefix_from_properties() {
  assert_eq!(
    strip_vendor_prefix("-webkit-tap-highlight-color"),
    Some("tap-highlight-color")
  );
  assert_eq!(strip_vendor_prefix("-moz-appearance"), Some("appearance"));
  assert_eq!(strip_vendor_prefix("color"), None);
}

#[test]
fn strips_vendor_prefix_from_pseudos() {
  assert_eq!(
    strip_vendor_prefix("-webkit-inner-spin-button"),
    Some("inner-spin-button")
  );
  assert_eq!(strip_vendor_prefix("hover"), None);
}

#[test]
fn test_parse_version_f64() {
  assert!((parse_version_f64("138") - 138.0).abs() < f64::EPSILON);
  assert!((parse_version_f64("17.0") - 17.0).abs() < f64::EPSILON);
  assert!((parse_version_f64("\u{2264}79") - 79.0).abs() < f64::EPSILON);
}

#[test]
fn options_are_validated_like_the_plugin() {
  use crate::testing::invalid_options;
  let valid = serde_json::json!([true, {
    "browserslist": "> 1%",
    "allow": { "features": ["properties.gap"], "flagged": false, "prefix": true }
  }]);
  assert!(invalid_options(NAME, valid).is_empty());
  assert!(invalid_options(NAME, serde_json::json!(true)).is_empty());
  assert_eq!(
    invalid_options(NAME, serde_json::json!([true, { "browserslist": 5 }])),
    ["Invalid option \"{\"browserslist\":5}\" for rule \"plugin/browser-compat\""]
  );
  assert_eq!(
    invalid_options(
      NAME,
      serde_json::json!([true, { "allow": { "prefix": "yes" } }])
    ),
    ["Invalid option \"{\"allow\":{\"prefix\":\"yes\"}}\" for rule \"plugin/browser-compat\""]
  );
}
