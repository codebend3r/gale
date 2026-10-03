use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Edit, Fix, Severity, Span};

use crate::pattern::option_matches;
use crate::postcss_tree::{NodeKind, PostcssTree};
use crate::rule::{Rule, RuleContext};
use crate::standard_syntax::{is_standard_syntax_property, is_standard_syntax_value};

/// Disallow duplicate properties within declaration blocks.
///
/// Equivalent to Stylelint's `declaration-block-no-duplicate-properties`
/// rule, autofix included: the fix removes the declaration that loses (the
/// earlier one, or the later one when only the earlier is `!important`).
/// Secondary options:
///   - `ignore`: `consecutive-duplicates`,
///     `consecutive-duplicates-with-different-values`,
///     `consecutive-duplicates-with-different-syntaxes`,
///     `consecutive-duplicates-with-same-prefixless-values`
///   - `ignoreProperties`: names or `/regex/` entries
///
/// Each block (style rule, at-rule or the stylesheet itself) is checked on
/// its own, as Stylelint's `eachDeclarationBlock` does, over the
/// [`PostcssTree`] of the source so values compare as PostCSS reads them.
/// Custom properties, `src`, and SCSS/Less variables and interpolated
/// properties are never duplicates.
pub struct DeclarationBlockNoDuplicateProperties;

/// The `ignore` flags that change how consecutive duplicates are treated.
#[derive(Debug, Clone, Copy, Default)]
struct Ignore {
  /// `consecutive-duplicates`.
  consecutive: bool,
  /// `consecutive-duplicates-with-different-values`.
  different_values: bool,
  /// `consecutive-duplicates-with-different-syntaxes`.
  different_syntaxes: bool,
  /// `consecutive-duplicates-with-same-prefixless-values`.
  same_prefixless_values: bool,
}

impl Rule for DeclarationBlockNoDuplicateProperties {
  fn name(&self) -> &'static str {
    "declaration-block-no-duplicate-properties"
  }

  fn description(&self) -> &'static str {
    "Disallow duplicate properties within declaration blocks"
  }

  fn default_severity(&self) -> Severity {
    Severity::Warning
  }

  /// Checks the declarations of every block in the document.
  fn check_root(&self, _nodes: &[CssNode], ctx: &RuleContext) -> Vec<Diagnostic> {
    let secondary = ctx.secondary_options();
    let ignore_option = secondary.and_then(|s| s.get("ignore"));
    let ignore = Ignore {
      consecutive: option_matches(ignore_option, "consecutive-duplicates"),
      different_values: option_matches(
        ignore_option,
        "consecutive-duplicates-with-different-values",
      ),
      different_syntaxes: option_matches(
        ignore_option,
        "consecutive-duplicates-with-different-syntaxes",
      ),
      same_prefixless_values: option_matches(
        ignore_option,
        "consecutive-duplicates-with-same-prefixless-values",
      ),
    };
    let ignore_properties = secondary.and_then(|s| s.get("ignoreProperties"));

    let tree = ctx.postcss_tree();
    let mut checker = Checker {
      rule: self,
      tree: &tree,
      ignore,
      ignore_properties,
      diags: Vec::new(),
    };
    checker.check_block(&tree.root);
    for i in 0..tree.nodes.len() {
      let node = &tree.nodes[i];
      if matches!(node.kind, NodeKind::Rule | NodeKind::AtRule)
        && let Some(children) = &node.children
        && is_walked(&tree, i)
      {
        checker.check_block(children);
      }
    }
    checker.diags
  }
}

/// Whether `eachDeclarationBlock` reaches the block of node `i`: it only
/// descends through the stylesheet, rules and at-rules, so a block inside an
/// SCSS nested property (`font: { ... }`) is never visited.
fn is_walked(tree: &PostcssTree, i: usize) -> bool {
  let mut parent = tree.nodes[i].parent;
  while let Some(p) = parent {
    if tree.nodes[p].kind == NodeKind::Decl {
      return false;
    }
    parent = tree.nodes[p].parent;
  }
  true
}

