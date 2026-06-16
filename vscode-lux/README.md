# Lux for VS Code

This extension provides Lux editor support by launching the workspace compiler
as `luxc lsp`.

## Features

- Lux TextMate grammar and semantic tokens.
- LSP diagnostics, completion, hover, definition, formatting, and code actions.
- Realm-aware import/export assistance from the Lux compiler analysis API.
- GMod API hover, signature help, hook assistance, and official documentation actions.
- Commands for restarting the server, showing module exports, showing the active realm, compiling the current project, and updating the bundled GMod API database.

## Server Resolution

The extension starts the language server with:

```text
luxc lsp
```

It resolves `luxc` in this order:

1. `lux.compiler.path`
2. workspace `.lux/bin/luxc`
3. `LUXC` environment variable
4. `luxc` on `PATH`

The TypeScript extension does not reimplement the Lux resolver and does not
bundle a language server. Lux semantics come from the selected compiler.

## Settings

- `lux.compiler.path`: explicit `luxc` path for `luxc lsp` and extension commands.
- `lux.lsp.trace.server`: LSP protocol tracing.
- `lux.docs.url`: Lux documentation URL.
- `lux.gmod.docsUrl`: Facepunch GMod documentation base URL.

## Packaging

Release builds package only the VS Code UI and forwarding shell. Local development can run:

```powershell
npm install
npm run compile
npm run package
```
