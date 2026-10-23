use gale_css_parser::postcss_tree::PostcssTree;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "time-min-milliseconds";

declare_rule! {
  /// Specify the minimum number of milliseconds for time values.
  ///
  /// Equivalent to Stylelint's `time-min-milliseconds` rule.
  /// Default minimum: 100ms.
  ///
  /// Secondary options:
  /// - `ignore: ["delay"]`: ignore delay-related properties and delay
  ///   positions in shorthand properties.
  pub struct TimeMinMilliseconds => TimeMinMillisecondsCheck {
    name: NAME,
    description: "Specify the minimum number of milliseconds for time values",
    default_severity: Warning,
  }
}

/// `time-min-milliseconds` as configured.
pub struct TimeMinMillisecondsCheck {
  /// The shortest time allowed, in milliseconds.
  min: f64,
  /// `ignore: ["delay"]`: delays are not checked.
  ignore_delay: bool,
}

impl TimeMinMillisecondsCheck {
  /// Stylelint's options: a number (`possible: isNumber`), then
  /// `ignore: ["delay"]`.  Gale reads no number as 100.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    let min = options.primary_test(serde_json::Value::is_number);
    let ignore = options.keywords("ignore", &["delay"]);
    options.finish()?;
    Ok(Self {
      min: min
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(DEFAULT_MIN),
      ignore_delay: ignore.contains(&"delay"),
    })
  }
}

const DEFAULT_MIN: f64 = 100.0;

/// Properties that are delay-specific.
fn is_delay_property(prop: &str) -> bool {
  let lower = prop.to_ascii_lowercase();
  lower.ends_with("-delay")
}

/// Check if a property is animation/transition related.
fn is_time_property(prop: &str) -> bool {
  let lower = prop.to_ascii_lowercase();
  // Strip vendor prefix
  let stripped = if lower.starts_with("-webkit-") {
    &lower[8..]
  } else if lower.starts_with("-moz-") {
    &lower[5..]
  } else if lower.starts_with("-ms-") {
    &lower[4..]
  } else if lower.starts_with("-o-") {
    &lower[3..]
  } else {
    &lower
  };

  matches!(
    stripped,
    "transition"
      | "transition-duration"
      | "transition-delay"
      | "animation"
      | "animation-duration"
      | "animation-delay"
  )
}

/// Check if a property is a shorthand (transition or animation).
fn is_shorthand_property(prop: &str) -> bool {
  let lower = prop.to_ascii_lowercase();
  let stripped = if lower.starts_with("-webkit-") {
    &lower[8..]
  } else if lower.starts_with("-moz-") {
    &lower[5..]
  } else if lower.starts_with("-ms-") {
    &lower[4..]
  } else if lower.starts_with("-o-") {
    &lower[3..]
  } else {
    &lower
  };
  matches!(stripped, "transition" | "animation")
}

/// A time value found in a CSS property.
struct TimeValue {
  ms: f64,
  /// Byte offset within the declaration value string.
  offset: usize,
  /// Length of the time token in bytes.
  length: usize,
  /// Whether this is in a delay position (for shorthand properties).
  is_delay: bool,
}

