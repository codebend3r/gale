use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing::debug;

use gale_config::GaleConfig;
use gale_css_parser::detect_syntax;
use gale_diagnostics::{Diagnostic as GaleDiagnostic, Severity, SourceLineIndex, Span};
use gale_linter::{LintRunner, RuleRegistry};

// ---------------------------------------------------------------------------
// UTF-16 column conversion
// ---------------------------------------------------------------------------

/// Convert a byte-based column offset to a UTF-16 code-unit offset.
///
/// The LSP specification requires `Position.character` to be measured in UTF-16
/// code units. `SourceLineIndex` returns byte-based columns, so we must convert
/// by iterating through the characters on the line and summing `len_utf16()`.
///
/// `line_start_byte` is the byte offset where the line begins and `byte_col` is
/// the number of bytes from that start (0-indexed).
fn byte_col_to_utf16(source: &str, line_start_byte: usize, byte_col: usize) -> u32 {
  let end = (line_start_byte + byte_col).min(source.len());
  let slice = &source[line_start_byte..end];
  slice.chars().map(|ch| ch.len_utf16() as u32).sum()
}

/// Convert a byte span in `source` to an LSP range (0-indexed lines, UTF-16
/// characters).
fn span_to_range(source: &str, line_index: &SourceLineIndex, span: Span) -> Range {
  let (start_line, start_col) = line_index.offset_to_location(span.offset);
  let (end_line, end_col) = line_index.offset_to_location(span.end());

  // SourceLineIndex returns 1-indexed line/col where col is byte-based.
  let start_line_byte = span.offset - (start_col - 1);
  let end_line_byte = span.end() - (end_col - 1);

  Range {
    start: Position {
      line: start_line.saturating_sub(1) as u32,
      character: byte_col_to_utf16(source, start_line_byte, start_col - 1),
    },
    end: Position {
      line: end_line.saturating_sub(1) as u32,
      character: byte_col_to_utf16(source, end_line_byte, end_col - 1),
    },
  }
}

/// Whether two ranges share at least one position.
fn ranges_overlap(a: &Range, b: &Range) -> bool {
  a.start <= b.end && b.start <= a.end
}

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

/// The last text the client sent for a document and what the linter found in
/// it, kept so code actions can offer the fixes those diagnostics carry.
struct Document {
  text: String,
  diagnostics: Vec<GaleDiagnostic>,
}

pub struct GaleLspServer {
  client: Client,
  runner: RwLock<Option<LintRunner>>,
  /// Explicit config path from `gale --lsp --config <path>`, if any.
  config_path: Option<PathBuf>,
  documents: RwLock<HashMap<Url, Document>>,
}

impl GaleLspServer {
  fn new(client: Client, config_path: Option<PathBuf>) -> Self {
    Self {
      client,
      runner: RwLock::new(None),
      config_path,
      documents: RwLock::new(HashMap::new()),
    }
  }

  /// Build `LintRunner` from the resolved config.
  ///
  /// This must configure the runner exactly as the CLI does — enabled rules,
  /// per-rule options, per-rule severities and the default severity —
  /// otherwise the editor reports different results than `gale` on the same
  /// file with the same config.
  fn build_runner(config: &GaleConfig, has_config_file: bool) -> LintRunner {
    let registry = RuleRegistry::default();

    let enabled_rules: Vec<String> = if config.rules.is_empty() && !has_config_file {
      // No config file found — fall back to the recommended preset rather
      // than every rule.  Enabling all of them (including the whole
      // @stylistic namespace) buries a config-less project in
      // formatting warnings it never asked for.
      gale_config::recommended_rule_names()
        .iter()
        .map(|name| name.to_string())
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
        .map(|(name, _)| name.clone())
        .collect()
    };

    // Resolve config keys (which may be deprecated aliases) to the
    // canonical rule names the runner looks options up by.
    let canonical = |name: &String| {
      registry
        .get(name)
        .map(|r| r.name().to_string())
        .unwrap_or_else(|| name.clone())
    };

    let rule_options: HashMap<String, serde_json::Value> = config
      .rules
      .iter()
      .filter_map(|(name, cfg)| {
        cfg
          .options
          .as_ref()
          .map(|opts| (canonical(name), opts.clone()))
      })
      .collect();

    let rule_severities: HashMap<String, Severity> = config
      .rules
      .iter()
      .filter_map(|(name, cfg)| match cfg.severity.as_ref()? {
        gale_config::Severity::Error => Some((canonical(name), Severity::Error)),
        gale_config::Severity::Warning => Some((canonical(name), Severity::Warning)),
        gale_config::Severity::Off => None,
      })
      .collect();

    let mut runner = LintRunner::with_options_and_severities(
      registry,
      enabled_rules,
      rule_options,
      rule_severities,
    );
    runner.set_default_severity(config.default_severity.map(|s| match s {
      gale_config::Severity::Error => Severity::Error,
      gale_config::Severity::Warning => Severity::Warning,
      gale_config::Severity::Off => Severity::Warning,
    }));
    runner
  }

