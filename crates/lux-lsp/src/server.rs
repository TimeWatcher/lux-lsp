use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use gmod_api_db::{ApiIndex, entry_markdown, hook_markdown};
use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Notification as LspNotification, PublishDiagnostics,
};
use lsp_types::request::{
    CodeActionRequest, Completion, Formatting, GotoDefinition, HoverRequest, Request as LspRequest,
    SemanticTokensFullRequest, SignatureHelpRequest,
};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CompletionItem,
    CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    DocumentFormattingParams, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents,
    HoverParams, InitializeParams, InitializeResult, Location, MarkupContent, MarkupKind, OneOf,
    ParameterInformation, ParameterLabel, Position, PublishDiagnosticsParams, Range, SemanticToken,
    SemanticTokenType, SemanticTokens, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, ServerCapabilities, SignatureHelp,
    SignatureHelpOptions, SignatureInformation, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextEdit, Uri, WorkDoneProgressOptions,
};
use luxc::analysis::{
    AnalysisCodeAction, AnalysisConfig, AnalysisDiagnostic, AnalysisEditKind, AnalysisFile,
    AnalysisRange, AnalysisSemanticToken, AnalysisWorkspace, CompletionCandidate, ProjectAnalysis,
    SemanticTokenKind, format_text,
};
use luxc::diag::Severity;
use luxc::module::RealmSet;
use luxc::project::ProjectManifest;
use url::Url;

pub fn run() -> Result<(), String> {
    let (connection, io_threads) = Connection::stdio();
    let server_capabilities = serde_json::to_value(server_capabilities())
        .map_err(|err| format!("failed to encode capabilities: {err}"))?;
    let initialize_params = connection
        .initialize(server_capabilities)
        .map_err(|err| format!("initialize failed: {err}"))?;
    let initialize_params: InitializeParams = serde_json::from_value(initialize_params)
        .map_err(|err| format!("invalid initialize params: {err}"))?;

    let mut server = Server::new(connection, initialize_params);
    server.event_loop()?;
    io_threads
        .join()
        .map_err(|err| format!("stdio thread failed: {err:?}"))?;
    Ok(())
}

fn server_capabilities() -> InitializeResult {
    InitializeResult {
        capabilities: ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![".".into(), "{".into(), "\"".into()]),
                all_commit_characters: None,
                work_done_progress_options: WorkDoneProgressOptions::default(),
                completion_item: None,
            }),
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: Some(vec!["(".into(), ",".into(), "\"".into()]),
                retrigger_characters: Some(vec![",".into()]),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            definition_provider: Some(OneOf::Left(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
            code_action_provider: Some(lsp_types::CodeActionProviderCapability::Simple(true)),
            semantic_tokens_provider: Some(
                lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(
                    SemanticTokensOptions {
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                        legend: semantic_tokens_legend(),
                        range: Some(false),
                        full: Some(lsp_types::SemanticTokensFullOptions::Bool(true)),
                    },
                ),
            ),
            ..ServerCapabilities::default()
        },
        server_info: Some(lsp_types::ServerInfo {
            name: "lux-lsp".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
    }
}

fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::KEYWORD,
            SemanticTokenType::new("realm"),
            SemanticTokenType::FUNCTION,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::NAMESPACE,
            SemanticTokenType::TYPE,
            SemanticTokenType::STRING,
            SemanticTokenType::NUMBER,
            SemanticTokenType::COMMENT,
            SemanticTokenType::OPERATOR,
            SemanticTokenType::new("export"),
            SemanticTokenType::new("import"),
            SemanticTokenType::new("external"),
            SemanticTokenType::new("unknownExternal"),
        ],
        token_modifiers: Vec::new(),
    }
}

struct Server {
    connection: Connection,
    root: PathBuf,
    documents: HashMap<Uri, String>,
    published_diagnostics: BTreeSet<Uri>,
    workspace: Option<AnalysisWorkspace>,
    gmod_api: ApiIndex,
}

impl Server {
    fn new(connection: Connection, initialize: InitializeParams) -> Self {
        let root = workspace_root(&initialize);
        Self {
            connection,
            root,
            documents: HashMap::new(),
            published_diagnostics: BTreeSet::new(),
            workspace: None,
            gmod_api: ApiIndex::bundled(),
        }
    }

