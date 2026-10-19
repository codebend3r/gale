//! A port of stylelint-scss's `sassValueParser`, which finds the Sass
//! operators in a value.
//!
//! Whether a `+`, `-`, `*`, `/` or `%` is an operator Sass evaluates, a
//! sign (`-1px`), part of an identifier (`sans-serif`) or a plain CSS
//! slash (`font: 12px/1.5 serif`) takes the heuristics upstream uses, and
//! the rules built on them (`scss/operator-no-newline-before` and kin) only
//! report what Stylelint reports if they agree case by case.
//! [`find_operators`] follows `findOperators` and `mathOperatorCharType`
//! step by step, quirks included, with byte offsets in place of UTF-16
//! ones.  JavaScript's `\w` and `\d` are spelled as the ASCII classes they
//! are.

use std::sync::LazyLock;

use regex::Regex;

/// One operator found in a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operator {
  /// Byte offset of the operator's first character.
  pub start: usize,
  /// Byte offset just past the operator: one character, or two for `==`,
  /// `!=`, `<=` and `>=`.
  pub end: usize,
}

/// The operators Sass would evaluate in `string`: math operators, `<` and
/// `>`, and the two-character comparisons.  Nothing inside a quoted string
/// counts, except within `#{...}` interpolation.
///
/// `is_after_colon` is upstream's `isAfterColon`: the string is a variable's
/// value or a parameter default, where `+` and `/` between numbers are
/// operators rather than a sign or a CSS slash.
pub fn find_operators(string: &str, is_after_colon: bool) -> Vec<Operator> {
  let bytes = string.as_bytes();
  let mut modes = vec![Mode::Normal];
  // Upstream keeps the index of the current mode beside the stack and moves
  // it on every push and pop, even a pop of an empty stack (a stray `}`), so
  // it can point past the stack; the mode there is `undefined`.
  let mut last: isize = 0;
  let mut result = Vec::new();

  for (i, &b) in bytes.iter().enumerate() {
    let last_mode = usize::try_from(last)
      .ok()
      .and_then(|index| modes.get(index))
      .copied();

    if b == b'"' || b == b'\'' {
      if last_mode.is_some_and(Mode::calculation_enabled) {
        modes.push(Mode::String(b));
        last += 1;
      } else if last_mode == Some(Mode::String(b)) && (i == 0 || bytes[i - 1] != b'\\') {
        modes.pop();
        last -= 1;
      }
    }

    // `#{` within the last two characters is not worth entering.
    if b == b'#' && i + 3 < bytes.len() && bytes[i + 1] == b'{' {
      modes.push(Mode::Interpolation);
      last += 1;
    } else if b == b'}' {
      modes.pop();
      last -= 1;
    }

    if last_mode.is_some_and(|mode| !mode.calculation_enabled()) {
      continue;
    }

    let math = matches!(b, b'+' | b'/' | b'-' | b'*' | b'%')
      && char_type(string, i, is_after_colon) == CharType::Op;
    // `<` and `>` are operators unless they open a comparison or sit in the
    // `<color>` of a CSS `type()`.
    let angle = matches!(b, b'<' | b'>')
      && bytes.get(i + 1) != Some(&b'=')
      && !is_inside_type_function_call(string, i);
    if math || angle {
      result.push(Operator {
        start: i,
        end: i + 1,
      });
    }
    if matches!(b, b'>' | b'<' | b'=' | b'!') && bytes.get(i + 1) == Some(&b'=') {
      result.push(Operator {
        start: i,
        end: i + 2,
      });
    }
  }
  result
}

/// What `findOperators` is inside at a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
  /// Plain value text.
  Normal,
  /// A string opened by this quote.
  String(u8),
  /// `#{...}` interpolation, where operators count even inside a string.
  Interpolation,
}

impl Mode {
  /// Whether operators count in this mode (`isCalculationEnabled`).
  fn calculation_enabled(self) -> bool {
    !matches!(self, Mode::String(_))
  }
}

