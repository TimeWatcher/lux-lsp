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
- `vscode-lux`: a thin VS Code extension shell for activation, grammar,
  settings, snippets, commands, and server startup.
- `gmod-api-db`: versioned Garry's Mod API data shared by hover, completion,
  signature help, diagnostics, and compiler realm checking.
- shared analysis APIs extracted from the Lux compiler, rather than parsing CLI
  stderr.

## Current Implementation

The Phase 1, Phase 2, and core Phase 3 foundation is in place:

- `luxc::analysis` is the stable analysis entry point shared by compiler, CLI,
  LSP, and tests.
- The LSP analyzes unsaved buffers through in-memory overlays and does not parse
  `luxc` command-line output.
- The server supports LSP 3.17 initialize, full text sync, diagnostics, hover,
  completion, definition, formatting, semantic tokens, and code actions.
- Completion is connected to Lux module/export semantics: module paths, export
  lists, import specifiers, and regular bindings are selected by context.
- Hover and definition support module-private bindings, export aliases, import
  bindings, and unknown external symbols.
- Diagnostics and quick fixes come from compiler analysis, including guided
  `extern` suggestions for unknown external symbols.
- `gmod-api-db` now has a generated offline database built from the official
  Facepunch Wiki JSON page list and per-page markup.
- The generated database currently covers 6,335 official pages and 6,122 API
  candidate pages. The latest bundled manifest has 6,121 structured conversions,
  1 fallback documentation page, 10,023 entries, 497 hooks, 186 classes, and
  zero failed page conversions.
- Official class and Derma panel parent metadata is parsed into the database, so
  inherited method completion and docs follow the official Facepunch markup
  instead of a hand-maintained type table.
- Compiler realm checks and LSP hover, completion, signature help, and GMod docs
  code actions use the same `gmod-api-db` query interface.

The VS Code extension is not released yet. The next stage adds the VS Code
shell, update command UX, curated override support, and release packaging.

## Local Development

```powershell
cargo test
cargo run -p lux-lsp
```

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
as the coverage baseline, downloads official page JSON payloads, converts
Facepunch markup, applies optional `--override <json>` files, and fails if any
API candidate page cannot be converted unless `--allow-failures` is explicitly
passed for parser development.

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

## License

The repository is dual-licensed under MIT or Apache-2.0 at your option.
Generated documentation data must preserve source attribution and license
metadata.