    fn event_loop(&mut self) -> Result<(), String> {
        self.reanalyze_and_publish();
        while let Ok(message) = self.connection.receiver.recv() {
            match message {
                Message::Request(request) => {
                    if self
                        .connection
                        .handle_shutdown(&request)
                        .map_err(|err| err.to_string())?
                    {
                        return Ok(());
                    }
                    self.handle_request(request)?;
                }
                Message::Notification(notification) => {
                    self.handle_notification(notification)?;
                }
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn handle_request(&mut self, request: Request) -> Result<(), String> {
        let request = match self.try_request::<HoverRequest>(request, Self::hover)? {
            Some(request) => request,
            None => return Ok(()),
        };
        let request = match self.try_request::<Completion>(request, Self::completion)? {
            Some(request) => request,
            None => return Ok(()),
        };
        let request =
            match self.try_request::<SignatureHelpRequest>(request, Self::signature_help)? {
                Some(request) => request,
                None => return Ok(()),
            };
        let request = match self.try_request::<GotoDefinition>(request, Self::definition)? {
            Some(request) => request,
            None => return Ok(()),
        };
        let request = match self.try_request::<Formatting>(request, Self::formatting)? {
            Some(request) => request,
            None => return Ok(()),
        };
        let request =
            match self.try_request::<SemanticTokensFullRequest>(request, Self::semantic_tokens)? {
                Some(request) => request,
                None => return Ok(()),
            };
        let request = match self.try_request::<CodeActionRequest>(request, Self::code_actions)? {
            Some(request) => request,
            None => return Ok(()),
        };

        self.respond_error(
            request.id,
            lsp_server::ErrorCode::MethodNotFound as i32,
            format!("unsupported request `{}`", request.method),
        )
    }

    fn try_request<R>(
        &mut self,
        request: Request,
        handler: fn(&mut Self, R::Params) -> Result<serde_json::Value, String>,
    ) -> Result<Option<Request>, String>
    where
        R: LspRequest,
        R::Params: serde::de::DeserializeOwned,
    {
        let invalid_id = request.id.clone();
        match request.extract::<R::Params>(R::METHOD) {
            Ok((id, params)) => {
                let result = handler(self, params);
                match result {
                    Ok(value) => self.respond(id, value),
                    Err(err) => {
                        self.respond_error(id, lsp_server::ErrorCode::InternalError as i32, err)
                    }
                }?;
                Ok(None)
            }
            Err(ExtractError::MethodMismatch(request)) => Ok(Some(request)),
            Err(ExtractError::JsonError { method, error }) => {
                self.respond_error(
                    invalid_id,
                    lsp_server::ErrorCode::InvalidParams as i32,
                    format!("invalid params for {method}: {error}"),
                )?;
                Ok(None)
            }
        }
    }

    fn handle_notification(&mut self, notification: Notification) -> Result<(), String> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams = serde_json::from_value(notification.params)
                    .map_err(|err| format!("invalid didOpen params: {err}"))?;
                self.documents
                    .insert(params.text_document.uri, params.text_document.text);
                self.reanalyze_and_publish();
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)
                        .map_err(|err| format!("invalid didChange params: {err}"))?;
                if let Some(change) = params.content_changes.into_iter().last() {
                    self.documents.insert(params.text_document.uri, change.text);
                    self.reanalyze_and_publish();
                }
            }
            DidSaveTextDocument::METHOD => {
                let params: DidSaveTextDocumentParams = serde_json::from_value(notification.params)
                    .map_err(|err| format!("invalid didSave params: {err}"))?;
                if let Some(text) = params.text {
                    self.documents.insert(params.text_document.uri, text);
                }
                self.reanalyze_and_publish();
            }
            DidCloseTextDocument::METHOD => {
                let params: DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)
                        .map_err(|err| format!("invalid didClose params: {err}"))?;
                self.documents.remove(&params.text_document.uri);
                self.reanalyze_and_publish();
            }
            _ => {}
        }
        Ok(())
    }

    fn hover(&mut self, params: HoverParams) -> Result<serde_json::Value, String> {
        let Some((analysis, path, offset)) = self.analysis_and_offset(
            &params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position,
        ) else {
            return json_result::<Option<Hover>>(None);
        };
        if let Some(markdown) = hook_hover_markdown(analysis, &self.gmod_api, &path, offset) {
            return json_result(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: markdown,
                }),
                range: None,
            }));
        }
        if let Some(markdown) = external_api_hover_markdown(analysis, &self.gmod_api, &path, offset)
        {
            return json_result(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: markdown,
                }),
                range: None,
            }));
        }
        let Some(markdown) = analysis.hover_markdown_at_path_offset(&path, offset) else {
            return json_result::<Option<Hover>>(None);
        };
        json_result(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: markdown,
            }),
            range: None,
        }))
    }

    fn completion(&mut self, params: CompletionParams) -> Result<serde_json::Value, String> {
        let Some(analysis) = self.analysis() else {
            return json_result::<Option<CompletionResponse>>(None);
        };
        let Some(path) = url_to_path(&params.text_document_position.text_document.uri) else {
            return json_result::<Option<CompletionResponse>>(None);
        };
        let (line_prefix, line_suffix, offset) = analysis
            .file_by_path(&path)
            .map(|file| {
                let offset = file.offset_at_line_col_utf16(
                    params.text_document_position.position.line as usize,
                    params.text_document_position.position.character as usize,
                );
                let line_prefix = file.text[..offset].rsplit('\n').next().unwrap_or_default();
                let line_suffix = file.text[offset..].split('\n').next().unwrap_or_default();
                (line_prefix, line_suffix, offset)
            })
            .unwrap_or_default();

        let candidates = match completion_context(line_prefix, line_suffix) {
            CompletionContext::ImportSource => analysis.module_path_completions(),
            CompletionContext::ImportSpecifierList { source } => analysis.importable_exports(
                &path,
                &source,
                analysis
                    .active_realms_at_path_offset(&path, offset)
                    .unwrap_or(RealmSet::SHARED),
            ),
            CompletionContext::ExportList => analysis.exportable_bindings(&path),
            CompletionContext::ApiMember { prefix } => api_completion_candidates(
                &self.gmod_api,
                &prefix,
                analysis.file_by_path(&path).map(|file| file.text.as_str()),
            ),
            CompletionContext::General => {
                let mut candidates = general_binding_completions(analysis, &path);
                candidates.extend(api_root_completion_candidates(&self.gmod_api));
                candidates
            }
        };

        let items = candidates
            .into_iter()
            .map(completion_item)
            .collect::<Vec<_>>();
        json_result(Some(CompletionResponse::Array(items)))
    }

    fn signature_help(
        &mut self,
        params: lsp_types::SignatureHelpParams,
    ) -> Result<serde_json::Value, String> {
        let Some((analysis, path, offset)) = self.analysis_and_offset(
            &params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position,
        ) else {
            return json_result::<Option<SignatureHelp>>(None);
        };
        let Some(file) = analysis.file_by_path(&path) else {
            return json_result::<Option<SignatureHelp>>(None);
        };
        let Some(help) = signature_help_at(file, &self.gmod_api, offset) else {
            return json_result::<Option<SignatureHelp>>(None);
        };
        json_result(Some(help))
    }

    fn definition(&mut self, params: GotoDefinitionParams) -> Result<serde_json::Value, String> {
        let Some((analysis, path, offset)) = self.analysis_and_offset(
            &params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position,
        ) else {
            return json_result::<Option<GotoDefinitionResponse>>(None);
        };
        let Some(symbol) = analysis.symbol_at_path_offset(&path, offset) else {
            return json_result::<Option<GotoDefinitionResponse>>(None);
        };
        let Some(def_span) = symbol.definition_span else {
            return json_result::<Option<GotoDefinitionResponse>>(None);
        };
        let Some(def_path) = symbol.definition_path else {
            return json_result::<Option<GotoDefinitionResponse>>(None);
        };
        let Some(file) = analysis.file_by_id(def_span.file_id) else {
            return json_result::<Option<GotoDefinitionResponse>>(None);
        };
        let Some(uri) = path_to_url(&def_path) else {
            return json_result::<Option<GotoDefinitionResponse>>(None);
        };
        json_result(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: range(file, def_span),
        })))
    }

    fn formatting(
        &mut self,
        params: DocumentFormattingParams,
    ) -> Result<serde_json::Value, String> {
        let Some(path) = url_to_path(&params.text_document.uri) else {
            return json_result::<Option<Vec<TextEdit>>>(None);
        };
        let text = self
            .documents
            .get(&params.text_document.uri)
            .cloned()
            .or_else(|| std::fs::read_to_string(&path).ok())
            .unwrap_or_default();
        let output = format_text(path.clone(), text.clone());
        let file = luxc::source::SourceFile::new(0, Some(path), text);
        let edits = if output.text == file.text {
            Vec::new()
        } else {
            vec![TextEdit {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: range(
                        &file,
                        luxc::source::SourceSpan::new(file.id, 0, file.text.len()),
                    )
                    .end,
                },
                new_text: output.text,
            }]
        };
        json_result(Some(edits))
    }

    fn semantic_tokens(
        &mut self,
        params: SemanticTokensParams,
    ) -> Result<serde_json::Value, String> {
        let Some(analysis) = self.analysis() else {
            return json_result::<Option<SemanticTokensResult>>(None);
        };
        let Some(path) = url_to_path(&params.text_document.uri) else {
            return json_result::<Option<SemanticTokensResult>>(None);
        };
        let Some(file) = analysis.file_by_path(&path) else {
            return json_result::<Option<SemanticTokensResult>>(None);
        };
        let data = encode_semantic_tokens(file, analysis.semantic_tokens_for_path(&path));
        json_result(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    fn code_actions(&mut self, params: CodeActionParams) -> Result<serde_json::Value, String> {
        let Some(analysis) = self.analysis() else {
            return json_result::<Option<Vec<CodeActionOrCommand>>>(None);
        };
        let Some(path) = url_to_path(&params.text_document.uri) else {
            return json_result::<Option<Vec<CodeActionOrCommand>>>(None);
        };
        let actions = analysis
            .code_actions_for_path(&path)
            .into_iter()
            .map(|action| code_action(action, &params.text_document.uri))
            .chain(api_doc_code_actions(
                analysis,
                &self.gmod_api,
                &path,
                &params.text_document.uri,
            ))
            .chain(manifest_extern_code_actions(analysis, &path, &self.root))
            .collect::<Vec<_>>();
        json_result(Some(actions))
    }

    fn analysis_and_offset(
        &self,
        uri: &Uri,
        position: Position,
    ) -> Option<(&ProjectAnalysis, PathBuf, usize)> {
        let analysis = self.analysis()?;
        let path = url_to_path(uri)?;
        let offset = analysis.offset_for_position(
            &path,
            position.line as usize,
            position.character as usize,
        )?;
        Some((analysis, path, offset))
    }

    fn reanalyze_and_publish(&mut self) {
        let config = analysis_config(&self.root);
        let overlays = self
            .documents
            .iter()
            .filter_map(|(uri, text): (&Uri, &String)| {
                Some(AnalysisFile {
                    path: url_to_path(uri)?,
                    text: text.clone(),
                })
            })
            .collect::<Vec<_>>();
        let result = if let Some(workspace) = &mut self.workspace {
            workspace.update_source_root(config, overlays).map(|_| ())
        } else {
            AnalysisWorkspace::load(config, overlays).map(|workspace| {
                self.workspace = Some(workspace);
            })
        };
        match result {
            Ok(()) => {
                let analysis = self
                    .workspace
                    .as_ref()
                    .expect("workspace loaded")
                    .analysis()
                    .clone();
                self.publish_diagnostics(&analysis);
            }
            Err(err) => {
                eprintln!("analysis failed: {err}");
                self.clear_all_diagnostics();
                self.workspace = None;
            }
        }
    }

    fn analysis(&self) -> Option<&ProjectAnalysis> {
        self.workspace.as_ref().map(AnalysisWorkspace::analysis)
    }

    fn publish_diagnostics(&mut self, analysis: &ProjectAnalysis) {
        let mut diagnostics_by_url = BTreeMap::<Uri, Vec<Diagnostic>>::new();
        for file in &analysis.files {
            let Some(path) = file.path.as_ref() else {
                continue;
            };
            let Some(uri) = path_to_url(path) else {
                continue;
            };
            let diagnostics = analysis
                .lsp_diagnostics_for_path(path)
                .into_iter()
                .map(lsp_diagnostic)
                .collect::<Vec<_>>();
            diagnostics_by_url.insert(uri, diagnostics);
        }
        for uri in self
            .published_diagnostics
            .difference(&diagnostics_by_url.keys().cloned().collect::<BTreeSet<_>>())
        {
            let params = PublishDiagnosticsParams {
                uri: uri.clone(),
                diagnostics: Vec::new(),
                version: None,
            };
            let _ = self
                .connection
                .sender
                .send(Message::Notification(Notification {
                    method: PublishDiagnostics::METHOD.into(),
                    params: serde_json::to_value(params).unwrap_or_default(),
                }));
        }
        self.published_diagnostics = diagnostics_by_url.keys().cloned().collect();
        for (uri, diagnostics) in diagnostics_by_url {
            let params = PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: None,
            };
            let _ = self
                .connection
                .sender
                .send(Message::Notification(Notification {
                    method: PublishDiagnostics::METHOD.into(),
                    params: serde_json::to_value(params).unwrap_or_default(),
                }));
        }
    }

    fn clear_all_diagnostics(&mut self) {
        for uri in std::mem::take(&mut self.published_diagnostics) {
            let params = PublishDiagnosticsParams {
                uri,
                diagnostics: Vec::new(),
                version: None,
            };
            let _ = self
                .connection
                .sender
                .send(Message::Notification(Notification {
                    method: PublishDiagnostics::METHOD.into(),
                    params: serde_json::to_value(params).unwrap_or_default(),
                }));
        }
    }

    fn respond(&self, id: RequestId, result: serde_json::Value) -> Result<(), String> {
        self.connection
            .sender
            .send(Message::Response(Response {
                id,
                result: Some(result),
                error: None,
            }))
            .map_err(|err| format!("failed to send response: {err}"))
    }

    fn respond_error(&self, id: RequestId, code: i32, message: String) -> Result<(), String> {
        self.connection
            .sender
            .send(Message::Response(Response {
                id,
                result: None,
                error: Some(lsp_server::ResponseError {
                    code,
                    message,
                    data: None,
                }),
            }))
            .map_err(|err| format!("failed to send error response: {err}"))
    }
}

