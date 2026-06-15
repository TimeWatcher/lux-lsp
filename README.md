# Lux LSP

Lux LSP is the language tooling repository for Lux. It hosts the reusable
language server, the VS Code extension shell design, and the shared Garry's Mod
API intelligence standards used by the compiler and editor.

The goal is not a minimal syntax plugin. Lux developers should get the editor
experience they already expect from mature GLua tooling, especially GLua
Enhanced, plus Lux-specific module, realm, export, and syntax intelligence.

中文文档见 [README.zh-CN.md](README.zh-CN.md).

## Scope

- `lux-lsp`: a standalone Language Server Protocol implementation, now backed
  by `luxc::analysis`.
- `vscode-lux`: the VS Code extension shell for activation, grammar, semantic
  token scopes, settings, snippets, commands, server startup, and VSIX
  packaging.
- `gmod-api-db`: versioned Garry's Mod documentation and API data shared by
  hover, completion, signature help, diagnostics, and compiler realm checking.
- shared analysis APIs extracted from the Lux compiler, rather than parsing CLI
  stderr.

## Current Implementation

The Phase 1, Phase 2, and core Phase 3 foundation is in place:

- `luxc::analysis` is the stable analysis entry point shared by compiler, CLI,
  LSP, and tests.
- The LSP analyzes unsaved buffers through in-memory overlays and does not parse
  `luxc` command-line output.
- The server supports LSP 3.17 initialize, full text sync, diagnostics, hover,
  completion, definition, formatting, semantic tokens, code actions, and
  workspace commands.
- Completion is connected to Lux module/export semantics: module paths, export
  lists, import specifiers, and regular bindings are selected by context.
- Hover and definition support module-private bindings, export aliases, import
  bindings, and unknown external symbols.
- Diagnostics and quick fixes come from compiler analysis, including guided
  `extern` suggestions for unknown external symbols.
- `gmod-api-db` now has a generated offline database built from the official
  Facepunch Wiki JSON page list and per-page markup. The official pagelist is
  the coverage baseline; the primary database is not hand-maintained.
- Release quality requires complete official coverage: every Facepunch pagelist
  item must exist in `documents[]`, every API candidate page must become
  structured API data, and the bundled coverage manifest must report zero
  failed or fallback pages.
- The generated database currently contains document records for all 6,335
  official pages and a semantic API index for 6,121 API candidate pages. The
  latest bundled manifest has 6,121 structured conversions, zero fallback
  documentation pages, 10,022 entries, 497 hooks, 186 classes, and zero failed
  page conversions.
- Official class and Derma panel parent metadata is parsed into the database, so
  inherited method completion and docs follow the official Facepunch markup
  instead of a hand-maintained type table.
- Compiler realm checks and LSP hover, completion, signature help, workspace
  commands, and GMod docs code actions use the same `gmod-api-db` query
  interface.
- `vscode-lux` now ships a complete extension shell: TextMate grammar,
  semantic token scopes, snippets, settings, server resolution, editor commands,
  quick-fix command handling, and VSIX packaging.
- GitHub Actions build the Rust server, package the extension, and attach VSIX
  plus prebuilt server archives to tagged GitHub Releases.

## Local Development

```powershell
cargo test
cargo run -p lux-lsp
```

Set `LUX_LSP_DEBUG=1` before launching the server when you need raw document
change and diagnostics lifecycle logs.

Build and package the VS Code extension:

```powershell
cd vscode-lux
npm install
npm run compile
npm run package
```

The release workflow builds server binaries for:

- `windows-x64`
- `linux-x64`
- `macos-arm64`

It then copies those binaries into `vscode-lux/server/<platform>/`, packages the
VSIX, and uploads both standalone server archives and the VSIX to the GitHub
Release.

Update the bundled official GMod API database:

```powershell
luxc gmod api update `
  --out crates\gmod-api-db\data\generated\gmod_api.json `
  --coverage-out crates\gmod-api-db\data\generated\coverage_manifest.json `
  --cache-dir target\gmod-api-cache
```

The standalone development entry point is still available as
`cargo run -p gmod-api-update -- ...`. Both paths use the same Rust updater
library. The updater uses `https://wiki.facepunch.com/gmod/~pagelist?format=json`
as the coverage baseline, downloads every official page JSON payload, converts
Facepunch markup, writes one document record for every official page, builds the
semantic API index from structured API markup, applies optional
`--override <json>` files, and fails if any official page cannot be fetched or
represented in `documents[]`, or any API candidate page cannot be converted into
structured data unless `--allow-failures` is explicitly passed for parser
development.

Do not replace this pipeline with a manually maintained API table. Handwritten
data is allowed only as test fixtures or reviewed override patches applied
after official data generation.

In the main Lux repository, this repository is checked out as the `lsp`
submodule. `lux-lsp` depends on the sibling `../compiler` crate, so the
recommended setup is to clone Lux with submodules initialized.

## Standards

- [Architecture](docs/en/architecture.md)
- [Garry's Mod API Database](docs/en/gmod-api-db.md)
- [Document Hover](docs/en/document-hover.md)
- [VS Code Experience](docs/en/vscode-experience.md)
- [Roadmap](docs/en/roadmap.md)

## Non-negotiable baseline

Lux editor support must meet the GLua developer expectation:

- complete GMod API completion
- documentation-level hover
- hook name and callback signature assistance
- signature help with parameter documentation
- class and method completion for `Player:`, `Entity:`, `Panel:`, and related
  types
- inherited Derma panel methods from official parent metadata, such as
  `DButton` resolving `Panel:SetSize`
- official documentation links
- real-time compiler and lint diagnostics

Lux then adds language-aware features that GLua tooling cannot provide:

- realm-aware completion and diagnostics
- smart import/export completion based on module public APIs
- navigation across multi-part modules
- hover for exports, aliases, internal bindings, and realm availability
- formatting and semantic tokens for Lux syntax

## VS Code

The extension starts `lux-lsp` in this order:

1. `lux.lsp.serverPath`
2. bundled release binary under `server/<platform>/`
3. `lux-lsp` on `PATH`
4. `cargo run -p lux-lsp`, only when the development fallback setting is enabled

Editor commands include restart server, open Lux docs, open official GMod docs,
update the GMod API database, compile the current project, format the current
document, show module exports, show the active realm, and show generated API
coverage.

## License

The repository is dual-licensed under MIT or Apache-2.0 at your option.
Generated documentation data must preserve source attribution and license
metadata.
