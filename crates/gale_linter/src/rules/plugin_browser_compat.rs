use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Span};

use crate::lint_env::LintEnv;
use crate::options::{InvalidOption, Setting};
use crate::rule::{Check, RuleContext, declare_rule};

const NAME: &str = "plugin/browser-compat";

declare_rule! {
  /// Browser compatibility checker — Rust implementation of
  /// `stylelint-browser-compat` (`plugin/browser-compat`).
  ///
  /// Checks that CSS properties, pseudo-elements, and pseudo-classes used in
  /// the stylesheet are supported by the project's target browsers (as defined
  /// by a browserslist configuration).
  ///
  /// Browser support data is read at runtime from the
  /// `@mdn/browser-compat-data` npm package in `node_modules/`.
  pub struct PluginBrowserCompat => PluginBrowserCompatCheck {
    name: NAME,
    description: "Disallow CSS features unsupported by target browsers",
    default_severity: Warning,
  }
}

/// `plugin/browser-compat` as configured.
pub struct PluginBrowserCompatCheck {
  /// The options, or `None` when the rule is given no secondary options,
  /// in which case gale checks nothing.
  options: Option<BrowserCompatOptions>,
}

impl PluginBrowserCompatCheck {
  /// The plugin validates its secondary options against a schema (see
  /// [`is_valid_options`]) through `validateOptions` with a function as
  /// `possible`, and none of its primary option.
  fn from_setting(setting: &Setting, _env: &LintEnv) -> Result<Self, Vec<InvalidOption>> {
    let mut options = setting.options();
    match setting.secondary() {
      Some(serde_json::Value::Bool(false)) => {
        options.report(InvalidOption::disabled_with_false(NAME));
      }
      Some(secondary) if !secondary.is_null() && !is_valid_options(secondary) => {
        options.report(InvalidOption::rejected(NAME, secondary));
      }
      _ => {}
    }
    options.finish()?;
    Ok(Self {
      options: setting.secondary().and_then(parse_options),
    })
  }
}

/// The plugin's `ruleOptionsSchema`: an object whose `allow` is an object
/// of `features` (a list of strings) and the `flagged`,
/// `partialImplementation` and `prefix` flags, and whose `browserslist` is
/// a query or a list of them.  Other keys are left alone.
fn is_valid_options(value: &serde_json::Value) -> bool {
  let Some(object) = value.as_object() else {
    return false;
  };
  let is_string_list = |value: &serde_json::Value| {
    value
      .as_array()
      .is_some_and(|items| items.iter().all(serde_json::Value::is_string))
  };
  let allow_ok = match object.get("allow") {
    None => true,
    Some(allow) => allow.as_object().is_some_and(|allow| {
      allow.get("features").is_none_or(is_string_list)
        && ["flagged", "partialImplementation", "prefix"]
          .iter()
          .all(|key| allow.get(*key).is_none_or(serde_json::Value::is_boolean))
    }),
  };
  let browserslist_ok = object
    .get("browserslist")
    .is_none_or(|list| list.is_string() || is_string_list(list));
  allow_ok && browserslist_ok
}

// ── Rule implementation ─────────────────────────────────────────────────

impl Check for PluginBrowserCompatCheck {
  /// Unused: the work happens in `check_root`, which needs whole-file context.
  fn check(&self, _node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
    vec![]
  }

  /// Resolves the browserslist targets and MDN compat data, then flags properties
  /// and selectors the targets do not support. Missing data disables the rule.
  fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic> {
    let Some(opts) = &self.options else {
      return vec![];
    };

    let file_path = Path::new(context.file_path);
    let node_modules = find_node_modules(file_path);
    let nm = match node_modules {
      Some(ref p) => p,
      None => return vec![],
    };

    let compat_data = load_mdn_compat_data(nm);
    if compat_data.is_empty() {
      return vec![];
    }

    let targets = match resolve_browserslist(&opts.browserslist, file_path) {
      Some(t) if !t.is_empty() => t,
      _ => {
        // Fallback: try resolving the project's own browserslist config
        // (from package.json, .browserslistrc, etc.) without an explicit query.
        match resolve_browserslist_default(file_path) {
          Some(t) if !t.is_empty() => t,
          _ => return vec![],
        }
      }
    };

    let mut diagnostics = Vec::new();
    collect_from_nodes(
      nodes,
      context,
      opts,
      &compat_data,
      &targets,
      &mut diagnostics,
    );
    diagnostics
  }
}