trait BindingKindName {
    fn name(self) -> &'static str;
}

impl BindingKindName for luxc::resolve::BindingKind {
    fn name(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Const => "const",
            Self::Param => "parameter",
            Self::Function => "function",
            Self::Import => "import",
            Self::MacroImport => "macro import",
        }
    }
}

fn analysis_config(root: &Path) -> AnalysisConfig {
    let manifest_path = find_manifest(root);
    manifest_path
        .and_then(|path| ProjectManifest::load(path).ok())
        .map(AnalysisConfig::from_manifest)
        .unwrap_or_else(|| AnalysisConfig::new(root))
}

#[allow(deprecated)]
fn workspace_root(initialize: &InitializeParams) -> PathBuf {
    initialize
        .workspace_folders
        .as_ref()
        .and_then(|folders| folders.first())
        .and_then(|folder| url_to_path(&folder.uri))
        .or_else(|| initialize.root_uri.as_ref().and_then(url_to_path))
        .or_else(|| initialize.root_path.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn find_manifest(root: &Path) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    loop {
        let candidate = current.join("lux.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn manifest_extern_code_actions(
    analysis: &ProjectAnalysis,
    path: &Path,
    root: &Path,
) -> Vec<CodeActionOrCommand> {
    let Some(manifest_path) = find_manifest(root) else {
        return Vec::new();
    };
    analysis
        .diagnostics_for_path(path)
        .into_iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("REALM_UNKNOWN"))
        .filter_map(|diagnostic| diagnostic_symbol_name(&diagnostic.message))
        .flat_map(|symbol| {
            ["shared", "client", "server"]
                .into_iter()
                .map(move |realm| (symbol.clone(), realm))
        })
        .filter_map(|(symbol, realm)| {
            let uri = path_to_url(&manifest_path)?;
            let edit = manifest_extern_edit(&manifest_path, &symbol, realm);
            let mut changes = HashMap::<Uri, Vec<TextEdit>>::new();
            changes.insert(uri, vec![edit]);
            Some(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Add package extern {realm} {symbol}"),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: None,
                edit: Some(lsp_types::WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                }),
                command: None,
                is_preferred: None,
                disabled: None,
                data: None,
            }))
        })
        .collect()
}

