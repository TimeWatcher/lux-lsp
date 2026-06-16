# Roadmap

## Phase 0: Standards And Repository

- Create the `lux-lsp` repository.
- Define LSP, VS Code, GMod API database, and Document Hover standards.
- Define the replacement of the old realm guard with the `gmod-api-db` driven
  Realm Availability Engine.

## Phase 1: Compiler Analysis API

- Done: extract stable analysis APIs from the Lux compiler.
- Done: provide parse, resolve, module graph, part order, realm stack,
  diagnostics, formatting, hover, completion, definition, semantic token, and
  code action data.
- Done: share the same semantic entry points between CLI, `luxc lsp`, and tests.
- Done: cover multi-part modules, export aliases, unknown externals, UTF-16
  positions, realm domain blocks, and use-before-initialization.

## Phase 2: LSP Server Foundation

- Done: implement an LSP 3.17 server inside `luxc`.
- Done: support initialize, text sync, diagnostics, hover, completion,
  definition, formatting, semantic tokens, and code actions.
- Done: start with Lux symbols before depending on the GMod API database.
- Done: analyze unsaved buffers through workspace root plus in-memory overlays.
- Done: import/export completion, cross-part definition, export alias hover, and
  unknown external quick fixes use the compiler analysis API.

## Phase 3: GMod API Database

- Done: define the database schema used by compiler and LSP.
- Done: implement `gmod-api-update`, which fetches the official Facepunch Wiki
  page list, downloads per-page JSON, parses Facepunch markup, and writes a
  coverage manifest.
- Done: bundle an offline generated database.
- Done: use the official page list as the coverage baseline. The current
  generated database has document records for all 6,335 official pages and a
  semantic API index for 6,121 API candidate pages: 6,121 structured
  conversions, zero fallback documentation pages, and zero failed conversions.
- Done: parse official class and Derma panel parent metadata, so method
  completion, hover, and signature help follow the official class parent chain
  instead of a hand-maintained inheritance table.
- Done: add curated lightweight JSON override layers for known documentation
  corrections.
- Done: expose `luxc gmod api update` through the main compiler CLI.
- VS Code command exposure is tracked in Phase 6 with the extension shell.

## Phase 4: Document Hover And GLua Baseline

- Done: provide documentation-level hover from generated GMod API data.
- Done: include official descriptions, parameters, returns, warnings, notes,
  examples, and links when the official page provides them.
- Done: support hook name hover and callback signatures.
- Done: support API root/member completion and signature help from the shared
  database.
- Done: add receiver/constructor-aware method completion for common GMod
  patterns such as `LocalPlayer()` and `vgui.Create("DButton")`.
- Done: broaden receiver type propagation through local aliases and simple
  function-return facts used by method completion, hover, and signature help.
- Done: use official class and panel parent metadata for inherited method
  completion and documentation, for example `DButton` resolving inherited
  `Panel:SetSize`.

## Phase 5: Realm Availability Engine

- Done: replace the old realm guard with `gmod-api-db`.
- Done: share one query interface between compiler and LSP.
- Done: support path-level realm annotations from the generated official data.
- Done: support source extern declarations and unknown external
  allow/warn/error.
- Done: support package-level extern config from `lux.toml`.
- Done: provide quick fixes for source extern declarations, package-level extern
  entries, and official-docs actions for realm mismatch.
- Done: add export realm widening quick fixes that narrow invalid `export
  shared` declarations to the binding's actual realm when that rewrite is
  unambiguous.

## Phase 6: VS Code Extension

- Done: ship `vscode-lux` with TextMate grammar and language configuration.
- Done: map LSP semantic token types to stable VS Code scopes.
- Done: provide Lux snippets for imports, exports, realm declarations, realm
  blocks, enums, matches, externs, and hook callbacks.
- Done: add settings for compiler path, docs URLs, and LSP trace.
- Done: add commands for restart server, open docs, open GMod docs, update API
  database, compile project, format document, show module exports, show active
  realm, and show GMod API coverage.
- Done: route quick-fix commands through VS Code commands and LSP
  `workspace/executeCommand`; TypeScript does not duplicate the Lux resolver.
- Done: resolve `luxc` from user settings, workspace `.lux/bin`, `LUXC`, or
  PATH, then start `luxc lsp`.
- Done: package a VSIX with `@vscode/vsce`.

## Phase 7: Release

- Done: language server semantics ship with the compiler as `luxc lsp`.
- Done: VSIX release artifacts stay a UI and forwarding shell, without bundled
  server binaries.
- Done: docs site has LSP and VS Code installation pages.
- Done: main Lux README links to this repository and the public documentation.

## Out Of Scope For The First Stage

- copying GLua Enhanced GPL data or implementation
- duplicating the Lux resolver in the VS Code TypeScript extension
- treating unknown externals as shared-safe
- claiming VS Code support is complete after syntax highlighting only