// ── Options ─────────────────────────────────────────────────────────────

struct BrowserCompatOptions {
  browserslist: Vec<String>,
  allow_features: HashSet<String>,
  allow_prefix: bool,
  allow_flagged: bool,
  allow_partial_implementation: bool,
}

/// Reads `browserslist`, `allow` and the flag/partial tolerances.
fn parse_options(opts_val: &serde_json::Value) -> Option<BrowserCompatOptions> {
  let obj = opts_val.as_object()?;

  let browserslist: Vec<String> = match obj.get("browserslist") {
    Some(serde_json::Value::String(query)) => vec![query.clone()],
    Some(serde_json::Value::Array(arr)) => arr
      .iter()
      .filter_map(|v| v.as_str().map(|s| s.to_string()))
      .collect(),
    _ => Vec::new(),
  };

  let allow = obj.get("allow").and_then(|v| v.as_object());

  let allow_features: HashSet<String> = allow
    .and_then(|a| a.get("features"))
    .and_then(|v| v.as_array())
    .map(|arr| {
      arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect()
    })
    .unwrap_or_default();

  let allow_prefix = allow
    .and_then(|a| a.get("prefix"))
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

  let allow_flagged = allow
    .and_then(|a| a.get("flagged"))
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

  let allow_partial_implementation = allow
    .and_then(|a| a.get("partialImplementation"))
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

  Some(BrowserCompatOptions {
    browserslist,
    allow_features,
    allow_prefix,
    allow_flagged,
    allow_partial_implementation,
  })
}

// ── MDN compat data loading ─────────────────────────────────────────────

type CompatData = HashMap<String, CompatEntry>;

#[derive(Clone)]
struct CompatEntry {
  mdn_url: Option<String>,
  support: HashMap<String, Vec<SupportStatement>>,
}

#[derive(Clone)]
struct SupportStatement {
  version_added: VersionAdded,
  version_removed: Option<String>,
  prefix: Option<String>,
  alternative_name: Option<String>,
  flags: bool,
  partial_implementation: bool,
}

#[derive(Clone)]
enum VersionAdded {
  Bool(bool),
  Version(String),
}

/// Global cache: MDN data keyed by node_modules path. Entries are shared via
/// `Arc` so each file borrows the table instead of deep-cloning it.
static MDN_CACHE: OnceLock<std::sync::Mutex<HashMap<String, Arc<CompatData>>>> = OnceLock::new();

/// Process-wide cache of parsed MDN compat data, keyed by `data.json` path.
fn mdn_cache() -> &'static std::sync::Mutex<HashMap<String, Arc<CompatData>>> {
  MDN_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Loads and parses `@mdn/browser-compat-data`, memoised. The lock is held