/// What `mathOperatorCharType` makes of a math character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharType {
  /// An operator in a math or string operation.
  Op,
  /// A `+` or `-` sign before an operand.
  Sign,
  /// Part of a string or identifier.
  Char,
}

/// `mathOperatorCharType`: whether the `+`, `-`, `*`, `%` or `/` at `index`
/// is an operator, a sign, or part of a word.  An escaped one is a
/// character.
fn char_type(string: &str, index: usize, is_after_colon: bool) -> CharType {
  let bytes = string.as_bytes();
  if index > 0 && bytes[index - 1] == b'\\' {
    return CharType::Char;
  }
  match bytes[index] {
    b'+' => check_plus(string, index, is_after_colon),
    b'-' => check_minus(string, index),
    b'*' => check_multiplication(string, index),
    b'%' => check_percent(string, index),
    b'/' => check_slash(string, index, is_after_colon),
    _ => CharType::Char,
  }
}

/// `checkMultiplication`: always an operator, except as a selector inside a
/// function (`selector(:has(*))`) or a Tailwind variant (`*:w-full`).
fn check_multiplication(string: &str, index: usize) -> CharType {
  if string[index + 1..].starts_with(':') {
    return CharType::Char;
  }
  if let Some(name) = inside_function_call(string, index) {
    let args = Regex::new(&format!(r"{}\(([^)]+)\)", regex::escape(name))).ok();
    let only_star = args
      .and_then(|args| args.captures(string))
      .and_then(|captures| captures.get(1))
      .is_some_and(|arg| arg.as_str() == "*");
    if only_star {
      return CharType::Char;
    }
  }
  CharType::Op
}

/// `checkPlus`: a sign at the start or right before an operand that follows
/// whitespace, an operator otherwise.
fn check_plus(string: &str, index: usize, is_after_colon: bool) -> CharType {
  let before = &string[..index];
  let after = &string[index + 1..];
  let at_start = is_at_start(string, index);
  let at_end = is_at_end(string, index);
  let space_before = ends_with_space(before);
  let space_after = starts_with_space(after);

  if at_start {
    return CharType::Sign;
  }
  // `1+1`, `string+#fff`; `1-+1` is a sign.
  if !space_before && !at_end && !space_after {
    return sign_after_operator(string, index);
  }
  // `sth +sth`
  if !at_end && !space_after {
    if is_no_operand_before(string, index) {
      return CharType::Sign;
    }
    let unit = value_with_unit_after(after);
    let number = number_after(after);
    if (unit.is && !unit.ops_between) || (number.is && !number.ops_between) {
      if is_after_colon
        || is_inside_parens(string, index)
        || inside_function_call(string, index).is_some()
        || is_inside_interpolation(string, index)
      {
        return CharType::Op;
      }
      return CharType::Sign;
    }
    // `sth +#fff`, `sth +string`, `sth +#{...}`, `sth +$var`.  Upstream
    // also tests the interpolation's `opsBefore`, a field it never sets.
    if is_string_after(after)
      || is_hex_color_after(after)
      || after.starts_with('$')
      || interpolation_after(after).is
    {
      return sign_after_operator(string, index);
    }
    return CharType::Op;
  }
  // `$var+ sth`, `sth + sth`
  CharType::Op
}

