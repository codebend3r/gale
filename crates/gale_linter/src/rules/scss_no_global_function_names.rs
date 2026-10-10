use std::collections::{HashMap, HashSet};

use gale_css_parser::{CssNode, Syntax};
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::rule::{Rule, RuleContext};

/// Disallow deprecated global SCSS function calls that should use modules.
///
/// e.g. `adjust-color()` should be `color.adjust()`.
///
/// Unlike stylelint-scss's rule, this one is fixable in SCSS: `--fix`
/// renames the call to its module function and adds the `@use` it needs
/// (see [`Rule::check_root`] below).
pub struct ScssNoGlobalFunctionNames;

/// Returns the Stylelint-compatible message for a deprecated global function.
/// Matches the exact format produced by stylelint-scss no-global-function-names.
fn function_message(name: &str) -> &'static str {
  match name {
    // rule_mapping entries: have specific argument transformations
    "darken" => {
      "Expected color.adjust($color, $lightness: -$amount) instead of darken($color, $amount)"
    }
    "lighten" => {
      "Expected color.adjust($color, $lightness: $amount) instead of lighten($color, $amount)"
    }
    "adjust-hue" => {
      "Expected color.adjust($color, $hue: $amount) instead of adjust-hue($color, $amount)"
    }
    "desaturate" => {
      "Expected color.adjust($color, $saturation: -$amount) instead of desaturate($color, $amount)"
    }
    "opacify" => {
      "Expected color.adjust($color, $alpha: -$amount) instead of opacify($color, $amount)"
    }
    "saturate" => {
      "Expected color.adjust($color, $saturation: $amount) instead of saturate($color, $amount)"
    }
    "transparentize" => {
      "Expected color.adjust($color, $alpha: -$amount) instead of transparentize($color, $amount)"
    }
    // new_rule_names entries (no rule_mapping): Expected module.new_name instead of name
    "adjust-color" => "Expected color.adjust instead of adjust-color",
    "scale-color" => "Expected color.scale instead of scale-color",
    "change-color" => "Expected color.change instead of change-color",
    "map-get" => "Expected map.get instead of map-get",
    "map-merge" => "Expected map.merge instead of map-merge",
    "map-remove" => "Expected map.remove instead of map-remove",
    "map-keys" => "Expected map.keys instead of map-keys",
    "map-values" => "Expected map.values instead of map-values",
    "map-has-key" => "Expected map.has-key instead of map-has-key",
    "str-length" => "Expected string.length instead of str-length",
    "str-insert" => "Expected string.insert instead of str-insert",
    "str-index" => "Expected string.index instead of str-index",
    "str-slice" => "Expected string.slice instead of str-slice",
    "unitless" => "Expected math.is-unitless instead of unitless",
    "comparable" => "Expected math.compatible instead of comparable",
    "list-separator" => "Expected list.separator instead of list-separator",
    "selector-nest" => "Expected selector.nest instead of selector-nest",
    "selector-append" => "Expected selector.append instead of selector-append",
    "selector-replace" => "Expected selector.replace instead of selector-replace",
    "selector-unify" => "Expected selector.unify instead of selector-unify",
    "selector-parse" => "Expected selector.parse instead of selector-parse",
    "selector-extend" => "Expected selector.extend instead of selector-extend",
    "is-superselector" => "Expected selector.is-superselector instead of is-superselector",
    // Remaining: Expected module.name instead of name
    "red" => "Expected color.red instead of red",
    "blue" => "Expected color.blue instead of blue",
    "green" => "Expected color.green instead of green",
    "mix" => "Expected color.mix instead of mix",
    "hue" => "Expected color.hue instead of hue",
    "saturation" => "Expected color.saturation instead of saturation",
    "lightness" => "Expected color.lightness instead of lightness",
    "complement" => "Expected color.complement instead of complement",
    "ie-hex-str" => "Expected color.ie-hex-str instead of ie-hex-str",
    "unquote" => "Expected string.unquote instead of unquote",
    "quote" => "Expected string.quote instead of quote",
    "to-upper-case" => "Expected string.to-upper-case instead of to-upper-case",
    "to-lower-case" => "Expected string.to-lower-case instead of to-lower-case",
    "unique-id" => "Expected string.unique-id instead of unique-id",
    "percentage" => "Expected math.percentage instead of percentage",
    "ceil" => "Expected math.ceil instead of ceil",
    "floor" => "Expected math.floor instead of floor",
    "abs" => "Expected math.abs instead of abs",
    "random" => "Expected math.random instead of random",
    "unit" => "Expected math.unit instead of unit",
    "length" => "Expected list.length instead of length",
    "nth" => "Expected list.nth instead of nth",
    "set-nth" => "Expected list.set-nth instead of set-nth",
    "join" => "Expected list.join instead of join",
    "append" => "Expected list.append instead of append",
    "zip" => "Expected list.zip instead of zip",
    "index" => "Expected list.index instead of index",
    "feature-exists" => "Expected meta.feature-exists instead of feature-exists",
    "variable-exists" => "Expected meta.variable-exists instead of variable-exists",
    "global-variable-exists" => {
      "Expected meta.global-variable-exists instead of global-variable-exists"
    }
    "function-exists" => "Expected meta.function-exists instead of function-exists",
    "mixin-exists" => "Expected meta.mixin-exists instead of mixin-exists",
    "inspect" => "Expected meta.inspect instead of inspect",
    "get-function" => "Expected meta.get-function instead of get-function",
    "type-of" => "Expected meta.type-of instead of type-of",
    "call" => "Expected meta.call instead of call",
    "content-exists" => "Expected meta.content-exists instead of content-exists",
    "keywords" => "Expected meta.keywords instead of keywords",
    "simple-selectors" => "Expected selector.simple-selectors instead of simple-selectors",
    // Fallback (should not be reached for known functions)
    _ => "Expected a Sass module function instead of a global function",
  }
}