/// across the parse so parallel workers do not each read the file.
fn load_mdn_compat_data(node_modules: &Path) -> Arc<CompatData> {
  let data_path = node_modules
    .join("@mdn")
    .join("browser-compat-data")
    .join("data.json");

  let key = data_path.to_string_lossy().to_string();

  // Hold the lock for the whole load: files are linted in parallel, and
  // without this every worker would parse data.json on the first miss.
  let mut cache = mdn_cache().lock().unwrap();
  if let Some(cached) = cache.get(&key) {
    return Arc::clone(cached);
  }

  let contents = match std::fs::read_to_string(&data_path) {
    Ok(c) => c,
    Err(_) => return Arc::new(CompatData::new()),
  };

  let parsed: serde_json::Value = match serde_json::from_str(&contents) {
    Ok(v) => v,
    Err(_) => return Arc::new(CompatData::new()),
  };

  let css = match parsed.get("css") {
    Some(c) => c,
    None => return Arc::new(CompatData::new()),
  };

  let mut data = CompatData::new();

  // Extract properties: css.properties.*
  if let Some(properties) = css.get("properties").and_then(|v| v.as_object()) {
    for (prop_name, prop_val) in properties {
      if let Some(entry) = extract_compat_entry(prop_val) {
        data.insert(format!("properties.{}", prop_name), entry);
      }
    }
  }

  // Extract selectors: css.selectors.*
  if let Some(selectors) = css.get("selectors").and_then(|v| v.as_object()) {
    for (sel_name, sel_val) in selectors {
      if let Some(entry) = extract_compat_entry(sel_val) {
        data.insert(format!("selectors.{}", sel_name), entry);
      }
    }
  }

  let data = Arc::new(data);
  cache.insert(key, Arc::clone(&data));
  data
}

/// Pulls the `__compat` block out of an MDN node, if it has one.
fn extract_compat_entry(val: &serde_json::Value) -> Option<CompatEntry> {
  let compat = val.get("__compat")?;
  let mdn_url = compat
    .get("mdn_url")
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());

  let support_obj = compat.get("support")?.as_object()?;
  let mut support = HashMap::new();

  for (browser, support_val) in support_obj {
    let statements = parse_support_statements(support_val);
    if !statements.is_empty() {
      support.insert(browser.clone(), statements);
    }
  }

  Some(CompatEntry { mdn_url, support })
}

/// Normalises MDN's single-object or array support value into a `Vec`.
fn parse_support_statements(val: &serde_json::Value) -> Vec<SupportStatement> {
  match val {
    serde_json::Value::Object(_) => {
      vec![parse_single_support(val)]
    }
    serde_json::Value::Array(arr) => arr.iter().map(parse_single_support).collect(),
    _ => vec![],
  }
}

/// Reads one support statement's added/removed versions, prefix and flags.
fn parse_single_support(val: &serde_json::Value) -> SupportStatement {
  let version_added = match val.get("version_added") {
    Some(serde_json::Value::Bool(b)) => VersionAdded::Bool(*b),
    Some(serde_json::Value::String(s)) => VersionAdded::Version(s.clone()),
    Some(serde_json::Value::Null) | None => VersionAdded::Bool(false),
    _ => VersionAdded::Bool(false),
  };

  let version_removed = val
    .get("version_removed")
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());

  let prefix = val
    .get("prefix")
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());

  let alternative_name = val
    .get("alternative_name")
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());

  let flags = val
    .get("flags")
    .and_then(|v| v.as_array())
    .map(|a| !a.is_empty())
    .unwrap_or(false);

  let partial_implementation = val
    .get("partial_implementation")
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

  SupportStatement {
    version_added,
    version_removed,
    prefix,
    alternative_name,
    flags,
    partial_implementation,
  }
}

// ── Browserslist resolution ─────────────────────────────────────────────

#[derive(Clone, Debug)]
struct BrowserTarget {
  /// The browserslist identifier (e.g. "chrome", "firefox").
  browserslist_id: String,
  /// The MDN identifier (e.g. "chrome", "firefox", "safari_ios").
  mdn_id: String,
  /// Human-readable name (e.g. "Chrome", "Firefox").
  display_name: String,
  /// Numeric version (e.g. 138.0).
  version: f64,
  /// Original version string from browserslist (e.g. "138", "17.0").
  version_string: String,
}