/// The state of one run of the rule over a tree.
struct Checker<'a, 't> {
  rule: &'a DeclarationBlockNoDuplicateProperties,
  tree: &'a PostcssTree<'t>,
  ignore: Ignore,
  ignore_properties: Option<&'a serde_json::Value>,
  diags: Vec<Diagnostic>,
}

impl Checker<'_, '_> {
  /// Stylelint's loop over the declarations among `children`.
  fn check_block(&mut self, children: &[usize]) {
    let tree = self.tree;
    // The "active" declaration for each property seen so far.
    let mut active: Vec<usize> = Vec::new();
    let mut previous_prop = String::new();
    for &decl in children {
      let node = &tree.nodes[decl];
      if node.kind != NodeKind::Decl {
        continue;
      }
      let prop = node.name.as_str();
      let lower_prop = prop.to_ascii_lowercase();
      if !is_standard_syntax_property(prop)
        || prop.starts_with("--")
        || option_matches(self.ignore_properties, prop)
        || lower_prop == "src"
      {
        continue;
      }

      let index = active
        .iter()
        .position(|&d| tree.nodes[d].name.eq_ignore_ascii_case(&lower_prop));
      let consecutive = previous_prop == lower_prop;
      previous_prop = lower_prop;
      let Some(index) = index else {
        active.push(decl);
        continue;
      };
      let duplicate = active[index];

      let value = node.value.as_str();
      let duplicate_value = tree.nodes[duplicate].value.as_str();
      let duplicate_is_more_important = !node.important && tree.nodes[duplicate].important;
      let Ignore {
        consecutive: ignore_consecutive,
        different_values,
        different_syntaxes,
        same_prefixless_values,
      } = self.ignore;

      if different_values || different_syntaxes || same_prefixless_values {
        if !consecutive
          || (same_prefixless_values && unprefixed(value) != unprefixed(duplicate_value))
        {
          self.fix_or_report(&mut active, index, decl, duplicate_is_more_important);
          continue;
        }
        if different_syntaxes && is_equal_value_syntaxes(value, duplicate_value, prop) {
          self.fix_or_report(&mut active, index, decl, duplicate_is_more_important);
          continue;
        }
        if value != duplicate_value {
          continue;
        }
        // The current declaration becomes the active one either way.
        active[index] = decl;
        self.report(duplicate);
        continue;
      }

      if ignore_consecutive && consecutive {
        continue;
      }
      self.fix_or_report(&mut active, index, decl, duplicate_is_more_important);
    }
  }

  /// Stylelint's `fixOrReport`: report (and remove) the declaration that
  /// loses, keeping the current one active unless the earlier one is more
  /// important.
  fn fix_or_report(
    &mut self,
    active: &mut [usize],
    index: usize,
    decl: usize,
    duplicate_is_more_important: bool,
  ) {
    if duplicate_is_more_important {
      self.report(decl);
    } else {
      let duplicate = active[index];
      active[index] = decl;
      self.report(duplicate);
    }
  }

  /// Report the declaration `node` at its property, with the fix that
  /// removes it.
  fn report(&mut self, node: usize) {
    let tree = self.tree;
    let n = &tree.nodes[node];
    let edits = tree
      .removal_ranges(node)
      .into_iter()
      .map(|range| Edit::new(Span::from_range(range.start, range.end), String::new()))
      .collect();
    self.diags.push(
      Diagnostic::new(
        self.rule.name(),
        format!("Unexpected duplicate \"{}\"", n.name),
      )
      .severity(self.rule.default_severity())
      .span(Span::new(n.start, n.name.len().min(n.end - n.start)))
      .fix(Fix::new("Remove the duplicate declaration", edits)),
    );
  }
}

