//! Karu Language Server
//!
//! LSP implementation for Karu policy files, providing diagnostics,
//! hover information, document symbols, completion, semantic tokens,
//! and go-to-definition.

use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[cfg(feature = "cedar")]
use karu::lsp::{
    cedar_document_symbols, cedar_parse_diagnostics, convert_cedar_to_karu, is_cedar_uri,
    is_cedarschema_uri,
};
#[cfg(all(feature = "dev", feature = "cedar"))]
use karu::lsp::{
    cedar_ts_parse_diagnostics, cedarschema_document_symbols, cedarschema_parse_diagnostics,
};
use karu::lsp::{
    cedar_semantic_tokens, document_symbols, find_definition, keyword_completions, keyword_hover,
    parse_diagnostics, run_inline_tests, semantic_tokens, SEMANTIC_TOKEN_TYPES,
};

/// Document state for a single file
#[derive(Debug, Default)]
struct Document {
    content: String,
}

/// The Karu Language Server
struct KaruLanguageServer {
    client: Client,
    documents: Arc<RwLock<std::collections::HashMap<Url, Document>>>,
}

impl KaruLanguageServer {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Publish diagnostics for a document
    async fn publish_diagnostics(&self, uri: Url, content: &str) {
        #[cfg(all(feature = "dev", feature = "cedar"))]
        let diagnostics = if is_cedarschema_uri(uri.as_str()) {
            cedarschema_parse_diagnostics(content)
        } else if is_cedar_uri(uri.as_str()) {
            // Use tree-sitter for error-tolerant parsing, fall back to handrolled
            let ts_diags = cedar_ts_parse_diagnostics(content);
            let hr_diags = cedar_parse_diagnostics(content);
            // Prefer handrolled if it has results (better messages), else use tree-sitter
            if !hr_diags.is_empty() {
                hr_diags
            } else {
                ts_diags
            }
        } else {
            parse_diagnostics(content)
        };

        #[cfg(all(feature = "cedar", not(feature = "dev")))]
        let diagnostics = if is_cedar_uri(uri.as_str()) {
            cedar_parse_diagnostics(content)
        } else {
            parse_diagnostics(content)
        };

        #[cfg(not(feature = "cedar"))]
        let diagnostics = parse_diagnostics(content);

        self.client
            .publish_diagnostics(uri.clone(), diagnostics.clone(), None)
            .await;

        // Run inline tests for .karu files and send results
        let is_karu = uri.as_str().ends_with(".karu");
        if is_karu {
            if let Some(test_results) = run_inline_tests(content) {
                // Build coverage diagnostics before consuming test_results
                let lines: Vec<&str> = content.lines().collect();
                let mut all_diagnostics = diagnostics;
                for cov in &test_results.coverage {
                    if cov.status == "full" {
                        continue;
                    }
                    let message = if cov.status == "none" {
                        format!("Rule '{}' has no test coverage", cov.name)
                    } else if !cov.has_positive {
                        format!(
                            "Rule '{}' is partially covered: no test triggers this rule",
                            cov.name
                        )
                    } else {
                        format!(
                            "Rule '{}' is partially covered: no test checks the negative case",
                            cov.name
                        )
                    };

                    let line_text = lines.get(cov.line as usize).unwrap_or(&"");
                    let name_start = line_text.find(&cov.name).unwrap_or(0) as u32;
                    let name_end = name_start + cov.name.len() as u32;

                    all_diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: cov.line,
                                character: name_start,
                            },
                            end: Position {
                                line: cov.line,
                                character: name_end,
                            },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        source: Some("karu".to_string()),
                        message,
                        ..Default::default()
                    });
                }

                // Send test pass/fail + coverage for gutter icons
                let _ = self
                    .client
                    .send_notification::<TestResultsNotification>(TestResultsParams {
                        uri: uri.to_string(),
                        tests: test_results.tests,
                        coverage: test_results.coverage,
                    })
                    .await;