/// Returns true if `name` is a deprecated global SCSS function (per stylelint-scss rules).
fn is_deprecated_global(name: &str) -> bool {
  matches!(
    name,
    "abs"
      | "adjust-color"
      | "adjust-hue"
      | "append"
      | "blue"
      | "call"
      | "ceil"
      | "change-color"
      | "comparable"
      | "complement"
      | "content-exists"
      | "darken"
      | "desaturate"
      | "feature-exists"
      | "floor"
      | "function-exists"
      | "get-function"
      | "global-variable-exists"
      | "green"
      | "hue"
      | "ie-hex-str"
      | "index"
      | "inspect"
      | "is-superselector"
      | "join"
      | "keywords"
      | "length"
      | "lighten"
      | "lightness"
      | "list-separator"
      | "map-get"
      | "map-has-key"
      | "map-keys"
      | "map-merge"
      | "map-remove"
      | "map-values"
      | "mix"
      | "mixin-exists"
      | "nth"
      | "opacify"
      | "percentage"
      | "quote"
      | "random"
      | "red"
      | "saturate"
      | "saturation"
      | "scale-color"
      | "selector-append"
      | "selector-extend"
      | "selector-nest"
      | "selector-parse"
      | "selector-replace"
      | "selector-unify"
      | "set-nth"
      | "simple-selectors"
      | "str-index"
      | "str-insert"
      | "str-length"
      | "str-slice"
      | "to-lower-case"
      | "to-upper-case"
      | "transparentize"
      | "type-of"
      | "unique-id"
      | "unit"
      | "unitless"
      | "unquote"
      | "variable-exists"
      | "zip"
  )
}

/// The module function a deprecated global is renamed to, as
/// `(module, member)`: `map-get` becomes `("map", "get")`.
///
/// Read from the rule's message, so the two cannot disagree.  `None` for
/// the colour functions whose replacement takes different arguments, such
/// as `darken($color, $amount)`, which a rename cannot fix.
fn module_replacement(name: &str) -> Option<(&'static str, &'static str)> {
  let target = function_message(name)
    .strip_prefix("Expected ")?
    .split(" instead of ")
    .next()?;
  if target.contains('(') {
    return None;
  }
  target.split_once('.')
}

