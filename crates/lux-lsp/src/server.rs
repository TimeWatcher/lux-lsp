use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Notification as LspNotification, PublishDiagnostics,
};
use lsp_types::request::{
    CodeActionRequest, Completion, Formatting, GotoDefinition, HoverRequest, Request as LspRequest,
    SemanticTokensFullRequest,
};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CompletionItem,
    CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    DocumentFormattingParams, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents,
    HoverParams, InitializeParams, InitializeResult, Location, MarkupContent, MarkupKind, OneOf,
    Position, PublishDiagnosticsParams, Range, SemanticToken, SemanticTokenType, SemanticTokens,
    SemanticTokensLegend, SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri,
    WorkDoneProgressOptions,
};
use luxc::analysis::{
    AnalysisCodeAction, AnalysisConfig, AnalysisDiagnostic, AnalysisEditKind, AnalysisFile,
    AnalysisRange, AnalysisSemanticToken, CompletionCandidate, ProjectAnalysis, SemanticTokenKind,
    analyze_source_root, format_text,
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
    analysis: Option<ProjectAnalysis>,
}

impl Server {
    fn new(connection: Connection, initialize: InitializeParams) -> Self {
        let root = workspace_root(&initialize);
        Self {
            connection,
            root,
            documents: HashMap::new(),
            published_diagnostics: BTreeSet::new(),
            analysis: None,
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
        let Some(analysis) = self.analysis.as_ref() else {
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
            CompletionContext::General => general_binding_completions(analysis, &path),
        };

        let items = candidates
            .into_iter()
            .map(completion_item)
            .collect::<Vec<_>>();
        json_result(Some(CompletionResponse::Array(items)))
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
        let Some(analysis) = self.analysis.as_ref() else {
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
        let Some(analysis) = self.analysis.as_ref() else {
            return json_result::<Option<Vec<CodeActionOrCommand>>>(None);
        };
        let Some(path) = url_to_path(&params.text_document.uri) else {
            return json_result::<Option<Vec<CodeActionOrCommand>>>(None);
        };
        let actions = analysis
            .code_actions_for_path(&path)
            .into_iter()
            .map(|action| code_action(action, &params.text_document.uri))
            .collect::<Vec<_>>();
        json_result(Some(actions))
    }

    fn analysis_and_offset(
        &self,
        uri: &Uri,
        position: Position,
    ) -> Option<(&ProjectAnalysis, PathBuf, usize)> {
        let analysis = self.analysis.as_ref()?;
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
        match analyze_source_root(config, overlays) {
            Ok(analysis) => {
                self.publish_diagnostics(&analysis);
                self.analysis = Some(analysis);
            }
            Err(err) => {
                eprintln!("analysis failed: {err}");
                self.clear_all_diagnostics();
                self.analysis = None;
            }
        }
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
    General,
}

fn completion_context(prefix: &str, suffix: &str) -> CompletionContext {
    let line = format!("{prefix}{suffix}");
    let cursor = prefix.len();
    let trimmed = line.trim_start();
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
            completion_context("fn run() = inv", ""),
            CompletionContext::General
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