/// Extract time values from a declaration value and convert to milliseconds.
/// Also tracks their positions within the value string.
fn extract_time_values(value: &str, is_shorthand: bool) -> Vec<TimeValue> {
  let mut times = Vec::new();
  let lower = value.to_ascii_lowercase();
  let bytes = lower.as_bytes();
  let len = bytes.len();

  if is_shorthand {
    // For shorthand properties (transition/animation), parse each
    // comma-separated segment. In each segment, the first time value
    // is duration and the second is delay.
    let segments = split_comma_segments(value);
    for (seg_text, seg_offset) in &segments {
      let seg_times = extract_times_from_segment(seg_text);
      for (idx, time_val) in seg_times.iter().enumerate() {
        times.push(TimeValue {
          ms: time_val.ms,
          offset: seg_offset + time_val.offset,
          length: time_val.length,
          is_delay: idx >= 1, // second time is delay
        });
      }
    }
  } else {
    // For longhand properties, just find all time values.
    let mut i = 0;
    while i < len {
      // Handle negative sign
      let neg = i < len && bytes[i] == b'-';
      let num_start = if neg { i + 1 } else { i };

      if num_start < len
        && (bytes[num_start].is_ascii_digit()
          || (bytes[num_start] == b'.'
            && num_start + 1 < len
            && bytes[num_start + 1].is_ascii_digit()))
      {
        let token_start = i;
        let mut j = num_start;
        // Consume digits and dot
        while j < len && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
          j += 1;
        }
        let num_str = &lower[num_start..j];
        if let Ok(num) = num_str.parse::<f64>() {
          let num = if neg { -num } else { num };
          // Check for unit
          if j + 2 <= len && bytes[j] == b'm' && bytes[j + 1] == b's' {
            let after_unit = j + 2;
            if after_unit >= len || !bytes[after_unit].is_ascii_alphabetic() {
              times.push(TimeValue {
                ms: num,
                offset: token_start,
                length: j + 2 - token_start,
                is_delay: false,
              });
              i = j + 2;
              continue;
            }
          } else if j < len && bytes[j] == b's' {
            let after_unit = j + 1;
            if after_unit >= len || !bytes[after_unit].is_ascii_alphabetic() {
              times.push(TimeValue {
                ms: num * 1000.0,
                offset: token_start,
                length: j + 1 - token_start,
                is_delay: false,
              });
              i = j + 1;
              continue;
            }
          }
        }
        i = j;
      } else {
        i += 1;
      }
    }
  }

  times
}

/// Simple time extraction from a single shorthand segment.
struct SegmentTime {
  ms: f64,
  offset: usize,
  length: usize,
}

/// Pulls the time values out of one comma-separated segment, keeping each
/// value's position within the segment.
fn extract_times_from_segment(value: &str) -> Vec<SegmentTime> {
  let mut times = Vec::new();
  let lower = value.to_ascii_lowercase();
  let bytes = lower.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Handle negative sign
    let neg = i < len && bytes[i] == b'-';
    let num_start = if neg { i + 1 } else { i };

    if num_start < len
      && (bytes[num_start].is_ascii_digit()
        || (bytes[num_start] == b'.'
          && num_start + 1 < len
          && bytes[num_start + 1].is_ascii_digit()))
    {
      let token_start = i;
      let mut j = num_start;
      while j < len && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
        j += 1;
      }
      let num_str = &lower[num_start..j];
      if let Ok(num) = num_str.parse::<f64>() {
        let num = if neg { -num } else { num };
        if j + 2 <= len && bytes[j] == b'm' && bytes[j + 1] == b's' {
          let after_unit = j + 2;
          if after_unit >= len || !bytes[after_unit].is_ascii_alphabetic() {
            times.push(SegmentTime {
              ms: num,
              offset: token_start,
              length: j + 2 - token_start,
            });
            i = j + 2;
            continue;
          }
        } else if j < len && bytes[j] == b's' {
          let after_unit = j + 1;
          if after_unit >= len || !bytes[after_unit].is_ascii_alphabetic() {
            times.push(SegmentTime {
              ms: num * 1000.0,
              offset: token_start,
              length: j + 1 - token_start,
            });
            i = j + 1;
            continue;
          }
        }
      }
      i = j;
    } else {
      i += 1;
    }
  }

  times
}

/// Split a CSS value into comma-separated segments, tracking offsets.
fn split_comma_segments(value: &str) -> Vec<(String, usize)> {
  let mut segments = Vec::new();
  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut start = 0;
  let mut paren_depth = 0;
  let mut i = 0;

  while i < len {
    match bytes[i] {
      b'(' => paren_depth += 1,
      b')' => {
        if paren_depth > 0 {
          paren_depth -= 1;
        }
      }
      b',' if paren_depth == 0 => {
        segments.push((value[start..i].to_string(), start));
        start = i + 1;
        // Skip whitespace after comma
        while start < len && bytes[start].is_ascii_whitespace() {
          start += 1;
        }
        i = start;
        continue;
      }
      _ => {}
    }
    i += 1;
  }

  if start < len {
    segments.push((value[start..].to_string(), start));
  }

  segments
}