/// Finds the deprecated global function calls in `value`, recording each
/// name and the offset in `source` where it starts.  Offsets are
/// re-derived from the source for an exact span.
fn scan_value_for_global_functions(
  value: &str,
  decl_span: gale_css_parser::Span,
  source: &str,
  calls: &mut Vec<(String, usize)>,
) {
  // Compute the byte offset in `source` where the value string starts.
  // The declaration span covers "property: value", so we search for the
  // value within that range to find the exact offset.
  let decl_start = decl_span.offset as usize;
  let decl_end = (decl_span.offset + decl_span.length) as usize;
  let decl_end = decl_end.min(source.len());
  let decl_text = source.get(decl_start..decl_end).unwrap_or("");
  let value_pos_in_decl = decl_text.find(value).unwrap_or(0);

  let bytes = value.as_bytes();
  let len = bytes.len();
  let mut i = 0;
  let mut in_single_quote = false;
  let mut in_double_quote = false;

  while i < len {
    // Track string context to skip function names inside strings
    if bytes[i] == b'"' && !in_single_quote {
      in_double_quote = !in_double_quote;
      i += 1;
      continue;
    }
    if bytes[i] == b'\'' && !in_double_quote {
      in_single_quote = !in_single_quote;
      i += 1;
      continue;
    }
    if in_single_quote || in_double_quote {
      i += 1;
      continue;
    }

    // Skip non-alpha/hyphen
    if !bytes[i].is_ascii_alphabetic() && bytes[i] != b'-' {
      i += 1;
      continue;
    }

    // Collect function name
    let start = i;
    while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
      i += 1;
    }

    // Check if followed by `(`
    if i < len && bytes[i] == b'(' {
      let func_name = &value[start..i];

      // Skip namespaced calls (e.g., `math.div(`, `map.get(`)
      let is_namespaced = start > 0 && bytes[start - 1] == b'.';

      if !is_namespaced && is_deprecated_global(func_name) {
        calls.push((
          func_name.to_string(),
          decl_start + value_pos_in_decl + start,
        ));
      }
    }

    i += 1;
  }
}

/// Collects the calls in `node` and everything under it, visiting what the
/// runner's walk would hand to a per-node check: the declarations of every
/// style rule, and declarations nested in at-rules.
fn collect_calls(node: &CssNode, source: &str, calls: &mut Vec<(String, usize)>) {
  match node {
    CssNode::Declaration(decl) => {
      scan_value_for_global_functions(&decl.value, decl.span, source, calls)
    }
    CssNode::Style(rule) => collect_style_calls(rule, source, calls),
    CssNode::AtRule(at) => {
      for child in &at.children {
        collect_calls(child, source, calls);
      }
    }
    CssNode::Comment(_) => {}
  }
}

/// [`collect_calls`] for a style rule and the rules and at-rules nested in it.
fn collect_style_calls(
  rule: &gale_css_parser::StyleRule,
  source: &str,
  calls: &mut Vec<(String, usize)>,
) {
  for decl in &rule.declarations {
    scan_value_for_global_functions(&decl.value, decl.span, source, calls);
  }
  for child in &rule.children {
    collect_style_calls(child, source, calls);
  }
  for at in &rule.nested_at_rules {
    collect_calls(at, source, calls);
  }
}

/// How a file loads a built-in module with `@use "sass:<module>"`.
enum Namespace {
  /// `@use "sass:map"` or `@use "sass:map" as m`: members are `m.get`.
  Named(String),
  /// `@use "sass:map" as *`: members are global again, so there is no
  /// namespace to rename to.
  Star,
}

/// What the file's top-level `@use` rules load, and where a new one goes.
struct Loads {
  /// The built-in modules loaded, by module name (`map` for `sass:map`).
  modules: HashMap<String, Namespace>,
  /// Every namespace a `@use` takes, built-in or not.
  namespaces: HashSet<String>,
  /// Where to insert a new `@use`: just past the `;` of the last `@use`
  /// or `@forward`, else past the `@charset`, else the start of the file.
  insert_at: usize,
  /// Whether `insert_at` follows an existing statement.
  after_statement: bool,
}