/// Standard mapping from browserslist names to MDN names.
fn browserslist_to_mdn() -> &'static [(&'static str, &'static str, &'static str)] {
  &[
    ("chrome", "chrome", "Chrome"),
    ("and_chr", "chrome", "Android Chrome"),
    ("edge", "edge", "Edge"),
    ("firefox", "firefox", "Firefox"),
    ("and_ff", "firefox_android", "Android Firefox"),
    ("ie", "ie", "IE"),
    ("opera", "opera", "Opera"),
    ("op_mob", "opera_android", "Opera Android"),
    ("safari", "safari", "Safari"),
    ("ios_saf", "safari_ios", "iOS Safari"),
    ("samsung", "samsunginternet_android", "Samsung Browser"),
    ("android", "webview_android", "Android Webview"),
  ]
}

/// Global cache: browserslist results keyed by query + working directory.
static BROWSERSLIST_CACHE: OnceLock<std::sync::Mutex<HashMap<String, Vec<BrowserTarget>>>> =
  OnceLock::new();

/// Process-wide cache of resolved browserslist targets, keyed by query.
fn browserslist_cache() -> &'static std::sync::Mutex<HashMap<String, Vec<BrowserTarget>>> {
  BROWSERSLIST_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Find the `browserslist` binary in the project's own node_modules/.bin.
/// Walks up from `start_dir` until it finds one, so it works for files
/// nested inside monorepo sub-packages.  Returns an absolute path so the
/// binary can be invoked with a different `current_dir`.
fn find_browserslist_bin(start_dir: &Path) -> Option<PathBuf> {
  // Canonicalize to an absolute path so that walking up works correctly
  // even when start_dir is relative (e.g. "src/forms").
  let start_abs = start_dir.canonicalize().ok().unwrap_or_else(|| {
    std::env::current_dir()
      .ok()
      .map(|cwd| cwd.join(start_dir))
      .unwrap_or_else(|| start_dir.to_path_buf())
  });
  let mut dir = start_abs;
  loop {
    let candidate = dir.join("node_modules").join(".bin").join("browserslist");
    if candidate.exists() {
      return Some(candidate);
    }
    if !dir.pop() {
      return None;
    }
  }
}

/// Run `browserslist [query]` using the project-local binary when available,
/// falling back to `npx browserslist`.  Using the local binary ensures that
/// the caniuse-lite database version matches the one Stylelint uses, which
/// is critical for byte-for-byte identical `plugin/browser-compat` output.
fn run_browserslist(query: Option<&str>, work_dir: &Path) -> Option<std::process::Output> {
  let local_bin = find_browserslist_bin(work_dir);
  let mut cmd = match local_bin {
    Some(ref bin) => std::process::Command::new(bin),
    None => {
      let mut c = std::process::Command::new("npx");
      c.arg("browserslist");
      c
    }
  };
  if let Some(q) = query {
    cmd.arg(q);
  }
  cmd.current_dir(work_dir).output().ok()
}

