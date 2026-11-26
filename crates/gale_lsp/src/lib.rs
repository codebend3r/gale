use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing::debug;

use gale_config::{
  ConfigWarning, ResolverOptions, RootConfig, RuleSetResolver, RunSettings, SkippedRuleWarnings,
  WithoutConfig, unsupported_plugin_warnings,
};
use gale_diagnostics::{Diagnostic as GaleDiagnostic, Severity, SourceLineIndex, Span};
use gale_linter::LintEnv;
use gale_linter::panic_guard;

// ---------------------------------------------------------------------------
// UTF-16 column conversion
// ---------------------------------------------------------------------------

/// Convert a byte span to an LSP range: 0-indexed lines and UTF-16
/// characters, as the protocol counts them.  `SourceLineIndex` does the
/// counting, so an offset inside a multibyte character (a rule bug) is
/// pulled back to the start of that character rather than panicking.
fn span_to_range(line_index: &SourceLineIndex, span: Span) -> Range {
  let position = |offset: usize| {
    let (line, character) = line_index.offset_to_utf16_position(offset);
    Position::new(line.saturating_sub(1) as u32, character as u32)
  };
  Range {
    start: position(span.offset),
    end: position(span.end()),
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

/// Each document's settings, and the skipped rules already logged for the
/// configs they came from.
struct Workspace {
  resolver: RuleSetResolver,
  skipped: SkippedRuleWarnings,
}

/// What linting a document found, and the warnings about configs it was
/// the first to load.
struct Linted {
  diagnostics: Vec<GaleDiagnostic>,
  warnings: Vec<ConfigWarning>,
}

pub struct GaleLspServer {
  client: Client,
  /// Each document's settings, from the same layer the CLI uses, once
  /// `initialize` has found the workspace's config.
  workspace: Mutex<Option<Workspace>>,
  /// Explicit config path from `gale --lsp --config <path>`, if any.
  config_path: Option<PathBuf>,
  documents: RwLock<HashMap<Url, Document>>,
}

impl GaleLspServer {
  /// Starts with no configs; `initialize` finds them.
  fn new(client: Client, config_path: Option<PathBuf>) -> Self {
    Self {
      client,
      workspace: Mutex::new(None),
      config_path,
      documents: RwLock::new(HashMap::new()),
    }
  }

  /// The resolver for documents in the workspace at `root`: every document
  /// gets its closest config with the overrides that match it, as `gale`
  /// gives a file on the command line, or the `--config` file when one was
  /// named.  A document no config covers gets the recommended preset:
  /// enabling every rule (including the whole @stylistic namespace) would
  /// bury a config-less project in formatting warnings it never asked for.
  fn build_resolver(config: RootConfig, root: &Path) -> RuleSetResolver {
    RuleSetResolver::new(ResolverOptions {
      source: config.into_source(),
      without_config: WithoutConfig::Recommended,
      forced_syntax: None,
      run: RunSettings {
        env: LintEnv::detect(root),
        ..RunSettings::default()
      },
      cwd: root.to_path_buf(),
    })
  }

  /// Log each warning about the configs to the client.
  async fn log_warnings(&self, warnings: impl IntoIterator<Item = ConfigWarning>) {
    for warning in warnings {
      self
        .client
        .log_message(MessageType::WARNING, warning.to_string())
        .await;
    }
  }

  /// Convert one of Gale's diagnostics to the LSP shape.
  fn to_lsp_diagnostic(
    d: &GaleDiagnostic,
    line_index: &SourceLineIndex,
  ) -> tower_lsp::lsp_types::Diagnostic {
    let severity = match d.severity {
      Severity::Error => Some(DiagnosticSeverity::ERROR),
      Severity::Warning => Some(DiagnosticSeverity::WARNING),
    };

    tower_lsp::lsp_types::Diagnostic {
      range: span_to_range(line_index, d.span),
      severity,
      code: Some(NumberOrString::String(d.rule_name.clone())),
      source: Some("gale".to_string()),
      message: d.message.clone(),
      related_information: (!d.related.is_empty()).then(|| {
        d.related
          .iter()
          .filter_map(|related| {
            Some(DiagnosticRelatedInformation {
              location: Location::new(
                Url::from_file_path(&d.file_path).ok()?,
                span_to_range(line_index, related.span),
              ),
              message: related.message.clone(),
            })
          })
          .collect()
      }),
      ..Default::default()
    }
  }

  /// Lint source text (sync part), returning Gale's own diagnostics and
  /// the skipped rules of any config the document was the first to load.
  ///
  /// The document's own config decides its rules (see
  /// [`Self::build_resolver`]).  A Vue, Svelte, Astro or HTML document is
  /// linted one style block at a time, with every diagnostic (and fix)
  /// placed in the document.
  fn lint(&self, uri: &Url, source: &str) -> Linted {
    let file_path = uri
      .to_file_path()
      .map(|p| p.display().to_string())
      .unwrap_or_else(|_| uri.to_string());

    // Settle the document's rules under the lock, then lint without it.
    let (settings, warnings) = {
      let mut workspace = self.workspace.lock().unwrap_or_else(|e| e.into_inner());
      let Some(Workspace { resolver, skipped }) = workspace.as_mut() else {
        return Linted {
          diagnostics: Vec::new(),
          warnings: Vec::new(),
        };
      };
      let settings = resolver.resolve(&file_path);
      (settings, skipped.new_warnings(resolver))
    };
    // A document the config ignores, or written in a `customSyntax` gale
    // cannot parse, gets no diagnostics, as the CLI reports nothing for it.
    let skipped = |warnings| Linted {
      diagnostics: Vec::new(),
      warnings,
    };
    if settings.ignored.is_some() {
      return skipped(warnings);
    }
    let Ok(syntax) = settings.syntax else {
      return skipped(warnings);
    };

    let result = settings.rules.lint(source, &file_path, syntax);
    // An invalid rule option (a pattern that does not compile, say) is not
    // a problem in the document, but the editor is the only place to say so:
    // show it at the top of the file.
    let mut diagnostics: Vec<GaleDiagnostic> = result
      .invalid_option_warnings
      .into_iter()
      .map(GaleDiagnostic::invalid_option)
      .collect();
    diagnostics.extend(result.diagnostics);
    Linted {
      diagnostics,
      warnings,
    }
  }

  /// Lint source text, remember it for code actions, and publish
  /// diagnostics to the client.
  ///
  /// Linting and the position conversion run inside a panic guard (which
  /// depends on panics unwinding, see [`panic_guard`]): a document that
  /// trips a bug is published with one internal-error diagnostic instead of
  /// taking the server down.
  async fn lint_and_publish(&self, uri: Url, source: &str) {
    let linted = panic_guard::catch(|| {
      let Linted {
        diagnostics,
        warnings,
      } = self.lint(&uri, source);
      let line_index = SourceLineIndex::build(source);
      let lsp_diagnostics: Vec<tower_lsp::lsp_types::Diagnostic> = diagnostics
        .iter()
        .map(|d| Self::to_lsp_diagnostic(d, &line_index))
        .collect();
      (diagnostics, lsp_diagnostics, warnings)
    });
    let (diagnostics, lsp_diagnostics, warnings) = linted.unwrap_or_else(|caught| {
      let diag = tower_lsp::lsp_types::Diagnostic {
        range: Range::default(),
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(
          gale_linter::runner::INTERNAL_ERROR_RULE.to_string(),
        )),
        source: Some("gale".to_string()),
        message: caught.report("while linting this file"),
        ..Default::default()
      };
      (Vec::new(), vec![diag], Vec::new())
    });
    self.log_warnings(warnings).await;

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
        let diag_range = span_to_range(&line_index, d.span);
        if !ranges_overlap(&diag_range, range) {
          return None;
        }

        let edits: Vec<TextEdit> = fix
          .edits
          .iter()
          .map(|edit| TextEdit {
            range: span_to_range(&line_index, edit.span),
            new_text: edit.new_text.clone(),
          })
          .collect();
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), edits);

        Some(CodeActionOrCommand::CodeAction(CodeAction {
          title: format!("Fix: {}", d.message),
          kind: Some(CodeActionKind::QUICKFIX),
          diagnostics: Some(vec![Self::to_lsp_diagnostic(d, &line_index)]),
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
  /// Finds the config (explicit `--config`, else the closest to the
  /// workspace root, else to the cwd) and sets up the per-document rules,
  /// then advertises full-text sync and code actions.
  async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
    // An explicit `--config` wins over discovery, matching the CLI.
    // Otherwise documents get their closest config, and those outside
    // every config's tree fall back on the workspace root's.
    let root_dir = params
      .root_uri
      .as_ref()
      .and_then(|uri| uri.to_file_path().ok())
      .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    debug!("LSP workspace root: {}", root_dir.display());
    let root = match RootConfig::load(self.config_path.as_deref(), &root_dir) {
      Ok(root) => root,
      Err(err) => {
        self
          .client
          .log_message(
            MessageType::ERROR,
            format!(
              "Failed to load config {}: {}",
              err.path.display(),
              err.error
            ),
          )
          .await;
        err.into_empty()
      }
    };
    // Plugins gale does not know, rules it has not implemented, and rules
    // Stylelint has removed are skipped; say so once, as the CLI does: the
    // plugins of the root config, and the skipped rules of every config
    // loaded, now and as documents load nested ones.  Names that are no
    // rule at all show up as `Unknown rule` diagnostics in each document
    // instead.
    let plugin_warnings = unsupported_plugin_warnings(&root.config);
    let resolver = Self::build_resolver(root, &root_dir);
    let mut skipped = SkippedRuleWarnings::default();
    let skipped_warnings = skipped.new_warnings(&resolver);
    *self.workspace.lock().unwrap_or_else(|e| e.into_inner()) =
      Some(Workspace { resolver, skipped });
    self
      .log_warnings(plugin_warnings.into_iter().chain(skipped_warnings))
      .await;

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

  /// Handshake complete; `initialize` already did the setup.
  async fn initialized(&self, _: InitializedParams) {
    debug!("Gale LSP server initialized");
  }

  /// Nothing to tear down.
  async fn shutdown(&self) -> Result<()> {
    Ok(())
  }

  /// Lints the newly opened document and publishes its diagnostics.
  async fn did_open(&self, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri;
    let source = params.text_document.text;
    self.lint_and_publish(uri, &source).await;
  }

  /// Re-lints on every edit; full sync means the last event holds all the text.
  async fn did_change(&self, params: DidChangeTextDocumentParams) {
    // We use full sync, so the last change event contains the full text.
    let uri = params.text_document.uri;
    if let Some(change) = params.content_changes.into_iter().last() {
      self.lint_and_publish(uri, &change.text).await;
    }
  }

  /// Forgets the document and clears its diagnostics from the editor.
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

  /// Offers quick fixes for the fixable diagnostics under the cursor.  A
  /// panic while building them yields no actions rather than a dead server.
  async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    let actions = panic_guard::catch(|| self.quick_fixes(&params.text_document.uri, &params.range))
      .unwrap_or_default();
    Ok(Some(actions))
  }

  /// Re-lints on save, using the notification's text or re-reading the file.
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
    let range = span_to_range(&line_index, Span::new(11, 4));
    assert_eq!(range.start, Position::new(0, 11));
    assert_eq!(range.end, Position::new(0, 15));

    let range = span_to_range(&line_index, Span::new(23, 1));
    assert_eq!(range.start, Position::new(1, 4));
  }

  #[test]
  fn span_to_range_survives_offsets_inside_a_multibyte_character() {
    // `é` is bytes 7 and 8; offset 8 falls inside it, and the span runs
    // past the end of the text.
    let source = "a { b: é; }\n";
    let line_index = SourceLineIndex::build(source);
    let range = span_to_range(&line_index, Span::new(8, 40));
    assert_eq!(range.start, Position::new(0, 7));
  }

  #[test]
  fn span_to_range_counts_utf16_once() {
    // An emoji is four bytes and two UTF-16 units; a byte order mark is
    // part of the document the editor counts in.
    let source = "\u{FEFF}a { content: \"😀\"; b: c }\n";
    let line_index = SourceLineIndex::build(source);
    let b = source.find("b:").unwrap();
    let range = span_to_range(&line_index, Span::new(b, 1));
    // BOM, `a { content: "`, two units of emoji, `"; `.
    assert_eq!(range.start, Position::new(0, 20));
    assert_eq!(range.end, Position::new(0, 21));
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
