//! JavaScript number conversions, reproduced exactly.
//!
//! Stylelint's autofixes build replacement values with `Number(...)`,
//! `parseFloat(...)`, `toPrecision(...)` and string interpolation, so a fix
//! only matches byte for byte when Gale formats numbers the way V8 does:
//! shortest round-trip digits, exponent notation outside `[1e-6, 1e21)`, and
//! `toPrecision` rounding half away from zero on the exact binary value.

/// `String(x)` for a JavaScript number: the shortest digits that round-trip,
/// in fixed notation for magnitudes in `[1e-6, 1e21)` and exponent notation
/// (`1e+21`, `1.5e-7`) outside it.
pub fn to_js_string(x: f64) -> String {
  if x.is_nan() {
    return "NaN".to_string();
  }
  if x.is_infinite() {
    return if x > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
  }
  if x == 0.0 {
    return "0".to_string();
  }
  let sign = if x < 0.0 { "-" } else { "" };
  // Rust's `{:e}` prints the shortest round-trip digits, as JavaScript does.
  let (digits, exponent) = split_exponent(&format!("{:e}", x.abs()));
  format!("{sign}{}", layout_shortest(&digits, exponent))
}

/// Lays out shortest digits `d1d2...dk × 10^(n-k)` (where `n = exponent + 1`)
/// following ECMAScript's Number::toString.
fn layout_shortest(digits: &str, exponent: i32) -> String {
  let k = digits.len() as i32;
  let n = exponent + 1;
  if k <= n && n <= 21 {
    format!("{digits}{}", "0".repeat((n - k) as usize))
  } else if 0 < n && n <= 21 {
    let (int, frac) = digits.split_at(n as usize);
    format!("{int}.{frac}")
  } else if -6 < n && n <= 0 {
    format!("0.{}{digits}", "0".repeat((-n) as usize))
  } else {
    let e = n - 1;
    let sign = if e < 0 { '-' } else { '+' };
    let (first, rest) = digits.split_at(1);
    if rest.is_empty() {
      format!("{first}e{sign}{}", e.abs())
    } else {
      format!("{first}.{rest}e{sign}{}", e.abs())
    }
  }
}

/// Splits Rust's `{:e}` output (`1.25e-3`) into its digits without the point
/// (`125`) and its decimal exponent (`-3`).
fn split_exponent(formatted: &str) -> (String, i32) {
  let (mantissa, exponent) = formatted.split_once('e').unwrap_or((formatted, "0"));
  let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
  (digits, exponent.parse().unwrap_or(0))
}

/// `x.toPrecision(precision)` for `precision` in `1..=100`.
///
/// Rounds half away from zero on the exact decimal expansion of `x`, and uses
/// exponent notation when the exponent is below -6 or at least `precision`.
pub fn to_precision(x: f64, precision: usize) -> String {
  if !x.is_finite() {
    return to_js_string(x);
  }
  let precision = precision.clamp(1, 100);
  if x == 0.0 {
    return if precision == 1 {
      "0".to_string()
    } else {
      format!("0.{}", "0".repeat(precision - 1))
    };
  }
  let sign = if x < 0.0 { "-" } else { "" };
  // Every finite double has an exact decimal expansion of at most 767
  // significant digits, so this prints it without any rounding.
  let (all_digits, mut exponent) = split_exponent(&format!("{:.800e}", x.abs()));
  let mut digits: Vec<u8> = all_digits.as_bytes()[..precision].to_vec();
  if all_digits.as_bytes()[precision] >= b'5' {
    // Round up, carrying through trailing nines.
    let mut i = precision;
    loop {
      if i == 0 {
        digits.insert(0, b'1');
        digits.pop();
        exponent += 1;
        break;
      }
      i -= 1;
      if digits[i] == b'9' {
        digits[i] = b'0';
      } else {
        digits[i] += 1;
        break;
      }
    }
  }
  let digits = String::from_utf8(digits).unwrap_or_default();
  let p = precision as i32;
  let body = if exponent < -6 || exponent >= p {
    let (first, rest) = digits.split_at(1);
    let sign = if exponent < 0 { '-' } else { '+' };
    if rest.is_empty() {
      format!("{first}e{sign}{}", exponent.abs())
    } else {
      format!("{first}.{rest}e{sign}{}", exponent.abs())
    }
  } else if exponent >= 0 {
    let (int, frac) = digits.split_at(exponent as usize + 1);
    if frac.is_empty() {
      int.to_string()
    } else {
      format!("{int}.{frac}")
    }
  } else {
    format!("0.{}{digits}", "0".repeat((-exponent - 1) as usize))
  };
  format!("{sign}{body}")
}

