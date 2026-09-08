mod cache;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use clap::Parser;
use ignore::WalkBuilder;
use rayon::prelude::*;
use tracing::debug;

use gale_config::{ConfigResolver, GaleConfig};
use gale_css_parser::{Syntax, detect_syntax};
use gale_diagnostics::{LintResult, Severity, apply_fixes};
use gale_formatter::{create_formatter_with_color, strip_ansi};
use gale_linter::{LintRunner, RuleRegistry};

use crate::cache::{LintCache, compute_config_hash, compute_hash, resolve_cache_path};

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
  name = "gale",
  version,
  about = "An extremely fast CSS linter, written in Rust"
)]
pub struct Cli {
  /// Files or glob patterns to lint
  #[arg(required_unless_present_any = ["stdin", "init", "lsp", "print_config"])]
  files: Vec<String>,

  /// Resolve and print the effective config for FILE as JSON, then exit
  #[arg(long, value_name = "FILE")]
  print_config: Option<PathBuf>,

  /// Start the LSP server (for editor integration)
  #[arg(long)]
  lsp: bool,

  /// Generate a starter gale.json config file in the current directory
  #[arg(long)]
  init: bool,

  /// Read source from stdin instead of files
  #[arg(long)]
  stdin: bool,

  /// Virtual filename for stdin input (used for syntax detection and diagnostics)
  #[arg(long, default_value = "stdin.css")]
  stdin_filename: String,

  /// Config file path
  #[arg(short, long)]
  config: Option<PathBuf>,

  /// Output format
  ///
  /// Restricted to the implemented set so a typo (or a formatter Gale does
  /// not have) is rejected outright rather than silently falling back to
  /// text output.
  #[arg(
        short = 'f',
        long,
        default_value = "text",
        value_parser = clap::builder::PossibleValuesParser::new(gale_formatter::FORMATTER_NAMES)
    )]
  formatter: String,

  /// Maximum number of warnings before erroring
  #[arg(long)]
  max_warnings: Option<usize>,

  /// Automatically fix problems (default: strict, or specify =lax)
  ///
  /// `require_equals` is essential: without it clap treats a bare `--fix` as
  /// taking the next positional argument as its MODE value, so
  /// `gale --fix "src/**/*.css"` would consume the glob and then fail with
  /// "no files provided".
  #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "strict",
        value_name = "MODE"
    )]
  fix: Option<String>,

  /// Only report errors
  #[arg(short, long)]
  quiet: bool,

  /// Force colour on in the text and verbose formatters
  #[arg(long, overrides_with = "no_color")]
  color: bool,

  /// Force colour off in the text and verbose formatters
  #[arg(long, overrides_with = "color")]
  no_color: bool,

  /// Path to a custom ignore file (uses gitignore syntax)
  #[arg(long, value_name = "FILE")]
  ignore_path: Option<PathBuf>,

  /// Glob pattern of files to ignore, on top of the ignore files (repeatable)
  #[arg(long, visible_alias = "ip", value_name = "PATTERN", action = clap::ArgAction::Append)]
  ignore_pattern: Vec<String>,

  /// Lint node_modules too instead of always skipping it
  #[arg(long, visible_alias = "di")]
  disable_default_ignores: bool,

  /// Accepted for Stylelint compatibility; Gale emits no deprecation warnings
  #[arg(long)]
  quiet_deprecation_warnings: bool,

  /// Parse every file with this syntax: postcss, postcss-scss, postcss-less or postcss-sass
  #[arg(long, value_name = "SYNTAX")]
  custom_syntax: Option<String>,

  /// Write the report to this file as well as printing it
  #[arg(short = 'o', long, value_name = "PATH")]
  output_file: Option<PathBuf>,

  /// Disable all ignore file processing (gitignore, .galeignore, custom)
  #[arg(long)]
  no_ignore: bool,

  /// Enable file-based caching to skip unchanged files
  #[arg(long)]
  cache: bool,

  /// Override the cache file location (default: .gale_cache in the current directory)
  #[arg(long, value_name = "PATH")]
  cache_location: Option<PathBuf>,

  /// Don't error when no files match the glob pattern
  #[arg(long)]
  allow_empty_input: bool,

  /// Ignore all `/* stylelint-disable */` comments
  #[arg(long)]
  ignore_disables: bool,

  /// Report `/* stylelint-disable */` comments that don't suppress any warnings
  #[arg(long)]
  report_needless_disables: bool,

  /// Report `/* stylelint-disable */` comments that reference rules not being linted
  #[arg(long)]
  report_invalid_scope_disables: bool,

  /// Report `/* stylelint-disable */` comments without a description
  #[arg(long)]
  report_descriptionless_disables: bool,

  /// Report `/* stylelint-disable */` comments that name no rule
  #[arg(long)]
  report_unscoped_disables: bool,
}

// ---------------------------------------------------------------------------
// Supported file extensions
// ---------------------------------------------------------------------------

const CSS_EXTENSIONS: &[&str] = &["css", "scss", "less", "sass"];

/// Map a Stylelint `customSyntax` package name onto the parser Gale uses.
///
/// Returns `None` for syntaxes Gale cannot parse (`postcss-html`,
/// `postcss-markdown`, ...).  This is the only place that decides which
/// `customSyntax` values Gale understands, whether they arrive from the config
/// file or from `--custom-syntax`.
fn syntax_for_custom_syntax(name: &str) -> Option<Syntax> {
  match name.to_ascii_lowercase().as_str() {
    "postcss" => Some(Syntax::Css),
    "postcss-scss" => Some(Syntax::Scss),
    "postcss-less" => Some(Syntax::Less),
    "postcss-sass" => Some(Syntax::Sass),
    _ => None,
  }
}

/// The parser to use for `file_path`, or `None` when the file should be
/// skipped because the `customSyntax` in play names a syntax Gale cannot parse.
///
/// `--custom-syntax` applies to every file and picks the parser directly.  The
/// config's own `customSyntax` only decides whether a file is skipped —
/// supported values still parse by file extension.
fn resolve_syntax(
  cli_custom_syntax: Option<&str>,
  config: &GaleConfig,
  file_path: &str,
) -> Option<Syntax> {
  if let Some(name) = cli_custom_syntax {
    let syntax = syntax_for_custom_syntax(name);
    if syntax.is_none() {
      debug!("Skipping {file_path}: unsupported --custom-syntax '{name}'");
    }
    return syntax;
  }

  if let Some(name) = config.custom_syntax_for_file(file_path)
    && syntax_for_custom_syntax(name).is_none()
  {
    debug!("Skipping {file_path}: unsupported customSyntax '{name}'");
    return None;
  }

  Some(detect_syntax(file_path))
}

/// Decide whether the human-readable formatters colour their output.
///
/// This is picocolors' rule, which Stylelint relies on:
///
/// ```text
/// colour = !(NO_COLOR || --no-color)
///       && (FORCE_COLOR || --color || CI || (stdout is a TTY && TERM != dumb))
/// ```
///
/// An environment variable counts as set only when it is non-empty, matching
/// JavaScript truthiness.
fn color_enabled(
  force_on: bool,
  force_off: bool,
  stdout_is_tty: bool,
  env: impl Fn(&str) -> Option<String>,
) -> bool {
  let set = |name: &str| env(name).is_some_and(|v| !v.is_empty());
  if force_off || set("NO_COLOR") {
    return false;
  }
  if force_on || set("FORCE_COLOR") || set("CI") {
    return true;
  }
  stdout_is_tty && env("TERM").as_deref() != Some("dumb")
}

/// Write a report to `path`, creating parent directories and stripping ANSI
/// escapes the way Stylelint's `--output-file` does.
fn write_output_file(path: &Path, report: &str) -> std::io::Result<()> {
  if let Some(parent) = path.parent()
    && !parent.as_os_str().is_empty()
  {
    std::fs::create_dir_all(parent)?;
  }
  std::fs::write(path, strip_ansi(report))
}

fn is_css_file(path: &Path) -> bool {
  path
    .extension()
    .and_then(|ext| ext.to_str())
    .is_some_and(|ext| CSS_EXTENSIONS.contains(&ext))
}

// ---------------------------------------------------------------------------
// File discovery
// ---------------------------------------------------------------------------

struct DiscoverOptions<'a> {
  no_ignore: bool,
  ignore_path: Option<&'a Path>,
  ignore_patterns: &'a [String],
  /// Stylelint's `--disable-default-ignores`: walk into `node_modules`.
  disable_default_ignores: bool,
}