fn manifest_extern_edit(manifest_path: &Path, symbol: &str, realm: &str) -> TextEdit {
    let text = std::fs::read_to_string(manifest_path).unwrap_or_default();
    let escaped_symbol = symbol.replace('\\', "\\\\").replace('"', "\\\"");
    let new_entry = format!("{escaped_symbol} = \"{realm}\"\n");
    if let Some((line, character)) = manifest_section_insert_position(&text, "target.gmod.extern") {
        TextEdit {
            range: Range {
                start: Position { line, character },
                end: Position { line, character },
            },
            new_text: new_entry,
        }
    } else {
        let prefix = if text.trim().is_empty() || text.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        TextEdit {
            range: end_of_document_range(&text),
            new_text: format!("{prefix}\n[target.gmod.extern]\n{new_entry}"),
        }
    }
}

fn manifest_section_insert_position(text: &str, section: &str) -> Option<(u32, u32)> {
    let mut in_section = false;
    let mut insert_line = None;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_section {
                return Some((index as u32, 0));
            }
            in_section = trimmed == format!("[{section}]");
        } else if in_section && !trimmed.is_empty() {
            insert_line = Some(index + 1);
        }
    }
    in_section.then_some((
        insert_line.unwrap_or_else(|| text.lines().count()) as u32,
        0,
    ))
}

fn end_of_document_range(text: &str) -> Range {
    let line_count = text.lines().count();
    let last_line_len = text.lines().last().map(utf16_len).unwrap_or(0);
    let line = if text.ends_with('\n') {
        line_count as u32
    } else {
        line_count.saturating_sub(1) as u32
    };
    let character = if text.ends_with('\n') {
        0
    } else {
        last_line_len as u32
    };
    Range {
        start: Position { line, character },
        end: Position { line, character },
    }
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn lsp_diagnostic(diagnostic: AnalysisDiagnostic) -> Diagnostic {
    Diagnostic {
        range: lsp_range(diagnostic.range),
        severity: Some(match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Note => DiagnosticSeverity::INFORMATION,
        }),
        code: diagnostic.code.map(lsp_types::NumberOrString::String),
        code_description: None,
        source: Some("luxc".into()),
        message: diagnostic.message,
        related_information: if diagnostic.notes.is_empty() && diagnostic.help.is_none() {
            None
        } else {
            Some(
                diagnostic
                    .notes
                    .into_iter()
                    .chain(diagnostic.help)
                    .map(|message| DiagnosticRelatedInformation {
                        location: Location {
                            uri: path_to_url(&diagnostic.path)
                                .unwrap_or_else(|| uri_from_url(Url::parse("file:///").unwrap())),
                            range: lsp_range(diagnostic.range),
                        },
                        message,
                    })
                    .collect(),
            )
        },
        tags: None,
        data: None,
    }
}

