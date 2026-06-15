# Lux LSP

Lux LSP is the language tooling repository for Lux. It will host the reusable
language server, the VS Code extension, and the shared Garry's Mod API
intelligence database used by the compiler and editor.

The goal is not a minimal syntax plugin. Lux developers should get the editor
experience they already expect from mature GLua tooling, especially GLua
Enhanced, plus Lux-specific module, realm, export, and syntax intelligence.

中文文档见 [README.zh-CN.md](README.zh-CN.md).

## Scope

- `lux-lsp`: a standalone Language Server Protocol implementation.
- `vscode-lux`: a thin VS Code extension shell for activation, grammar,
  settings, snippets, commands, and server startup.
- `gmod-api-db`: versioned Garry's Mod API data shared by hover, completion,
  signature help, diagnostics, and compiler realm checking.
- shared analysis APIs extracted from the Lux compiler, rather than parsing CLI
  stderr.

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