/// PostCSS's `vendor.unprefixed`: `value` without a leading `-\w+-`.
fn unprefixed(value: &str) -> &str {
  let Some(rest) = value.strip_prefix('-') else {
    return value;
  };
  let word = rest
    .bytes()
    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
    .count();
  // `\w+` backtracks to the last `-` it can end on, but a word never
  // contains `-`, so the prefix ends at the first one.
  match rest.as_bytes().get(word) {
    Some(b'-') if word > 0 => &rest[word + 1..],
    _ => value,
  }
}

/// Stylelint's `isEqualValueSyntaxes`: the same text, or both standard and
/// parsed by css-tree into the same shape (see [`is_equal_value_nodes`]).
fn is_equal_value_syntaxes(value1: &str, value2: &str, property: &str) -> bool {
  if value1 == value2 {
    return true;
  }
  if !(is_standard_syntax_value(value1) && is_standard_syntax_value(value2)) {
    return false;
  }
  let (Some(nodes1), Some(nodes2)) = (value_syntax::parse(value1), value_syntax::parse(value2))
  else {
    return false;
  };
  is_equal_value_nodes(&nodes1, &nodes2, property)
}

/// Stylelint's `isEqualValueNodes`: the same node types in the same order,
/// with the same identifier and function names (ignoring case), the same
/// units, and the same shape inside functions and brackets.  Two custom
/// property names count as equal, as do two named colors in a color
/// property.
fn is_equal_value_nodes(
  nodes1: &[value_syntax::Node],
  nodes2: &[value_syntax::Node],
  property: &str,
) -> bool {
  use value_syntax::Node;
  if nodes1.len() != nodes2.len() {
    return false;
  }
  for (node1, node2) in nodes1.iter().zip(nodes2) {
    if std::mem::discriminant(node1) != std::mem::discriminant(node2) {
      return false;
    }
    if let (Node::Identifier(name1), Node::Identifier(name2)) = (node1, node2) {
      if name1.starts_with("--") && name2.starts_with("--") {
        continue;
      }
      if is_color_property(&property.to_ascii_lowercase())
        && is_named_color(&name1.to_ascii_lowercase())
        && is_named_color(&name2.to_ascii_lowercase())
      {
        continue;
      }
    }
    if !node1.name().eq_ignore_ascii_case(node2.name()) || node1.unit() != node2.unit() {
      return false;
    }
    if let (Some(children1), Some(children2)) = (node1.children(), node2.children())
      && !is_equal_value_nodes(children1, children2, property)
    {
      return false;
    }
  }
  true
}

/// The value syntax tree css-tree builds for `parse(value, { context:
/// 'value' })`, reduced to what [`is_equal_value_nodes`] compares.
mod value_syntax {
  /// One node of a parsed value.
  #[derive(Debug, Clone, PartialEq)]
  pub enum Node {
    /// `ident`, with its name.
    Identifier(String),
    /// `name(...)`, with its name and arguments.
    Function(String, Vec<Node>),
    /// A number with a unit, with the unit as written.
    Dimension(String),
    /// A plain number.
    Number,
    /// `n%`.
    Percentage,
    /// A quoted string.
    String,
    /// `url(...)`.
    Url,
    /// `#hash`.
    Hash,
    /// `U+...`.
    UnicodeRange,
    /// `,`, `/`, `*`, `+` or `-`; css-tree keeps no name for these, so all
    /// operators look alike.
    Operator,
    /// `( ... )`.
    Parentheses(Vec<Node>),
    /// `[ ... ]`.
    Brackets(Vec<Node>),
    /// The unparsed fallback of `var(--x, ...)`.
    Raw,
  }

  impl Node {
    /// css-tree's `name`: identifiers and functions have one.
    pub fn name(&self) -> &str {
      match self {
        Node::Identifier(name) | Node::Function(name, _) => name,
        _ => "",
      }
    }

    /// css-tree's `unit`: dimensions have one.
    pub fn unit(&self) -> &str {
      match self {
        Node::Dimension(unit) => unit,
        _ => "",
      }
    }