/// Build an `ignore::gitignore::Gitignore` matcher from `.stylelintignore`,
/// `.galeignore`, and an optional user-supplied ignore file in the cwd.
///
/// This is used to filter explicitly-passed files, matching Stylelint's
/// behaviour of always respecting `.stylelintignore` even for explicit paths.
fn build_explicit_ignore_matcher(
  user_ignore: Option<&Path>,
) -> Option<ignore::gitignore::Gitignore> {
  let cwd = std::env::current_dir().ok()?;
  let mut builder = ignore::gitignore::GitignoreBuilder::new(&cwd);

  // Load .stylelintignore from cwd (if it exists).
  let stylelintignore = cwd.join(".stylelintignore");
  if stylelintignore.is_file() {
    builder.add(&stylelintignore);
  }

  // Load .galeignore from cwd (if it exists).
  let galeignore = cwd.join(".galeignore");
  if galeignore.is_file() {
    builder.add(&galeignore);
  }

  // Load user-supplied ignore file.
  if let Some(path) = user_ignore
    && path.is_file()
  {
    builder.add(path);
  }

  builder.build().ok()
}

/// Returns `true` if the string contains glob meta-characters (`*`, `?`, `{`, `[`)
/// or extglob operators (`+(`, `?(`, `@(`, `*(`, `!(`).
fn is_glob_pattern(s: &str) -> bool {
  s.contains('*') || s.contains('?') || s.contains('{') || s.contains('[') || has_extglob(s)
}

/// Returns `true` if `s` contains any extglob operator: `+(`, `?(`, `@(`, `*(`, `!(`.
/// Also detects bare `(alt|alt)` at path-segment boundaries which some tools treat
/// as an implicit `@(...)`.
fn has_extglob(s: &str) -> bool {
  let bytes = s.as_bytes();
  for (i, &b) in bytes.iter().enumerate() {
    if b == b'(' {
      if i > 0 && matches!(bytes[i - 1], b'+' | b'?' | b'@' | b'*' | b'!') {
        return true;
      }
      // Bare `(alt|alt)` at path-segment start (e.g. `(src|docs)/**`)
      if (i == 0 || bytes[i - 1] == b'/') && s[i..].contains('|') {
        return true;
      }
    }
  }
  false
}

/// Convert extglob patterns to standard glob brace syntax that `globset` understands.
///
/// Handles the following bash extglob operators:
/// - `+(pattern|pattern)` (one or more)  -> `{pattern,pattern}`
/// - `@(pattern|pattern)` (exactly one)  -> `{pattern,pattern}`
/// - `?(pattern|pattern)` (zero or one)  -> `{pattern,pattern,}`
/// - `*(pattern|pattern)` (zero or more) -> `{pattern,pattern,}`
/// - `!(pattern|pattern)` (negation)     -> left as-is (not supported, warning printed)
///
/// Also converts bare `(alt|alt)` at path-segment boundaries to `{alt,alt}`.
fn convert_extglob(pattern: &str) -> String {
  let mut result = String::with_capacity(pattern.len());
  let bytes = pattern.as_bytes();
  let len = bytes.len();
  let mut i = 0;

  while i < len {
    // Check for extglob operator: +( ?( @( *( !(
    if i + 1 < len && bytes[i + 1] == b'(' && matches!(bytes[i], b'+' | b'?' | b'@' | b'!') {
      let op = bytes[i];
      if op == b'!' {
        // Negation extglob is not easily convertible; pass through as-is
        // and let globset handle it (it will likely fail, but we can't
        // do much better without a full negation engine).
        result.push(bytes[i] as char);
        i += 1;
        continue;
      }
      // Find matching close paren
      if let Some(close) = find_matching_paren(pattern, i + 1) {
        let inner = &pattern[i + 2..close];
        // Replace | with , and wrap in braces
        let converted = inner.replace('|', ",");
        // For ?() and *() add empty alternative (zero occurrences)
        if op == b'?' {
          result.push('{');
          result.push_str(&converted);
          result.push_str(",}");
        } else {
          // +() and @() -> {alternatives}
          result.push('{');
          result.push_str(&converted);
          result.push('}');
        }
        i = close + 1;
        continue;
      }
    }

    // Check for *( extglob (need special care since * is also a glob char)
    if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'(' {
      // Distinguish `*(` extglob from `*` glob followed by `(`.
      // In extglob, `*(` has alternatives separated by `|` inside.
      if let Some(close) = find_matching_paren(pattern, i + 1) {
        let inner = &pattern[i + 2..close];
        if inner.contains('|') {
          // This is a *() extglob
          let converted = inner.replace('|', ",");
          result.push('{');
          result.push_str(&converted);
          result.push_str(",}");
          i = close + 1;
          continue;
        }
      }
    }

    // Check for bare (alt|alt) at path-segment boundary
    if bytes[i] == b'('
      && (i == 0 || bytes[i - 1] == b'/')
      && let Some(close) = find_matching_paren(pattern, i)
    {
      let inner = &pattern[i + 1..close];
      if inner.contains('|') && !inner.contains('/') {
        let converted = inner.replace('|', ",");
        result.push('{');
        result.push_str(&converted);
        result.push('}');
        i = close + 1;
        continue;
      }
    }

    result.push(bytes[i] as char);
    i += 1;
  }

  result
}

/// Find the matching `)` for a `(` at position `open_pos`, handling nesting.
fn find_matching_paren(s: &str, open_pos: usize) -> Option<usize> {
  let bytes = s.as_bytes();
  if bytes[open_pos] != b'(' {
    return None;
  }
  let mut depth = 0;
  for (i, &b) in bytes[open_pos..].iter().enumerate() {
    match b {
      b'(' => depth += 1,
      b')' => {
        depth -= 1;
        if depth == 0 {
          return Some(open_pos + i);
        }
      }
      _ => {}
    }
  }
  None
}