                // Re-publish with coverage diagnostics included
                self.client
                    .publish_diagnostics(uri, all_diagnostics, None)
                    .await;
            } else {
                // No tests - clear stale gutter decorations and warn about uncovered rules
                let _ = self
                    .client
                    .send_notification::<TestResultsNotification>(TestResultsParams {
                        uri: uri.to_string(),
                        tests: vec![],
                        coverage: vec![],
                    })
                    .await;

                // Emit "no test coverage" diagnostics for every rule
                if let Ok(compiled) = karu::compile(content) {
                    let lines: Vec<&str> = content.lines().collect();
                    let mut all_diagnostics = diagnostics;
                    for rule in &compiled.rules {
                        let rule_line = lines
                            .iter()
                            .enumerate()
                            .find(|(_, line)| {
                                line.contains(&format!("allow {}", rule.name))
                                    || line.contains(&format!("deny {}", rule.name))
                            })
                            .map(|(i, _)| i as u32)
                            .unwrap_or(0);

                        let line_text = lines.get(rule_line as usize).unwrap_or(&"");
                        let name_start = line_text.find(&rule.name).unwrap_or(0) as u32;
                        let name_end = name_start + rule.name.len() as u32;

                        all_diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position {
                                    line: rule_line,
                                    character: name_start,
                                },
                                end: Position {
                                    line: rule_line,
                                    character: name_end,
                                },
                            },
                            severity: Some(DiagnosticSeverity::WARNING),
                            source: Some("karu".to_string()),
                            message: format!("Rule '{}' has no test coverage", rule.name),
                            ..Default::default()
                        });
                    }
                    self.client
                        .publish_diagnostics(uri, all_diagnostics, None)
                        .await;
                }
            }
        }
    }

    /// Build the semantic tokens legend
    fn semantic_tokens_legend() -> SemanticTokensLegend {
        SemanticTokensLegend {
            token_types: SEMANTIC_TOKEN_TYPES
                .iter()
                .map(|t| SemanticTokenType::new(t.as_str()))
                .collect(),
            token_modifiers: vec![],
        }
    }
}

/// Custom notification type for test results.
struct TestResultsNotification;

impl tower_lsp::lsp_types::notification::Notification for TestResultsNotification {
    type Params = TestResultsParams;
    const METHOD: &'static str = "karu/testResults";
}

/// Parameters for the test results notification.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct TestResultsParams {
    uri: String,
    tests: Vec<karu::lsp::TestResult>,
    coverage: Vec<karu::lsp::RuleCoverage>,
}

