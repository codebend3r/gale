//! A rule's custom `message` option, applied the way Stylelint applies it.
//!
//! Stylelint's `report` fills a custom message from the warning's
//! `messageArgs` (a function message is called with them; a string one has
//! each `%s` / `%d` replaced in turn), then, since Stylelint 16.25, appends
//! ` (rule-name)` unless the message already ends with it.  Older versions
//! print the custom message as is.
//!
//! The config loader turns a one-parameter message function such as
//! `` (name) => `Expected "${name}" to be kebab-case` `` into the string
//! `Expected "${name}" to be kebab-case`; here `${name}` stands for the first
//! message argument.

use gale_diagnostics::Diagnostic;

use crate::stylelint_version::appends_rule_name_to_custom_messages;

/// The placeholder the config loader writes for a message function's
/// parameter.
const PARAMETER: &str = "${name}";

/// Replace `diag`'s message with the custom `message`, filled in from its
/// message arguments.
pub fn apply(diag: &mut Diagnostic, message: &str) {
  diag.message = render(message, &diag.message_args);
  diag.bare_message = !appends_rule_name_to_custom_messages();
}

/// `message` filled in from `args`: a function message has its `${name}`
/// replaced by the first argument; a string message has each `%s` or `%d`
/// replaced by the next argument, as Stylelint's `printfLike` does.  Without
/// arguments the message is left as written.
pub fn render(message: &str, args: &[String]) -> String {
  let Some(first) = args.first() else {
    return message.to_string();
  };
  if message.contains(PARAMETER) {
    return message.replace(PARAMETER, first);
  }
  let mut out = message.to_string();
  for arg in args {
    let next = ["%s", "%d"].iter().filter_map(|spec| out.find(spec)).min();
    match next {
      Some(at) => out.replace_range(at..at + 2, arg),
      None => break,
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  /// `values` as owned message arguments.
  fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
  }

  #[test]
  fn function_messages_take_the_first_argument() {
    assert_eq!(
      render(
        "Expected container name \"${name}\" to be kebab-case",
        &args(&["myBox", "^[a-z-]+$"])
      ),
      "Expected container name \"myBox\" to be kebab-case"
    );
  }

  #[test]
  fn string_messages_are_filled_like_printf() {
    assert_eq!(
      render("Bad %s, want %d", &args(&["x", "^y$"])),
      "Bad x, want ^y$"
    );
    assert_eq!(render("Only %s", &args(&["a", "b"])), "Only a");
    assert_eq!(render("Plain", &args(&["a"])), "Plain");
  }

  #[test]
  fn messages_without_arguments_stay_as_written() {
    assert_eq!(render("Use \"${name}\" %s", &[]), "Use \"${name}\" %s");
  }

  #[test]
  fn the_runner_fills_custom_messages_from_the_rule() {
    use std::collections::HashMap;

    use gale_css_parser::Syntax;

    use crate::{LintRunner, RuleRegistry};

    // stylelint-config-standard's message function, as the config loader
    // converts it.
    let mut options = HashMap::new();
    options.insert(
      "keyframes-name-pattern".to_string(),
      serde_json::json!([
        "^([a-z][a-z0-9]*)(-[a-z0-9]+)*$",
        { "message": "Expected keyframe name \"${name}\" to be kebab-case" }
      ]),
    );
    let runner = LintRunner::with_options(
      RuleRegistry::default(),
      vec!["keyframes-name-pattern".to_string()],
      options,
    );
    let result = runner.lint_source("@keyframes slideIn { }", "t.css", Syntax::Css);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
      result.diagnostics[0].message,
      "Expected keyframe name \"slideIn\" to be kebab-case"
    );
  }
}