    /// css-tree's `children`, for functions, parentheses and brackets.
    pub fn children(&self) -> Option<&[Node]> {
      match self {
        Node::Function(_, children) | Node::Parentheses(children) | Node::Brackets(children) => {
          Some(children)
        }
        _ => None,
      }
    }
  }

  /// Parse `value`, or `None` where css-tree would throw.
  pub fn parse(value: &str) -> Option<Vec<Node>> {
    let mut parser = Parser {
      chars: value.chars().collect(),
      pos: 0,
    };
    let nodes = parser.sequence()?;
    (parser.pos >= parser.chars.len()).then_some(nodes)
  }

  /// A cursor over the value's characters.
  struct Parser {
    chars: Vec<char>,
    pos: usize,
  }

  impl Parser {
    /// The character `offset` places ahead, or `'\0'` past the end.
    fn peek(&self, offset: usize) -> char {
      self.chars.get(self.pos + offset).copied().unwrap_or('\0')
    }

    /// Skip whitespace and comments.
    fn skip_space_and_comments(&mut self) {
      loop {
        let c = self.peek(0);
        if c.is_whitespace() && c != '\0' {
          self.pos += 1;
        } else if c == '/' && self.peek(1) == '*' {
          self.pos += 2;
          while self.pos < self.chars.len() && !(self.peek(0) == '*' && self.peek(1) == '/') {
            self.pos += 1;
          }
          self.pos = (self.pos + 2).min(self.chars.len());
        } else {
          return;
        }
      }
    }

    /// css-tree's `readSequence`: nodes up to the end, a `)` or a `]`.
    /// `None` on a token the value scope does not recognise.
    fn sequence(&mut self) -> Option<Vec<Node>> {
      let mut nodes = Vec::new();
      loop {
        self.skip_space_and_comments();
        if self.pos >= self.chars.len() || matches!(self.peek(0), ')' | ']') {
          return Some(nodes);
        }
        nodes.push(self.node()?);
      }
    }

    /// Read the node at the cursor.
    fn node(&mut self) -> Option<Node> {
      let c = self.peek(0);
      match c {
        ',' => {
          self.pos += 1;
          Some(Node::Operator)
        }
        '(' => {
          self.pos += 1;
          let children = self.sequence()?;
          self.expect(')')?;
          Some(Node::Parentheses(children))
        }
        '[' => {
          self.pos += 1;
          let children = self.sequence()?;
          self.expect(']')?;
          Some(Node::Brackets(children))
        }
        '"' | '\'' => {
          self.pos += 1;
          while self.pos < self.chars.len() && self.peek(0) != c {
            if self.peek(0) == '\\' {
              self.pos += 1;
            }
            self.pos += 1;
          }
          self.pos = (self.pos + 1).min(self.chars.len());
          Some(Node::String)
        }
        '#' if self.starts_ident_char(1) || self.peek(1).is_ascii_digit() => {
          self.pos += 1;
          self.ident_chars();
          Some(Node::Hash)
        }
        _ if self.starts_number() => {
          self.number();
          if self.peek(0) == '%' {
            self.pos += 1;
            Some(Node::Percentage)
          } else if self.starts_ident(0) {
            let unit = self.ident_chars();
            Some(Node::Dimension(unit))
          } else {
            Some(Node::Number)
          }
        }
        _ if self.starts_ident(0) => {
          let name = self.ident_chars();
          if self.peek(0) != '(' {
            if name.len() >= 2 && name[..2].eq_ignore_ascii_case("u+") {
              return Some(Node::UnicodeRange);
            }
            return Some(Node::Identifier(name));
          }
          self.pos += 1;
          if name.eq_ignore_ascii_case("url") {
            self.skip_to_close();
            return Some(Node::Url);
          }
          if name.eq_ignore_ascii_case("var") {
            return self.var_arguments(name);
          }
          let children = self.sequence()?;
          self.expect(')')?;
          Some(Node::Function(name, children))
        }
        '/' | '*' | '+' | '-' => {
          self.pos += 1;
          Some(Node::Operator)
        }
        _ => None,
      }
    }