  /// Convert one of Gale's diagnostics to the LSP shape.
  fn to_lsp_diagnostic(
    d: &GaleDiagnostic,
    source: &str,
    line_index: &SourceLineIndex,
  ) -> tower_lsp::lsp_types::Diagnostic {
    let severity = match d.severity {
      Severity::Error => Some(DiagnosticSeverity::ERROR),
      Severity::Warning => Some(DiagnosticSeverity::WARNING),
      Severity::Info => Some(DiagnosticSeverity::INFORMATION),
      Severity::Hint => Some(DiagnosticSeverity::HINT),
    };

    tower_lsp::lsp_types::Diagnostic {
      range: span_to_range(source, line_index, d.span),
      severity,
      code: Some(NumberOrString::String(d.rule_name.clone())),
      source: Some("gale".to_string()),
      message: d.message.clone(),
      ..Default::default()
    }
  }

  /// Lint source text (sync part), returning Gale's own diagnostics.
  fn lint(&self, uri: &Url, source: &str) -> Vec<GaleDiagnostic> {
    let runner_guard = self.runner.read().unwrap_or_else(|e| e.into_inner());
    let Some(runner) = runner_guard.as_ref() else {
      return Vec::new();
    };

    let file_path = uri
      .to_file_path()
      .map(|p| p.display().to_string())
      .unwrap_or_else(|_| uri.to_string());

    let syntax = detect_syntax(&file_path);
    runner.lint_source(source, &file_path, syntax).diagnostics
  }

  /// Lint source text, remember it for code actions, and publish
  /// diagnostics to the client.
  async fn lint_and_publish(&self, uri: Url, source: &str) {
    let diagnostics = self.lint(&uri, source);
    let line_index = SourceLineIndex::build(source);
    let lsp_diagnostics = diagnostics
      .iter()
      .map(|d| Self::to_lsp_diagnostic(d, source, &line_index))
      .collect();

    self
      .documents
      .write()
      .unwrap_or_else(|e| e.into_inner())
      .insert(
        uri.clone(),
        Document {
          text: source.to_string(),
          diagnostics,
        },
      );

    self
      .client
      .publish_diagnostics(uri, lsp_diagnostics, None)
      .await;
  }

  /// Quick fixes for the fixable diagnostics that touch `range` in the
  /// document at `uri`, built from the last text the client sent.
  fn quick_fixes(&self, uri: &Url, range: &Range) -> Vec<CodeActionOrCommand> {
    let documents = self.documents.read().unwrap_or_else(|e| e.into_inner());
    let Some(doc) = documents.get(uri) else {
      return Vec::new();
    };
    let line_index = SourceLineIndex::build(&doc.text);

    doc
      .diagnostics
      .iter()
      .filter_map(|d| {
        let fix = d.fix.as_ref()?;
        let diag_range = span_to_range(&doc.text, &line_index, d.span);
        if !ranges_overlap(&diag_range, range) {
          return None;
        }

        let edits: Vec<TextEdit> = fix
          .edits
          .iter()
          .map(|edit| TextEdit {
            range: span_to_range(&doc.text, &line_index, edit.span),
            new_text: edit.new_text.clone(),
          })
          .collect();
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), edits);

        Some(CodeActionOrCommand::CodeAction(CodeAction {
          title: format!("Fix: {}", d.message),
          kind: Some(CodeActionKind::QUICKFIX),
          diagnostics: Some(vec![Self::to_lsp_diagnostic(d, &doc.text, &line_index)]),
          edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
          }),
          is_preferred: Some(true),
          ..Default::default()
        }))
      })
      .collect()
  }
}

// ---------------------------------------------------------------------------
// LanguageServer trait implementation
// ---------------------------------------------------------------------------