/// Resolves browserslist queries to concrete browser versions, memoised.
fn resolve_browserslist(queries: &[String], file_path: &Path) -> Option<Vec<BrowserTarget>> {
  if queries.is_empty() {
    return None;
  }

  let mut work_dir = if file_path.is_file() {
    file_path.parent()?.to_path_buf()
  } else {
    file_path.to_path_buf()
  };
  if work_dir.as_os_str().is_empty() {
    work_dir = PathBuf::from(".");
  }

  let query = queries.join(", ");
  // With an explicit query the answer depends only on which browserslist
  // (and caniuse-lite) binary runs, not on the file's directory, so key the
  // cache by the resolved binary. Keying by directory spawned one node
  // process per subdirectory in large repos.
  let bin_key = find_browserslist_bin(&work_dir)
    .map(|p| p.display().to_string())
    .unwrap_or_else(|| "npx".to_string());
  let cache_key = format!("{}|{}", bin_key, query);

  // Held across the subprocess call so parallel workers don't all spawn it.
  let mut cache = browserslist_cache().lock().unwrap();
  if let Some(cached) = cache.get(&cache_key) {
    return Some(cached.clone());
  }

  let output = run_browserslist(Some(&query), &work_dir)?;

  if !output.status.success() {
    return None;
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  let mapping = browserslist_to_mdn();

  let mut targets: Vec<BrowserTarget> = stdout
    .lines()
    .filter_map(|line| {
      let line = line.trim();
      let (name, version_str) = line.split_once(' ')?;

      // Skip Safari TP
      if name == "safari" && version_str == "TP" {
        return None;
      }

      let info = mapping.iter().find(|(bl, _, _)| *bl == name)?;

      // Handle version ranges like "18.5-18.6"
      let min_version_str = version_str.split('-').next()?;

      // Handle "all" version
      let version = if min_version_str == "all" {
        0.0
      } else {
        // Special case: op_mob 10 → 10.1
        if name == "op_mob" && min_version_str == "10" {
          10.1
        } else {
          min_version_str.parse::<f64>().ok()?
        }
      };

      Some(BrowserTarget {
        browserslist_id: name.to_string(),
        mdn_id: info.1.to_string(),
        display_name: info.2.to_string(),
        version,
        version_string: version_str.to_string(),
      })
    })
    .collect();

  // Sort descending by (browserslist_id desc, version desc)
  targets.sort_by(|a, b| {
    let name_cmp = b.browserslist_id.cmp(&a.browserslist_id);
    if name_cmp != std::cmp::Ordering::Equal {
      return name_cmp;
    }
    b.version
      .partial_cmp(&a.version)
      .unwrap_or(std::cmp::Ordering::Equal)
  });

  // Keep only the LAST (oldest) version per browser group.
  // This matches the JS plugin's deduplication behavior.
  let mut deduped: Vec<BrowserTarget> = Vec::new();
  for (i, target) in targets.iter().enumerate() {
    let is_last_of_group =
      i + 1 == targets.len() || targets[i + 1].browserslist_id != target.browserslist_id;
    if is_last_of_group {
      deduped.push(target.clone());
    }
  }

  cache.insert(cache_key, deduped.clone());
  Some(deduped)
}

/// Resolve the browserslist using the project's own config (no explicit query).
/// Runs `npx browserslist` in the project directory which uses `.browserslistrc`,
/// `package.json` browserslist field, or the project's node_modules.
fn resolve_browserslist_default(file_path: &Path) -> Option<Vec<BrowserTarget>> {
  let work_dir = if file_path.is_file() {
    file_path.parent()?.to_path_buf()
  } else {
    file_path.to_path_buf()
  };

  // Walk up to find project root (directory with package.json)
  let mut project_root = work_dir.clone();
  loop {
    if project_root.join("package.json").exists() {
      break;
    }
    if !project_root.pop() {
      // Couldn't find project root, use work_dir
      project_root = work_dir.clone();
      break;
    }
  }
  // Empty path (from walking up a relative path like "src") is not a valid
  // directory for Command::current_dir().  Use "." instead.
  if project_root.as_os_str().is_empty() {
    project_root = PathBuf::from(".");
  }

  let cache_key = format!("{}|__default__", project_root.display());

  // Held across the subprocess call so parallel workers don't all spawn it.
  let mut cache = browserslist_cache().lock().unwrap();
  if let Some(cached) = cache.get(&cache_key) {
    return Some(cached.clone());
  }

  let output = run_browserslist(None, &project_root)?;

  if !output.status.success() {
    return None;
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  let mapping = browserslist_to_mdn();

  let mut targets: Vec<BrowserTarget> = stdout
    .lines()
    .filter_map(|line| {
      let line = line.trim();
      if line.is_empty() {
        return None;
      }
      let (name, version_str) = line.split_once(' ')?;

      if name == "safari" && version_str == "TP" {
        return None;
      }

      let info = mapping.iter().find(|(bl, _, _)| *bl == name)?;
      let min_version_str = version_str.split('-').next()?;
      let version = if min_version_str == "all" {
        0.0
      } else if name == "op_mob" && min_version_str == "10" {
        10.1
      } else {
        min_version_str.parse::<f64>().ok()?
      };

      Some(BrowserTarget {
        browserslist_id: name.to_string(),
        mdn_id: info.1.to_string(),
        display_name: info.2.to_string(),
        version,
        version_string: version_str.to_string(),
      })
    })
    .collect();

  targets.sort_by(|a, b| {
    let name_cmp = b.browserslist_id.cmp(&a.browserslist_id);
    if name_cmp != std::cmp::Ordering::Equal {
      return name_cmp;
    }
    b.version
      .partial_cmp(&a.version)
      .unwrap_or(std::cmp::Ordering::Equal)
  });

  let mut deduped: Vec<BrowserTarget> = Vec::new();
  for (i, target) in targets.iter().enumerate() {
    let is_last_of_group =
      i + 1 == targets.len() || targets[i + 1].browserslist_id != target.browserslist_id;
    if is_last_of_group {
      deduped.push(target.clone());
    }
  }

  cache.insert(cache_key, deduped.clone());
  Some(deduped)
}

// ── Support checking ────────────────────────────────────────────────────

/// Whether `target` supports the feature. Features with no data count as
/// supported; flagged and partial implementations depend on the options.
fn is_supported(entry: &CompatEntry, target: &BrowserTarget, opts: &BrowserCompatOptions) -> bool {
  let support_list = match entry.support.get(&target.mdn_id) {
    Some(list) => list,
    None => return true, // No data → assume supported
  };

  for s in support_list {
    if s.alternative_name.is_some() {
      continue; // TODO: handle alternative names
    }
    if s.flags && !opts.allow_flagged {
      continue;
    }
    if s.partial_implementation && !opts.allow_partial_implementation {
      continue;
    }
    if s.prefix.is_some() && !opts.allow_prefix {
      continue;
    }

    match &s.version_added {
      VersionAdded::Bool(true) => return true,
      VersionAdded::Bool(false) => return false,
      VersionAdded::Version(v) => {
        if v.starts_with('\u{2264}') {
          // ≤ prefix
          return true;
        }
        if v == "preview" {
          return false;
        }
        // Compare versions: added_version <= target_version
        let added = parse_version_f64(v);
        if added <= target.version {
          // Check version_removed
          if let Some(ref removed_str) = s.version_removed {
            let removed = parse_version_f64(removed_str);
            if removed <= target.version {
              continue; // Removed before target version
            }
          }
          return true;
        }
      }
    }
  }
  false
}

/// Parses an MDN version string, taking the low end of a range.
fn parse_version_f64(v: &str) -> f64 {
  // Strip any ≤ prefix
  let v = v.trim_start_matches('\u{2264}');
  // Take first component of ranges
  let v = v.split('-').next().unwrap_or(v);
  v.parse::<f64>().unwrap_or(0.0)
}

// ── Feature collection and checking ─────────────────────────────────────

/// Runs [`collect_from_node`] over a node list.
fn collect_from_nodes(
  nodes: &[CssNode],
  ctx: &RuleContext,
  opts: &BrowserCompatOptions,
  compat_data: &CompatData,
  targets: &[BrowserTarget],
  diagnostics: &mut Vec<Diagnostic>,
) {
  for node in nodes {
    collect_from_node(node, ctx, opts, compat_data, targets, diagnostics);
  }
}

/// Checks one node's selectors and declarations, recursing into children.
fn collect_from_node(
  node: &CssNode,
  ctx: &RuleContext,
  opts: &BrowserCompatOptions,
  compat_data: &CompatData,
  targets: &[BrowserTarget],
  diagnostics: &mut Vec<Diagnostic>,
) {
  match node {
    CssNode::Style(rule) => {
      // Check selectors for pseudo-elements and pseudo-classes
      check_selectors(
        &rule.selector,
        rule.span.offset,
        ctx,
        opts,
        compat_data,
        targets,
        diagnostics,
      );
      // Check declarations directly on this style rule
      for decl in &rule.declarations {
        check_property(decl, ctx, opts, compat_data, targets, diagnostics);
      }
      // Check nested style rule children
      for child in &rule.children {
        collect_from_style_rule(child, ctx, opts, compat_data, targets, diagnostics);
      }
      // Check nested at-rules
      collect_from_nodes(
        &rule.nested_at_rules,
        ctx,
        opts,
        compat_data,
        targets,
        diagnostics,
      );
    }
    CssNode::AtRule(at_rule) => {
      collect_from_nodes(
        &at_rule.children,
        ctx,
        opts,
        compat_data,
        targets,
        diagnostics,
      );
    }
    CssNode::Declaration(decl) => {
      check_property(decl, ctx, opts, compat_data, targets, diagnostics);
    }
    CssNode::Comment(_) => {}
  }
}

/// Checks a style rule's selector and declarations, then its nested rules.
fn collect_from_style_rule(
  rule: &gale_css_parser::StyleRule,
  ctx: &RuleContext,
  opts: &BrowserCompatOptions,
  compat_data: &CompatData,
  targets: &[BrowserTarget],
  diagnostics: &mut Vec<Diagnostic>,
) {
  check_selectors(
    &rule.selector,
    rule.span.offset,
    ctx,
    opts,
    compat_data,
    targets,
    diagnostics,
  );
  for decl in &rule.declarations {
    check_property(decl, ctx, opts, compat_data, targets, diagnostics);
  }
  for child in &rule.children {
    collect_from_style_rule(child, ctx, opts, compat_data, targets, diagnostics);
  }
  collect_from_nodes(
    &rule.nested_at_rules,
    ctx,
    opts,
    compat_data,
    targets,
    diagnostics,
  );
}

/// Check CSS properties for browser compatibility.
fn check_property(
  decl: &gale_css_parser::Declaration,
  ctx: &RuleContext,
  opts: &BrowserCompatOptions,
  compat_data: &CompatData,
  targets: &[BrowserTarget],
  diagnostics: &mut Vec<Diagnostic>,
) {
  let prop = &decl.property;

  // Try direct match first (handles -webkit-tap-highlight-color etc.)
  let feature_id = format!("properties.{}", prop);
  if let Some(entry) = compat_data.get(&feature_id) {
    if opts.allow_features.contains(&feature_id) {
      return;
    }
    check_support_and_report(
      entry,
      &format!("\"{}\" property", prop),
      decl.span.offset,
      ctx,
      opts,
      targets,
      diagnostics,
    );
    return;
  }

  // Try stripping vendor prefix
  if let Some(stripped) = strip_vendor_prefix(prop) {
    let feature_id = format!("properties.{}", stripped);
    if let Some(entry) = compat_data.get(&feature_id) {
      if opts.allow_features.contains(&feature_id) {
        return;
      }
      check_support_and_report(
        entry,
        &format!("\"{}\" property", stripped),
        decl.span.offset,
        ctx,
        opts,
        targets,
        diagnostics,
      );
    }
  }
}

/// Check selectors for pseudo-element/pseudo-class browser compatibility.
fn check_selectors(
  selector: &str,
  base_offset: usize,
  ctx: &RuleContext,
  opts: &BrowserCompatOptions,
  compat_data: &CompatData,
  targets: &[BrowserTarget],
  diagnostics: &mut Vec<Diagnostic>,
) {
  // Find pseudo-elements (::name) and pseudo-classes (:name) in the selector.
  let bytes = selector.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Skip strings
    if bytes[i] == b'"' || bytes[i] == b'\'' {
      let quote = bytes[i];
      i += 1;
      while i < len && bytes[i] != quote {
        if bytes[i] == b'\\' {
          i += 1;
        }
        i += 1;
      }
      i += 1;
      continue;
    }

    // Detect :: (pseudo-element) or : (pseudo-class)
    if bytes[i] == b':' {
      let colon_start = i;
      i += 1;
      let is_pseudo_element = i < len && bytes[i] == b':';
      if is_pseudo_element {
        i += 1;
      }

      // Read the pseudo name
      let name_start = i;
      while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
        i += 1;
      }

      if i > name_start {
        let pseudo_name = &selector[name_start..i];
        let full_pseudo = if is_pseudo_element {
          format!("::{}", pseudo_name)
        } else {
          format!(":{}", pseudo_name)
        };

        let pseudo_type = if is_pseudo_element {
          "pseudo-element"
        } else {
          "pseudo-class"
        };

        // Try direct match (handles -webkit-inner-spin-button etc.)
        let feature_id = format!("selectors.{}", pseudo_name);
        if let Some(entry) = compat_data.get(&feature_id) {
          if !opts.allow_features.contains(&feature_id) {
            check_support_and_report(
              entry,
              &format!("\"{}\" {}", full_pseudo, pseudo_type),
              base_offset + colon_start,
              ctx,
              opts,
              targets,
              diagnostics,
            );
          }
          continue;
        }

        // Try stripping vendor prefix from pseudo name
        if let Some(stripped) = strip_vendor_prefix(pseudo_name) {
          let feature_id = format!("selectors.{}", stripped);
          if let Some(entry) = compat_data.get(&feature_id)
            && !opts.allow_features.contains(&feature_id)
          {
            check_support_and_report(
              entry,
              &format!("\"{}\" {}", full_pseudo, pseudo_type),
              base_offset + colon_start,
              ctx,
              opts,
              targets,
              diagnostics,
            );
          }
        }
      }
      continue;
    }

    i += 1;
  }
}