    /// css-tree's `var()`: the custom property name, then a comma and the
    /// fallback as one raw node.
    fn var_arguments(&mut self, name: String) -> Option<Node> {
      self.skip_space_and_comments();
      if !self.starts_ident(0) {
        return None;
      }
      let mut children = vec![Node::Identifier(self.ident_chars())];
      self.skip_space_and_comments();
      if self.peek(0) == ',' {
        self.pos += 1;
        children.push(Node::Operator);
        let mut depth = 0usize;
        while self.pos < self.chars.len() {
          match self.peek(0) {
            '(' => depth += 1,
            ')' if depth == 0 => break,
            ')' => depth -= 1,
            _ => {}
          }
          self.pos += 1;
        }
        children.push(Node::Raw);
      }
      self.expect(')')?;
      Some(Node::Function(name, children))
    }

    /// Consume `close`, or fail.
    fn expect(&mut self, close: char) -> Option<()> {
      self.skip_space_and_comments();
      (self.peek(0) == close).then(|| self.pos += 1)
    }

    /// Skip to just past the `)` that closes the current function.
    fn skip_to_close(&mut self) {
      while self.pos < self.chars.len() && self.peek(0) != ')' {
        self.pos += 1;
      }
      self.pos = (self.pos + 1).min(self.chars.len());
    }

    /// Whether the character `offset` ahead can start an identifier body.
    fn starts_ident_char(&self, offset: usize) -> bool {
      let c = self.peek(offset);
      c.is_ascii_alphabetic() || c == '_' || c == '-' || !c.is_ascii() || c == '\\'
    }

    /// CSS's "would start an identifier" at `offset` ahead.
    fn starts_ident(&self, offset: usize) -> bool {
      let c = self.peek(offset);
      if c == '-' {
        let next = self.peek(offset + 1);
        return next == '-'
          || next.is_ascii_alphabetic()
          || next == '_'
          || (!next.is_ascii() && next != '\0')
          || (next == '\\' && self.peek(offset + 2) != '\n');
      }
      c.is_ascii_alphabetic() || c == '_' || (!c.is_ascii() && c != '\0') || c == '\\'
    }

    /// CSS's "would start a number" at the cursor.
    fn starts_number(&self) -> bool {
      let digit_or_dot = |offset: usize| {
        let c = self.peek(offset);
        c.is_ascii_digit() || (c == '.' && self.peek(offset + 1).is_ascii_digit())
      };
      match self.peek(0) {
        '+' | '-' => digit_or_dot(1),
        _ => digit_or_dot(0),
      }
    }

    /// Consume a number: sign, digits, fraction and exponent.
    fn number(&mut self) {
      if matches!(self.peek(0), '+' | '-') {
        self.pos += 1;
      }
      while self.peek(0).is_ascii_digit() {
        self.pos += 1;
      }
      if self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
        self.pos += 1;
        while self.peek(0).is_ascii_digit() {
          self.pos += 1;
        }
      }
      if matches!(self.peek(0), 'e' | 'E') {
        let sign = usize::from(matches!(self.peek(1), '+' | '-'));
        if self.peek(1 + sign).is_ascii_digit() {
          self.pos += 1 + sign;
          while self.peek(0).is_ascii_digit() {
            self.pos += 1;
          }
        }
      }
    }

    /// Consume identifier characters (and escapes) and return them.
    fn ident_chars(&mut self) -> String {
      let start = self.pos;
      while self.pos < self.chars.len() {
        let c = self.peek(0);
        if c == '\\' {
          self.pos = (self.pos + 2).min(self.chars.len());
        } else if c.is_ascii_alphanumeric() || c == '_' || c == '-' || !c.is_ascii() {
          self.pos += 1;
        } else {
          break;
        }
      }
      self.chars[start..self.pos].iter().collect()
    }
  }
}