/// `checkMinus`: an operator between spaced operands or after a number, a
/// sign before a number, part of an identifier otherwise.
fn check_minus(string: &str, index: usize) -> CharType {
  let before = &string[..index];
  let after = &string[index + 1..];
  let at_start = is_at_start(string, index);
  let at_end = is_at_end(string, index);
  let space_before = ends_with_space(before);
  let space_after = starts_with_space(after);
  let unit = value_with_unit_after(after);
  let number = number_after(after);
  let operand_after = (unit.is && !unit.ops_between) || (number.is && !number.ops_between);

  if is_comparison_operator_before(before) || at_start {
    return CharType::Sign;
  }
  // `10 -    11`
  if !at_end && space_before && space_after {
    return CharType::Op;
  }
  // `sth -10px`
  if !at_end && space_before && !space_after {
    // `sth -(...)`; upstream's `opsBefore` test is on a field never set.
    if parens_after(after).is {
      return CharType::Op;
    }
    // `#{10px -1}`, `#{math.acos(-0.5)}`
    if is_inside_interpolation(string, index) {
      return match inside_function_call(string, index) {
        Some("url") => CharType::Op,
        Some(_) => CharType::Sign,
        // `#{$i * -10}px`
        None if number.is && is_preceding_operator(string, index) => CharType::Sign,
        None => CharType::Op,
      };
    }
    // `sth -1px`, `sth -1`, even in parentheses or function arguments.
    if operand_after {
      return CharType::Sign;
    }
    // `sth --1`, `sth +-2px`
    if unit.is || number.is {
      return CharType::Op;
    }
    // `sth -string`, `sth -#{...}`
    if is_string_after(after) || interpolation_after(after).is {
      return CharType::Char;
    }
    // `#0af -#f0a`
    if is_hex_color_after(after) && is_hex_color_before(before.trim()) {
      return CharType::Op;
    }
    if after.starts_with('$') {
      return sign_after_operator(string, index);
    }
    return CharType::Sign;
  }
  // `10x- sth`
  if !at_end && !space_before && space_after {
    if is_parens_before(before)
      || is_inside_interpolation(string, index)
      || is_number_before(before)
      || is_hex_color_before(before)
    {
      return CharType::Op;
    }
    return CharType::Char;
  }
  // `10px-1`
  if !at_end && !space_before && !space_after {
    // `10px-1`, `1-10px`, `1-1`, `1x-1x`
    if operand_after && (is_value_with_unit_before(before) || is_number_before(before)) {
      return CharType::Op;
    }
    // `1-$var`
    if is_number_before(before) && after.starts_with('$') {
      return CharType::Op;
    }
    // `fn()-10px`
    if is_function_before(before) && operand_after {
      return CharType::Op;
    }
  }
  CharType::Char
}

/// `checkSlash`: a division when an operand is a variable, a function call
/// or parenthesised, or when math surrounds it; a CSS slash otherwise.
fn check_slash(string: &str, index: usize, is_after_colon: bool) -> CharType {
  // Spaces around a slash make no difference.
  let before = string[..index].trim();
  let after = string[index + 1..].trim();

  if interpolation_before(before).is || interpolation_after(after).is {
    return CharType::Char;
  }
  // A relative path: `url(../../image.png)`.
  if before.ends_with('.') {
    return CharType::Char;
  }
  let function = inside_function_call(string, index);
  if function == Some("url") {
    // `url(https://my-url.com/image-#{$i /2}-dark.svg)`
    return if is_inside_interpolation(string, index) {
      CharType::Op
    } else {
      CharType::Char
    };
  }
  // `10px/normal`
  if string_before(before) || is_string_after(after) {
    return CharType::Char;
  }
  if is_variable_before(before)
    || variable_after(after).is
    || is_function_before(before)
    || function_after(after).is
    || is_parens_before(before)
    || parens_after(after).is
  {
    return CharType::Op;
  }
  // `$var: 10px/2;`
  if is_after_colon
    && (value_with_unit_after(after).is || number_after(after).is)
    && (is_value_with_unit_before(before) || is_number_before(before))
  {
    return CharType::Op;
  }
  if SLASH_MATH_BEFORE.iter().any(|re| re.is_match(before))
    || SLASH_MATH_AFTER.iter().any(|re| re.is_match(after))
  {
    return CharType::Op;
  }
  if is_inside_parens(string, index) || function.is_some() {
    return CharType::Op;
  }
  CharType::Char
}