/// `Number(text)` for a JavaScript string: surrounding whitespace is ignored,
/// an empty string is 0, `0x`/`0o`/`0b` integers are read in their base, and
/// anything else that is not a whole decimal literal (or `Infinity`) is NaN.
pub fn parse_number(text: &str) -> f64 {
  let trimmed = text.trim();
  if trimmed.is_empty() {
    return 0.0;
  }
  for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
    let lower = trimmed.get(..2).map(str::to_ascii_lowercase);
    if lower.as_deref() == Some(prefix) {
      let digits = &trimmed[2..];
      if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return f64::NAN;
      }
      return digits.chars().fold(0.0, |n, c| {
        n * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0))
      });
    }
  }
  let unsigned = trimmed.trim_start_matches(['+', '-']);
  if unsigned == "Infinity" && trimmed.len() - unsigned.len() <= 1 {
    return if trimmed.starts_with('-') {
      f64::NEG_INFINITY
    } else {
      f64::INFINITY
    };
  }
  if decimal_literal_len(trimmed) != trimmed.len() {
    return f64::NAN;
  }
  trimmed.parse().unwrap_or(f64::NAN)
}

/// `parseFloat(text)`: the longest decimal literal at the start of `text`
/// (after leading whitespace), or NaN when there is none.
pub fn parse_float(text: &str) -> f64 {
  let trimmed = text.trim_start();
  let unsigned = trimmed.trim_start_matches(['+', '-']);
  if unsigned.starts_with("Infinity") && trimmed.len() - unsigned.len() <= 1 {
    return if trimmed.starts_with('-') {
      f64::NEG_INFINITY
    } else {
      f64::INFINITY
    };
  }
  let len = decimal_literal_len(trimmed);
  if len == 0 {
    return f64::NAN;
  }
  trimmed[..len].parse().unwrap_or(f64::NAN)
}

/// Byte length of the longest `StrDecimalLiteral` prefix of `text`:
/// an optional sign, digits with an optional fraction (at least one digit
/// overall), and an optional exponent.  Returns 0 when there is none.
fn decimal_literal_len(text: &str) -> usize {
  let bytes = text.as_bytes();
  let mut i = 0;
  if matches!(bytes.first(), Some(b'+' | b'-')) {
    i += 1;
  }
  let int_start = i;
  while bytes.get(i).is_some_and(u8::is_ascii_digit) {
    i += 1;
  }
  let mut digits = i - int_start;
  if bytes.get(i) == Some(&b'.') {
    let frac_start = i + 1;
    let mut j = frac_start;
    while bytes.get(j).is_some_and(u8::is_ascii_digit) {
      j += 1;
    }
    if digits > 0 || j > frac_start {
      digits += j - frac_start;
      i = j;
    }
  }
  if digits == 0 {
    return 0;
  }
  if matches!(bytes.get(i), Some(b'e' | b'E')) {
    let mut j = i + 1;
    if matches!(bytes.get(j), Some(b'+' | b'-')) {
      j += 1;
    }
    let exp_start = j;
    while bytes.get(j).is_some_and(u8::is_ascii_digit) {
      j += 1;
    }
    if j > exp_start {
      i = j;
    }
  }
  i
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn to_js_string_matches_v8() {
    assert_eq!(to_js_string(0.5), "0.5");
    assert_eq!(to_js_string(50.0), "50");
    assert_eq!(to_js_string(-0.0), "0");
    assert_eq!(to_js_string(0.1 + 0.2), "0.30000000000000004");
    assert_eq!(to_js_string(1e21), "1e+21");
    assert_eq!(to_js_string(123e19), "1.23e+21");
    assert_eq!(to_js_string(1e-7), "1e-7");
    assert_eq!(to_js_string(1.5e-7), "1.5e-7");
    assert_eq!(to_js_string(0.000001), "0.000001");
    assert_eq!(to_js_string(-12.25), "-12.25");
  }

  #[test]
  fn to_precision_matches_v8() {
    assert_eq!(to_precision(50.0, 3), "50.0");
    assert_eq!(to_precision(0.14 * 100.0, 3), "14.0");
    assert_eq!(to_precision(0.003, 3), "0.00300");
    assert_eq!(to_precision(0.125, 2), "0.13");
    assert_eq!(to_precision(2.5, 1), "3");
    assert_eq!(to_precision(999.5, 3), "1.00e+3");
    assert_eq!(to_precision(0.0, 3), "0.00");
    assert_eq!(to_precision(1e-7, 2), "1.0e-7");
    assert_eq!(to_precision(123.456, 3), "123");
    assert_eq!(to_precision(-0.5, 1), "-0.5");
  }

  #[test]
  fn parse_number_and_parse_float() {
    assert_eq!(parse_number(" 12 "), 12.0);
    assert_eq!(parse_number(""), 0.0);
    assert_eq!(parse_number(".5"), 0.5);
    assert_eq!(parse_number("+1e2"), 100.0);
    assert!(parse_number("12px").is_nan());
    assert_eq!(parse_number("0x1A"), 26.0);
    assert_eq!(parse_number("0B11"), 3.0);
    assert!(parse_number("0x").is_nan());
    assert!(parse_number("-0x1").is_nan());
    assert_eq!(parse_float("50%"), 50.0);
    assert_eq!(parse_float("  .5e1x"), 5.0);
    assert_eq!(parse_float("1e"), 1.0);
    assert!(parse_float("abc").is_nan());
  }
}