/// Stylelint's `colorProperties`.
fn is_color_property(prop: &str) -> bool {
  matches!(
    prop,
    "accent-color"
      | "background-color"
      | "border-block-color"
      | "border-block-end-color"
      | "border-block-start-color"
      | "border-bottom-color"
      | "border-inline-color"
      | "border-inline-end-color"
      | "border-inline-start-color"
      | "border-left-color"
      | "border-right-color"
      | "border-top-color"
      | "caret-color"
      | "color"
      | "column-rule-color"
      | "outline-color"
      | "text-decoration-color"
      | "text-emphasis-color"
      | "flood-color"
      | "lighting-color"
      | "stop-color"
      | "border-color"
      | "scrollbar-color"
  )
}

/// Stylelint's `namedColorsKeywords`: the CSS Color 4 named colors.
fn is_named_color(name: &str) -> bool {
  NAMED_COLORS.binary_search(&name).is_ok()
}

/// The CSS Color 4 named colors, sorted.
const NAMED_COLORS: &[&str] = &[
  "aliceblue",
  "antiquewhite",
  "aqua",
  "aquamarine",
  "azure",
  "beige",
  "bisque",
  "black",
  "blanchedalmond",
  "blue",
  "blueviolet",
  "brown",
  "burlywood",
  "cadetblue",
  "chartreuse",
  "chocolate",
  "coral",
  "cornflowerblue",
  "cornsilk",
  "crimson",
  "cyan",
  "darkblue",
  "darkcyan",
  "darkgoldenrod",
  "darkgray",
  "darkgreen",
  "darkgrey",
  "darkkhaki",
  "darkmagenta",
  "darkolivegreen",
  "darkorange",
  "darkorchid",
  "darkred",
  "darksalmon",
  "darkseagreen",
  "darkslateblue",
  "darkslategray",
  "darkslategrey",
  "darkturquoise",
  "darkviolet",
  "deeppink",
  "deepskyblue",
  "dimgray",
  "dimgrey",
  "dodgerblue",
  "firebrick",
  "floralwhite",
  "forestgreen",
  "fuchsia",
  "gainsboro",
  "ghostwhite",
  "gold",
  "goldenrod",
  "gray",
  "green",
  "greenyellow",
  "grey",
  "honeydew",
  "hotpink",
  "indianred",
  "indigo",
  "ivory",
  "khaki",
  "lavender",
  "lavenderblush",
  "lawngreen",
  "lemonchiffon",
  "lightblue",
  "lightcoral",
  "lightcyan",
  "lightgoldenrodyellow",
  "lightgray",
  "lightgreen",
  "lightgrey",
  "lightpink",
  "lightsalmon",
  "lightseagreen",
  "lightskyblue",
  "lightslategray",
  "lightslategrey",
  "lightsteelblue",
  "lightyellow",
  "lime",
  "limegreen",
  "linen",
  "magenta",
  "maroon",
  "mediumaquamarine",
  "mediumblue",
  "mediumorchid",
  "mediumpurple",
  "mediumseagreen",
  "mediumslateblue",
  "mediumspringgreen",
  "mediumturquoise",
  "mediumvioletred",
  "midnightblue",
  "mintcream",
  "mistyrose",
  "moccasin",
  "navajowhite",
  "navy",
  "oldlace",
  "olive",
  "olivedrab",
  "orange",
  "orangered",
  "orchid",
  "palegoldenrod",
  "palegreen",
  "paleturquoise",
  "palevioletred",
  "papayawhip",
  "peachpuff",
  "peru",
  "pink",
  "plum",
  "powderblue",
  "purple",
  "rebeccapurple",
  "red",
  "rosybrown",
  "royalblue",
  "saddlebrown",
  "salmon",
  "sandybrown",
  "seagreen",
  "seashell",
  "sienna",
  "silver",
  "skyblue",
  "slateblue",
  "slategray",
  "slategrey",
  "snow",
  "springgreen",
  "steelblue",
  "tan",
  "teal",
  "thistle",
  "tomato",
  "turquoise",
  "violet",
  "wheat",
  "white",
  "whitesmoke",
  "yellow",
  "yellowgreen",
];

