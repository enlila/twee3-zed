use regex::Regex;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
struct PassageMeta {
    uri: Url,
    range: Range,
    preview: String,
}

#[derive(Deserialize, Debug, Default)]
struct TweeConfig {
    #[serde(rename = "storyFormat")]
    story_format: Option<String>,
    #[serde(rename = "sourceDir")]
    source_dir: Option<String>,
    #[serde(rename = "outputFile")]
    output_file: Option<String>,
    modules: Option<Vec<String>>,
    assets: Option<String>,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    custom_macros: Arc<RwLock<HashSet<String>>>,
    workspace_path: Arc<RwLock<Option<PathBuf>>>,
    // Map of file URI to a map of referenced passage name -> count
    passage_references: Arc<RwLock<HashMap<Url, HashMap<String, usize>>>>,
    defined_passages: Arc<RwLock<HashMap<String, PassageMeta>>>,
    variables: Arc<RwLock<HashSet<String>>>,
    documents: Arc<RwLock<HashMap<Url, String>>>,
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
                    trigger_characters: Some(vec!["<".to_string(), "[".to_string(), "$".to_string()]),
                    ..Default::default()
                }),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["twee3.runPassage".to_string()],
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
                    SemanticTokensOptions {
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                        legend: SemanticTokensLegend {
                            token_types: vec![
                                SemanticTokenType::NAMESPACE,
                                SemanticTokenType::MACRO,
                                SemanticTokenType::PARAMETER,
                                SemanticTokenType::VARIABLE,
                                SemanticTokenType::STRING,
                                SemanticTokenType::OPERATOR,
                            ],
                            token_modifiers: vec![],
                        },
                        range: Some(false),
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                    }
                )),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "twee3-language-server".to_string(),
                version: Some("0.0.1".to_string()),
            }),
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
                let node_modules_types = workspace_path
                    .join("node_modules")
                    .join("@types")
                    .join("twine-sugarcube");
                let twee3_dir = workspace_path.join(".twee3");
                let twee3_node_modules_types = twee3_dir
                    .join("node_modules")
                    .join("@types")
                    .join("twine-sugarcube");

                if package_json_path.exists() {
                    // Node project: install into the project's devDependencies if not present
                    if !node_modules_types.exists() {
                        self.client
                            .log_message(
                                MessageType::INFO,
                                "Found package.json. Installing @types/twine-sugarcube...",
                            )
                            .await;
                        let _ = Command::new("npm")
                            .args(&["install", "--save-dev", "@types/twine-sugarcube"])
                            .current_dir(&workspace_path)
                            .output();
                    }
                } else {
                    // Non-Node project: install into a hidden .twee3 folder if not present
                    if !twee3_node_modules_types.exists() {
                        self.client.log_message(MessageType::INFO, "No package.json found. Installing types into .twee3 hidden directory...").await;
                        let _ = std::fs::create_dir_all(&twee3_dir);
                        let _ = Command::new("npm")
                            .args(&["install", "--no-save", "@types/twine-sugarcube"])
                            .current_dir(&twee3_dir)
                            .output();
                    }

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
                for entry in WalkDir::new(&workspace_path)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "js") {
                        // ignore node_modules and .twee3
                        if path
                            .components()
                            .any(|c| c.as_os_str() == "node_modules" || c.as_os_str() == ".twee3")
                        {
                            continue;
                        }
                        if let Ok(contents) = std::fs::read_to_string(path) {
                            for cap in re.captures_iter(&contents) {
                                if let Some(m) = cap.get(1) {
                                    macros.insert(m.as_str().to_string());
                                }
                            }
                        }
                    } else if path.is_file() && path.extension().map_or(false, |ext| ext == "twee") {
                        if path
                            .components()
                            .any(|c| c.as_os_str() == "node_modules" || c.as_os_str() == ".twee3")
                        {
                            continue;
                        }
                        if let Ok(contents) = std::fs::read_to_string(path) {
                            if let Ok(uri) = Url::from_file_path(path) {
                                self.parse_document(uri, contents).await;
                            }
                        }
                    }
                }
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("Extracted {} custom macros.", macros.len()),
                    )
                    .await;
            }
        }
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.documents.write().await.insert(params.text_document.uri.clone(), params.text_document.text.clone());

        self.parse_document(params.text_document.uri.clone(), params.text_document.text.clone()).await;
        self.validate_text_document(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.pop() {
            self.documents.write().await.insert(params.text_document.uri.clone(), change.text.clone());

            self.parse_document(params.text_document.uri.clone(), change.text.clone()).await;
            self.validate_text_document(params.text_document.uri, change.text)
                .await;
        }
    }

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let mut completions = vec![];

        let macros = self.custom_macros.read().await;
        for m in macros.iter() {
            completions.push(CompletionItem {
                label: m.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("Custom SugarCube Macro".to_string()),
                ..Default::default()
            });
        }

        let defined = self.defined_passages.read().await;
        for name in defined.keys() {
            completions.push(CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::REFERENCE),
                detail: Some("Passage".to_string()),
                ..Default::default()
            });
        }

        let vars = self.variables.read().await;
        for v in vars.iter() {
            completions.push(CompletionItem {
                label: v.clone(),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: Some("Variable".to_string()),
                ..Default::default()
            });
        }

        let built_in_macros = vec![
            "set", "if", "else", "elseif", "for", "print", "link", "button", 
            "include", "return", "switch", "case", "default", "widget", 
            "catch", "finally", "run", "script", "replace", "append", "prepend"
        ];

        for m in built_in_macros {
            completions.push(CompletionItem {
                label: m.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Built-in SugarCube Macro".to_string()),
                ..Default::default()
            });
        }

        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn completion_resolve(&self, item: CompletionItem) -> Result<CompletionItem> {
        Ok(item)
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri.clone();

        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("CodeLens: Failed to convert URI to file path: {}", uri),
                    )
                    .await;
                return Ok(None);
            }
        };

        let text = match self.documents.read().await.get(&uri).cloned().ok_or_else(|| "Document not found in memory".to_string()) {
            Ok(t) => t,
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("CodeLens: Failed to read file {}: {}", path.display(), e),
                    )
                    .await;
                return Ok(None);
            }
        };

        let mut lenses = vec![];
        let re = Regex::new(r"(?m)^::\s*(.+?)(?:\s*\[|$)").unwrap();

        for cap in re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                if let Some(name_match) = cap.get(1) {
                    let passage_name = name_match.as_str().trim().to_string();
                    let start_pos = byte_offset_to_position(&text, m.start());
                    let end_pos = byte_offset_to_position(&text, m.end());

                    let command = tower_lsp::lsp_types::Command {
                        title: "▶ Run Passage".to_string(),
                        command: "twee3.runPassage".to_string(),
                        arguments: Some(vec![
                            serde_json::Value::String(passage_name.clone()),
                            serde_json::Value::String(uri.to_string()),
                        ]),
                    };

                    lenses.push(CodeLens {
                        range: Range {
                            start: start_pos,
                            end: end_pos,
                        },
                        command: Some(command),
                        data: None,
                    });

                    // Add reference count lens
                    let pr = self.passage_references.read().await;
                    let count: usize = pr.values().filter_map(|refs| refs.get(&passage_name)).sum();

                    let ref_command = tower_lsp::lsp_types::Command {
                        title: format!("{} references", count),
                        command: "editor.action.showReferences".to_string(),
                        arguments: None,
                    };

                    lenses.push(CodeLens {
                        range: Range {
                            start: start_pos,
                            end: end_pos,
                        },
                        command: Some(ref_command),
                        data: None,
                    });
                }
            }
        }

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "CodeLens: Found {} lenses for {}",
                    lenses.len(),
                    path.display()
                ),
            )
            .await;

        Ok(Some(lenses))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();

        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        let text = match self.documents.read().await.get(&uri).cloned().ok_or_else(|| "Document not found in memory".to_string()) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        let mut actions = vec![];
        let re = Regex::new(r"(?m)^::\s*(.+?)(?:\s*\[|$)").unwrap();

        for cap in re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                let start_pos = byte_offset_to_position(&text, m.start());
                let _end_pos = byte_offset_to_position(&text, m.end());

                // If the cursor is anywhere on the passage header line
                if params.range.start.line == start_pos.line
                    || params.range.end.line == start_pos.line
                {
                    if let Some(name_match) = cap.get(1) {
                        let passage_name = name_match.as_str().trim().to_string();

                        let command = tower_lsp::lsp_types::Command {
                            title: format!("▶ Run Passage: {}", passage_name),
                            command: "twee3.runPassage".to_string(),
                            arguments: Some(vec![
                                serde_json::Value::String(passage_name.clone()),
                                serde_json::Value::String(uri.to_string()),
                            ]),
                        };

                        let action = tower_lsp::lsp_types::CodeAction {
                            title: format!("▶ Run Passage: {}", passage_name),
                            kind: None,
                            diagnostics: None,
                            edit: None,
                            command: Some(command),
                            is_preferred: Some(true),
                            disabled: None,
                            data: None,
                        };

                        actions.push(CodeActionOrCommand::CodeAction(action));
                    }
                }
            }
        }

        Ok(Some(actions))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri.clone();
        let pos = params.text_document_position_params.position;

        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        let text = match self.documents.read().await.get(&uri).cloned().ok_or_else(|| "Document not found in memory".to_string()) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        // Match [img[...]] or [img[...][...]]
        let re = Regex::new(r"\[img\[([^\]]+)\](?:\[[^\]]+\])?\]").unwrap();
        // Match <img src="...">
        let html_re = Regex::new(r#"<img[^>]+src=["']([^"']+)["'][^>]*>"#).unwrap();

        let wp = self.workspace_path.read().await.clone();
        let workspace_path = match wp {
            Some(w) => w,
            None => return Ok(None),
        };

        let mut config = TweeConfig::default();
        let config_path = workspace_path.join("twee-config.yaml");
        if let Ok(yaml) = std::fs::read_to_string(&config_path) {
            if let Ok(parsed) = serde_yaml::from_str::<TweeConfig>(&yaml) {
                config = parsed;
            }
        }
        
        let assets_dir = config.assets.unwrap_or_else(|| "assets".to_string());

        let mut check_match = |start_idx: usize, end_idx: usize, asset_path: &str| -> Option<GotoDefinitionResponse> {
            let start_pos = byte_offset_to_position(&text, start_idx);
            let end_pos = byte_offset_to_position(&text, end_idx);

            if (pos.line > start_pos.line || (pos.line == start_pos.line && pos.character >= start_pos.character))
                && (pos.line < end_pos.line || (pos.line == end_pos.line && pos.character <= end_pos.character))
            {
                // Parse alt text out if it exists [img[alt|path]]
                let path_str = if let Some((_, p)) = asset_path.split_once('|') {
                    p.trim()
                } else if let Some((p, _)) = asset_path.split_once('|') {
                    p.trim() // if it's path|alt? Twee usually uses alt|path
                } else {
                    asset_path.trim()
                };

                let abs_path = workspace_path.join(&assets_dir).join(path_str);
                if abs_path.exists() {
                    if let Ok(target_uri) = Url::from_file_path(abs_path) {
                        return Some(GotoDefinitionResponse::Scalar(Location {
                            uri: target_uri,
                            range: Range {
                                start: Position { line: 0, character: 0 },
                                end: Position { line: 0, character: 0 },
                            },
                        }));
                    }
                }
            }
            None
        };

        for cap in re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                if let Some(asset_match) = cap.get(1) {
                    if let Some(res) = check_match(m.start(), m.end(), asset_match.as_str()) {
                        return Ok(Some(res));
                    }
                }
            }
        }

        for cap in html_re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                if let Some(asset_match) = cap.get(1) {
                    if let Some(res) = check_match(m.start(), m.end(), asset_match.as_str()) {
                        return Ok(Some(res));
                    }
                }
            }
        }

        let link_re = Regex::new(r"\[\[(.*?)\]\]").unwrap();
        for cap in link_re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                let start_pos = byte_offset_to_position(&text, m.start());
                let end_pos = byte_offset_to_position(&text, m.end());

                if (pos.line > start_pos.line || (pos.line == start_pos.line && pos.character >= start_pos.character))
                    && (pos.line < end_pos.line || (pos.line == end_pos.line && pos.character <= end_pos.character))
                {
                    let link_content = cap.get(1).unwrap().as_str();
                    let target = if let Some((_, p)) = link_content.split_once('|') {
                        p.trim()
                    } else if let Some((_, p)) = link_content.split_once("->") {
                        p.trim()
                    } else if let Some((p, _)) = link_content.split_once("<-") {
                        p.trim()
                    } else {
                        link_content.trim()
                    };

                    let defined = self.defined_passages.read().await;
                    if let Some(meta) = defined.get(target) {
                        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                            uri: meta.uri.clone(),
                            range: meta.range,
                        })));
                    }
                }
            }
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.clone();
        let pos = params.text_document_position_params.position;

        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        let text = match self.documents.read().await.get(&uri).cloned().ok_or_else(|| "Document not found in memory".to_string()) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        let line = text.lines().nth(pos.line as usize).unwrap_or("");
        
        let link_re = Regex::new(r"\[\[(.*?)\]\]").unwrap();
        let macro_re = Regex::new(r"<<([a-zA-Z0-9_-]+)").unwrap();

        for cap in link_re.captures_iter(line) {
            if let Some(m) = cap.get(0) {
                let start_char = m.start() as u32;
                let end_char = m.end() as u32;
                if pos.character >= start_char && pos.character <= end_char {
                    let link_content = cap.get(1).unwrap().as_str();
                    let passage = if let Some((_, p)) = link_content.split_once('|') {
                        p.trim()
                    } else if let Some((_, p)) = link_content.split_once("->") {
                        p.trim()
                    } else if let Some((p, _)) = link_content.split_once("<-") {
                        p.trim()
                    } else {
                        link_content.trim()
                    };

                    let defined = self.defined_passages.read().await;
                    if let Some(meta) = defined.get(passage) {
                        return Ok(Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: format!("**{}**\n\n```twee\n{}\n```", passage, meta.preview),
                            }),
                            range: Some(Range {
                                start: Position { line: pos.line, character: start_char },
                                end: Position { line: pos.line, character: end_char },
                            }),
                        }));
                    }
                }
            }
        }

        for cap in macro_re.captures_iter(line) {
            if let Some(m) = cap.get(0) {
                let start_char = m.start() as u32;
                let end_char = m.end() as u32;
                if pos.character >= start_char && pos.character <= end_char {
                    let macro_name = cap.get(1).unwrap().as_str();
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("SugarCube Macro: **{}**", macro_name),
                        }),
                        range: Some(Range {
                            start: Position { line: pos.line, character: start_char },
                            end: Position { line: pos.line, character: end_char },
                        }),
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let defined = self.defined_passages.read().await;

        let mut symbols = vec![];
        for (name, meta) in defined.iter() {
            if meta.uri == uri {
                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: name.clone(),
                    detail: None,
                    kind: SymbolKind::STRING,
                    tags: None,
                    deprecated: None,
                    range: meta.range,
                    selection_range: meta.range,
                    children: None,
                });
            }
        }
        
        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DocumentSymbolResponse::Nested(symbols)))
        }
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;
        let new_name = params.new_name;

        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        let text = match self.documents.read().await.get(&uri).cloned().ok_or_else(|| "Document not found in memory".to_string()) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        let line = text.lines().nth(pos.line as usize).unwrap_or("");
        let header_re = Regex::new(r"^::\s*(.+?)(?:\s*\[|$)").unwrap();

        let mut old_name = String::new();
        let mut header_range = None;

        if let Some(cap) = header_re.captures(line) {
            if let Some(m) = cap.get(1) {
                let start_char = m.start() as u32;
                let end_char = m.end() as u32;
                if pos.character >= start_char && pos.character <= end_char {
                    old_name = m.as_str().trim().to_string();
                    header_range = Some(Range {
                        start: Position { line: pos.line, character: start_char },
                        end: Position { line: pos.line, character: end_char },
                    });
                }
            }
        }

        if old_name.is_empty() {
            return Ok(None);
        }

        let mut changes = HashMap::new();

        if let Some(r) = header_range {
            changes.entry(uri.clone()).or_insert_with(Vec::new).push(TextEdit {
                range: r,
                new_text: new_name.clone(),
            });
        }

        let pr = self.passage_references.read().await;
        for (ref_uri, refs) in pr.iter() {
            if refs.contains_key(&old_name) {
                if let Ok(ref_path) = ref_uri.to_file_path() {
                    if let Ok(ref_text) = std::fs::read_to_string(&ref_path) {
                        let link_re = Regex::new(r"\[\[(.*?)\]\]").unwrap();
                        for cap in link_re.captures_iter(&ref_text) {
                            if let Some(m) = cap.get(0) {
                                let link_content = cap.get(1).unwrap().as_str();
                                let (display, target) = if let Some((d, p)) = link_content.split_once('|') {
                                    (Some(d.trim()), p.trim())
                                } else if let Some((d, p)) = link_content.split_once("->") {
                                    (Some(d.trim()), p.trim())
                                } else if let Some((p, d)) = link_content.split_once("<-") {
                                    (Some(d.trim()), p.trim())
                                } else {
                                    (None, link_content.trim())
                                };

                                if target == old_name {
                                    let start_pos = byte_offset_to_position(&ref_text, m.start());
                                    let end_pos = byte_offset_to_position(&ref_text, m.end());

                                    let new_text = if let Some(d) = display {
                                        format!("[[{}|{}]]", d, new_name)
                                    } else {
                                        format!("[[{}]]", new_name)
                                    };

                                    changes.entry(ref_uri.clone()).or_insert_with(Vec::new).push(TextEdit {
                                        range: Range { start: start_pos, end: end_pos },
                                        new_text,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        self.client
            .log_message(
                MessageType::INFO,
                format!("Executing command: {}", params.command),
            )
            .await;
        if params.command == "twee3.runPassage" {
            let args = params.arguments;
            if args.len() >= 2 {
                if let (Some(passage_name), Some(_uri_str)) = (args[0].as_str(), args[1].as_str()) {
                    if let Err(e) = self.run_tweego(passage_name.to_string()).await {
                        self.client
                            .log_message(MessageType::ERROR, format!("Run passage failed: {}", e))
                            .await;
                    }
                }
            }
        }
        Ok(None)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;

        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        let text = match self.documents.read().await.get(&uri).cloned().ok_or_else(|| "Document not found in memory".to_string()) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        let line = text.lines().nth(pos.line as usize).unwrap_or("");
        
        let header_re = Regex::new(r"^::\s*(.+?)(?:\s*\[|$)").unwrap();
        let link_re = Regex::new(r"\[\[(.*?)\]\]").unwrap();

        let mut target_name = String::new();

        if let Some(cap) = header_re.captures(line) {
            if let Some(m) = cap.get(1) {
                let start_char = m.start() as u32;
                let end_char = m.end() as u32;
                if pos.character >= start_char && pos.character <= end_char {
                    target_name = m.as_str().trim().to_string();
                }
            }
        }

        if target_name.is_empty() {
            for cap in link_re.captures_iter(line) {
                if let Some(m) = cap.get(0) {
                    let start_char = m.start() as u32;
                    let end_char = m.end() as u32;
                    if pos.character >= start_char && pos.character <= end_char {
                        let link_content = cap.get(1).unwrap().as_str();
                        let target = if let Some((_, p)) = link_content.split_once('|') {
                            p.trim()
                        } else if let Some((_, p)) = link_content.split_once("->") {
                            p.trim()
                        } else if let Some((p, _)) = link_content.split_once("<-") {
                            p.trim()
                        } else {
                            link_content.trim()
                        };
                        target_name = target.to_string();
                        break;
                    }
                }
            }
        }

        if target_name.is_empty() {
            return Ok(None);
        }

        let mut locations = vec![];

        if params.context.include_declaration {
            let defined = self.defined_passages.read().await;
            if let Some(meta) = defined.get(&target_name) {
                locations.push(Location {
                    uri: meta.uri.clone(),
                    range: meta.range,
                });
            }
        }

        let pr = self.passage_references.read().await;
        for (ref_uri, refs) in pr.iter() {
            if refs.contains_key(&target_name) {
                if let Ok(ref_path) = ref_uri.to_file_path() {
                    if let Ok(ref_text) = std::fs::read_to_string(&ref_path) {
                        let link_re2 = Regex::new(r"\[\[(.*?)\]\]").unwrap();
                        for cap in link_re2.captures_iter(&ref_text) {
                            if let Some(m) = cap.get(0) {
                                let link_content = cap.get(1).unwrap().as_str();
                                let target = if let Some((_, p)) = link_content.split_once('|') {
                                    p.trim()
                                } else if let Some((_, p)) = link_content.split_once("->") {
                                    p.trim()
                                } else if let Some((p, _)) = link_content.split_once("<-") {
                                    p.trim()
                                } else {
                                    link_content.trim()
                                };

                                if target == target_name {
                                    let start_pos = byte_offset_to_position(&ref_text, m.start());
                                    let end_pos = byte_offset_to_position(&ref_text, m.end());
                                    locations.push(Location {
                                        uri: ref_uri.clone(),
                                        range: Range { start: start_pos, end: end_pos },
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(Some(locations))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.clone();
        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        let text = match self.documents.read().await.get(&uri).cloned().ok_or_else(|| "Document not found in memory".to_string()) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        struct TokenLoc {
            line: u32,
            start_char: u32,
            length: u32,
            token_type: u32,
        }

        let mut tokens = vec![];

        let header_re = Regex::new(r"(?m)^::\s*(.+?)(?:\s*\[|$)").unwrap();
        for cap in header_re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                let start_pos = byte_offset_to_position(&text, m.start());
                tokens.push(TokenLoc {
                    line: start_pos.line,
                    start_char: start_pos.character,
                    length: text[m.start()..m.end()].chars().count() as u32,
                    token_type: 0, // NAMESPACE
                });
            }
        }

        let link_re = Regex::new(r"\[\[(.*?)\]\]").unwrap();
        for cap in link_re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                let start_pos = byte_offset_to_position(&text, m.start());
                tokens.push(TokenLoc {
                    line: start_pos.line,
                    start_char: start_pos.character,
                    length: text[m.start()..m.end()].chars().count() as u32,
                    token_type: 4, // STRING
                });
            }
        }

        let macro_block_re = Regex::new(r"<<(.*?)>>").unwrap();
        for block_cap in macro_block_re.captures_iter(&text) {
            if let Some(block_m) = block_cap.get(0) {
                let block_start = block_m.start();
                let block_end = block_m.end();
                let inner = block_cap.get(1).unwrap();
                
                let block_start_pos = byte_offset_to_position(&text, block_start);
                tokens.push(TokenLoc {
                    line: block_start_pos.line,
                    start_char: block_start_pos.character,
                    length: 2,
                    token_type: 5, // OPERATOR
                });
                
                let block_end_pos = byte_offset_to_position(&text, block_end - 2);
                tokens.push(TokenLoc {
                    line: block_end_pos.line,
                    start_char: block_end_pos.character,
                    length: 2,
                    token_type: 5, // OPERATOR
                });

                let inner_text = inner.as_str();
                let inner_offset = inner.start();
                let mut chars = inner_text.char_indices().peekable();
                let mut is_first = true;

                while let Some((i, c)) = chars.next() {
                    if c.is_whitespace() {
                        continue;
                    }
                    
                    let token_start_idx = inner_offset + i;
                    let token_start_pos = byte_offset_to_position(&text, token_start_idx);

                    if c == '$' || c == '_' {
                        let mut end_idx = i + c.len_utf8();
                        while let Some(&(j, next_c)) = chars.peek() {
                            if next_c.is_alphanumeric() || next_c == '_' {
                                end_idx = j + next_c.len_utf8();
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        tokens.push(TokenLoc {
                            line: token_start_pos.line,
                            start_char: token_start_pos.character,
                            length: text[token_start_idx..(inner_offset + end_idx)].chars().count() as u32,
                            token_type: 3, // VARIABLE
                        });
                        is_first = false;
                    } else if c == '"' || c == '\'' {
                        let quote = c;
                        let mut end_idx = i + c.len_utf8();
                        let mut escaped = false;
                        while let Some((j, next_c)) = chars.next() {
                            end_idx = j + next_c.len_utf8();
                            if next_c == '\\' {
                                escaped = !escaped;
                            } else if next_c == quote && !escaped {
                                break;
                            } else {
                                escaped = false;
                            }
                        }
                        tokens.push(TokenLoc {
                            line: token_start_pos.line,
                            start_char: token_start_pos.character,
                            length: text[token_start_idx..(inner_offset + end_idx)].chars().count() as u32,
                            token_type: 4, // STRING
                        });
                        is_first = false;
                    } else {
                        let mut end_idx = i + c.len_utf8();
                        while let Some(&(j, next_c)) = chars.peek() {
                            if next_c.is_whitespace() || next_c == '$' || next_c == '_' || next_c == '"' || next_c == '\'' {
                                break;
                            }
                            end_idx = j + next_c.len_utf8();
                            chars.next();
                        }
                        
                        let token_type = if is_first { 1 } else { 2 }; // MACRO or PARAMETER
                        tokens.push(TokenLoc {
                            line: token_start_pos.line,
                            start_char: token_start_pos.character,
                            length: text[token_start_idx..(inner_offset + end_idx)].chars().count() as u32,
                            token_type,
                        });
                        is_first = false;
                    }
                }
            }
        }

        let var_re = Regex::new(r"([$_][A-Za-z0-9_]+)").unwrap();
        for cap in var_re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                let start_pos = byte_offset_to_position(&text, m.start());
                tokens.push(TokenLoc {
                    line: start_pos.line,
                    start_char: start_pos.character,
                    length: text[m.start()..m.end()].chars().count() as u32,
                    token_type: 3, // VARIABLE
                });
            }
        }

        tokens.sort();
        tokens.dedup_by_key(|t| (t.line, t.start_char));

        let mut encoded = vec![];
        let mut last_line = 0;
        let mut last_char = 0;

        for t in tokens {
            let delta_line = t.line - last_line;
            let delta_start = if delta_line == 0 {
                t.start_char - last_char
            } else {
                t.start_char
            };

            encoded.push(SemanticToken {
                delta_line,
                delta_start,
                length: t.length,
                token_type: t.token_type,
                token_modifiers_bitset: 0,
            });

            last_line = t.line;
            last_char = t.start_char;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: encoded,
        })))
    }
}

impl Backend {
    async fn run_tweego(&self, passage_name: String) -> std::result::Result<(), String> {
        let wp = self.workspace_path.read().await.clone();
        let workspace_path = wp.ok_or("No workspace found")?;

        let config_path = workspace_path.join("twee-config.yaml");
        let mut config = TweeConfig::default();
        if let Ok(yaml) = std::fs::read_to_string(&config_path) {
            if let Ok(parsed) = serde_yaml::from_str::<TweeConfig>(&yaml) {
                config = parsed;
            }
        }

        let tweego_env = std::env::var("TWEEGO_PATH").unwrap_or_else(|_| "tweego".to_string());

        // If TWEEGO_PATH is relative, it might be relative to the Zed extension directory
        // where twee3-lsp is installed. Let's resolve it.
        let tweego_path = if std::path::Path::new(&tweego_env).is_absolute() {
            tweego_env
        } else if let Ok(exe_path) = std::env::current_exe() {
            // current_exe is typically <extension_dir>/twee3-lsp-<version>/twee3-lsp.exe
            if let Some(ext_dir) = exe_path.parent().and_then(|p| p.parent()) {
                let absolute_path = ext_dir.join(&tweego_env);
                if absolute_path.exists() {
                    absolute_path.to_string_lossy().to_string()
                } else {
                    tweego_env
                }
            } else {
                tweego_env
            }
        } else {
            tweego_env
        };

        let out_file = config
            .output_file
            .unwrap_or_else(|| "dist/game.html".to_string());
        let src_dir = config.source_dir.unwrap_or_else(|| "src".to_string());

        let mut args = vec![
            "-o".to_string(),
            out_file.clone(),
            "-s".to_string(),
            passage_name.clone(),
            src_dir,
        ];

        if let Some(format) = config.story_format {
            args.push("-f".to_string());
            args.push(format);
        }

        if let Some(modules) = config.modules {
            for md in modules {
                args.push(format!("{}", md));
            }
        }

        self.client
            .log_message(
                MessageType::INFO,
                format!("Running Tweego: {} {}", tweego_path, args.join(" ")),
            )
            .await;

        let output = Command::new(&tweego_path)
            .args(&args)
            .current_dir(&workspace_path)
            .output()
            .map_err(|e| format!("Failed to spawn Tweego ({}): {}", tweego_path, e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Tweego build failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.trim().is_empty() {
            self.client
                .log_message(MessageType::INFO, format!("Tweego stdout:\n{}", stdout))
                .await;
        }
        if !stderr.trim().is_empty() {
            self.client
                .log_message(MessageType::INFO, format!("Tweego stderr:\n{}", stderr))
                .await;
        }

        let out_abs = workspace_path.join(out_file);
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Successfully compiled passage '{}' to {:?}",
                    passage_name, out_abs
                ),
            )
            .await;

        // Open in browser
        #[cfg(target_os = "windows")]
        {
            let path_str = out_abs.to_string_lossy().replace("/", "\\");
            let _ = Command::new("cmd")
                .args(&["/C", "start", "", &path_str])
                .spawn();
        }

        #[cfg(target_os = "macos")]
        let _ = Command::new("open").arg(&out_abs).spawn();

        #[cfg(target_os = "linux")]
        let _ = Command::new("xdg-open").arg(&out_abs).spawn();

        Ok(())
    }

    async fn validate_text_document(&self, uri: Url, text: String) {
        let mut diagnostics = vec![];

        // 1. Broken Links
        let link_re = Regex::new(r"\[\[(.*?)\]\]").unwrap();
        let defined = self.defined_passages.read().await;

        for cap in link_re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                let link_content = cap.get(1).unwrap().as_str();
                let passage = if let Some((_, p)) = link_content.split_once('|') {
                    p.trim()
                } else if let Some((_, p)) = link_content.split_once("->") {
                    p.trim()
                } else if let Some((p, _)) = link_content.split_once("<-") {
                    p.trim()
                } else {
                    link_content.trim()
                };

                if !defined.contains_key(passage) {
                    let start_pos = byte_offset_to_position(&text, m.start());
                    let end_pos = byte_offset_to_position(&text, m.end());
                    diagnostics.push(Diagnostic {
                        range: Range { start: start_pos, end: end_pos },
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!("Passage '{}' does not exist.", passage),
                        source: Some("twee3".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
        drop(defined);

        // 2. Duplicate Passages (this file specifically)
        let header_re = Regex::new(r"(?m)^::\s*(.+?)(?:\s*\[|$)").unwrap();
        let mut seen_headers = HashSet::new();

        let ignore_list = vec![
            "Start", "StoryInit", "StoryData", "StoryTitle", "StoryAuthor", 
            "PassageHeader", "PassageFooter", "PassageReady", "PassageDone",
            "StoryMenu", "StoryShare", "StorySubtitle", "StoryBanner"
        ];
        let pr = self.passage_references.read().await;

        for cap in header_re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                let passage_name = cap.get(1).unwrap().as_str().trim().to_string();
                if !seen_headers.insert(passage_name.clone()) {
                    let start_pos = byte_offset_to_position(&text, m.start());
                    let end_pos = byte_offset_to_position(&text, m.end());
                    diagnostics.push(Diagnostic {
                        range: Range { start: start_pos, end: end_pos },
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!("Duplicate passage name '{}'.", passage_name),
                        source: Some("twee3".to_string()),
                        ..Default::default()
                    });
                } else if !ignore_list.contains(&passage_name.as_str()) {
                    let total_refs: usize = pr.values().filter_map(|refs| refs.get(&passage_name)).sum();
                    if total_refs == 0 {
                        let start_pos = byte_offset_to_position(&text, m.start());
                        let end_pos = byte_offset_to_position(&text, m.end());
                        diagnostics.push(Diagnostic {
                            range: Range { start: start_pos, end: end_pos },
                            severity: Some(DiagnosticSeverity::HINT),
                            message: format!("Passage '{}' is never linked (orphaned).", passage_name),
                            source: Some("twee3".to_string()),
                            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    async fn parse_document(&self, uri: Url, text: String) {
        let mut references: HashMap<String, usize> = HashMap::new();
        // Match links like [[passage]] or [[text|passage]] or [[text->passage]] or [[passage<-text]]
        let link_re = Regex::new(r"\[\[(.*?)\]\]").unwrap();
        
        for cap in link_re.captures_iter(&text) {
            if let Some(m) = cap.get(1) {
                let link_content = m.as_str();
                let passage = if let Some((_, passage)) = link_content.split_once('|') {
                    passage.trim()
                } else if let Some((_, passage)) = link_content.split_once("->") {
                    passage.trim()
                } else if let Some((passage, _)) = link_content.split_once("<-") {
                    passage.trim()
                } else {
                    link_content.trim()
                };
                
                *references.entry(passage.to_string()).or_insert(0) += 1;
            }
        }
        
        let mut pr = self.passage_references.write().await;
        pr.insert(uri.clone(), references);
        drop(pr);

        let mut defined = self.defined_passages.write().await;
        // Remove existing definitions from this file
        defined.retain(|_, meta| meta.uri != uri);

        let mut vars = self.variables.write().await;
        let var_re = Regex::new(r"([$_][A-Za-z0-9_]+)").unwrap();
        for cap in var_re.captures_iter(&text) {
            if let Some(m) = cap.get(1) {
                vars.insert(m.as_str().to_string());
            }
        }

        let header_re = Regex::new(r"(?m)^::\s*(.+?)(?:\s*\[|$)").unwrap();
        for cap in header_re.captures_iter(&text) {
            if let Some(m) = cap.get(0) {
                if let Some(name_match) = cap.get(1) {
                    let passage_name = name_match.as_str().trim().to_string();
                    let start_pos = byte_offset_to_position(&text, m.start());
                    let end_pos = byte_offset_to_position(&text, m.end());

                    let text_after_header = &text[m.end()..];
                    let preview = text_after_header
                        .lines()
                        .skip_while(|l| l.trim().is_empty())
                        .take(3)
                        .collect::<Vec<_>>()
                        .join("\n");

                    defined.insert(passage_name, PassageMeta {
                        uri: uri.clone(),
                        range: Range { start: start_pos, end: end_pos },
                        preview,
                    });
                }
            }
        }
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
        passage_references: Arc::new(RwLock::new(HashMap::new())),
        defined_passages: Arc::new(RwLock::new(HashMap::new())),
        variables: Arc::new(RwLock::new(HashSet::new())),
        documents: Arc::new(RwLock::new(HashMap::new())),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
