# Lux for VS Code

This extension provides Lux editor support backed by `lux-lsp`.

## Features

- Lux TextMate grammar and semantic tokens.
- LSP diagnostics, completion, hover, definition, formatting, and code actions.
- Realm-aware import/export assistance from the Lux compiler analysis API.
- GMod API hover, signature help, hook assistance, and official documentation actions.
- Commands for restarting the server, showing module exports, showing the active realm, compiling the current project, and updating the bundled GMod API database.

## Server Resolution

The extension starts `lux-lsp` in this order:

1. `lux.lsp.serverPath`
2. bundled VSIX server binary under `server/<platform>/`
3. `lux-lsp` on `PATH`
4. `cargo run -p lux-lsp`, only when `lux.lsp.developmentCargoFallback` is enabled

The TypeScript extension does not reimplement the Lux resolver. It delegates Lux semantics to `lux-lsp` and the compiler analysis API.

## Settings

- `lux.lsp.serverPath`: explicit language server path.
- `lux.compiler.path`: explicit `luxc` path for extension commands.
- `lux.lsp.trace.server`: LSP protocol tracing.
- `lux.lsp.developmentCargoFallback`: development-only cargo fallback.
- `lux.docs.url`: Lux documentation URL.
- `lux.gmod.docsUrl`: Facepunch GMod documentation base URL.

## Packaging

Release builds bundle precompiled `lux-lsp` binaries. Local development can run:

```powershell
npm install
npm run compile
npm run package
```