#[tower_lsp::async_trait]
impl LanguageServer for KaruLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    ..Default::default()
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: Self::semantic_tokens_legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            ..Default::default()
                        },
                    ),
                ),
                document_formatting_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                execute_command_provider: {
                    #[cfg(feature = "cedar")]
                    {
                        Some(ExecuteCommandOptions {
                            commands: vec!["karu.convertCedarToKaru".to_string()],
                            ..Default::default()
                        })
                    }
                    #[cfg(not(feature = "cedar"))]
                    {
                        None
                    }
                },
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "karu-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Karu LSP initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;

        // Store document
        {
            let mut docs = self.documents.write().await;
            docs.insert(
                uri.clone(),
                Document {
                    content: content.clone(),
                },
            );
        }

        // Publish diagnostics
        self.publish_diagnostics(uri, &content).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Get latest content (we're using FULL sync)
        if let Some(change) = params.content_changes.into_iter().last() {
            let content = change.text;

            // Update stored document
            {
                let mut docs = self.documents.write().await;
                docs.insert(
                    uri.clone(),
                    Document {
                        content: content.clone(),
                    },
                );
            }

            // Publish diagnostics
            self.publish_diagnostics(uri, &content).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.documents.write().await;
        docs.remove(&params.text_document.uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        // Get the word at the cursor position
        let lines: Vec<&str> = doc.content.lines().collect();
        let line = match lines.get(position.line as usize) {
            Some(l) => *l,
            None => return Ok(None),
        };

        // Find word boundaries
        let col = position.character as usize;
        let chars: Vec<char> = line.chars().collect();

        if col >= chars.len() {
            return Ok(None);
        }

        // Find word start and end
        let mut start = col;
        while start > 0 && chars[start - 1].is_alphanumeric() {
            start -= 1;
        }
        let mut end = col;
        while end < chars.len() && chars[end].is_alphanumeric() {
            end += 1;
        }

        let word: String = chars[start..end].iter().collect();

        // Use the shared keyword_hover function
        Ok(keyword_hover(&word).map(|text| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text.to_string(),
            }),
            range: Some(Range {
                start: Position {
                    line: position.line,
                    character: start as u32,
                },
                end: Position {
                    line: position.line,
                    character: end as u32,
                },
            }),
        }))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;

        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        #[cfg(all(feature = "dev", feature = "cedar"))]
        let symbols = if is_cedarschema_uri(uri.as_str()) {
            cedarschema_document_symbols(&doc.content)
        } else if is_cedar_uri(uri.as_str()) {
            cedar_document_symbols(&doc.content)
        } else {
            document_symbols(&doc.content)
        };

        #[cfg(all(feature = "cedar", not(feature = "dev")))]
        let symbols = if is_cedar_uri(uri.as_str()) {
            cedar_document_symbols(&doc.content)
        } else {
            document_symbols(&doc.content)
        };

        #[cfg(not(feature = "cedar"))]
        let symbols = document_symbols(&doc.content);
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(keyword_completions())))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;

        // CedarSchema files don't have a dedicated tokenizer yet.
        if is_cedarschema_uri(uri.as_str()) {
            return Ok(None);
        }

        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        // Use the appropriate tokenizer for the file type.
        let tokens = if is_cedar_uri(uri.as_str()) {
            cedar_semantic_tokens(&doc.content)
        } else {
            semantic_tokens(&doc.content)
        };

        // Convert to lsp_types::SemanticToken (delta-encoded)
        let mut data = Vec::new();
        let mut prev_line = 0u32;
        let mut prev_start = 0u32;

        for token in tokens {
            let delta_line = token.line - prev_line;
            let delta_start = if delta_line == 0 {
                token.start - prev_start
            } else {
                token.start
            };

            data.push(tower_lsp::lsp_types::SemanticToken {
                delta_line,
                delta_start,
                length: token.length,
                token_type: token.token_type as u32,
                token_modifiers_bitset: 0,
            });

            prev_line = token.line;
            prev_start = token.start;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let def = find_definition(&doc.content, position.line, position.character);

        Ok(def.map(|loc| {
            GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: Range {
                    start: Position {
                        line: loc.line,
                        character: loc.column,
                    },
                    end: Position {
                        line: loc.line,
                        character: loc.end_column,
                    },
                },
            })
        }))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let content = match docs.get(uri) {
            Some(doc) => doc.content.clone(),
            None => return Ok(None),
        };
        drop(docs);

        match karu::format::format_source(&content) {
            Ok(formatted) if formatted == content => Ok(None), // Already formatted
            Ok(formatted) => {
                let line_count = content.lines().count() as u32;
                Ok(Some(vec![TextEdit {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: line_count + 1,
                            character: 0,
                        },
                    },
                    new_text: formatted,
                }]))
            }
            Err(_) => Ok(None), // Can't format files with syntax errors
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let mut actions: Vec<CodeActionOrCommand> = Vec::new();

        // Cedar-to-Karu conversion action
        #[cfg(feature = "cedar")]
        if is_cedar_uri(uri.as_str()) {
            let action = CodeAction {
                title: "Convert to Karu".to_string(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                diagnostics: None,
                edit: None,
                command: Some(Command {
                    title: "Convert to Karu".to_string(),
                    command: "karu.convertCedarToKaru".to_string(),
                    arguments: Some(vec![serde_json::Value::String(uri.to_string())]),
                }),
                is_preferred: Some(false),
                disabled: None,
                data: None,
            };
            actions.push(CodeActionOrCommand::CodeAction(action));
        }

        // W001 quick-fix: add `has` guard before unguarded `forall`
        {
            let docs = self.documents.read().await;
            if let Some(doc) = docs.get(uri) {
                let lsp_actions = karu::lsp_core::code_actions(&doc.content);

                for la in lsp_actions {
                    let mut text_edits = Vec::new();
                    for edit in &la.edits {
                        let line = edit.line;
                        text_edits.push(TextEdit {
                            range: Range {
                                start: Position { line, character: edit.col },
                                end: Position { line, character: edit.end_col },
                            },
                            new_text: edit.new_text.clone(),
                        });
                    }

                    // Attach the relevant diagnostic so the editor associates this
                    // action with the squiggly underline
                    let diagnostic = la.diagnostic_code.as_ref().and_then(|_code| {
                        params.context.diagnostics.iter().find(|d| {
                            d.source.as_deref() == Some("karu")
                                && d.message.contains("forall")
                        }).cloned()
                    });

                    let mut changes = std::collections::HashMap::new();
                    changes.insert(uri.clone(), text_edits);

                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: la.title,
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: diagnostic.map(|d| vec![d]),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        command: None,
                        is_preferred: Some(true),
                        disabled: None,
                        data: None,
                    }));
                }
            }
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        if params.command == "karu.convertCedarToKaru" {
            #[cfg(feature = "cedar")]
            {
                let uri_str = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                let uri = match Url::parse(uri_str) {
                    Ok(u) => u,
                    Err(_) => {
                        self.client
                            .show_message(MessageType::ERROR, "Invalid URI")
                            .await;
                        return Ok(None);
                    }
                };

                // Read source from stored documents
                let source = {
                    let docs = self.documents.read().await;
                    match docs.get(&uri) {
                        Some(doc) => doc.content.clone(),
                        None => {
                            self.client
                                .show_message(MessageType::ERROR, "Document not found")
                                .await;
                            return Ok(None);
                        }
                    }
                };

                // Convert
                match convert_cedar_to_karu(&source) {
                    Ok(karu_source) => {
                        // Build new URI: foo.cedar → foo-converted.karu
                        let new_uri_str = uri_str.replace(".cedar", "-converted.karu");
                        let new_uri = Url::parse(&new_uri_str).unwrap_or(uri.clone());

                        // Apply workspace edit: create new file with content
                        let edit = WorkspaceEdit {
                            document_changes: Some(DocumentChanges::Operations(vec![
                                DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                                    uri: new_uri.clone(),
                                    options: Some(CreateFileOptions {
                                        overwrite: Some(false),
                                        ignore_if_exists: Some(false),
                                    }),
                                    annotation_id: None,
                                })),
                                DocumentChangeOperation::Edit(TextDocumentEdit {
                                    text_document: OptionalVersionedTextDocumentIdentifier {
                                        uri: new_uri.clone(),
                                        version: None,
                                    },
                                    edits: vec![OneOf::Left(TextEdit {
                                        range: Range {
                                            start: Position {
                                                line: 0,
                                                character: 0,
                                            },
                                            end: Position {
                                                line: 0,
                                                character: 0,
                                            },
                                        },
                                        new_text: karu_source,
                                    })],
                                }),
                            ])),
                            ..Default::default()
                        };

                        let _ = self.client.apply_edit(edit).await;
                        self.client
                            .show_message(
                                MessageType::INFO,
                                format!("Converted to {}", new_uri.path()),
                            )
                            .await;
                    }
                    Err(msg) => {
                        self.client.show_message(MessageType::ERROR, msg).await;
                    }
                }
            }
        }

        Ok(None)
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(KaruLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
