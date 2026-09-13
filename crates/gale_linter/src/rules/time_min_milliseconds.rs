use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Specify the minimum number of milliseconds for time values.
///
/// Equivalent to Stylelint's `time-min-milliseconds` rule.
/// Default minimum: 100ms.
///
/// Secondary options:
/// - `ignore: ["delay"]`: ignore delay-related properties and delay
///   positions in shorthand properties.
pub struct TimeMinMilliseconds;

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

/// Given a declaration, find the byte offset in the source where the value
/// begins (after the `:` and any whitespace).
fn find_value_offset(source: &str, decl_offset: usize, property_len: usize) -> usize {
  let start = decl_offset + property_len;
  if start >= source.len() {
    return decl_offset;
  }
  let rest = &source[start..];
  let mut off = 0;
  let bytes = rest.as_bytes();
  while off < bytes.len() && (bytes[off] == b':' || bytes[off].is_ascii_whitespace()) {
    off += 1;
  }
  start + off
}

impl Rule for TimeMinMilliseconds {
  fn name(&self) -> &'static str {
    "time-min-milliseconds"
  }

  fn description(&self) -> &'static str {
    "Specify the minimum number of milliseconds for time values"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  fn check(&self, node: &CssNode, ctx: &RuleContext) -> Vec<Diagnostic> {
    let CssNode::Style(rule) = node else {
      return vec![];
    };

    let min = ctx
      .primary_option()
      .and_then(|v| v.as_f64())
      .unwrap_or(DEFAULT_MIN);

    let ignore_delay = ctx
      .secondary_options()
      .and_then(|v| v.get("ignore"))
      .and_then(|v| v.as_array())
      .map(|arr| arr.iter().any(|v| v.as_str() == Some("delay")))
      .unwrap_or(false);

    let mut diags = Vec::new();
    for decl in &rule.declarations {
      if !is_time_property(&decl.property) {
        continue;
      }

      let prop_is_delay = is_delay_property(&decl.property);

      // If ignoring delay and this is a delay property, skip entirely.
      if ignore_delay && prop_is_delay {
        continue;
      }

      let is_shorthand = is_shorthand_property(&decl.property);

      // Use source text when available for accurate positions.
      let (value_text, value_offset) = if !ctx.source.is_empty() {
        let vo = find_value_offset(ctx.source, decl.span.offset, decl.property.len());
        let rest = &ctx.source[vo..];
        let end = rest.find([';', '}']).unwrap_or(rest.len());
        (ctx.source[vo..vo + end].trim_end().to_string(), vo)
      } else {
        let vo = decl.span.offset + decl.property.len() + 2;
        (decl.value.clone(), vo)
      };

      let time_values = extract_time_values(&value_text, is_shorthand);

      for tv in &time_values {
        // Skip if ignoring delay and this is in a delay position.
        if ignore_delay && tv.is_delay {
          continue;
        }

        let abs_ms = tv.ms.abs();

        // Skip 0 — zero duration is intentional and should not be flagged.
        if abs_ms == 0.0 {
          continue;
        }

        // Skip negative values — they are valid for delays and intentional.
        if tv.ms < 0.0 {
          continue;
        }

        if abs_ms < min {
          let abs_offset = value_offset + tv.offset;
          diags.push(
            Diagnostic::new(
              self.name(),
              format!("Expected a minimum of {min}ms, found {abs_ms}ms"),
            )
            .severity(self.default_severity())
            .span(Span::new(abs_offset, tv.length)),
          );
        }
      }
    }
    diags
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan, StyleRule, Syntax};

  fn ctx() -> RuleContext<'static> {
    RuleContext {
      file_path: "t.css",
      source: "",
      syntax: Syntax::Css,
      options: None,
    }
  }

  fn style_with_decl(prop: &str, val: &str) -> CssNode {
    CssNode::Style(StyleRule {
      selector: "a".to_string(),
      declarations: vec![Declaration {
        property: prop.to_string(),
        value: val.to_string(),
        span: ParserSpan::new(0, prop.len() + val.len() + 2),
        important: false,
      }],
      span: ParserSpan::new(0, 0),
      ..Default::default()
    })
  }

  #[test]
  fn reports_time_below_minimum() {
    let d = TimeMinMilliseconds.check(&style_with_decl("transition-duration", "50ms"), &ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("50ms"));
  }

  #[test]
  fn allows_time_above_minimum() {
    let d = TimeMinMilliseconds.check(&style_with_decl("transition-duration", "200ms"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn converts_seconds_to_ms() {
    // 0.01s = 10ms, should flag with default minimum 100ms
    let d = TimeMinMilliseconds.check(&style_with_decl("transition-duration", "0.01s"), &ctx());
    assert_eq!(d.len(), 1);
  }

  #[test]
  fn allows_zero() {
    let d = TimeMinMilliseconds.check(&style_with_decl("animation-delay", "0ms"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_negative_values() {
    let d = TimeMinMilliseconds.check(&style_with_decl("animation-delay", "-20ms"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn rule_name_is_correct() {
    assert_eq!(TimeMinMilliseconds.name(), "time-min-milliseconds");
  }

  #[test]
  fn does_not_flag_non_time_properties() {
    let d = TimeMinMilliseconds.check(&style_with_decl("color", "red"), &ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn handles_transition_shorthand() {
    let d = TimeMinMilliseconds.check(&style_with_decl("transition", "foo 0.008s linear"), &ctx());
    assert_eq!(d.len(), 1);
  }
}