/// Expand top-level brace alternatives in a glob pattern.
///
/// Patterns like `{a,b}/**/*.scss` are expanded to `["a/**/*.scss", "b/**/*.scss"]`.
/// Nested or extension-level braces (e.g. `**/*.{css,scss}`) are left untouched
/// because `globset` handles those natively.
///
/// Only the *first* top-level brace group is expanded (recursion handles deeper
/// groups), and only when the group starts at a position where it forms path
/// alternatives (i.e. the brace content contains `/` or the brace is at the
/// start of the pattern).
fn expand_braces(pattern: &str) -> Vec<String> {
  // Find the first `{` that is a top-level brace group containing path
  // separators or sitting at pattern start (path-level alternatives).
  if let Some(open) = pattern.find('{') {
    // Find the matching closing brace (handles one level of nesting).
    let mut depth = 0;
    let mut close = None;
    for (i, ch) in pattern[open..].char_indices() {
      match ch {
        '{' => depth += 1,
        '}' => {
          depth -= 1;
          if depth == 0 {
            close = Some(open + i);
            break;
          }
        }
        _ => {}
      }
    }
    if let Some(close_pos) = close {
      let prefix = &pattern[..open];
      let suffix = &pattern[close_pos + 1..];
      let inner = &pattern[open + 1..close_pos];

      // Only expand if the brace content contains `/` (path alternatives)
      // or if the brace is at the very start of the pattern.
      // Extension braces like `*.{css,scss}` don't contain `/` and
      // globset handles them fine, so skip those.
      if inner.contains('/') || open == 0 {
        // Split on top-level commas (respecting nested braces).
        let mut alternatives = Vec::new();
        let mut current = String::new();
        let mut depth = 0;
        for ch in inner.chars() {
          match ch {
            '{' => {
              depth += 1;
              current.push(ch);
            }
            '}' => {
              depth -= 1;
              current.push(ch);
            }
            ',' if depth == 0 => {
              alternatives.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
          }
        }
        alternatives.push(current);

        // Recursively expand each alternative (handles nested braces).
        let mut result = Vec::new();
        for alt in alternatives {
          let expanded_pattern = format!("{prefix}{alt}{suffix}");
          result.extend(expand_braces(&expanded_pattern));
        }
        return result;
      }
    }
  }
  vec![pattern.to_string()]
}

fn discover_files(paths: &[String], opts: &DiscoverOptions<'_>) -> Vec<PathBuf> {
  let mut files: Vec<PathBuf> = Vec::new();

  // Build a gitignore matcher for .stylelintignore / .galeignore so we can
  // filter explicitly-passed files the same way Stylelint does.
  let explicit_ignore = if opts.no_ignore {
    None
  } else {
    build_explicit_ignore_matcher(opts.ignore_path)
  };

  // Build glob matchers from config ignore patterns (ignoreFiles / ignorePatterns).
  // These are applied to both explicit files and walked directories, matching
  // Stylelint's behaviour where `ignoreFiles` always excludes matching paths.
  let cwd = std::env::current_dir().unwrap_or_default();
  let config_ignore_matchers: Vec<globset::GlobMatcher> = opts
    .ignore_patterns
    .iter()
    .filter_map(|pat| {
      // Stylelint ignoreFiles patterns are relative to config location (usually
      // project root). Strip leading "./" for matching and treat as globs.
      let clean = pat.strip_prefix("./").unwrap_or(pat);
      globset::Glob::new(clean).ok().map(|g| g.compile_matcher())
    })
    .collect();

  // Convert extglob patterns (e.g. `+(css|scss)`, `@(src|docs)`) to
  // standard glob brace syntax before further processing.
  let converted: Vec<String> = paths.iter().map(|p| convert_extglob(p)).collect();

  // Pre-expand brace alternatives in glob patterns so that path-level
  // braces like `{src,packages}/**/*.scss` are handled correctly.
  let expanded: Vec<String> = converted.iter().flat_map(|p| expand_braces(p)).collect();

  for pattern in &expanded {
    // If the argument contains glob meta-characters, use the `ignore`
    // crate's `WalkBuilder` (same as for directory walks) so that
    // `.gitignore`, `.galeignore`, `.stylelintignore`, and default
    // `node_modules` exclusion are all respected.  The glob pattern is
    // compiled via `globset` and used as a post-filter on the walked
    // paths.
    if is_glob_pattern(pattern) {
      // Compile the glob pattern for matching against walked paths.
      // Use `literal_separator(true)` so that `*` does not match `/`,
      // preserving standard glob semantics where `*.css` only matches
      // files in the current directory while `**/*.css` matches
      // recursively.
      let glob_matcher = match globset::GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
      {
        Ok(g) => g.compile_matcher(),
        Err(err) => {
          eprintln!("Warning: invalid glob pattern '{pattern}': {err}");
          continue;
        }
      };

      // Determine the walk root: use the longest literal prefix of the
      // pattern (before the first meta-character).  Fall back to "."
      // when the pattern starts with a meta-character (e.g. `**/*.css`).
      let walk_root = {
        let first_meta = pattern.find(['*', '?', '{', '[']).unwrap_or(pattern.len());
        let prefix = &pattern[..first_meta];
        let p = Path::new(prefix);
        let parent = p
          .parent()
          .map(|p| p.to_string_lossy().to_string())
          .unwrap_or_default();
        let candidate = if parent.is_empty() {
          PathBuf::from(prefix)
        } else {
          PathBuf::from(parent)
        };
        // An empty path (e.g. when the pattern starts with `**` or `{`)
        // is not a valid walk root — fall back to the current directory.
        if candidate.as_os_str().is_empty() {
          PathBuf::from(".")
        } else {
          candidate
        }
      };

      let mut builder = WalkBuilder::new(&walk_root);
      builder.hidden(false);

      if opts.no_ignore {
        builder.git_ignore(false);
        builder.git_global(false);
        builder.git_exclude(false);
      } else {
        // Do NOT respect .gitignore — Stylelint only uses .stylelintignore,
        // so we must match that behavior for drop-in compatibility.
        builder.git_ignore(false);
        builder.git_global(false);
        builder.git_exclude(false);

        builder.add_custom_ignore_filename(".galeignore");
        builder.add_custom_ignore_filename(".stylelintignore");

        if let Some(ignore_file) = opts.ignore_path
          && ignore_file.exists()
          && let Some(err) = builder.add_ignore(ignore_file)
        {
          eprintln!(
            "Warning: failed to load ignore file {}: {err}",
            ignore_file.display()
          );
        }

        // Exclude node_modules unless asked not to (Stylelint's
        // default ignore, lifted by --disable-default-ignores).
        {
          let mut overrides = ignore::overrides::OverrideBuilder::new(&walk_root);
          if !opts.disable_default_ignores {
            let _ = overrides.add("!**/node_modules/**");
          }
          for pat in opts.ignore_patterns {
            if let Err(err) = overrides.add(&format!("!{pat}")) {
              eprintln!("Warning: invalid ignore pattern '{pat}': {err}");
            }
          }
          if let Ok(built) = overrides.build() {
            builder.overrides(built);
          }
        }
      }

      // Use parallel walk for faster directory traversal on large repos.
      let matched_files = std::sync::Mutex::new(Vec::new());
      let glob_ref = &glob_matcher;
      let cwd_ref = &cwd;
      let explicit_ignore_ref = &explicit_ignore;
      let config_ignore_ref = &config_ignore_matchers;
      builder.build_parallel().run(|| {
        Box::new(|entry_result| {
          let entry = match entry_result {
            Ok(e) => e,
            Err(_) => return ignore::WalkState::Continue,
          };
          let is_file = entry.file_type().map(|ft| ft.is_file()).unwrap_or(false);
          if !is_file {
            return ignore::WalkState::Continue;
          }
          let entry_path = entry.path();
          if !is_css_file(entry_path) {
            return ignore::WalkState::Continue;
          }
          // Check the glob pattern against the path.
          if !glob_ref.is_match(entry_path) {
            let rel = entry_path.strip_prefix(cwd_ref).unwrap_or(entry_path);
            // A walk rooted at "." yields paths like "./a.css".
            // A bare pattern such as `*.css` carries no "./" and
            // `*` does not cross a separator, so it would never
            // match without stripping the prefix first.
            let bare = rel.strip_prefix("./").unwrap_or(rel);
            if !glob_ref.is_match(rel) && !glob_ref.is_match(bare) {
              return ignore::WalkState::Continue;
            }
          }

          // Respect .stylelintignore / .galeignore for explicitly
          // matched files.
          if let Some(ref gi) = *explicit_ignore_ref
            && gi
              .matched_path_or_any_parents(entry_path, false)
              .is_ignore()
          {
            return ignore::WalkState::Continue;
          }

          // Check config ignore patterns.
          if !config_ignore_ref.is_empty() {
            let abs = entry_path
              .canonicalize()
              .unwrap_or_else(|_| entry_path.to_path_buf());
            let rel = abs.strip_prefix(cwd_ref).unwrap_or(&abs);
            if config_ignore_ref.iter().any(|m| m.is_match(rel)) {
              return ignore::WalkState::Continue;
            }
          }

          matched_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(entry_path.to_path_buf());
          ignore::WalkState::Continue
        })
      });
      files.extend(matched_files.into_inner().unwrap_or_default());
      continue;
    }

    let path = Path::new(pattern);

    if path.is_file() {
      if is_css_file(path) {
        // Respect .stylelintignore / .galeignore even for explicit files
        // (matching Stylelint's behaviour).
        if let Some(ref gi) = explicit_ignore {
          // matched_path_or_any_parents checks the path and all its
          // ancestor components, so `packages/theme/src/prebuilt/x.css`
          // will match a pattern `packages/theme/src/prebuilt`.
          if gi.matched_path_or_any_parents(path, false).is_ignore() {
            continue;
          }
        }

        // Check config ignore patterns (ignoreFiles / ignorePatterns).
        if !config_ignore_matchers.is_empty() {
          let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
          let rel = abs.strip_prefix(&cwd).unwrap_or(&abs);
          if config_ignore_matchers.iter().any(|m| m.is_match(rel)) {
            continue;
          }
        }

        files.push(path.to_path_buf());
      }
      continue;
    }

    let mut builder = WalkBuilder::new(path);

    // Show hidden CSS files (e.g. .hidden.css) but respect ignore files.
    builder.hidden(false);

    if opts.no_ignore {
      // Disable all ignore-file processing.
      builder.git_ignore(false);
      builder.git_global(false);
      builder.git_exclude(false);
    } else {
      // Do NOT respect .gitignore — Stylelint only uses .stylelintignore,
      // so we must match that behavior for drop-in compatibility.
      builder.git_ignore(false);
      builder.git_global(false);
      builder.git_exclude(false);

      // Automatically respect .galeignore files found in traversed dirs.
      builder.add_custom_ignore_filename(".galeignore");

      // Respect .stylelintignore files (Stylelint compatibility).
      builder.add_custom_ignore_filename(".stylelintignore");

      // Load a user-supplied custom ignore file.
      if let Some(ignore_file) = opts.ignore_path {
        if ignore_file.exists() {
          if let Some(err) = builder.add_ignore(ignore_file) {
            eprintln!(
              "Warning: failed to load ignore file {}: {err}",
              ignore_file.display()
            );
          }
        } else {
          eprintln!("Warning: ignore file not found: {}", ignore_file.display());
        }
      }

      // Apply ignore_patterns from config as glob overrides.
      // Exclude node_modules unless asked not to (Stylelint's default
      // ignore, lifted by --disable-default-ignores).
      {
        let mut overrides = ignore::overrides::OverrideBuilder::new(path);
        if !opts.disable_default_ignores {
          let _ = overrides.add("!**/node_modules/**");
        }
        for pat in opts.ignore_patterns {
          // Negate the pattern so matching files are excluded.
          if let Err(err) = overrides.add(&format!("!{pat}")) {
            eprintln!("Warning: invalid ignore pattern '{pat}': {err}");
          }
        }
        if let Ok(built) = overrides.build() {
          builder.overrides(built);
        }
      }
    }

    // Use parallel walk for faster directory traversal.
    let dir_matched = std::sync::Mutex::new(Vec::new());
    builder.build_parallel().run(|| {
      Box::new(|entry_result| {
        let entry = match entry_result {
          Ok(e) => e,
          Err(_) => return ignore::WalkState::Continue,
        };
        let is_file = entry.file_type().map(|ft| ft.is_file()).unwrap_or(false);
        if is_file && is_css_file(entry.path()) {
          dir_matched
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(entry.path().to_path_buf());
        }
        ignore::WalkState::Continue
      })
    });
    files.extend(dir_matched.into_inner().unwrap_or_default());
  }

  files
}

// ---------------------------------------------------------------------------
// Config initialisation (--init)
// ---------------------------------------------------------------------------

/// Config file names that, if present, prevent `--init` from creating a new one.
const EXISTING_CONFIG_FILES: &[&str] = &["gale.json", "gale.toml", ".stylelintrc.json"];

fn generate_init_config() -> Result<()> {
  let cwd = std::env::current_dir()?;

  for name in EXISTING_CONFIG_FILES {
    let path = cwd.join(name);
    if path.exists() {
      bail!("Configuration file already exists: {}", path.display());
    }
  }

  let config_content = r#"{
  "extends": "gale:recommended",
  "rules": {}
}
"#;

  let config_path = cwd.join("gale.json");
  std::fs::write(&config_path, config_content)?;
  println!("Created gale.json with recommended configuration.");
  Ok(())
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn run() -> Result<()> {
  let cli = Cli::parse();

  // Initialise tracing (controlled via GALE_LOG env var).
  tracing_subscriber::fmt()
    .with_env_filter(
      tracing_subscriber::EnvFilter::try_from_env("GALE_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
    )
    .init();

  // Handle --lsp: start the LSP server and exit early.
  if cli.lsp {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(gale_lsp::run_server(cli.config.clone()));
    return Ok(());
  }

  // Handle --init: generate a starter config file and exit early.
  if cli.init {
    return generate_init_config();
  }

  // Resolve configuration.
  //
  // `has_config_file` tracks whether the user has *any* config file (even if
  // it resolves to zero rules).  When true we respect whatever the config
  // says — including "no rules".  The "enable all rules" fallback only kicks
  // in when there is genuinely no config file anywhere.
  //
  // When `--config` is NOT specified we use a `ConfigResolver` for per-file
  // config lookup (matching Stylelint's cosmiconfig behaviour where each file
  // is linted with the closest config in the directory hierarchy).
  let use_per_file_config = cli.config.is_none();

  let (config, has_config_file) = if let Some(ref cfg_path) = cli.config {
    debug!("Using config file: {}", cfg_path.display());
    let cfg = gale_config::load_config(cfg_path).unwrap_or_else(|err| {
      eprintln!("Warning: failed to load config: {err}");
      GaleConfig::default()
    });
    (cfg, true)
  } else {
    let cwd = std::env::current_dir().unwrap_or_default();
    match gale_config::resolve_config(&cwd) {
      Some(cfg) => (cfg, true),
      None => (GaleConfig::default(), false),
    }
  };

  // Build rule registry and determine enabled rules.
  let registry = RuleRegistry::default();

  // Validate plugins declared in the config.
  for plugin in &config.plugins {
    if !gale_config::is_known_plugin(plugin) {
      eprintln!(
        "warning: Plugin \"{plugin}\" is not supported by Gale and will be ignored. \
                 Rules from this plugin will have no effect."
      );
    }
  }

  // Warn about rules from known plugins that Gale hasn't implemented yet.
  if has_config_file {
    for rule_name in config.rules.keys() {
      if gale_config::is_known_plugin_rule(rule_name) && registry.get(rule_name).is_none() {
        eprintln!(
          "warning: Rule \"{rule_name}\" is not yet supported by Gale and will be skipped."
        );
      }
    }
  }

  let enabled_rules: Vec<String> = if config.rules.is_empty() && !has_config_file {
    // No config file found at all — enable all registered rules as a
    // sensible default so `gale .` works out-of-the-box.
    registry
      .all()
      .iter()
      .map(|r| r.name().to_string())
      .collect()
  } else {
    config
      .rules
      .iter()
      .filter(|(_, cfg)| {
        cfg
          .severity
          .as_ref()
          .map(|s| !matches!(s, gale_config::Severity::Off))
          .unwrap_or(true)
      })
      .filter(|(name, _)| registry.get(name).is_some())
      .map(|(name, _)| name.clone())
      .collect()
  };

  // Handle --print-config: print resolved config as JSON and exit.
  if let Some(ref file) = cli.print_config {
    let file_str = file.display().to_string();
    // Use per-file config resolution when --config is not specified.
    let file_config;
    let effective_config = if use_per_file_config {
      let abs_path = std::env::current_dir().unwrap_or_default().join(file);
      file_config =
        gale_config::resolve_config_for_file(&abs_path).unwrap_or_else(|| config.clone());
      &file_config
    } else {
      &config
    };
    let effective_rules = effective_config.rules_for_file(&file_str);
    let mut rules_json = serde_json::Map::new();
    for (name, rc) in &effective_rules {
      let severity = rc
        .severity
        .as_ref()
        .map(|s| format!("{s:?}").to_lowercase())
        .unwrap_or_else(|| "error".to_string());
      if let Some(ref opts) = rc.options {
        // Show [severity, options] for rules with options.
        rules_json.insert(name.clone(), serde_json::json!([severity, opts]));
      } else {
        rules_json.insert(name.clone(), serde_json::Value::String(severity));
      }
    }
    let output = serde_json::json!({
        "file": file_str,
        "rules": rules_json,
        "ignorePatterns": effective_config.ignore_patterns,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    return Ok(());
  }

  // Gale never emits deprecation warnings, so the flag only needs accepting.
  if cli.quiet_deprecation_warnings {
    debug!("--quiet-deprecation-warnings has nothing to silence in Gale");
  }

  let cli_custom_syntax = cli.custom_syntax.as_deref();

  // Every switch below can come from the CLI or from the config file.  A
  // flag on the command line wins; otherwise the config decides, matching
  // Stylelint where unspecified CLI flags fall back to config properties.
  let quiet = cli.quiet || config.quiet;
  let use_cache = cli.cache || config.cache;
  let allow_empty_input = cli.allow_empty_input || config.allow_empty_input;
  let cache_location: Option<PathBuf> = cli
    .cache_location
    .clone()
    .or_else(|| config.cache_location.clone());
  let fix_mode: Option<String> = cli.fix.clone().or_else(|| {
    config.fix.map(|mode| match mode {
      gale_config::FixMode::Strict => "strict".to_string(),
      gale_config::FixMode::Lax => "lax".to_string(),
    })
  });

  // Set up caching if --cache is enabled.
  let cache_path = resolve_cache_path(cache_location.as_deref());
  let config_hash = compute_config_hash(&config.rules);
  let mut lint_cache = if use_cache {
    debug!("Loading cache from {}", cache_path.display());
    LintCache::load(&cache_path)
  } else {
    LintCache::default()
  };

  // Extract per-rule options from the config, keyed by canonical rule name.
  // Config may use deprecated aliases (e.g. "function-comma-space-after")
  // but the runner looks up options by canonical name
  // (e.g. "@stylistic/function-comma-space-after").
  let mut rule_options: std::collections::HashMap<String, serde_json::Value> = config
    .rules
    .iter()
    .filter_map(|(name, cfg)| {
      cfg.options.as_ref().map(|opts| {
        // Resolve to canonical name if this is a deprecated alias
        let canonical = registry
          .get(name)
          .map(|r| r.name().to_string())
          .unwrap_or_else(|| name.clone());
        (canonical, opts.clone())
      })
    })
    .collect();

  // Detect Stylelint version and inject compatibility options.
  // Older Stylelint versions (< 15) have a smaller known-units list that
  // doesn't include dynamic viewport units (dvh, dvw, etc.) or container
  // query units (cqw, cqh, etc.).  When running against repos that use an
  // older Stylelint, inject these as "additional unknown units" so Gale's
  // `unit-no-unknown` rule reports them consistently.
  if enabled_rules.iter().any(|r| r == "unit-no-unknown") {
    let detect_cwd = std::env::current_dir().unwrap_or_default();
    if let Some(extra) = detect_legacy_unknown_units(&detect_cwd) {
      let entry = rule_options
        .entry("unit-no-unknown".to_string())
        .or_insert_with(|| serde_json::json!({}));
      if let Some(obj) = entry.as_object_mut() {
        obj.insert(
          "__additionalUnknownUnits".to_string(),
          serde_json::json!(extra),
        );
      }
    }
  }

  // Extract per-rule severity overrides from the config, keyed by canonical name.
  let rule_severities: std::collections::HashMap<String, gale_diagnostics::Severity> = config
    .rules
    .iter()
    .filter_map(|(name, cfg)| {
      cfg.severity.as_ref().and_then(|s| {
        let canonical = registry
          .get(name)
          .map(|r| r.name().to_string())
          .unwrap_or_else(|| name.clone());
        match s {
          gale_config::Severity::Error => Some((canonical, gale_diagnostics::Severity::Error)),
          gale_config::Severity::Warning => Some((canonical, gale_diagnostics::Severity::Warning)),
          gale_config::Severity::Off => None,
        }
      })
    })
    .collect();

  let mut runner = LintRunner::with_options_and_severities(
    registry,
    enabled_rules.clone(),
    rule_options,
    rule_severities,
  );
  runner
    .set_report_needless_disables(config.report_needless_disables || cli.report_needless_disables);
  runner.set_report_invalid_scope_disables(
    config.report_invalid_scope_disables || cli.report_invalid_scope_disables,
  );
  runner.set_report_descriptionless_disables(
    config.report_descriptionless_disables || cli.report_descriptionless_disables,
  );
  runner
    .set_report_unscoped_disables(config.report_unscoped_disables || cli.report_unscoped_disables);
  runner.set_ignore_disables(cli.ignore_disables || config.ignore_disables);
  // Plugin rules the config names but Gale does not implement still count
  // as configured for reportInvalidScopeDisables.
  runner.set_configured_rules(config.rules.keys().cloned().collect());
  runner.set_default_severity(config.default_severity.map(|s| match s {
    gale_config::Severity::Error => gale_diagnostics::Severity::Error,
    gale_config::Severity::Warning => gale_diagnostics::Severity::Warning,
    gale_config::Severity::Off => gale_diagnostics::Severity::Warning, // shouldn't happen
  }));
  let has_overrides = config.has_overrides();

  /// Pre-computed lint parameters for a resolved config.
  ///
  /// When multiple files share the same config (same directory or same config
  /// file in the hierarchy), we compute the enabled rules, options, and
  /// severities once and share them via `Arc` to avoid redundant allocations
  /// in the parallel linting loop.
  struct ResolvedLintParams {
    config: Arc<GaleConfig>,
    enabled_rules: Vec<String>,
    rule_options: std::collections::HashMap<String, serde_json::Value>,
    rule_severities: std::collections::HashMap<String, gale_diagnostics::Severity>,
    has_overrides: bool,
  }

  /// Build `ResolvedLintParams` from a `GaleConfig`, pre-computing enabled
  /// rules, options, and severities so the parallel loop avoids per-file
  /// allocation.
  fn build_lint_params(config: Arc<GaleConfig>, runner: &LintRunner) -> Arc<ResolvedLintParams> {
    let has_overrides = config.has_overrides();
    let enabled_rules: Vec<String> = config
      .rules
      .iter()
      .filter(|(_, cfg)| {
        cfg
          .severity
          .as_ref()
          .map(|s| !matches!(s, gale_config::Severity::Off))
          .unwrap_or(true)
      })
      .filter(|(name, _)| runner.has_rule(name))
      .map(|(name, _)| name.clone())
      .collect();
    let mut rule_options: std::collections::HashMap<String, serde_json::Value> = config
      .rules
      .iter()
      .filter_map(|(name, cfg)| {
        cfg.options.as_ref().map(|opts| {
          // Resolve to canonical name if this is a deprecated alias
          let canonical = runner
            .registry()
            .get(name)
            .map(|r| r.name().to_string())
            .unwrap_or_else(|| name.clone());
          (canonical, opts.clone())
        })
      })
      .collect();
    // Inject legacy unknown units for older Stylelint versions.
    if enabled_rules.iter().any(|r| r == "unit-no-unknown") {
      let detect_cwd = std::env::current_dir().unwrap_or_default();
      if let Some(extra) = detect_legacy_unknown_units(&detect_cwd) {
        let entry = rule_options
          .entry("unit-no-unknown".to_string())
          .or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = entry.as_object_mut() {
          obj.insert(
            "__additionalUnknownUnits".to_string(),
            serde_json::json!(extra),
          );
        }
      }
    }
    let rule_severities: std::collections::HashMap<String, gale_diagnostics::Severity> = config
      .rules
      .iter()
      .filter_map(|(name, cfg)| {
        cfg.severity.as_ref().and_then(|s| {
          // Resolve to canonical name if this is a deprecated alias
          let canonical = runner
            .registry()
            .get(name)
            .map(|r| r.name().to_string())
            .unwrap_or_else(|| name.clone());
          match s {
            gale_config::Severity::Error => Some((canonical, gale_diagnostics::Severity::Error)),
            gale_config::Severity::Warning => {
              Some((canonical, gale_diagnostics::Severity::Warning))
            }
            gale_config::Severity::Off => None,
          }
        })
      })
      .collect();
    Arc::new(ResolvedLintParams {
      config,
      enabled_rules,
      rule_options,
      rule_severities,
      has_overrides,
    })
  }

  /// Lint a single file using a fully resolved config (with overrides).
  ///
  /// Extracts enabled rules, options, and severities from the config (applying
  /// per-file overrides) and runs the linter.
  fn lint_file_with_resolved_config(
    runner: &LintRunner,
    source: &str,
    file_path: &str,
    syntax: gale_css_parser::Syntax,
    config: &GaleConfig,
  ) -> LintResult {
    // Stylelint marks files as fully ignored when they match an override's
    // `files` pattern AND its `ignoreFiles` pattern.  Return an empty
    // result so we don't produce false positives.
    if config.is_file_ignored_by_override(file_path) {
      return LintResult {
        file_path: file_path.to_string(),
        diagnostics: Vec::new(),
        source: source.to_string(),
      };
    }
    let effective_rules = config.rules_for_file(file_path);
    let file_enabled: Vec<String> = effective_rules
      .iter()
      .filter(|(_, cfg)| {
        cfg
          .severity
          .as_ref()
          .map(|s| !matches!(s, gale_config::Severity::Off))
          .unwrap_or(true)
      })
      .filter(|(name, _)| runner.has_rule(name))
      .map(|(name, _)| name.clone())
      .collect();
    let override_options: std::collections::HashMap<String, serde_json::Value> = effective_rules
      .iter()
      .filter_map(|(name, cfg)| {
        cfg
          .options
          .as_ref()
          .map(|opts| (name.clone(), opts.clone()))
      })
      .collect();
    let override_severities: std::collections::HashMap<String, gale_diagnostics::Severity> =
      effective_rules
        .iter()
        .filter_map(|(name, cfg)| {
          cfg.severity.as_ref().and_then(|s| match s {
            gale_config::Severity::Error => Some((name.clone(), gale_diagnostics::Severity::Error)),
            gale_config::Severity::Warning => {
              Some((name.clone(), gale_diagnostics::Severity::Warning))
            }
            gale_config::Severity::Off => None,
          })
        })
        .collect();
    runner.lint_source_with_rules(
      source,
      file_path,
      syntax,
      &file_enabled,
      &override_options,
      &override_severities,
    )
  }

  /// Lint a single file using pre-computed params, falling back to override
  /// resolution only when the config has overrides.
  fn lint_file_with_params(
    runner: &LintRunner,
    source: &str,
    file_path: &str,
    syntax: gale_css_parser::Syntax,
    params: &ResolvedLintParams,
  ) -> LintResult {
    if params.has_overrides {
      lint_file_with_resolved_config(runner, source, file_path, syntax, &params.config)
    } else {
      runner.lint_source_with_rules(
        source,
        file_path,
        syntax,
        &params.enabled_rules,
        &params.rule_options,
        &params.rule_severities,
      )
    }
  }

  /// Lint a single file, computing the effective rules from overrides if needed.
  fn lint_file(
    runner: &LintRunner,
    source: &str,
    file_path: &str,
    syntax: gale_css_parser::Syntax,
    config: &GaleConfig,
    has_overrides: bool,
  ) -> LintResult {
    if has_overrides {
      lint_file_with_resolved_config(runner, source, file_path, syntax, config)
    } else {
      runner.lint_source(source, file_path, syntax)
    }
  }

  // Lint: either from stdin or from discovered files.
  let mut results: Vec<LintResult> = if cli.stdin {
    let mut source = String::new();
    std::io::stdin().read_to_string(&mut source)?;

    let file_path = &cli.stdin_filename;
    match resolve_syntax(cli_custom_syntax, &config, file_path) {
      Some(syntax) => {
        vec![lint_file(
          &runner,
          &source,
          file_path,
          syntax,
          &config,
          has_overrides,
        )]
      }
      None => vec![],
    }
  } else {
    // Discover files.  `--ignore-pattern` globs join the config's
    // ignoreFiles so both exclude matches everywhere.
    let mut ignore_patterns = config.ignore_patterns.clone();
    ignore_patterns.extend(cli.ignore_pattern.iter().cloned());
    let discover_opts = DiscoverOptions {
      no_ignore: cli.no_ignore,
      ignore_path: cli.ignore_path.as_deref(),
      ignore_patterns: &ignore_patterns,
      disable_default_ignores: cli.disable_default_ignores,
    };
    let files = discover_files(&cli.files, &discover_opts);
    debug!("Discovered {} CSS file(s)", files.len());

    if files.is_empty() {
      if !allow_empty_input {
        // Stylelint raises NoFilesFoundError and exits 1 here; exiting
        // 0 would let a CI step that lints a mistyped path pass.
        let patterns = cli
          .files
          .iter()
          .map(|p| format!("\"{p}\""))
          .collect::<Vec<_>>()
          .join(", ");
        eprintln!("No files matching the pattern {patterns} were found.");
        process::exit(1);
      }
      return Ok(());
    }

    // -----------------------------------------------------------------
    // Pre-resolve configs for all files (lock-free parallel linting).
    //
    // When `--config` is not specified, Gale uses per-file config
    // resolution (walking up directories to find the nearest config).
    // Previously this was done inside the parallel loop behind a Mutex,
    // causing severe contention.  Now we resolve all configs upfront
    // (single-threaded, ~100 unique directories even for huge repos)
    // and store them as `Arc<ResolvedLintParams>` keyed by parent dir.
    // The parallel loop does a simple HashMap lookup — no locking.
    // -----------------------------------------------------------------
    let cwd = std::env::current_dir().unwrap_or_default();
    let dir_params: Option<std::collections::HashMap<PathBuf, Arc<ResolvedLintParams>>> =
      if use_per_file_config {
        let mut resolver = ConfigResolver::new();
        // Convert relative paths to absolute for the resolver.
        let abs_files: Vec<PathBuf> = files
          .iter()
          .map(|f| {
            if f.is_absolute() {
              f.clone()
            } else {
              cwd.join(f)
            }
          })
          .collect();
        let dir_to_config = resolver.resolve_all_for_files(&abs_files, &config);
        // Build ResolvedLintParams per config (dedup by Arc pointer).
        let mut params_by_ptr: std::collections::HashMap<usize, Arc<ResolvedLintParams>> =
          std::collections::HashMap::new();
        let dir_params: std::collections::HashMap<PathBuf, Arc<ResolvedLintParams>> = dir_to_config
          .into_iter()
          .map(|(dir, cfg_arc)| {
            let ptr = Arc::as_ptr(&cfg_arc) as usize;
            let params = params_by_ptr
              .entry(ptr)
              .or_insert_with(|| build_lint_params(cfg_arc, &runner))
              .clone();
            (dir, params)
          })
          .collect();
        debug!(
          "Pre-resolved {} directory configs ({} unique configs)",
          dir_params.len(),
          params_by_ptr.len()
        );
        Some(dir_params)
      } else {
        None
      };

    if use_cache {
      // With caching: read files, check cache, skip clean ones.
      let cache_mutex = Mutex::new(&mut lint_cache);
      let results: Vec<LintResult> = files
        .par_iter()
        .filter_map(|file| {
          let source = std::fs::read_to_string(file).ok()?;
          let file_path = file.display().to_string();

          // Skip files with unsupported customSyntax (from overrides
          // or top-level config).
          let effective_config = if let Some(ref dp) = dir_params {
            let abs = if file.is_absolute() {
              file.clone()
            } else {
              cwd.join(file)
            };
            let dir = abs.parent().unwrap_or(Path::new("."));
            dp.get(dir).map(|p| p.config.as_ref())
          } else {
            None
          };
          let cfg_for_check = effective_config.unwrap_or(&config);
          let syntax = resolve_syntax(cli_custom_syntax, cfg_for_check, &file_path)?;

          let content_hash = compute_hash(&source, config_hash);

          // Check cache: skip only files that were clean (0 diagnostics).
          {
            let cache = cache_mutex.lock().unwrap_or_else(|e| e.into_inner());
            if cache.is_clean(&file_path, content_hash) {
              debug!("Cache hit (clean): {file_path}");
              return None;
            }
          }

          let result = if let Some(ref dp) = dir_params {
            let abs = if file.is_absolute() {
              file.clone()
            } else {
              cwd.join(file)
            };
            let dir = abs.parent().unwrap_or(Path::new("."));
            if let Some(params) = dp.get(dir) {
              lint_file_with_params(&runner, &source, &file_path, syntax, params)
            } else {
              lint_file(&runner, &source, &file_path, syntax, &config, has_overrides)
            }
          } else {
            lint_file(&runner, &source, &file_path, syntax, &config, has_overrides)
          };

          // Update cache with the new result.
          {
            let mut cache = cache_mutex.lock().unwrap_or_else(|e| e.into_inner());
            cache.record(file_path, content_hash, result.diagnostics.len());
          }

          Some(result)
        })
        .collect();
      results
    } else {
      // Without caching: lint each file in parallel (lock-free).
      files
        .par_iter()
        .filter_map(|file| {
          let source = std::fs::read_to_string(file).ok()?;
          let file_path = file.display().to_string();

          // Skip files with unsupported customSyntax.
          let effective_config = if let Some(ref dp) = dir_params {
            let abs = if file.is_absolute() {
              file.clone()
            } else {
              cwd.join(file)
            };
            let dir = abs.parent().unwrap_or(Path::new("."));
            dp.get(dir).map(|p| p.config.as_ref())
          } else {
            None
          };
          let cfg_for_check = effective_config.unwrap_or(&config);
          let syntax = resolve_syntax(cli_custom_syntax, cfg_for_check, &file_path)?;

          if let Some(ref dp) = dir_params {
            let abs = if file.is_absolute() {
              file.clone()
            } else {
              cwd.join(file)
            };
            let dir = abs.parent().unwrap_or(Path::new("."));
            if let Some(params) = dp.get(dir) {
              Some(lint_file_with_params(
                &runner, &source, &file_path, syntax, params,
              ))
            } else {
              Some(lint_file(
                &runner,
                &source,
                &file_path,
                syntax,
                &config,
                has_overrides,
              ))
            }
          } else {
            Some(lint_file(
              &runner,
              &source,
              &file_path,
              syntax,
              &config,
              has_overrides,
            ))
          }
        })
        .collect()
    }
  };

  // Apply fixes when --fix is set.
  if let Some(fix_mode) = &fix_mode {
    let is_strict = fix_mode != "lax";
    let mut total_fixed = 0usize;

    // `apply_fixes` drops overlapping edits, so when two rules want to
    // rewrite the same span only one lands per pass (e.g. color-hex-case
    // then color-hex-length on the same `#FFFFFF`).  Iterate until the
    // source stops changing so a single `--fix` converges the way
    // Stylelint's does, with a ceiling in case two rules disagree and
    // would otherwise fight forever.
    const MAX_FIX_PASSES: usize = 10;

    for result in &mut results {
      // In strict mode, skip files that have parse errors.
      if is_strict
        && result
          .diagnostics
          .iter()
          .any(|d| d.rule_name == "parse-error")
      {
        continue;
      }

      // These files were linted, so a parser was already chosen for them;
      // the config's own customSyntax only ever decides skipping.
      let syntax = cli_custom_syntax
        .and_then(syntax_for_custom_syntax)
        .unwrap_or_else(|| detect_syntax(&result.file_path));
      let mut current = result.source.clone();
      let mut file_fixed = 0usize;

      for _ in 0..MAX_FIX_PASSES {
        let (fixed_source, count) = apply_fixes(&current, &result.diagnostics);
        if count == 0 || fixed_source == current {
          break;
        }
        current = fixed_source;
        file_fixed += count;
        // Re-lint the fixed source to get remaining diagnostics, which
        // also feeds the next pass.
        *result = lint_file(
          &runner,
          &current,
          &result.file_path,
          syntax,
          &config,
          has_overrides,
        );
      }

      if file_fixed == 0 {
        continue;
      }

      if cli.stdin {
        // For stdin + fix, output the fixed source to stdout.
        print!("{current}");
        total_fixed += file_fixed;
      } else if let Err(err) = std::fs::write(&result.file_path, &current) {
        eprintln!("Error writing {}: {err}", result.file_path);
      } else {
        total_fixed += file_fixed;
      }
    }
    if total_fixed > 0 {
      eprintln!("Fixed {total_fixed} problem(s).");
    }
  }

  // Filter to errors-only when --quiet is set.
  if quiet {
    for result in &mut results {
      result.diagnostics.retain(|d| d.severity == Severity::Error);
    }
  }

  // Save cache if --cache is enabled.
  if use_cache {
    debug!("Saving cache to {}", cache_path.display());
    lint_cache.save(&cache_path);
  }

  // Format & print.
  let t_fmt = std::time::Instant::now();
  let color = color_enabled(
    cli.color,
    cli.no_color,
    std::io::IsTerminal::is_terminal(&std::io::stdout()),
    |name| std::env::var(name).ok(),
  );
  let formatter = create_formatter_with_color(&cli.formatter, color);
  let output = gale_formatter::Formatter::format(&*formatter, &results);
  if std::env::var("GALE_DEBUG_PERF").as_deref() == Ok("1") {
    eprintln!("[perf] format: {:.3}s", t_fmt.elapsed().as_secs_f64());
  }
  if !output.is_empty() {
    print!("{output}");
    if let Some(path) = &cli.output_file
      && let Err(err) = write_output_file(path, &output)
    {
      eprintln!("Error writing report to {}: {err}", path.display());
    }
  }

  // Summarise counts.
  let total_errors: usize = results
    .iter()
    .map(|r| r.count_by_severity(Severity::Error))
    .sum();
  let total_warnings: usize = results
    .iter()
    .map(|r| r.count_by_severity(Severity::Warning))
    .sum();

  // Check --max-warnings threshold.
  if let Some(max) = cli.max_warnings
    && total_warnings > max
  {
    eprintln!("Found {total_warnings} warning(s) (max allowed: {max})");
    process::exit(1);
  }

  // Exit with code 1 if there were any errors.
  if total_errors > 0 {
    process::exit(1);
  }

  Ok(())
}

/// Detect whether the project uses an older Stylelint version (< 15) that
/// doesn't recognize certain modern CSS units (dynamic viewport units,
/// container query units, etc.).
///
/// Returns `Some(units)` with the list of units to treat as unknown, or
/// `None` if Stylelint is >= 15 or not installed.
fn detect_legacy_unknown_units(cwd: &Path) -> Option<Vec<String>> {
  // Walk up from cwd to find node_modules/stylelint/package.json.
  let mut dir = cwd.to_path_buf();
  loop {
    let pkg = dir
      .join("node_modules")
      .join("stylelint")
      .join("package.json");
    if pkg.exists() {
      let contents = std::fs::read_to_string(&pkg).ok()?;
      let parsed: serde_json::Value = serde_json::from_str(&contents).ok()?;
      let version_str = parsed.get("version")?.as_str()?;
      let major: u32 = version_str.split('.').next()?.parse().ok()?;
      if major < 15 {
        // Stylelint < 15 doesn't know dynamic viewport units, container
        // query units, or several other modern CSS units.
        return Some(
          vec![
            "dvh", "dvw", "dvb", "dvi", "dvmax", "dvmin", "lvh", "lvw", "lvb", "lvi", "lvmax",
            "lvmin", "svh", "svw", "svb", "svi", "svmax", "svmin", "cqw", "cqh", "cqi", "cqb",
            "cqmin", "cqmax", "cap", "ic", "rcap", "rch", "rex", "ric", "vb", "vi",
          ]
          .into_iter()
          .map(|s| s.to_string())
          .collect(),
        );
      }
      return None;
    }
    if !dir.pop() {
      return None;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  /// Helper: create a temporary directory tree with CSS files for glob tests.
  fn create_test_tree(base: &Path) {
    let sub = base.join("sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(base.join("a.css"), "a {}").unwrap();
    fs::write(base.join("b.scss"), "$x: 1;").unwrap();
    fs::write(base.join("c.txt"), "not css").unwrap();
    fs::write(sub.join("d.css"), "d {}").unwrap();
    fs::write(sub.join("e.less"), ".e {}").unwrap();
  }

  fn default_opts() -> DiscoverOptions<'static> {
    DiscoverOptions {
      no_ignore: true,
      ignore_path: None,
      ignore_patterns: &[],
      disable_default_ignores: false,
    }
  }

  #[test]
  fn test_is_glob_pattern() {
    assert!(is_glob_pattern("**/*.css"));
    assert!(is_glob_pattern("src/*.scss"));
    assert!(is_glob_pattern("src/{a,b}.css"));
    assert!(is_glob_pattern("src/[ab].css"));
    assert!(is_glob_pattern("src/??.css"));
    assert!(!is_glob_pattern("src/file.css"));
    assert!(!is_glob_pattern("src/dir"));
    assert!(!is_glob_pattern("plain-path"));
  }

  #[test]
  fn test_glob_star_css() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let pattern = format!("{}/*.css", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts());

    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("a.css"));
  }

  #[test]
  fn test_glob_recursive_double_star() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let pattern = format!("{}/**/*.css", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts());

    // Should find a.css and sub/d.css
    assert_eq!(files.len(), 2);
    let names: Vec<String> = files
      .iter()
      .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
      .collect();
    assert!(names.contains(&"a.css".to_string()));
    assert!(names.contains(&"d.css".to_string()));
  }

  #[test]
  fn test_glob_scss_only() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let pattern = format!("{}/**/*.scss", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts());

    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("b.scss"));
  }

  #[test]
  fn test_glob_no_match_returns_empty() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let pattern = format!("{}/**/*.sass", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts());

    assert!(files.is_empty());
  }

  #[test]
  fn test_glob_mixed_with_directory_arg() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    // Mix a glob pattern with a plain directory argument.
    let glob_pat = format!("{}/*.scss", tmp.path().display());
    let dir_arg = tmp.path().join("sub").display().to_string();
    let files = discover_files(&[glob_pat, dir_arg], &default_opts());

    // Glob should find b.scss; directory walk should find d.css + e.less.
    assert_eq!(files.len(), 3);
  }

  #[test]
  fn test_non_glob_file_still_works() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let file_arg = tmp.path().join("a.css").display().to_string();
    let files = discover_files(&[file_arg], &default_opts());

    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("a.css"));
  }

  #[test]
  fn test_non_glob_directory_still_works() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    let dir_arg = tmp.path().display().to_string();
    let files = discover_files(&[dir_arg], &default_opts());

    // Should find all CSS files recursively: a.css, b.scss, sub/d.css, sub/e.less
    assert_eq!(files.len(), 4);
  }

  #[test]
  fn test_glob_brace_expansion_css_scss() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    // Brace expansion: should find both .css and .scss files recursively.
    let pattern = format!("{}/**/*.{{css,scss}}", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts());

    // a.css, b.scss, sub/d.css (not sub/e.less)
    assert_eq!(files.len(), 3);
  }

  #[test]
  fn node_modules_is_walked_only_with_disable_default_ignores() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.css"), "a {}").unwrap();
    let nm = tmp.path().join("node_modules").join("pkg");
    std::fs::create_dir_all(&nm).unwrap();
    std::fs::write(nm.join("x.css"), "a {}").unwrap();
    let pattern = format!("{}/**/*.css", tmp.path().display());

    let default = discover_files(
      std::slice::from_ref(&pattern),
      &DiscoverOptions {
        no_ignore: false,
        ..default_opts()
      },
    );
    assert_eq!(default.len(), 1);

    let lifted = discover_files(
      std::slice::from_ref(&pattern),
      &DiscoverOptions {
        no_ignore: false,
        disable_default_ignores: true,
        ..default_opts()
      },
    );
    assert_eq!(lifted.len(), 2);
  }

  #[test]
  fn ignore_patterns_exclude_matching_files_from_a_walk() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());
    let pattern = format!("{}/**/*.css", tmp.path().display());
    let patterns = vec!["**/sub/**".to_string()];

    let files = discover_files(
      std::slice::from_ref(&pattern),
      &DiscoverOptions {
        no_ignore: false,
        ignore_patterns: &patterns,
        ..default_opts()
      },
    );
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("a.css"));
  }

  #[test]
  fn colour_follows_picocolors_rules() {
    let none = |_: &str| None;
    let with = |vars: &'static [(&'static str, &'static str)]| {
      move |name: &str| {
        vars
          .iter()
          .find(|(k, _)| *k == name)
          .map(|(_, v)| v.to_string())
      }
    };

    // Piped output with no hints: off.  A terminal: on.
    assert!(!color_enabled(false, false, false, none));
    assert!(color_enabled(false, false, true, none));
    assert!(!color_enabled(
      false,
      false,
      true,
      with(&[("TERM", "dumb")])
    ));

    // Flags and FORCE_COLOR / CI turn it on when piped.
    assert!(color_enabled(true, false, false, none));
    assert!(color_enabled(
      false,
      false,
      false,
      with(&[("FORCE_COLOR", "1")])
    ));
    assert!(color_enabled(false, false, false, with(&[("CI", "true")])));

    // NO_COLOR and --no-color win over everything.
    assert!(!color_enabled(
      false,
      true,
      true,
      with(&[("FORCE_COLOR", "1")])
    ));
    assert!(!color_enabled(
      true,
      false,
      true,
      with(&[("NO_COLOR", "1")])
    ));

    // Empty variables do not count as set.
    assert!(color_enabled(false, false, true, with(&[("NO_COLOR", "")])));
    assert!(!color_enabled(
      false,
      false,
      false,
      with(&[("FORCE_COLOR", "")])
    ));
  }

  #[test]
  fn custom_syntax_names_map_to_parsers() {
    assert_eq!(syntax_for_custom_syntax("postcss"), Some(Syntax::Css));
    assert_eq!(syntax_for_custom_syntax("postcss-scss"), Some(Syntax::Scss));
    assert_eq!(syntax_for_custom_syntax("PostCSS-Less"), Some(Syntax::Less));
    assert_eq!(syntax_for_custom_syntax("postcss-sass"), Some(Syntax::Sass));
    assert_eq!(syntax_for_custom_syntax("postcss-html"), None);
    assert_eq!(syntax_for_custom_syntax("postcss-markdown"), None);
  }

  #[test]
  fn resolve_syntax_detects_from_the_extension_by_default() {
    let config = GaleConfig::default();
    assert_eq!(resolve_syntax(None, &config, "a.css"), Some(Syntax::Css));
    assert_eq!(resolve_syntax(None, &config, "a.scss"), Some(Syntax::Scss));
  }

  #[test]
  fn resolve_syntax_lets_the_cli_flag_force_a_parser() {
    let config = GaleConfig::default();
    assert_eq!(
      resolve_syntax(Some("postcss-scss"), &config, "a.css"),
      Some(Syntax::Scss)
    );
    // An unsupported flag skips the file rather than parsing it.
    assert_eq!(resolve_syntax(Some("postcss-html"), &config, "a.css"), None);
  }

  #[test]
  fn resolve_syntax_skips_files_whose_config_syntax_is_unsupported() {
    let config = GaleConfig {
      custom_syntax: Some("postcss-markdown".to_string()),
      ..Default::default()
    };
    assert_eq!(resolve_syntax(None, &config, "a.css"), None);
    // The CLI flag overrides the config's customSyntax entirely.
    assert_eq!(
      resolve_syntax(Some("postcss-less"), &config, "a.css"),
      Some(Syntax::Less)
    );
  }

  #[test]
  fn resolve_syntax_keeps_extension_detection_for_supported_config_syntax() {
    // A supported config `customSyntax` decides only that the file is not
    // skipped; the parser still comes from the extension.
    let config = GaleConfig {
      custom_syntax: Some("postcss-scss".to_string()),
      ..Default::default()
    };
    assert_eq!(resolve_syntax(None, &config, "a.css"), Some(Syntax::Css));
  }

  #[test]
  fn output_file_is_written_without_ansi_into_new_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("out").join("nested").join("report.txt");
    write_output_file(&path, "\x1b[31mred\x1b[39m plain\n").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "red plain\n");
  }

  #[test]
  fn test_glob_brace_expansion_relative() {
    // Test that a bare `**/*.{css,scss}` (no absolute prefix, starting with **)
    // resolves to cwd as the walk root and does not panic.
    // We just verify that is_glob_pattern detects it correctly.
    assert!(is_glob_pattern("**/*.{css,scss}"));
  }

  #[test]
  fn test_expand_braces_directory_alternatives() {
    let result = expand_braces("{public/sass,packages}/**/*.scss");
    assert_eq!(result, vec!["public/sass/**/*.scss", "packages/**/*.scss"]);
  }

  #[test]
  fn test_expand_braces_extension_not_expanded() {
    // Extension braces don't contain `/`, so they should NOT be expanded
    // (globset handles them natively).
    let result = expand_braces("src/**/*.{css,scss}");
    assert_eq!(result, vec!["src/**/*.{css,scss}"]);
  }

  #[test]
  fn test_expand_braces_no_braces() {
    let result = expand_braces("src/**/*.css");
    assert_eq!(result, vec!["src/**/*.css"]);
  }

  #[test]
  fn test_expand_braces_at_start() {
    let result = expand_braces("{src,lib}/**/*.css");
    assert_eq!(result, vec!["src/**/*.css", "lib/**/*.css"]);
  }

  #[test]
  fn test_expand_braces_three_alternatives() {
    let result = expand_braces("{a,b,c}/**/*.scss");
    assert_eq!(result, vec!["a/**/*.scss", "b/**/*.scss", "c/**/*.scss"]);
  }

  #[test]
  fn test_expand_braces_mixed_path_and_extension() {
    // Pattern with both path braces and extension braces.
    let result = expand_braces("{src,lib}/**/*.{css,scss}");
    assert_eq!(result, vec!["src/**/*.{css,scss}", "lib/**/*.{css,scss}"]);
  }

  // -----------------------------------------------------------------------
  // Extglob conversion tests
  // -----------------------------------------------------------------------

  #[test]
  fn test_has_extglob() {
    assert!(has_extglob("*.+(css|scss)"));
    assert!(has_extglob("src/**/*.@(css|scss)"));
    assert!(has_extglob("?(a|b).css"));
    assert!(has_extglob("!(a|b).css"));
    // Bare (alt|alt) at segment start
    assert!(has_extglob("(src|docs)/**/*.css"));
    assert!(has_extglob("root/(src|docs)/**/*.css"));
    // Not extglob
    assert!(!has_extglob("src/**/*.css"));
    assert!(!has_extglob("src/**/*.{css,scss}"));
    // A `(` that is NOT at segment boundary and has no extglob prefix
    assert!(!has_extglob("src/file(1).css"));
  }

  #[test]
  fn test_convert_extglob_plus() {
    // +(css|scss) -> {css,scss}
    assert_eq!(
      convert_extglob("src/**/*.+(css|scss)"),
      "src/**/*.{css,scss}"
    );
  }

  #[test]
  fn test_convert_extglob_at() {
    // @(css|scss) -> {css,scss}
    assert_eq!(
      convert_extglob("src/**/*.@(css|scss)"),
      "src/**/*.{css,scss}"
    );
  }

  #[test]
  fn test_convert_extglob_question() {
    // ?(css|scss) -> {css,scss,}  (zero or one)
    assert_eq!(
      convert_extglob("src/**/*.?(css|scss)"),
      "src/**/*.{css,scss,}"
    );
  }

  #[test]
  fn test_convert_extglob_star() {
    // *(css|scss) -> {css,scss,}  (zero or more)
    assert_eq!(
      convert_extglob("src/**/*.*(css|scss)"),
      "src/**/*.{css,scss,}"
    );
  }

  #[test]
  fn test_convert_extglob_bare_parens() {
    // Bare (src|docs) at start -> {src,docs}
    assert_eq!(
      convert_extglob("(src|docs)/**/*.css"),
      "{src,docs}/**/*.css"
    );
  }

  #[test]
  fn test_convert_extglob_bare_parens_mid_path() {
    // Bare (src|docs) after a slash -> {src,docs}
    assert_eq!(
      convert_extglob("root/(src|docs)/**/*.css"),
      "root/{src,docs}/**/*.css"
    );
  }

  #[test]
  fn test_convert_extglob_angular_pattern() {
    // The exact pattern from angular-components:
    // (src|docs)/**/*.+(css|scss)
    assert_eq!(
      convert_extglob("(src|docs)/**/*.+(css|scss)"),
      "{src,docs}/**/*.{css,scss}"
    );
  }

  #[test]
  fn test_convert_extglob_no_change() {
    // Standard glob patterns should pass through unchanged.
    assert_eq!(convert_extglob("src/**/*.css"), "src/**/*.css");
    assert_eq!(
      convert_extglob("src/**/*.{css,scss}"),
      "src/**/*.{css,scss}"
    );
    assert_eq!(convert_extglob("**/*.css"), "**/*.css");
  }

  #[test]
  fn test_is_glob_pattern_detects_extglob() {
    assert!(is_glob_pattern("src/**/*.+(css|scss)"));
    assert!(is_glob_pattern("(src|docs)/**/*.css"));
  }

  #[test]
  fn test_extglob_file_discovery() {
    let tmp = tempfile::tempdir().unwrap();
    create_test_tree(tmp.path());

    // Use extglob pattern +(css|scss) to find both CSS and SCSS files.
    let pattern = format!("{}/**/*.+(css|scss)", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts());

    // Should find a.css, b.scss, sub/d.css (not c.txt or sub/e.less)
    assert_eq!(files.len(), 3);
    let names: Vec<String> = files
      .iter()
      .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
      .collect();
    assert!(names.contains(&"a.css".to_string()));
    assert!(names.contains(&"b.scss".to_string()));
    assert!(names.contains(&"d.css".to_string()));
  }

  #[test]
  fn test_extglob_bare_parens_file_discovery() {
    let tmp = tempfile::tempdir().unwrap();
    // Create directory structure with two source dirs
    let src = tmp.path().join("src");
    let docs = tmp.path().join("docs");
    let other = tmp.path().join("other");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&docs).unwrap();
    fs::create_dir_all(&other).unwrap();
    fs::write(src.join("a.css"), "a {}").unwrap();
    fs::write(docs.join("b.scss"), "$x: 1;").unwrap();
    fs::write(other.join("c.css"), "c {}").unwrap();

    // Pattern: (src|docs)/**/*.+(css|scss) — should find files in src/ and docs/ only
    let pattern = format!("{}/(src|docs)/**/*.+(css|scss)", tmp.path().display());
    let files = discover_files(&[pattern], &default_opts());

    assert_eq!(files.len(), 2);
    let names: Vec<String> = files
      .iter()
      .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
      .collect();
    assert!(names.contains(&"a.css".to_string()));
    assert!(names.contains(&"b.scss".to_string()));
    // c.css in other/ should NOT be included
    assert!(!names.contains(&"c.css".to_string()));
  }
}
