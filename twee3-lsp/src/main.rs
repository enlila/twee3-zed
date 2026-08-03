use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use walkdir::WalkDir;
use std::process::Command;

#[derive(Debug)]
struct Backend {
    client: Client,
    custom_macros: Arc<RwLock<HashSet<String>>>,
    workspace_path: Arc<RwLock<Option<PathBuf>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let mut wp = self.workspace_path.write().await;
        if let Some(folders) = params.workspace_folders {
            if let Some(folder) = folders.first() {
                if let Ok(path) = folder.uri.to_file_path() {
                    *wp = Some(path);
                }
            }
        } else if let Some(uri) = params.root_uri {
            if let Ok(path) = uri.to_file_path() {
                *wp = Some(path);
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(true),
                    trigger_characters: Some(vec!["<".to_string()]),
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

        let wp = self.workspace_path.read().await.clone();
        if let Some(workspace_path) = wp {
            // Check if there are any .twee files in the workspace to see if it's a Twee project
            let is_twee = WalkDir::new(&workspace_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .any(|e| e.path().extension().map_or(false, |ext| ext == "twee"));

            if is_twee {
                // Dynamic NPM Installation for @types/twine-sugarcube
                let package_json_path = workspace_path.join("package.json");
                if package_json_path.exists() {
                    // Node project: install into the project's devDependencies
                    self.client.log_message(MessageType::INFO, "Found package.json. Installing @types/twine-sugarcube...").await;
                    let _ = Command::new("npm")
                        .args(&["install", "--save-dev", "@types/twine-sugarcube"])
                        .current_dir(&workspace_path)
                        .output();
                } else {
                    // Non-Node project: install into a hidden .twee3 folder
                    self.client.log_message(MessageType::INFO, "No package.json found. Installing types into .twee3 hidden directory...").await;
                    let twee3_dir = workspace_path.join(".twee3");
                    let _ = std::fs::create_dir_all(&twee3_dir);
                    let _ = Command::new("npm")
                        .args(&["install", "--no-save", "@types/twine-sugarcube"])
                        .current_dir(&twee3_dir)
                        .output();

                    // Write jsconfig.json if not present
                    let jsconfig_path = workspace_path.join("jsconfig.json");
                    if !jsconfig_path.exists() {
                        let jsconfig = r#"{
  "compilerOptions": {
    "typeRoots": [
      "./.twee3/node_modules/@types"
    ]
  },
  "include": [
    "**/*.js"
  ]
}"#;
                        let _ = std::fs::write(&jsconfig_path, jsconfig);
                    }
                }

                // Scan for custom macros
                let mut macros = self.custom_macros.write().await;
                let re = Regex::new(r"Macro\.add\(\s*['\x22]([^'\x22]+)['\x22]").unwrap();
                for entry in WalkDir::new(&workspace_path).into_iter().filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "js") {
                        // ignore node_modules and .twee3
                        if path.components().any(|c| c.as_os_str() == "node_modules" || c.as_os_str() == ".twee3") {
                            continue;
                        }
                        if let Ok(contents) = std::fs::read_to_string(path) {
                            for cap in re.captures_iter(&contents) {
                                if let Some(m) = cap.get(1) {
                                    macros.insert(m.as_str().to_string());
                                }
                            }
                        }
                    }
                }
                self.client.log_message(MessageType::INFO, format!("Extracted {} custom macros.", macros.len())).await;
            }
        }
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
        let mut completions = vec![
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

        let macros = self.custom_macros.read().await;
        for m in macros.iter() {
            completions.push(CompletionItem {
                label: m.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("Custom SugarCube Macro".to_string()),
                ..Default::default()
            });
        }

        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn completion_resolve(&self, mut item: CompletionItem) -> Result<CompletionItem> {
        Ok(item)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

impl Backend {
    async fn validate_text_document(&self, uri: Url, text: String) {
        let re = Regex::new(r"\b[A-Z]{2,}\b").unwrap();
        let mut diagnostics = vec![];

        for cap in re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
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
    let last_newline_pos = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    let character = prefix[last_newline_pos..].chars().count() as u32;
    Position { line, character }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        custom_macros: Arc::new(RwLock::new(HashSet::new())),
        workspace_path: Arc::new(RwLock::new(None)),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
