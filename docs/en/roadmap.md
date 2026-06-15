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
- Done: share the same semantic entry points between CLI, LSP, and tests.
- Done: cover multi-part modules, export aliases, unknown externals, UTF-16
  positions, realm domain blocks, and use-before-initialization.

## Phase 2: LSP Server Foundation

- Done: implement an LSP 3.17 server.
- Done: support initialize, text sync, diagnostics, hover, completion,
  definition, formatting, semantic tokens, and code actions.
- Done: start with Lux symbols before depending on the GMod API database.
- Done: analyze unsaved buffers through workspace root plus in-memory overlays.
- Done: import/export completion, cross-part definition, export alias hover, and
  unknown external quick fixes use the compiler analysis API.

## Phase 3: GMod API Database

- Define the database schema.
- Implement official documentation scraping and parsing tools.
- Add curated overrides.
- Bundle an offline database.
- Provide `lux gmod api update` and the matching VS Code command.

## Phase 4: Document Hover And GLua Baseline

- Provide documentation-level hover for common GMod APIs.
- Include official descriptions, parameters, returns, warnings, notes, example
  code, and links.
- Support hook name hover, callback signatures, panel hover, and class method
  hover.
- Complete signature help and completion.

## Phase 5: Realm Availability Engine

- Replace old realm guard with `gmod-api-db`.
- Share one query interface between compiler and LSP.
- Support path-level realm annotations.
- Support source extern declarations and package-level extern config.
- Support unknown external allow/warn/error.
- Provide quick fixes for realm mismatch, unknown external, and export realm
  widening.

## Phase 6: VS Code Extension

- TextMate grammar.
- Semantic token scopes.
- snippets.
- settings.
- commands.
- quick fix, source action, and workspace edit UX.
- server distribution.
- VSIX package.

## Phase 7: Release

- Build LSP server binaries with GitHub Actions.
- Attach prebuilt servers to GitHub Releases.
- Publish VSIX.
- Add LSP and VS Code installation pages to the docs site.
- Link this repository from the main Lux README.

## Out Of Scope For The First Stage

- copying GLua Enhanced GPL data or implementation
- duplicating the Lux resolver in the VS Code TypeScript extension
- treating unknown externals as shared-safe
- claiming VS Code support is complete after syntax highlighting only