impl Loads {
  /// Reads the `@use`, `@forward` and `@charset` rules among `nodes`.
  fn read(nodes: &[CssNode], source: &str) -> Self {
    let mut loads = Loads {
      modules: HashMap::new(),
      namespaces: HashSet::new(),
      insert_at: 0,
      after_statement: false,
    };
    let mut after_load = false;
    for node in nodes {
      let CssNode::AtRule(at) = node else {
        continue;
      };
      let name = at.name.to_ascii_lowercase();
      if !matches!(name.as_str(), "use" | "forward" | "charset") {
        continue;
      }
      let statement_end = source
        .get(at.span.offset as usize..)
        .and_then(|rest| rest.find(';'))
        .map(|semi| at.span.offset as usize + semi + 1);
      if let Some(end) = statement_end {
        if name != "charset" || !after_load {
          loads.insert_at = end;
          loads.after_statement = true;
        }
      }
      if name == "charset" {
        continue;
      }
      after_load = true;
      if name != "use" {
        continue;
      }

      let params = at.params.trim();
      let Some(quote @ ('"' | '\'')) = params.chars().next() else {
        continue;
      };
      let Some(url_end) = params[1..].find(quote) else {
        continue;
      };
      let url = &params[1..1 + url_end];
      let rest = &params[2 + url_end..];
      let mut words = rest.split_whitespace();
      let alias = loop {
        match words.next() {
          Some("as") => break words.next(),
          Some(_) => continue,
          None => break None,
        }
      };
      let namespace = match alias {
        Some("*") => Namespace::Star,
        Some(alias) => Namespace::Named(alias.trim_end_matches(';').to_string()),
        None => {
          // The default namespace is the URL's last component, without a
          // partial's leading underscore or a file extension.
          let last = url.rsplit(['/', ':']).next().unwrap_or(url);
          let last = last.trim_start_matches('_');
          let last = last.split('.').next().unwrap_or(last);
          Namespace::Named(last.to_string())
        }
      };
      if let Namespace::Named(namespace) = &namespace {
        loads.namespaces.insert(namespace.clone());
      }
      if let Some(module) = url.strip_prefix("sass:") {
        loads.modules.insert(module.to_string(), namespace);
      }
    }
    loads
  }

  /// The text that loads `modules` at [`Self::insert_at`].
  fn insertion(&self, modules: &[&str]) -> String {
    let lines: String = modules
      .iter()
      .map(|module| format!("@use \"sass:{module}\";"))
      .collect::<Vec<_>>()
      .join("\n");
    if self.after_statement {
      format!("\n{lines}")
    } else {
      format!("{lines}\n\n")
    }
  }
}