#[cfg(test)]
mod tests {
  use super::*;
  use gale_css_parser::Syntax;

  use crate::testing::{context, fix};

  const RULE: &str = "declaration-block-no-duplicate-properties";

  /// The offsets reported for `source` in `syntax` with `options`.
  fn offsets(source: &str, syntax: Syntax, options: serde_json::Value) -> Vec<usize> {
    let ctx = context(source, syntax, &options);
    DeclarationBlockNoDuplicateProperties
      .check_root(&[], &ctx)
      .into_iter()
      .map(|d| d.span.offset)
      .collect()
  }

  #[test]
  fn reports_the_earlier_duplicate_in_each_block() {
    let on = serde_json::json!(true);
    assert_eq!(
      offsets("a { color: pink; color: orange }", Syntax::Css, on.clone()),
      vec![4]
    );
    assert_eq!(
      offsets(
        "a { COlOr: pink; coLOR: pink; color: pink }",
        Syntax::Css,
        on.clone()
      ),
      vec![4, 17]
    );
    assert!(
      offsets(
        "a { color: pink; @media { color: orange; } }",
        Syntax::Css,
        on.clone()
      )
      .is_empty()
    );
    assert!(offsets("@font-face { src: a; SRC: b }", Syntax::Css, on.clone()).is_empty());
    assert!(
      offsets(
        "a { $s: 0; $s: 1; --c: 0; --c: 1; }",
        Syntax::Scss,
        on.clone()
      )
      .is_empty()
    );
    assert!(offsets("a { @l: 0; @l: 1; }", Syntax::Less, on).is_empty());
  }

