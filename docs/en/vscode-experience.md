# VS Code Experience Standard

## Baseline

Lux VS Code must start from mature GLua developer expectations. Many GMod
developers rely on GLua Enhanced for completion, hover, hook signatures, and
official documentation links. If Lux lacks these features, the language may be
better while the development experience becomes worse.

The minimum target is:

```text
GLua Enhanced-level GMod API experience
+ Lux module/export/realm semantics
+ Lux syntax, formatting, and diagnostics
```

## Syntax Highlighting

The VS Code extension needs two highlighting layers:

- TextMate grammar for fallback highlighting before the LSP is ready.
- Semantic tokens from the Lux parser/resolver.

Semantic tokens need to distinguish:

- keyword
- realm marker: `server`, `client`, `shared`
- domain block
- function declaration
- top-level module binding
- part-local import binding
- exported public name
- private module binding
- extern symbol
- GMod API symbol
- unknown external
- type/class/panel/hook/constant/enum

Scopes should be stable and theme-friendly. The extension should not force a
brand color over the user's theme.

## Completion

Completion must be semantic, not text-only.

### Import Completion

```lux
import { | } from "inventory"
```

Only exports visible to the current realm should be shown.

If the module exports:

```lux
local player_inventory = ...
export { p_inv = player_inventory }
```

completion shows `p_inv`, not the internal name `player_inventory`.

### Export Completion

```lux
export { | }
```

Show exportable module-scope bindings. Do not show:

- part-local import bindings
- already duplicated public names
- bindings that would violate realm narrowing rules

### GMod API Completion

```lux
net.
```

Filter or de-prioritize members by active realm:

- server context: server + shared
- client context: client + shared
- shared context: shared; server/client-only items are hidden or shown as
  unavailable depending on settings

```lux
hook.Add("|
```

Complete hook names with realm, callback signature, and documentation summary.

```lux
vgui.Create("|
```

Complete panel classes with panel docs.

```lux
ply:
```

If `ply` is inferred as `Player`, complete `Player` methods.

## Signature Help

Signature help must come from the same `gmod-api-db`:

- parameter names, types, and defaults
- parameter documentation
- overloads
- hook callback signatures
- method receiver types

Lux functions also need signatures, including hoisted top-level functions across
module parts.

## Definition And References

Lux symbols:

- go to declaration
- import to export
- export public name to internal binding
- navigation across module parts
- references across internal module use and cross-module imports

GMod API:

- definition opens official docs by default
- hover and completion keep official docs links
- optional `Open in Lux API Browser` command

Unknown external:

- do not invent a definition
- hover explains that it is unverified and suggests extern declarations

## Diagnostics

Diagnostics come from the compiler analysis API and `gmod-api-db`:

- syntax errors
- parse recovery errors
- unresolved import/export
- duplicate module-scope binding
- use before initialization
- realm mismatch
- unknown external realm risk
- export realm widening
- imported export not available in current realm
- stale API database warning

Diagnostics should include actionable fixes:

```text
`net.Broadcast` is server-only, but this code runs in shared context.

Wrap the call in:
  server { ... }

or move it into a server-only function:
  server fn sendUpdate(...) { ... }
```

## Code Actions And Quick Fixes

Code actions are first-class editor features. Every recoverable diagnostic
should provide a quick fix when possible. Each quick fix must come from the
compiler analysis API or `gmod-api-db`; the VS Code extension must not guess with
fragile string rewrites.

Quick fixes have three levels:

- safe fix: does not change semantics and can be applied directly.
- guided fix: may change visibility, realm, or public API and requires an
  explicit user choice.
- refactor action: cross-file or cross-module edits with a dedicated action flow.

### Required Quick Fixes

Realm mismatch:

```lux
shared {
  net.Broadcast()
}
```

Provide:

- Wrap in `server { ... }`
- Move call to `server fn`
- Hide unavailable realm completion items
- Open official docs for `net.Broadcast`

Unknown external:

```lux
ThirdPartyAddon.DoThing()
```

