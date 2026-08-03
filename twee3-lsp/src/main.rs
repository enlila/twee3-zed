use regex::Regex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "twee3-lsp initialized!")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.validate_text_document(
            params.text_document.uri,
            params.text_document.text,
        )
        .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.pop() {
            self.validate_text_document(params.text_document.uri, change.text)
                .await;
        }
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        let completions = vec![
            CompletionItem {
                label: "passage".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Twee3 Passage Details".to_string()),
                documentation: Some(Documentation::String(
                    "Creates a new Twee3 passage.".to_string(),
                )),
                ..Default::default()
            },
            CompletionItem {
                label: "macro".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Twee3 Macro Details".to_string()),
                documentation: Some(Documentation::String(
                    "Creates a new Twee3 macro.".to_string(),
                )),
                ..Default::default()
            },
        ];
        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn completion_resolve(&self, mut item: CompletionItem) -> Result<CompletionItem> {
        if item.label == "passage" {
            item.detail = Some("Twee3 Passage Details".to_string());
            item.documentation = Some(Documentation::String("Creates a new Twee3 passage.".to_string()));
        } else if item.label == "macro" {
            item.detail = Some("Twee3 Macro Details".to_string());
            item.documentation = Some(Documentation::String("Creates a new Twee3 macro.".to_string()));
        }
        Ok(item)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

impl Backend {
    async fn validate_text_document(&self, uri: Url, text: String) {
        // Find uppercase words of 2 or more characters
        let re = Regex::new(r"\b[A-Z]{2,}\b").unwrap();
        let mut diagnostics = vec![];

        for cap in re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                // Calculate line and character position
                // Since `tower-lsp` doesn't have a document manager built-in, 
                // we do a simple manual calculation for the start and end positions.
                let start_idx = m.start();
                let end_idx = m.end();
                
                let start_pos = byte_offset_to_position(&text, start_idx);
                let end_pos = byte_offset_to_position(&text, end_idx);

                diagnostics.push(Diagnostic {
                    range: Range {
                        start: start_pos,
                        end: end_pos,
                    },
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: None,
                    code_description: None,
                    source: Some("twee3".to_string()),
                    message: format!("{} is all uppercase.", m.as_str()),
                    related_information: Some(vec![DiagnosticRelatedInformation {
                        location: Location {
                            uri: uri.clone(),
                            range: Range { start: start_pos, end: end_pos },
                        },
                        message: "Spelling matters".to_string(),
                    }]),
                    tags: None,
                    data: None,
                });
            }
        }

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

fn byte_offset_to_position(text: &str, byte_offset: usize) -> Position {
    let prefix = &text[..byte_offset];
    let line = prefix.chars().filter(|&c| c == '\n').count() as u32;
    
    // Find the byte offset of the last newline character before the target offset.
    let last_newline_pos = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    
    // Character offset (not byte offset) from the last newline.
    let character = prefix[last_newline_pos..].chars().count() as u32;
    
    Position { line, character }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