/// Reports the feature if any target browser lacks support for it.
fn check_support_and_report(
  entry: &CompatEntry,
  feature_name: &str,
  offset: usize,
  ctx: &RuleContext,
  opts: &BrowserCompatOptions,
  targets: &[BrowserTarget],
  diagnostics: &mut Vec<Diagnostic>,
) {
  let unsupported: Vec<&BrowserTarget> = targets
    .iter()
    .filter(|t| !is_supported(entry, t, opts))
    .collect();

  if unsupported.is_empty() {
    return;
  }

  // Deduplicate by display name (keep first occurrence = oldest version).
  let mut seen = HashSet::new();
  let deduped: Vec<&&BrowserTarget> = unsupported
    .iter()
    .filter(|t| seen.insert(t.display_name.clone()))
    .collect();

  let targets_text: Vec<String> = deduped
    .iter()
    .map(|t| format!("{} {}", t.display_name, t.version_string))
    .collect();
  let targets_str = targets_text.join(", ");

  let mdn_url = entry.mdn_url.as_deref().unwrap_or("");
  let msg = if mdn_url.is_empty() {
    format!("{} is not supported in {}.", feature_name, targets_str)
  } else {
    format!(
      "{} is not supported in {}. See {}.",
      feature_name, targets_str, mdn_url
    )
  };

  diagnostics.push(
    Diagnostic::new("plugin/browser-compat", msg)
      .span(Span::new(offset, 0))
      .file_path(ctx.file_path),
  );
}

// ── Utility functions ───────────────────────────────────────────────────

/// The name after a `-webkit-`, `-moz-`, `-ms-` or `-o-` prefix, or `None`
/// without one.
fn strip_vendor_prefix(prop: &str) -> Option<&str> {
  let prefixes = ["-webkit-", "-moz-", "-ms-", "-o-"];
  for prefix in &prefixes {
    if let Some(stripped) = prop.strip_prefix(prefix) {
      return Some(stripped);
    }
  }
  None
}

/// Nearest `node_modules` directory at or above `file_path`.
fn find_node_modules(file_path: &Path) -> Option<std::path::PathBuf> {
  let mut dir = if file_path.is_file() {
    file_path.parent()?
  } else {
    file_path
  };

  loop {
    let nm = dir.join("node_modules");
    if nm.is_dir() {
      return Some(nm);
    }
    dir = dir.parent()?;
  }
}

#[cfg(test)]
mod tests {
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
}