impl Check for TimeMinMillisecondsCheck {
  /// Flags time values below the minimum in style-rule declarations, read as
  /// written. With `ignore: ["delay"]`, delay properties and the delay slot
  /// of shorthands are skipped.
  fn check(&self, tree: &PostcssTree, _ctx: &RuleContext) -> Vec<Diagnostic> {
    let min = self.min;
    let ignore_delay = self.ignore_delay;
    let mut diags = Vec::new();
    for decl in tree.walk_decls() {
      if !decl.parent_is_rule() {
        continue;
      }
      let prop = decl.prop();
      if !is_time_property(prop) {
        continue;
      }
      // If ignoring delay and this is a delay property, skip entirely.
      if ignore_delay && is_delay_property(prop) {
        continue;
      }
      let value = decl.raw_value();
      for tv in extract_time_values(&value, is_shorthand_property(prop)) {
        // Skip if ignoring delay and this is in a delay position.
        if ignore_delay && tv.is_delay {
          continue;
        }
        // Skip 0 — zero duration is intentional and should not be flagged —
        // and negative values, which are valid for delays and intentional.
        if tv.ms <= 0.0 || tv.ms >= min {
          continue;
        }
        diags.push(
          Diagnostic::new(
            NAME,
            format!("Expected a minimum of {min}ms, found {}ms", tv.ms),
          )
          .span(Span::new(decl.value_start() + tv.offset, tv.length)),
        );
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;
  use serde_json::json;

  use crate::testing::{invalid_options, lint};

  /// The reports for `a { <prop>: <value>; }` in CSS, with the default
  /// minimum.
  fn decl(prop: &str, value: &str) -> Vec<Diagnostic> {
    lint(
      NAME,
      json!(100),
      &format!("a {{ {prop}: {value}; }}"),
      Syntax::Css,
    )
  }

  #[test]
  fn reports_time_below_minimum_at_the_time() {
    let d = decl("transition-duration", "50ms");
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("50ms"));
    assert_eq!((d[0].span.offset, d[0].span.length), (25, 4));
  }

  #[test]
  fn allows_time_above_minimum() {
    assert!(decl("transition-duration", "200ms").is_empty());
  }

  #[test]
  fn converts_seconds_to_ms() {
    // 0.01s = 10ms, should flag with default minimum 100ms
    assert_eq!(decl("transition-duration", "0.01s").len(), 1);
  }

  #[test]
  fn allows_zero_and_negative_values() {
    assert!(decl("animation-delay", "0ms").is_empty());
    assert!(decl("animation-delay", "-20ms").is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(NAME, "time-min-milliseconds");
  }

  #[test]
  fn does_not_flag_non_time_properties() {
    assert!(decl("color", "red").is_empty());
  }

  #[test]
  fn handles_transition_shorthand() {
    assert_eq!(decl("transition", "foo 0.008s linear").len(), 1);
  }

  #[test]
  fn ignore_delay_skips_the_delay_slot() {
    let css = "a { transition: foo 0.5s linear 5ms; animation-delay: 5ms; }";
    assert_eq!(lint(NAME, json!(100), css, Syntax::Css).len(), 2);
    let options = json!([100, { "ignore": ["delay"] }]);
    assert!(lint(NAME, options, css, Syntax::Css).is_empty());
  }

  #[test]
  fn options_are_validated_like_stylelint() {
    assert!(invalid_options(NAME, serde_json::json!(200)).is_empty());
    assert!(invalid_options(NAME, serde_json::json!([200, {"ignore": ["delay"]}])).is_empty());
    assert_eq!(
      invalid_options(NAME, serde_json::json!("200")),
      ["Invalid option \"200\" for rule \"time-min-milliseconds\""]
    );
    assert_eq!(
      invalid_options(NAME, serde_json::json!([200, {"ignore": ["duration"]}])),
      ["Invalid value \"duration\" for option \"ignore\" of rule \"time-min-milliseconds\""]
    );
  }
}