/// `checkPercent`: modulo when it follows whitespace, a variable or a
/// closing parenthesis; a percent unit otherwise.
fn check_percent(string: &str, index: usize) -> CharType {
  let before = &string[..index];
  let after = &string[index + 1..];

  if interpolation_before(before.trim()).is || interpolation_after(after.trim()).is {
    return CharType::Char;
  }
  if is_at_start(string, index) || is_at_end(string, index) {
    return CharType::Char;
  }
  // `sth %sth`
  if ends_with_space(before) && !starts_with_space(after) {
    return CharType::Op;
  }
  // `$var% 1`, `$var%1`, `(...)%1`
  if is_variable_before(before) || is_parens_before(before) {
    return CharType::Op;
  }
  CharType::Char
}

/// A sign when another operator comes right before the character at
/// `index` (`1-+1`), an operator otherwise.
fn sign_after_operator(string: &str, index: usize) -> CharType {
  if is_preceding_operator(string, index) {
    CharType::Sign
  } else {
    CharType::Op
  }
}

/// What upstream's `is*After` helpers return: whether the operand is there,
/// and whether operators stand between it and the character tested.
struct Lookahead {
  /// The operand follows.
  is: bool,
  /// One or more operators come first (`+ -1`).
  ops_between: bool,
}

/// Matches `text` against an `is*After` pattern whose first group is the
/// run of operators before the operand.
fn lookahead(pattern: &Regex, text: &str) -> Lookahead {
  match pattern.captures(text) {
    Some(captures) => Lookahead {
      is: true,
      ops_between: captures.get(1).is_some(),
    },
    None => Lookahead {
      is: false,
      ops_between: false,
    },
  }
}

/// Compiles one of the patterns below.
fn re(pattern: &str) -> Regex {
  Regex::new(pattern).expect("valid regex")
}

static INSIDE_PARENS_BEFORE: LazyLock<Regex> = LazyLock::new(|| re(r"(?:^|[,{\s])\([^(){},]+$"));
static INSIDE_PARENS_AFTER: LazyLock<Regex> = LazyLock::new(|| re(r"^[^(){},\s]+\s*\)"));
static INSIDE_INTERPOLATION: LazyLock<Regex> = LazyLock::new(|| re(r"#\{[^}]*$"));
static FUNCTION_CALL_BEFORE: LazyLock<Regex> =
  LazyLock::new(|| re(r"(?i)(?:[a-z_-][0-9a-z_-]*\()?(:?[a-z_-][0-9a-z_-]*)\("));
static FUNCTION_CALL_AFTER: LazyLock<Regex> = LazyLock::new(|| re(r"^[^(]+\)"));
static TYPE_FUNCTION_CALL: LazyLock<Regex> =
  LazyLock::new(|| re(r"(?i)(?:^|[^0-9a-z_-])type\([^()]*$"));
static TRAILING_STRING_OPS: LazyLock<Regex> = LazyLock::new(|| re(r"(?:\s*[+/*%]|\s+-)+$"));
static STRING_BEFORE: LazyLock<Regex> =
  LazyLock::new(|| re(r"(?i)(?:^|[/(){},: ])(?:[a-z_][0-9a-z_-]*|-+[a-z_][0-9a-z_-]*)$"));
static STRING_AFTER: LazyLock<Regex> =
  LazyLock::new(|| re(r"(?i)^(?:[a-z_][0-9a-z_-]*|-+[a-z_][0-9a-z_-]*)(?:$|[)}, ])"));