fn code_action(action: AnalysisCodeAction, uri: &Uri) -> CodeActionOrCommand {
    let diagnostics = action
        .diagnostics
        .iter()
        .map(|code| Diagnostic {
            range: Range::default(),
            severity: None,
            code: Some(lsp_types::NumberOrString::String(code.clone())),
            code_description: None,
            source: Some("luxc".into()),
            message: code.clone(),
            related_information: None,
            tags: None,
            data: None,
        })
        .collect::<Vec<_>>();
    let mut changes = HashMap::<Uri, Vec<TextEdit>>::new();
    for edit in action.edits {
        let edit_uri = path_to_url(&edit.path).unwrap_or_else(|| uri.clone());
        changes.entry(edit_uri).or_default().push(TextEdit {
            range: lsp_range(edit.range),
            new_text: edit.new_text,
        });
    }
    CodeActionOrCommand::CodeAction(CodeAction {
        title: action.title,
        kind: Some(match action.kind {
            AnalysisEditKind::Safe => CodeActionKind::QUICKFIX,
            AnalysisEditKind::Guided => CodeActionKind::QUICKFIX,
            AnalysisEditKind::Refactor => CodeActionKind::REFACTOR,
        }),
        diagnostics: Some(diagnostics),
        edit: (!changes.is_empty()).then_some(lsp_types::WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: action.command.map(|command| lsp_types::Command {
            title: command.clone(),
            command,
            arguments: None,
        }),
        is_preferred: None,
        disabled: None,
        data: None,
    })
}

fn api_doc_code_actions(
    analysis: &ProjectAnalysis,
    api: &ApiIndex,
    path: &Path,
    _uri: &Uri,
) -> Vec<CodeActionOrCommand> {
    analysis
        .diagnostics_for_path(path)
        .into_iter()
        .filter(|diagnostic| diagnostic.code.as_deref() == Some("REALM001"))
        .filter_map(|diagnostic| diagnostic_symbol_name(&diagnostic.message))
        .filter_map(|symbol| {
            api.entry(&symbol)
                .and_then(|entry| entry.official_url.as_ref())
        })
        .map(|url| {
            CodeActionOrCommand::CodeAction(CodeAction {
                title: "Open official GMod documentation".into(),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: None,
                edit: None,
                command: Some(lsp_types::Command {
                    title: "Open official GMod documentation".into(),
                    command: "lux.openGmodDocs".into(),
                    arguments: Some(vec![serde_json::Value::String(url.clone())]),
                }),
                is_preferred: None,
                disabled: None,
                data: None,
            })
        })
        .collect()
}

fn diagnostic_symbol_name(message: &str) -> Option<String> {
    let start = message.find('`')? + 1;
    let end = message[start..].find('`')? + start;
    Some(message[start..end].to_string())
}

fn completion_item(candidate: CompletionCandidate) -> CompletionItem {
    CompletionItem {
        label: candidate.label,
        kind: Some(CompletionItemKind::VARIABLE),
        detail: candidate.detail,
        documentation: candidate.documentation.map(|value| {
            lsp_types::Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            })
        }),
        ..CompletionItem::default()
    }
}

fn encode_semantic_tokens(
    file: &luxc::source::SourceFile,
    mut tokens: Vec<AnalysisSemanticToken>,
) -> Vec<SemanticToken> {
    tokens.sort_by_key(|token| {
        let token_range = range(file, token.span);
        (
            token_range.start.line,
            token_range.start.character,
            token.span.len(),
        )
    });
    let mut encoded = Vec::new();
    let mut last_line = 0u32;
    let mut last_start = 0u32;
    for token in tokens {
        let token_range = range(file, token.span);
        if token_range.start.line != token_range.end.line {
            continue;
        }
        let line = token_range.start.line;
        let start = token_range.start.character;
        let delta_line = line.saturating_sub(last_line);
        let delta_start = if delta_line == 0 {
            start.saturating_sub(last_start)
        } else {
            start
        };
        let length = token_range
            .end
            .character
            .saturating_sub(token_range.start.character);
        if length == 0 {
            continue;
        }
        encoded.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type: semantic_token_type(token.kind),
            token_modifiers_bitset: 0,
        });
        last_line = line;
        last_start = start;
    }
    encoded
}

fn semantic_token_type(kind: SemanticTokenKind) -> u32 {
    match kind {
        SemanticTokenKind::Keyword => 0,
        SemanticTokenKind::Realm => 1,
        SemanticTokenKind::Function => 2,
        SemanticTokenKind::Parameter => 3,
        SemanticTokenKind::Variable => 4,
        SemanticTokenKind::Property => 5,
        SemanticTokenKind::Namespace => 6,
        SemanticTokenKind::Type => 7,
        SemanticTokenKind::String => 8,
        SemanticTokenKind::Number => 9,
        SemanticTokenKind::Comment => 10,
        SemanticTokenKind::Operator => 11,
        SemanticTokenKind::Export => 12,
        SemanticTokenKind::Import => 13,
        SemanticTokenKind::External => 14,
        SemanticTokenKind::UnknownExternal => 15,
    }
}

fn range(file: &luxc::source::SourceFile, span: luxc::source::SourceSpan) -> Range {
    let analysis_range = AnalysisRange {
        start: {
            let (line, col) = file.line_col_utf16(span.byte_start);
            luxc::analysis::AnalysisPosition {
                line: line.saturating_sub(1) as u32,
                character: col.saturating_sub(1) as u32,
            }
        },
        end: {
            let (line, col) = file.line_col_utf16(span.byte_end);
            luxc::analysis::AnalysisPosition {
                line: line.saturating_sub(1) as u32,
                character: col.saturating_sub(1) as u32,
            }
        },
    };
    lsp_range(analysis_range)
}

fn lsp_range(range: AnalysisRange) -> Range {
    Range {
        start: Position {
            line: range.start.line,
            character: range.start.character,
        },
        end: Position {
            line: range.end.line,
            character: range.end.character,
        },
    }
}

