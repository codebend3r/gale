use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing::debug;

use gale_config::GaleConfig;
use gale_css_parser::detect_syntax;
use gale_diagnostics::{Severity, SourceLineIndex};
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

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

pub struct GaleLspServer {
    client: Client,
    runner: RwLock<Option<LintRunner>>,
    /// Explicit config path from `gale --lsp --config <path>`, if any.
    config_path: Option<PathBuf>,
}

impl GaleLspServer {
    fn new(client: Client, config_path: Option<PathBuf>) -> Self {
        Self {
            client,
            runner: RwLock::new(None),
            config_path,
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
                    cfg.severity
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
                cfg.options
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

    /// Lint source text and convert to LSP diagnostics (sync part).
    fn lint_to_diagnostics(
        &self,
        uri: &Url,
        source: &str,
    ) -> Vec<tower_lsp::lsp_types::Diagnostic> {
        let runner_guard = self.runner.read().unwrap_or_else(|e| e.into_inner());
        let Some(runner) = runner_guard.as_ref() else {
            return Vec::new();
        };

        let file_path = uri
            .to_file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| uri.to_string());

        let syntax = detect_syntax(&file_path);
        let result = runner.lint_source(source, &file_path, syntax);

        let line_index = SourceLineIndex::build(source);

        result
            .diagnostics
            .iter()
            .map(|d| {
                let (start_line, start_col) = line_index.offset_to_location(d.span.offset);
                let (end_line, end_col) = line_index.offset_to_location(d.span.end());

                // SourceLineIndex returns 1-indexed line/col where col is
                // byte-based. LSP uses 0-indexed line and UTF-16 code-unit
                // character offsets.
                let start_line_byte = d.span.offset - (start_col - 1);
                let end_line_byte = d.span.end() - (end_col - 1);

                let range = Range {
                    start: Position {
                        line: start_line.saturating_sub(1) as u32,
                        character: byte_col_to_utf16(source, start_line_byte, start_col - 1),
                    },
                    end: Position {
                        line: end_line.saturating_sub(1) as u32,
                        character: byte_col_to_utf16(source, end_line_byte, end_col - 1),
                    },
                };

                let severity = match d.severity {
                    Severity::Error => Some(DiagnosticSeverity::ERROR),
                    Severity::Warning => Some(DiagnosticSeverity::WARNING),
                    Severity::Info => Some(DiagnosticSeverity::INFORMATION),
                    Severity::Hint => Some(DiagnosticSeverity::HINT),
                };

                tower_lsp::lsp_types::Diagnostic {
                    range,
                    severity,
                    code: Some(NumberOrString::String(d.rule_name.clone())),
                    source: Some("gale".to_string()),
                    message: d.message.clone(),
                    ..Default::default()
                }
            })
            .collect()
    }

    /// Lint source text and publish diagnostics to the client.
    async fn lint_and_publish(&self, uri: Url, source: &str) {
        let diagnostics = self.lint_to_diagnostics(&uri, source);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
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
                    self.client
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
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
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
        // Clear diagnostics for the closed file, otherwise the editor keeps
        // showing warnings for a document that is no longer open.
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
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