static INTERPOLATION_AFTER: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*([+/*%-]\s*)*#\{"));
static TRAILING_OPS: LazyLock<Regex> = LazyLock::new(|| re(r"(?:\s*[+/*%-])+$"));
static PARENS_AFTER: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*([+/*%-]\s*)*\("));
static PARENS_BEFORE: LazyLock<Regex> = LazyLock::new(|| re(r"\)\s*$"));
static VALUE_WITH_UNIT_BEFORE: LazyLock<Regex> =
  LazyLock::new(|| re(r"(?:^|[/(, .])[0-9][0-9A-Za-z_-]+$"));
static VALUE_WITH_UNIT_AFTER: LazyLock<Regex> = LazyLock::new(|| {
  re(r"(?i)^\s*([+/*%-]\s*)*(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)[a-z_-]+(?:$|[)}, ])")
});
static NUMBER_AFTER: LazyLock<Regex> =
  LazyLock::new(|| re(r"^\s*([+/*%-]\s*)*(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)(?:$|[)}, ])"));
static NUMBER_BEFORE: LazyLock<Regex> =
  LazyLock::new(|| re(r"(?:^|[/(){},\s])(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)$"));
static VARIABLE_BEFORE: LazyLock<Regex> = LazyLock::new(|| re(r"\$[0-9A-Za-z_-]+$"));
static VARIABLE_AFTER: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*([+/*%-]\s*)*\$"));
/// JavaScript's `.` stops at line terminators.
static FUNCTION_BEFORE: LazyLock<Regex> =
  LazyLock::new(|| re(r"[0-9A-Za-z_-]\([^\n\r\x{2028}\x{2029}]*\)\s*$"));
static FUNCTION_AFTER: LazyLock<Regex> =
  LazyLock::new(|| re(r"(?i)^\s*(-\s+|[+/*%]\s*)*[a-z_-][0-9a-z_-]*\("));
static COMPARISON_BEFORE: LazyLock<Regex> = LazyLock::new(|| re(r"(?:[><=!]=|[<>])\s+$"));
static HEX_COLOR: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^#(?:[0-9a-f]{3}|[0-9a-f]{6})$"));
/// Upstream's class holds a literal `s` where `\s` was surely meant.
static HEX_COLOR_BEFORE: LazyLock<Regex> =
  LazyLock::new(|| re(r"(?i)(?:[/(){},+*%\-s]|^)#(?:[0-9a-f]{3}|[0-9a-f]{6})$"));

/// Math right before a slash that makes it a division: `+`, `*` or `%`
/// after an operand, or a spaced ` - `.
static SLASH_MATH_BEFORE: LazyLock<[Regex; 2]> = LazyLock::new(|| {
  [
    re(r"[^{,(}\s]\s*[+*%][^(){},]+$"),
    re(r"[^{,(}\s]\s+-\s[^(){},]+$"),
  ]
});
/// Math right after a slash that makes it a division: `10/2 * 3`,
/// `10px/2px+1`, `10/10 -$var`, `10s/10s-10`, and so on.
static SLASH_MATH_AFTER: LazyLock<[Regex; 6]> = LazyLock::new(|| {
  [
    re(r"^[^(){},]+[*%]"),
    re(r"^[^(){},\s]+\+"),
    re(r"^[^(){},\s]+\s+\+[^0-9]"),
    re(r"^[^(){},\s]+\s+-(?:\$|\s)"),
    re(r"^[^(){},\s]+-\.?[0-9]"),
    re(r"(?i)^(?:[0-9]*\.)?[0-9]+-\s*[^#a-z_\s]"),
  ]
});

/// Whether `text` ends with whitespace (`/\s$/`).
fn ends_with_space(text: &str) -> bool {
  text.chars().next_back().is_some_and(char::is_whitespace)
}

/// Whether `text` starts with whitespace (`/^\s/`).
fn starts_with_space(text: &str) -> bool {
  text.chars().next().is_some_and(char::is_whitespace)
}

/// `isAtStart`: nothing but `(`, `{` or `,` before the character.
fn is_at_start(string: &str, index: usize) -> bool {
  let before = string[..index].trim();
  before.is_empty() || before.ends_with(['(', '{', ','])
}

/// `isAtEnd`: nothing but `,`, `)` or `}` after the character.
fn is_at_end(string: &str, index: usize) -> bool {
  let after = string[index + 1..].trim();
  after.is_empty() || after.starts_with([',', ')', '}'])
}

/// `isInsideParens`: the character is between the only two operands of a
/// parenthesised group.
fn is_inside_parens(string: &str, index: usize) -> bool {
  INSIDE_PARENS_BEFORE.is_match(string[..index].trim())
    && INSIDE_PARENS_AFTER.is_match(string[index + 1..].trim())
}

/// `isInsideInterpolation`: an unclosed `#{` before the character.
fn is_inside_interpolation(string: &str, index: usize) -> bool {
  INSIDE_INTERPOLATION.is_match(string[..index].trim())
}

/// `isInsideFunctionCall`: the name of the function whose arguments hold
/// the character, as upstream guesses it (the first call in the text
/// before, or the one nested right in it), or `None` outside one.
fn inside_function_call(string: &str, index: usize) -> Option<&str> {
  let name = FUNCTION_CALL_BEFORE
    .captures(string[..index].trim())?
    .get(1)?
    .as_str();
  FUNCTION_CALL_AFTER
    .is_match(string[index + 1..].trim())
    .then_some(name)
}

/// `isInsideTypeFunctionCall`: the nearest unclosed `(` before the
/// character opens a `type(`.
fn is_inside_type_function_call(string: &str, index: usize) -> bool {
  TYPE_FUNCTION_CALL.is_match(&string[..index])
}

/// `isStringBefore(...).is`: a quoted string or an identifier ends the
/// text, operators after it aside.
fn string_before(before: &str) -> bool {
  let clipped = TRAILING_STRING_OPS.replace(before, "");
  clipped.ends_with(['"', '\'']) || STRING_BEFORE.is_match(&clipped)
}

/// `isStringAfter`: a quoted string or an identifier starts the text.
fn is_string_after(after: &str) -> bool {
  let trimmed = after.trim();
  trimmed.starts_with(['"', '\'']) || STRING_AFTER.is_match(trimmed)
}

/// `isInterpolationAfter`
fn interpolation_after(after: &str) -> Lookahead {
  lookahead(&INTERPOLATION_AFTER, after)
}

/// `isInterpolationBefore`: a `}` ends the text, operators after it aside.
fn interpolation_before(before: &str) -> Lookahead {
  let clipped = TRAILING_OPS.replace(before, "");
  Lookahead {
    is: clipped.ends_with('}'),
    ops_between: clipped.len() != before.len(),
  }
}

/// `isParensAfter`
fn parens_after(after: &str) -> Lookahead {
  lookahead(&PARENS_AFTER, after)
}

/// `isParensBefore`: a `)` ends the text.
fn is_parens_before(before: &str) -> bool {
  PARENS_BEFORE.is_match(before)
}

/// `isValueWithUnitBefore`: `1px`, `.2p-` and the like end the text.
fn is_value_with_unit_before(before: &str) -> bool {
  VALUE_WITH_UNIT_BEFORE.is_match(before.trim())
}

/// `isValueWithUnitAfter`
fn value_with_unit_after(after: &str) -> Lookahead {
  lookahead(&VALUE_WITH_UNIT_AFTER, after)
}

/// `isNumberAfter`
fn number_after(after: &str) -> Lookahead {
  lookahead(&NUMBER_AFTER, after)
}

/// `isNumberBefore`: a bare number ends the text.
fn is_number_before(before: &str) -> bool {
  NUMBER_BEFORE.is_match(before.trim())
}

/// `isVariableBefore`: a `$variable` ends the text.
fn is_variable_before(before: &str) -> bool {
  VARIABLE_BEFORE.is_match(before.trim())
}

/// `isVariableAfter`
fn variable_after(after: &str) -> Lookahead {
  lookahead(&VARIABLE_AFTER, after)
}

/// `isFunctionBefore`: a function call ends the text.
fn is_function_before(before: &str) -> bool {
  FUNCTION_BEFORE.is_match(before.trim())
}

/// `isFunctionAfter`
fn function_after(after: &str) -> Lookahead {
  lookahead(&FUNCTION_AFTER, after)
}

/// `isComparisonOperatorBefore`: a comparison and whitespace end the text.
fn is_comparison_operator_before(before: &str) -> bool {
  COMPARISON_BEFORE.is_match(before)
}

/// `isHexColorAfter`: the text up to the next operator, bracket, comma or
/// whitespace is a hex colour.
fn is_hex_color_after(after: &str) -> bool {
  let end = after
    .find(|c: char| matches!(c, ')' | '}' | ',' | '+' | '/' | '*' | '%' | '-') || c.is_whitespace())
    .unwrap_or(after.len());
  HEX_COLOR.is_match(after[..end].trim())
}

/// `isHexColorBefore`: a hex colour ends the text.
fn is_hex_color_before(before: &str) -> bool {
  HEX_COLOR_BEFORE.is_match(before)
}

/// `isNoOperandBefore`: nothing before the character.  (Upstream's second
/// test looks for a literal `&` after `(`, `{` or `,`.)
fn is_no_operand_before(string: &str, index: usize) -> bool {
  let before = string[..index].trim();
  before.is_empty() || before.contains("(&") || before.contains("{&") || before.contains(",&")
}

/// `isPrecedingOperator`: the last non-whitespace character before `index`
/// is itself an operator.
fn is_preceding_operator(string: &str, index: usize) -> bool {
  string[..index]
    .char_indices()
    .rev()
    .find(|&(_, c)| !c.is_whitespace())
    .is_some_and(|(prev, _)| char_type(string, prev, false) == CharType::Op)
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The text of each operator `find_operators` finds in `string`.
  fn operators(string: &str, is_after_colon: bool) -> Vec<&str> {
    find_operators(string, is_after_colon)
      .into_iter()
      .map(|op| &string[op.start..op.end])
      .collect()
  }

  #[test]
  fn spaced_math_is_an_operator() {
    assert_eq!(operators("1 + 2", false), ["+"]);
    assert_eq!(operators("$a - $b", false), ["-"]);
    assert_eq!(operators("$a * 2", false), ["*"]);
    assert_eq!(operators("$a % 2", false), ["%"]);
    assert_eq!(operators("$a / 2", false), ["/"]);
  }

  #[test]
  fn signs_are_not_operators() {
    assert_eq!(operators("1 +1", false), Vec::<&str>::new());
    assert_eq!(operators("10px -1", false), Vec::<&str>::new());
    assert_eq!(operators("-$x", false), Vec::<&str>::new());
    // After a colon, `+` before a number is addition.
    assert_eq!(operators("1 +1", true), ["+"]);
  }

  #[test]
  fn hyphens_units_and_css_slashes_are_not_operators() {
    for value in [
      "sans-serif",
      "50%",
      "12px/1.5 serif",
      "url(../a/b.png)",
      "s1-",
    ] {
      assert_eq!(operators(value, false), Vec::<&str>::new(), "{value}");
    }
  }

  #[test]
  fn slash_with_a_variable_is_a_division() {
    assert_eq!(operators("$a/2", false), ["/"]);
    assert_eq!(operators("10px/2", true), ["/"]);
  }

  #[test]
  fn strings_hide_operators_but_interpolation_does_not() {
    assert_eq!(operators("'a + b'", false), Vec::<&str>::new());
    assert_eq!(operators("'a #{1 + 2} b'", false), ["+"]);
  }

  #[test]
  fn comparisons_take_two_characters() {
    let found = find_operators("$a >= 1", false);
    assert_eq!(found, [Operator { start: 3, end: 5 }]);
    assert_eq!(operators("$a == 1", false), ["=="]);
    assert_eq!(operators("$a < 1", false), ["<"]);
    assert_eq!(operators("type(<color>)", false), Vec::<&str>::new());
  }

  #[test]
  fn star_alone_in_a_function_is_a_selector() {
    assert_eq!(operators("selector(:has(*))", false), Vec::<&str>::new());
    assert_eq!(operators("*:w-full", false), Vec::<&str>::new());
  }

  #[test]
  fn multibyte_text_does_not_split() {
    assert_eq!(operators("'é' + 1 + ✓", false), ["+", "+"]);
  }
}