fn external_api_hover_markdown(
    analysis: &ProjectAnalysis,
    api: &ApiIndex,
    path: &Path,
    offset: usize,
) -> Option<String> {
    let file = analysis.file_by_path(path)?;
    if let Some(method_path) = method_path_at_offset(&file.text, offset) {
        let facts = GmodTypeFacts::from_text(&file.text);
        if let Some(resolved_path) = resolve_typed_method_path(&facts, &method_path)
            && let Some(entry) = api.entry(&resolved_path)
        {
            return Some(entry_markdown(entry));
        }
    }
    let symbol = analysis.symbol_at_path_offset(path, offset)?;
    let external = symbol.external_availability.as_ref()?;
    if matches!(external, luxc::module::RealmAvailability::UnknownExternal) {
        return None;
    }
    let entry = api.entry(&symbol.name)?;
    Some(entry_markdown(entry))
}

fn hook_hover_markdown(
    analysis: &ProjectAnalysis,
    api: &ApiIndex,
    path: &Path,
    offset: usize,
) -> Option<String> {
    let file = analysis.file_by_path(path)?;
    let hook_name = hook_name_at_offset(&file.text, offset)?;
    api.hook(&hook_name).map(hook_markdown)
}

fn hook_name_at_offset(text: &str, offset: usize) -> Option<String> {
    let clamped = offset.min(text.len());
    let before = &text[..clamped];
    let after = &text[clamped..];
    let quote_start = before.rfind(['"', '\''])?;
    let quote = before[quote_start..].chars().next()?;
    let hook_prefix = before[..quote_start].trim_end();
    if !hook_prefix.ends_with("hook.Add(") {
        return None;
    }
    let quote_end = after.find(quote).unwrap_or(after.len());
    Some(format!(
        "{}{}",
        &before[quote_start + quote.len_utf8()..],
        &after[..quote_end]
    ))
}

fn signature_help_at(
    file: &luxc::source::SourceFile,
    api: &ApiIndex,
    offset: usize,
) -> Option<SignatureHelp> {
    let text = &file.text[..offset.min(file.text.len())];
    if let Some(hook_name) = hook_name_in_call_prefix(text)
        && let Some(hook) = api.hook(&hook_name)
    {
        return Some(signature_help_from_signature(&hook.callback));
    }
    let call_path = call_path_before_cursor(text)?;
    let facts = GmodTypeFacts::from_text(&file.text);
    let resolved_call_path = resolve_typed_method_path(&facts, &call_path).unwrap_or(call_path);
    let entry = api.entry(&resolved_call_path)?;
    entry.signatures.first().map(signature_help_from_signature)
}

fn hook_name_in_call_prefix(text: &str) -> Option<String> {
    let hook_index = text.rfind("hook.Add(")?;
    let after = &text[hook_index + "hook.Add(".len()..];
    let quote = after.chars().find(|ch| *ch == '"' || *ch == '\'')?;
    let start = after.find(quote)? + quote.len_utf8();
    let rest = &after[start..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn call_path_before_cursor(text: &str) -> Option<String> {
    let open = text.rfind('(')?;
    let before = text[..open].trim_end();
    let token = before
        .rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':')))
        .next()
        .unwrap_or_default();
    (!token.is_empty()).then(|| token.to_string())
}

fn method_path_at_offset(text: &str, offset: usize) -> Option<String> {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let after = &text[offset..];
    let left = before
        .rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':')))
        .next()
        .unwrap_or_default();
    let right = after
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .next()
        .unwrap_or_default();
    let path = format!("{left}{right}");
    path.contains(':').then_some(path)
}

fn resolve_typed_method_path(facts: &GmodTypeFacts, path: &str) -> Option<String> {
    let (receiver, method) = path.split_once(':')?;
    if receiver.is_empty() || method.is_empty() {
        return None;
    }
    facts
        .receiver_class(receiver)
        .map(|class_name| format!("{class_name}:{method}"))
}

fn signature_help_from_signature(signature: &gmod_api_db::ApiSignature) -> SignatureHelp {
    SignatureHelp {
        signatures: vec![SignatureInformation {
            label: signature.label.clone(),
            documentation: None,
            parameters: Some(
                signature
                    .parameters
                    .iter()
                    .map(|parameter| ParameterInformation {
                        label: ParameterLabel::Simple(parameter.name.clone()),
                        documentation: Some(lsp_types::Documentation::String(format!(
                            "{} - {}",
                            parameter.ty, parameter.description
                        ))),
                    })
                    .collect(),
            ),
            active_parameter: None,
        }],
        active_signature: Some(0),
        active_parameter: Some(0),
    }
}

fn json_result<T: serde::Serialize>(value: T) -> Result<serde_json::Value, String> {
    serde_json::to_value(value).map_err(|err| format!("failed to encode LSP result: {err}"))
}

fn url_to_path(uri: &Uri) -> Option<PathBuf> {
    let parsed = Url::parse(uri.as_str()).ok()?;
    parsed.to_file_path().ok()
}

fn path_to_url(path: &Path) -> Option<Uri> {
    Url::from_file_path(path).ok().map(uri_from_url)
}

fn uri_from_url(url: Url) -> Uri {
    url.as_str()
        .parse()
        .expect("file URL should be a valid URI")
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CompletionContext {
    ImportSource,
    ImportSpecifierList { source: String },
    ExportList,
    ApiMember { prefix: String },
    General,
}

fn completion_context(prefix: &str, suffix: &str) -> CompletionContext {
    let line = format!("{prefix}{suffix}");
    let cursor = prefix.len();
    let trimmed = line.trim_start();
    if let Some(prefix) = api_member_prefix(prefix) {
        return CompletionContext::ApiMember { prefix };
    }
    if is_import_specifier_context(&line, cursor) {
        if let Some(source) = import_source_for_specifier_list(&line) {
            return CompletionContext::ImportSpecifierList { source };
        }
    }
    if is_import_source_context(prefix) {
        return CompletionContext::ImportSource;
    }
    if trimmed.starts_with("export") && is_cursor_inside_braces(&line, cursor) {
        return CompletionContext::ExportList;
    }
    CompletionContext::General
}

fn api_member_prefix(prefix: &str) -> Option<String> {
    let token = prefix
        .rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':')))
        .next()
        .unwrap_or_default();
    if token.ends_with('.') || token.ends_with(':') {
        return Some(token.to_string());
    }
    token
        .rfind(['.', ':'])
        .map(|index| token[..index].to_string())
        .filter(|prefix| !prefix.is_empty())
}