Provide:

- Add `extern shared ThirdPartyAddon.DoThing`
- Add `extern server ThirdPartyAddon.DoThing`
- Add `extern client ThirdPartyAddon.DoThing`
- Add package-level extern entry
- Change `unknown_external` to `allow/warn/error`

Unresolved import:

```lux
import { player_inventory } from "inventory"
```

If the target module exports only:

```lux
export { p_inv = player_inventory }
```

Provide:

- Replace import with `p_inv`
- Import as local alias: `import { p_inv as player_inventory } from "inventory"`
- Open target module exports

Missing export:

```lux
import { grant } from "permissions"
```

If the target module has a private binding named `grant`:

- Export `grant`
- Export `grant` as alias
- Export `grant` for server/client/shared, narrowed by the binding realm

Export realm widening:

```lux
server fn grant() { ... }
export shared { grant }
```

Provide:

- Change export realm to `server`
- Move declaration to shared realm, if dependencies allow it
- Show blocking server-only dependencies

Duplicate module binding:

- Rename current binding
- Rename all references in module
- Convert one binding to a part-local import alias, when applicable

Use before initialization:

- Move declaration earlier in part order
- Add or update module part order entry
- Convert top-level non-function initializer into `fn` or a lazy initializer

Formatting diagnostics:

- Format document
- Format selection
- Normalize import/export list order

### Source Actions

Available on save or from the command palette:

```text
Lux: Fix All Safe Issues
Lux: Organize Imports
Lux: Sort Exports
Lux: Add Missing Externs
Lux: Update Part Order
Lux: Convert Lua Callback To Lux Fn
Lux: Wrap Selection In server/client/shared Block
```

`Fix All Safe Issues` may only apply safe fixes. It must not automatically widen
exports, change realm, or add uncertain extern declarations.

### UX Requirements

- Quick fix titles must be specific, for example `Add extern server net.Broadcast`, not `Fix realm issue`.
- Action preview must show which files will be edited.
- Cross-file actions must use workspace edits.
- GMod API quick fixes should include `Open official docs`.
- Lux import/export actions must use resolver results, not text-only path guesses.
- Realm fixes must explain the current context and the target API realm.

## Formatting

Formatting is provided by the Lux compiler formatting API. The VS Code extension
only calls LSP formatting.

Required support:

- format document
- format selection
- format on save
- range formatting
- stable formatting for match, arrow functions, implicit expression returns,
  domain blocks, export/import, and part order declarations

## Commands

VS Code commands:

```text
Lux: Restart Language Server
Lux: Show Project Diagnostics
Lux: Open Lux Documentation
Lux: Open Garry's Mod API Documentation
Lux: Update Garry's Mod API Database
Lux: Show Active Realm
Lux: Show Module Exports
Lux: Fix All Safe Issues
Lux: Organize Imports
Lux: Update Part Order
Lux: Compile Current Project
Lux: Format Current Document
```

## Settings

Suggested settings:

```json
{
  "lux.compiler.path": null,
  "lux.gmod.apiDatabasePath": null,
  "lux.gmod.apiAutoUpdate": false,
  "lux.gmod.unknownExternal": "warn",
  "lux.completion.hideUnavailableRealmItems": false,
  "lux.hover.showOfficialExamples": true,
  "lux.hover.showLuxExamples": true,
  "lux.diagnostics.enableCompilerDiagnostics": true,
  "lux.codeActions.enableQuickFixes": true,
  "lux.codeActions.fixAllOnSave": false
}
```

## Acceptance Criteria

The first implementation is not complete unless:

- a real GMod Lux project gets highlighting and diagnostics without setup
- common GMod APIs have completion, documentation-level hover, signature help,
  and official links
- hook names and callback parameters are completable and hoverable
- import/export completion respects realm and aliases
- multi-part module definition navigation works
- common diagnostics provide concrete quick fixes or source actions
- the old realm guard is gone as a separate whitelist and realm checks are
  powered by `gmod-api-db`