  #[test]
  fn an_important_duplicate_wins() {
    let on = serde_json::json!(true);
    assert_eq!(
      offsets(
        "a { color: red !important; color: blue; }",
        Syntax::Css,
        on.clone()
      ),
      vec![27]
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { color: red !important; color: blue; }",
        Syntax::Css
      ),
      "a { color: red !important; }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { color: red !important; color: blue }",
        Syntax::Css
      ),
      "a { color: red !important }"
    );
    assert_eq!(
      fix(
        RULE,
        on,
        "a { color: red ! IMPORTANT; color: blue; }",
        Syntax::Css
      ),
      "a { color: red ! IMPORTANT; }"
    );
  }

  #[test]
  fn fix_removes_the_losing_declarations() {
    let on = serde_json::json!(true);
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a { color: pink; color: pink; color: orange }",
        Syntax::Css
      ),
      "a { color: orange }"
    );
    assert_eq!(
      fix(
        RULE,
        on.clone(),
        "a {\n  color: pink;\n  /* c */\n  color: orange;\n}",
        Syntax::Css
      ),
      "a {\n  /* c */\n  color: orange;\n}"
    );
    assert_eq!(
      fix(
        RULE,
        on,
        "a { color: pink; @media { color: orange; color: black; } }",
        Syntax::Css
      ),
      "a { color: pink; @media { color: black; } }"
    );
  }

  #[test]
  fn consecutive_ignore_options() {
    let consecutive = serde_json::json!([true, { "ignore": ["consecutive-duplicates"] }]);
    assert!(
      offsets(
        "p { font-size: 16px; font-size: 1rem; }",
        Syntax::Css,
        consecutive.clone()
      )
      .is_empty()
    );
    assert_eq!(
      fix(
        RULE,
        consecutive,
        "p { font-size: 16px !important; font-weight: 400; font-size: 1rem; }",
        Syntax::Css
      ),
      "p { font-size: 16px !important; font-weight: 400; }"
    );
    let values =
      serde_json::json!([true, { "ignore": ["consecutive-duplicates-with-different-values"] }]);
    assert!(
      offsets(
        "p { font-size: 16px; font-size: 18px; }",
        Syntax::Css,
        values.clone()
      )
      .is_empty()
    );
    assert_eq!(
      fix(
        RULE,
        values,
        "p { font-size: 16px; font-size: 16px; font-weight: 400; }",
        Syntax::Css
      ),
      "p { font-size: 16px; font-weight: 400; }"
    );
    let prefixless = serde_json::json!([true, { "ignore": ["consecutive-duplicates-with-same-prefixless-values"] }]);
    assert!(
      offsets(
        "p { width: fit-content; width: -moz-fit-content; }",
        Syntax::Css,
        prefixless.clone()
      )
      .is_empty()
    );
    assert_eq!(
      fix(
        RULE,
        prefixless,
        "p { width: 100%; width: -moz-fit-content; height: 32px; }",
        Syntax::Css
      ),
      "p { width: -moz-fit-content; height: 32px; }"
    );
  }

  #[test]
  fn different_syntaxes_compare_value_shapes() {
    assert!(is_equal_value_syntaxes("100vw", "50vw", "width"));
    assert!(!is_equal_value_syntaxes("100vw", "100dvw", "width"));
    assert!(!is_equal_value_syntaxes("100%", "fit-content", "width"));
    assert!(!is_equal_value_syntaxes(
      "min(10px, 11px)",
      "max(10px, 11px)",
      "width"
    ));
    assert!(is_equal_value_syntaxes(
      "CaLC(10px + 4rem)",
      "calc(10px + 2rem)",
      "width"
    ));
    assert!(is_equal_value_syntaxes(
      "calc(100vw   + 10vw)",
      "calc(100vw + 10vw)",
      "width"
    ));
    assert!(!is_equal_value_syntaxes(
      "calc((10px + 2px))",
      "calc((10rem + 2rem))",
      "width"
    ));
    assert!(is_equal_value_syntaxes("var(--foo)", "var(--bar)", "width"));
    assert!(!is_equal_value_syntaxes("env(foo)", "env(--bar)", "width"));
    assert!(is_equal_value_syntaxes("red", "blue", "color"));
    assert!(!is_equal_value_syntaxes("red", "blue", "animation-name"));
    assert!(!is_equal_value_syntaxes("red", "transparent", "color"));
    assert!(!is_equal_value_syntaxes("_$a", "_$a2", "width"));
    assert!(!is_equal_value_syntaxes("$a", "calc(1 + $a)", "width"));
    let syntaxes =
      serde_json::json!([true, { "ignore": ["consecutive-duplicates-with-different-syntaxes"] }]);
    assert_eq!(
      fix(
        RULE,
        syntaxes.clone(),
        "p { width: calc(100vw /* a comment */  + 10vw); width: calc(100vw + 10vw); }",
        Syntax::Css
      ),
      "p { width: calc(100vw + 10vw); }"
    );
    assert!(
      offsets(
        "p { width: 100vw; width: 100dvw; height: 100vh; }",
        Syntax::Css,
        syntaxes
      )
      .is_empty()
    );
  }

  #[test]
  fn ignore_properties_takes_names_and_regexes() {
    let options = serde_json::json!([true, { "ignoreProperties": ["color", "/^back/"] }]);
    assert_eq!(
      offsets(
        "p { color: a; color: b; background: c; background: d; margin: 0; margin: 1; }",
        Syntax::Css,
        options
      ),
      vec![54]
    );
  }

  #[test]
  fn unprefixed_strips_one_vendor_prefix() {
    assert_eq!(unprefixed("-moz-fit-content"), "fit-content");
    assert_eq!(unprefixed("-webkit-box"), "box");
    assert_eq!(unprefixed("fit-content"), "fit-content");
    assert_eq!(unprefixed("--x"), "--x");
    assert_eq!(unprefixed("-1px"), "-1px");
  }
}