fn general_binding_completions(
    analysis: &ProjectAnalysis,
    path: &Path,
) -> Vec<CompletionCandidate> {
    analysis
        .module_for_path(path)
        .map(|module| {
            module
                .resolved
                .bindings
                .iter()
                .map(|binding| CompletionCandidate {
                    label: binding.name.clone(),
                    detail: Some(binding.kind.name().into()),
                    documentation: Some(format!(
                        "Lux binding in `{}` ({})",
                        module.id,
                        binding.available_realms.display_name()
                    )),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn api_root_completion_candidates(api: &ApiIndex) -> Vec<CompletionCandidate> {
    api.roots().into_iter().map(api_entry_candidate).collect()
}

fn api_completion_candidates(
    api: &ApiIndex,
    prefix: &str,
    file_text: Option<&str>,
) -> Vec<CompletionCandidate> {
    if prefix.ends_with(':') {
        let receiver = prefix.trim_end_matches(':');
        if let Some(class_name) = file_text.and_then(|text| {
            let facts = GmodTypeFacts::from_text(text);
            facts.receiver_class(receiver)
        }) {
            let candidates = api
                .methods_for_class(&class_name)
                .into_iter()
                .map(api_entry_candidate)
                .collect::<Vec<_>>();
            if !candidates.is_empty() {
                return candidates;
            }
        }
        let candidates = api
            .methods_for_class(receiver)
            .into_iter()
            .map(api_entry_candidate)
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            return candidates;
        }
    }
    let needle = if prefix.ends_with('.') || prefix.ends_with(':') {
        prefix.to_string()
    } else {
        format!("{prefix}.")
    };
    api.completions_for_prefix(&needle)
        .into_iter()
        .map(api_entry_candidate)
        .collect()
}

fn infer_receiver_class(text: &str, receiver: &str) -> Option<String> {
    GmodTypeFacts::from_text(text).receiver_class(receiver)
}

#[derive(Debug, Default)]
struct GmodTypeFacts {
    locals: HashMap<String, String>,
    functions: HashMap<String, String>,
}

impl GmodTypeFacts {
    fn from_text(text: &str) -> Self {
        let mut facts = Self::default();
        for line in text.lines() {
            facts.learn_line(line.trim());
        }
        facts
    }

    fn receiver_class(&self, receiver: &str) -> Option<String> {
        self.locals
            .get(receiver)
            .cloned()
            .or_else(|| self.functions.get(receiver).cloned())
            .or_else(|| gmod_constructor_class(receiver).map(str::to_string))
    }

    fn learn_line(&mut self, line: &str) {
        if line.starts_with("--") || line.is_empty() {
            return;
        }
        if let Some(rest) = line.strip_prefix("fn ")
            && let Some((name, expr)) = split_function_expr(rest)
            && let Some(class_name) = self.class_for_expr(expr)
        {
            self.functions.insert(name.to_string(), class_name);
            return;
        }
        if let Some(rest) = line.strip_prefix("local ") {
            self.learn_assignment(rest);
            return;
        }
        self.learn_assignment(line);
    }

    fn learn_assignment(&mut self, input: &str) {
        let Some((name, expr)) = input.split_once('=') else {
            return;
        };
        let name = name.trim();
        if !is_simple_ident(name) {
            return;
        }
        if let Some(class_name) = self.class_for_expr(expr.trim()) {
            self.locals.insert(name.to_string(), class_name);
        }
    }

    fn class_for_expr(&self, expr: &str) -> Option<String> {
        let expr = expr.trim();
        if expr.starts_with("LocalPlayer(") || expr.starts_with("Player(") {
            Some("Player".to_string())
        } else if expr.starts_with("Entity(") {
            Some("Entity".to_string())
        } else if let Some(rest) = expr.strip_prefix("vgui.Create(") {
            quoted_first_arg(rest).or_else(|| Some("Panel".to_string()))
        } else if let Some(name) = expr.strip_suffix("()").filter(|name| is_simple_ident(name)) {
            self.functions.get(name).cloned()
        } else if is_simple_ident(expr) {
            self.locals.get(expr).cloned()
        } else {
            None
        }
    }
}

fn split_function_expr(input: &str) -> Option<(&str, &str)> {
    let (name_and_args, expr) = input.split_once('=')?;
    let name = name_and_args.split('(').next()?.trim();
    is_simple_ident(name).then_some((name, expr.trim()))
}

fn is_simple_ident(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn gmod_constructor_class(name: &str) -> Option<&'static str> {
    match name {
        "LocalPlayer" | "Player" => Some("Player"),
        "Entity" => Some("Entity"),
        _ => None,
    }
}

fn quoted_first_arg(text: &str) -> Option<String> {
    let text = text.trim_start();
    let quote = text.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &text[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn api_entry_candidate(entry: &gmod_api_db::ApiEntry) -> CompletionCandidate {
    CompletionCandidate {
        label: entry
            .path
            .rsplit(['.', ':'])
            .next()
            .unwrap_or(&entry.path)
            .to_string(),
        detail: Some(format!(
            "GMod {}, {}",
            entry.kind.label(),
            entry.realm.as_str()
        )),
        documentation: Some(entry_markdown(entry)),
    }
}

fn is_import_source_context(prefix: &str) -> bool {
    let trimmed = prefix.trim_start();
    if !trimmed.starts_with("import") {
        return false;
    }
    let Some(from_index) = trimmed.rfind("from") else {
        return false;
    };
    let after_from = trimmed[from_index + "from".len()..].trim_start();
    after_from.starts_with('"') || after_from.starts_with('\'') || after_from.is_empty()
}

fn is_import_specifier_context(line: &str, cursor: usize) -> bool {
    line.trim_start().starts_with("import") && is_cursor_inside_braces(line, cursor)
}

fn is_cursor_inside_braces(line: &str, cursor: usize) -> bool {
    let Some(open) = line.find('{') else {
        return false;
    };
    let close = line[open + 1..]
        .find('}')
        .map(|offset| open + 1 + offset)
        .unwrap_or(line.len());
    open < cursor && cursor <= close
}

fn import_source_for_specifier_list(prefix: &str) -> Option<String> {
    let from_index = prefix.rfind("from")?;
    let after_from = prefix[from_index + "from".len()..].trim_start();
    let quote = after_from.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &after_from[quote.len_utf8()..];
    let value = rest.split(quote).next().unwrap_or(rest).to_string();
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::{
        CompletionContext, completion_context, encode_semantic_tokens, path_to_url, url_to_path,
    };
    use super::{
        GmodTypeFacts, api_completion_candidates, hook_name_at_offset, infer_receiver_class,
        manifest_section_insert_position, method_path_at_offset, resolve_typed_method_path,
        signature_help_at,
    };
    use gmod_api_db::ApiIndex;
    use lsp_types::SemanticToken;
    use luxc::analysis::{AnalysisSemanticToken, SemanticTokenKind};
    use luxc::source::{SourceFile, SourceSpan};

    #[test]
    fn completion_context_detects_import_source_and_specifier_lists() {
        assert_eq!(
            completion_context("import { p_", " } from \"inventory\""),
            CompletionContext::ImportSpecifierList {
                source: "inventory".into()
            }
        );
        assert_eq!(
            completion_context("  import { } from \"", ""),
            CompletionContext::ImportSource
        );
        assert_eq!(
            completion_context("export { player_", " }"),
            CompletionContext::ExportList
        );
        assert_eq!(
            completion_context("net.", ""),
            CompletionContext::ApiMember {
                prefix: "net.".into()
            }
        );
        assert_eq!(
            completion_context("fn run() = inv", ""),
            CompletionContext::General
        );
    }

    #[test]
    fn hook_hover_context_extracts_hook_names() {
        let text = "hook.Add(\"PlayerInitialSpawn\", \"id\", function(ply) end)";
        let offset = text.find("Initial").expect("offset");
        assert_eq!(
            hook_name_at_offset(text, offset),
            Some("PlayerInitialSpawn".into())
        );
    }

    #[test]
    fn signature_help_uses_gmod_api_database() {
        let api = ApiIndex::bundled();
        let file = SourceFile::new(0, None, "net.Start(");
        let help = signature_help_at(&file, &api, file.text.len()).expect("signature help");
        assert_eq!(
            help.signatures[0].label,
            "net.Start(messageName, unreliable = false)"
        );
        assert_eq!(help.signatures[0].parameters.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn manifest_extern_insert_position_targets_existing_section() {
        let text = "source_root = \"src\"\n\n[target.gmod.extern]\nA = \"shared\"\n\n[gmod]\naddon_root = \".\"\n";
        assert_eq!(
            manifest_section_insert_position(text, "target.gmod.extern"),
            Some((5, 0))
        );
    }

    #[test]
    fn infers_gmod_receiver_class_from_common_constructors() {
        let text = "fn current() = LocalPlayer()\nlocal ply = current()\nlocal alias = ply\nlocal button = vgui.Create(\"DButton\")\n";
        assert_eq!(infer_receiver_class(text, "ply"), Some("Player".into()));
        assert_eq!(infer_receiver_class(text, "alias"), Some("Player".into()));
        assert_eq!(infer_receiver_class(text, "button"), Some("DButton".into()));
    }

    #[test]
    fn api_completion_uses_receiver_class_methods_for_colon_calls() {
        let api = ApiIndex::bundled();
        let text = "local ply = LocalPlayer()\nply:";
        let labels = api_completion_candidates(&api, "ply:", Some(text))
            .into_iter()
            .map(|candidate| candidate.label)
            .collect::<Vec<_>>();
        assert!(labels.iter().any(|label| label == "Nick"), "{labels:#?}");
    }

    #[test]
    fn signature_help_uses_receiver_type_facts_for_method_calls() {
        let api = ApiIndex::bundled();
        let file = SourceFile::new(0, None, "local ply = LocalPlayer()\nply:Nick(");
        let help = signature_help_at(&file, &api, file.text.len()).expect("signature help");
        assert_eq!(help.signatures[0].label, "Player:Nick()");
    }

    #[test]
    fn hover_method_path_uses_receiver_type_facts() {
        let text = "local ply = LocalPlayer()\nply:Nick()";
        let offset = text.find("Nick").expect("offset");
        let path = method_path_at_offset(text, offset).expect("method path");
        let facts = GmodTypeFacts::from_text(text);
        assert_eq!(path, "ply:Nick");
        assert_eq!(
            resolve_typed_method_path(&facts, &path),
            Some("Player:Nick".into())
        );
    }

    #[test]
    fn semantic_tokens_are_sorted_and_delta_encoded() {
        let file = SourceFile::new(0, None, "fn run()\n  local value = 1\n");
        let tokens = vec![
            AnalysisSemanticToken {
                span: SourceSpan::new(file.id, 11, 16),
                kind: SemanticTokenKind::Keyword,
            },
            AnalysisSemanticToken {
                span: SourceSpan::new(file.id, 3, 6),
                kind: SemanticTokenKind::Function,
            },
        ];

        let encoded = encode_semantic_tokens(&file, tokens);
        assert_eq!(
            encoded,
            vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 3,
                    length: 3,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 1,
                    delta_start: 2,
                    length: 5,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
            ]
        );
    }

    #[test]
    fn file_uri_round_trip_preserves_paths() {
        let path = std::env::current_dir()
            .expect("cwd")
            .join("src")
            .join("module.lux");
        let uri = path_to_url(&path).expect("file uri");
        let round_tripped = url_to_path(&uri).expect("path");
        assert_eq!(round_tripped, path);
    }
}
