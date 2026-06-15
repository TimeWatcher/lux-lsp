# Lux LSP Architecture Standard

## Goal

Lux LSP is not a small editor plugin separate from the compiler. It should expose
the Lux compiler's semantic model to editors and provide the Garry's Mod API
experience that GLua developers already expect from mature tooling such as GLua
Enhanced.

The architecture has three layers:

```text
Lux compiler analysis API
  -> lux-lsp
  -> vscode-lux

gmod-api-db
  -> compiler realm checker
  -> lux-lsp hover/completion/signature/diagnostics
  -> docs links and API browser
```

## Repository Layout

This repository will host:

- `crates/lux-lsp`: standalone LSP 3.17 server.
- `extensions/vscode-lux`: thin VS Code shell for activation, TextMate grammar,
  settings, snippets, commands, and server startup.
- `crates/gmod-api-db`: Garry's Mod API data model, loader, query interface, and
  version metadata.
- `tools/gmod-api-update`: official documentation updater.
- `docs`: language service standards and user documentation.

The first version may contain only standards and documentation, but the
implementation must keep the LSP reusable outside VS Code.

## Compiler Analysis API

The LSP must not call `luxc` and parse stderr. The compiler needs a stable
analysis API for:

```text
parse
expand
resolve
build module graph
compute part order
compute active realm stack
resolve imports/exports
collect diagnostics
format source
emit semantic tokens
```

The CLI, LSP, and tests should share this API. The CLI is one frontend, not the
semantic source of truth.

## Project Model

The LSP must understand Lux project semantics:

- Packages are minimal and modules are discovered automatically.
- A module is a directory, not a single file.
- All part files inside a module share one logical module scope.
- Top-level declarations are module-private by default and visible to all parts
  of the same module.
- Top-level imports are part-local bindings.
- Top-level `fn` declarations are hoisted across the whole module.
- Top-level non-function locals are not hoisted as initialized values. Their
  initialization follows deterministic part order.
- Use before initialization is an error.
- Exports map module-scope bindings to public API names and do not affect
  internal visibility.
- MVP 0.1 treats duplicate module-scope binding names as errors, even when they
  are declared in different realms.

## Realm Model

The compiler and LSP must share the same realm model:

```text
Lux symbol       -> strict
known GMod API   -> strict
unknown external -> allow / warn / error
```

Unknown external symbols must not be classified as shared:

```text
RealmAvailability
  = Known(RealmSet)
  | UnknownExternal
```

Availability must keep its source:

```text
AvailabilitySource
  = LuxBinding
  | GmodApiDb
  | ExternDeclaration
  | UnknownExternal
```

This lets diagnostics explain whether a symbol came from Lux, the GMod API
database, an extern declaration, or an unverified external symbol.

## Replacing Old Realm Guard

The old realm guard was a small handwritten table. It must be replaced by the
shared Realm Availability Engine.

New rules:

- `gmod-api-db` is the single source of truth for GMod API realm availability.
- Compiler diagnostics, LSP diagnostics, hover, completion, and signature help
  all query the same database.
- The database supports path-level realm annotations, so `net`, `net.Start`, and
  `net.Broadcast` can have different availability.
- Symbol resolution uses longest-path matching.
- Unknown external symbols stay outside the shared/client/server sets. They warn
  by default and can be configured to allow or error.

## Incremental Analysis

The LSP should support incremental file updates, while semantic results are
computed at module granularity:

- When one part changes, rebuild the owning module's parse tree, binding graph,
  export table, and diagnostics.
- When imports or exports change, rebuild dependent import resolution.
- When `lux.toml` or part order changes, rebuild the project graph.
- When `gmod-api-db` changes, rebuild external symbol realm and hover caches.

## VS Code Boundary

The VS Code extension is an editor integration shell:

- activate the language service
- provide TextMate grammar fallback
- register semantic tokens, formatting, diagnostics, completion, hover,
  signature help, definition, and references
- provide commands for updating the GMod API database and opening docs

Semantic logic should not be duplicated in the TypeScript extension.