impl Rule for ScssNoGlobalFunctionNames {
  fn name(&self) -> &'static str {
    "scss/no-global-function-names"
  }

  fn description(&self) -> &'static str {
    "Disallow global function names that should use Sass modules"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Reports deprecated global function calls in declaration values.
  ///
  /// In SCSS each report carries a fix, which Stylelint does not have: the
  /// call is renamed to its module function, under the namespace the file
  /// already loads the module with, and the first call of the file whose
  /// module is not loaded yet also adds one `@use` for every such module, in
  /// the order they first appear.  The fix is planned for the whole file, so
  /// no two reports add the same `@use`.  Calls whose arguments change, such
  /// as `darken()`, a module loaded `as *`, and a module whose namespace
  /// another `@use` already takes are reported without a fix.
  fn check_root(&self, nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    if !matches!(ctx.syntax, Syntax::Scss | Syntax::Sass) {
      return vec![];
    }

    let mut calls = Vec::new();
    for node in nodes {
      collect_calls(node, ctx.source, &mut calls);
    }
    calls.sort_by_key(|(_, offset)| *offset);

    // Fixes are computed against SCSS; `--fix` leaves `.sass` files alone.
    let fixable = matches!(ctx.syntax, Syntax::Scss);
    let loads = Loads::read(nodes, ctx.source);
    let needs_load =
      |module: &str| !loads.modules.contains_key(module) && !loads.namespaces.contains(module);
    let mut missing: Vec<&'static str> = Vec::new();
    for (name, _) in &calls {
      if let Some((module, _)) = module_replacement(name) {
        if needs_load(module) && !missing.contains(&module) {
          missing.push(module);
        }
      }
    }
    let mut insertion_pending = !missing.is_empty();

    calls
      .iter()
      .map(|(name, offset)| {
        let span = Span::new(*offset, name.len());
        let diag = Diagnostic::new(self.name(), function_message(name).to_string())
          .severity(self.default_severity())
          .span(span);
        let Some((module, member)) = module_replacement(name).filter(|_| fixable) else {
          return diag;
        };
        let namespace = match loads.modules.get(module) {
          Some(Namespace::Named(namespace)) => namespace.as_str(),
          Some(Namespace::Star) => return diag,
          None if loads.namespaces.contains(module) => return diag,
          None => module,
        };
        let mut edits = vec![Edit::new(span, format!("{namespace}.{member}"))];
        if insertion_pending && missing.contains(&module) {
          insertion_pending = false;
          edits.push(Edit::new(
            Span::new(loads.insert_at, 0),
            loads.insertion(&missing),
          ));
        }
        diag.fix(Fix::new(format!("Use {namespace}.{member}"), edits))
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::{Declaration, Span as ParserSpan};

  use crate::testing::{ctx, scss_ctx};

  fn decl(value: &str) -> CssNode {
    CssNode::Declaration(Declaration {
      property: "color".to_string(),
      value: value.to_string(),
      span: ParserSpan::new(0, 10),
      important: false,
    })
  }

  #[test]
  fn skips_non_scss() {
    assert!(
      ScssNoGlobalFunctionNames
        .check_root(&[decl("adjust-color(red, $red: 10)")], &ctx())
        .is_empty()
    );
  }

  #[test]
  fn reports_deprecated_global() {
    let d =
      ScssNoGlobalFunctionNames.check_root(&[decl("adjust-color(red, $red: 10)")], &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("adjust-color"));
  }

  #[test]
  fn reports_map_get() {
    let d = ScssNoGlobalFunctionNames.check_root(&[decl("map-get($map, key)")], &scss_ctx());
    assert_eq!(d.len(), 1);
    assert!(d[0].message.contains("map-get"));
  }

  #[test]
  fn allows_module_function() {
    // `color.adjust()` is not a deprecated global.
    let d =
      ScssNoGlobalFunctionNames.check_root(&[decl("color.adjust(red, $red: 10)")], &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_namespaced_map_get() {
    // `map.get()` should not be flagged — it uses a namespace
    let d = ScssNoGlobalFunctionNames.check_root(&[decl("map.get($map, key)")], &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_css_native_min_max() {
    // min() and max() are NOT in stylelint-scss deprecated list, not flagged
    let d = ScssNoGlobalFunctionNames.check_root(&[decl("min(100px, 50vw)")], &scss_ctx());
    assert!(d.is_empty());
    let d = ScssNoGlobalFunctionNames.check_root(&[decl("max(100px, 50vw)")], &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_css_native_round() {
    // round() is NOT in stylelint-scss deprecated list, not flagged
    let d = ScssNoGlobalFunctionNames.check_root(&[decl("round(1.5)")], &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_function_in_string() {
    let d = ScssNoGlobalFunctionNames.check_root(&[decl("\"use map-get() instead\"")], &scss_ctx());
    assert!(d.is_empty());
  }

  #[test]
  fn allows_non_deprecated() {
    let d = ScssNoGlobalFunctionNames.check_root(&[decl("rgba(0, 0, 0, 0.5)")], &scss_ctx());
    assert!(d.is_empty());
  }

  /// `source` after `gale --fix` with only this rule on.
  fn fixed(source: &str) -> String {
    crate::testing::fix(
      "scss/no-global-function-names",
      serde_json::json!(true),
      source,
      Syntax::Scss,
    )
  }

  #[test]
  fn fix_renames_the_call_and_loads_the_module() {
    assert_eq!(
      fixed("a {\n  b: map-get($m, a);\n}\n"),
      "@use \"sass:map\";\n\na {\n  b: map.get($m, a);\n}\n"
    );
  }

  #[test]
  fn fix_loads_each_module_once_in_the_order_they_appear() {
    assert_eq!(
      fixed("a {\n  b: str-length(\"x\");\n  c: map-get($m, a);\n  d: map-keys($m);\n}\n"),
      "@use \"sass:string\";\n@use \"sass:map\";\n\n\
       a {\n  b: string.length(\"x\");\n  c: map.get($m, a);\n  d: map.keys($m);\n}\n"
    );
  }

  #[test]
  fn fix_adds_the_use_after_the_last_load_rule() {
    assert_eq!(
      fixed("@use \"config\";\n@forward \"tokens\";\n\na {\n  b: nth($l, 1);\n}\n"),
      "@use \"config\";\n@forward \"tokens\";\n@use \"sass:list\";\n\na {\n  b: list.nth($l, 1);\n}\n"
    );
  }

  #[test]
  fn fix_adds_the_use_after_the_charset() {
    assert_eq!(
      fixed("@charset \"UTF-8\";\n\na {\n  b: unquote(\"x\");\n}\n"),
      "@charset \"UTF-8\";\n@use \"sass:string\";\n\na {\n  b: string.unquote(\"x\");\n}\n"
    );
  }

  #[test]
  fn fix_uses_the_namespace_the_file_gave_the_module() {
    assert_eq!(
      fixed("@use \"sass:map\" as m;\n\na {\n  b: map-get($m, a);\n}\n"),
      "@use \"sass:map\" as m;\n\na {\n  b: m.get($m, a);\n}\n"
    );
    assert_eq!(
      fixed("@use \"sass:math\";\n\na {\n  b: percentage(0.5);\n}\n"),
      "@use \"sass:math\";\n\na {\n  b: math.percentage(0.5);\n}\n"
    );
  }

  #[test]
  fn fix_leaves_what_a_rename_cannot_fix() {
    for source in [
      // The replacement takes different arguments.
      "a {\n  color: darken($c, 10%);\n}\n",
      // The module's members are global again.
      "@use \"sass:map\" as *;\n\na {\n  b: map-get($m, a);\n}\n",
      // Another module already takes the `map` namespace.
      "@use \"src/map\";\n\na {\n  b: map-get($m, a);\n}\n",
    ] {
      assert_eq!(fixed(source), source);
    }
  }

  #[test]
  fn fix_reaches_declarations_in_nested_rules_and_at_rules() {
    assert_eq!(
      fixed(
        "a {\n  b {\n    c: map-get($m, a);\n  }\n  @media print {\n    d: map-keys($m);\n  }\n}\n"
      ),
      "@use \"sass:map\";\n\na {\n  b {\n    c: map.get($m, a);\n  }\n  @media print {\n    d: map.keys($m);\n  }\n}\n"
    );
  }

  #[test]
  fn reports_without_a_fix_in_the_indented_syntax() {
    let diags = crate::testing::lint(
      "scss/no-global-function-names",
      serde_json::json!(true),
      "a\n  b: map-get($m, a)\n",
      Syntax::Sass,
    );
    assert_eq!(diags.len(), 1);
    assert!(diags[0].fix.is_none());
  }

  #[test]
  fn darken_message_matches_stylelint() {
    let d = ScssNoGlobalFunctionNames.check_root(&[decl("darken($color, 10%)")], &scss_ctx());
    assert_eq!(d.len(), 1);
    assert_eq!(
      d[0].message,
      "Expected color.adjust($color, $lightness: -$amount) instead of darken($color, $amount)"
    );
  }
}