#[tower_lsp::async_trait]
impl LanguageServer for GaleLspServer {
  async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
    // An explicit `--config` wins over discovery, matching the CLI.
    let (config, has_config_file) = if let Some(path) = &self.config_path {
      match gale_config::load_config(path) {
        Ok(cfg) => (cfg, true),
        Err(err) => {
          self
            .client
            .log_message(
              MessageType::ERROR,
              format!("Failed to load config {}: {err}", path.display()),
            )
            .await;
          (GaleConfig::default(), false)
        }
      }
    } else if let Some(root_uri) = params.root_uri {
      if let Ok(root_path) = root_uri.to_file_path() {
        debug!("LSP workspace root: {}", root_path.display());
        match gale_config::resolve_config(&root_path) {
          Some(cfg) => (cfg, true),
          None => (GaleConfig::default(), false),
        }
      } else {
        (GaleConfig::default(), false)
      }
    } else {
      let cwd = std::env::current_dir().unwrap_or_default();
      match gale_config::resolve_config(&cwd) {
        Some(cfg) => (cfg, true),
        None => (GaleConfig::default(), false),
      }
    };

    // Build the lint runner once.
    let runner = Self::build_runner(&config, has_config_file);
    *self.runner.write().unwrap_or_else(|e| e.into_inner()) = Some(runner);

    Ok(InitializeResult {
      capabilities: ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        ..Default::default()
      },
      server_info: Some(ServerInfo {
        name: "gale-lsp".to_string(),
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
      }),
    })
  }

  async fn initialized(&self, _: InitializedParams) {
    debug!("Gale LSP server initialized");
  }

  async fn shutdown(&self) -> Result<()> {
    Ok(())
  }

  async fn did_open(&self, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri;
    let source = params.text_document.text;
    self.lint_and_publish(uri, &source).await;
  }

  async fn did_change(&self, params: DidChangeTextDocumentParams) {
    // We use full sync, so the last change event contains the full text.
    let uri = params.text_document.uri;
    if let Some(change) = params.content_changes.into_iter().last() {
      self.lint_and_publish(uri, &change.text).await;
    }
  }

  async fn did_close(&self, params: DidCloseTextDocumentParams) {
    self
      .documents
      .write()
      .unwrap_or_else(|e| e.into_inner())
      .remove(&params.text_document.uri);
    // Clear diagnostics for the closed file, otherwise the editor keeps
    // showing warnings for a document that is no longer open.
    self
      .client
      .publish_diagnostics(params.text_document.uri, Vec::new(), None)
      .await;
  }

  async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    Ok(Some(
      self.quick_fixes(&params.text_document.uri, &params.range),
    ))
  }

  async fn did_save(&self, params: DidSaveTextDocumentParams) {
    let uri = params.text_document.uri;
    // If the save notification includes text, use it; otherwise read from disk.
    let source = if let Some(text) = params.text {
      text
    } else if let Ok(path) = uri.to_file_path() {
      match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return,
      }
    } else {
      return;
    };
    self.lint_and_publish(uri, &source).await;
  }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Start the Gale LSP server on stdin/stdout.
///
/// `config_path` mirrors the CLI's `--config` flag; when `None` the server
/// discovers a config from the workspace root the client reports.
pub async fn run_server(config_path: Option<PathBuf>) {
  let stdin = tokio::io::stdin();
  let stdout = tokio::io::stdout();

  let (service, socket) =
    LspService::new(move |client| GaleLspServer::new(client, config_path.clone()));
  Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn span_to_range_uses_zero_based_lines_and_utf16_columns() {
    let source = "a { color: #FFF; }\nb { }\n";
    let line_index = SourceLineIndex::build(source);
    let range = span_to_range(source, &line_index, Span::new(11, 4));
    assert_eq!(range.start, Position::new(0, 11));
    assert_eq!(range.end, Position::new(0, 15));

    let range = span_to_range(source, &line_index, Span::new(23, 1));
    assert_eq!(range.start, Position::new(1, 4));
  }

  #[test]
  fn ranges_overlap_when_they_share_a_position() {
    let r = |sl, sc, el, ec| Range::new(Position::new(sl, sc), Position::new(el, ec));
    assert!(ranges_overlap(&r(0, 11, 0, 15), &r(0, 0, 0, 18)));
    assert!(ranges_overlap(&r(0, 11, 0, 15), &r(0, 15, 0, 15)));
    assert!(!ranges_overlap(&r(0, 11, 0, 15), &r(1, 0, 1, 3)));
    assert!(!ranges_overlap(&r(0, 11, 0, 15), &r(0, 0, 0, 10)));
  }
}
