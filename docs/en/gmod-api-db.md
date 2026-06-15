# Garry's Mod API Database Standard

## Goal

`gmod-api-db` is the foundation for a high-quality GMod development experience
in Lux. It is not a simple completion list. It is a shared semantic database for
the compiler and LSP.

It must support:

- GMod API completion
- documentation-level hover
- signature help
- hook callback signatures
- classes, methods, panels, constants, and enums
- realm availability checks
- official documentation links
- official example code
- Lux-specific notes

## Sources

The primary source should be the Facepunch Garry's Mod Wiki and other official
or public sources.

GLua Enhanced is a user-experience reference only. Lux must not copy its GPL
implementation or bundled data. Lux should generate and curate its own database
with attribution metadata.

Every generated database must record:

- source URL
- scraped_at
- source revision or page update time, when available
- parser version
- override version
- database version

## Data Model

Base symbol model:

```ts
ApiSymbol {
  path: string
  kind: "function" | "method" | "library" | "hook" | "class" | "enum" | "constant" | "panel" | "field"
  realm: "shared" | "server" | "client" | "unknown"
  signatures: Signature[]
  returns: ReturnValue[]
  doc: DocPage
  related_symbols: string[]
  docs_url: string
  source_url?: string
  scraped_at: string
  doc_revision?: string
}
```

Documentation must not be compressed into a one-line summary:

```ts
DocPage {
  title: string
  summary?: string
  sections: DocSection[]
  warnings: DocBlock[]
  notes: DocBlock[]
  examples: CodeExample[]
  lux_notes: DocBlock[]
}
```

`DocSection` should preserve the official page structure:

```ts
DocSection
  = Paragraph
  | List
  | Table
  | CodeBlock
  | Warning
  | Note
  | Heading
```

## Path-Level Realm

GMod API realm cannot be modeled only at the global level. Many libraries are
shared while individual members differ:

```text
net                shared
net.Start          shared
net.Receive        shared
net.Broadcast      server
net.Send           server
net.SendToServer   client
```

Queries use longest-path matching:

```text
net.Broadcast
  -> first try net.Broadcast
  -> fallback to net
```

The same rule is used by compiler realm checks, LSP completion filtering, hover,
and signature help.

## Extern

The database cannot cover every third-party addon, binary module, or dynamic
global. Lux must let users add extern declarations:

```lux
extern server ThirdPartyAddon
extern client FancyHud
extern shared SharedLibrary

extern shared net
extern server net.Send
extern client net.SendToServer
```

Package-level config is also required:

```toml
[target.gmod.extern]
ThirdPartyAddon = "server"
FancyHud = "client"
SharedLibrary = "shared"

[target.gmod.extern."ThirdPartyAddon.DoSomething"]
realm = "server"
```

Externs use path-level annotations and longest-path matching.

## Unknown External

Unknown external symbols are not shared-safe. They have their own state:

```text
UnknownExternal
```

Default behavior:

```toml
[target.gmod.realm]
unknown_external = "warn"
```

Options:

- `allow`: no diagnostic
- `warn`: default, report risk but continue
- `error`: for CI or strict projects

Warning deduplication key:

```text
(symbol_path, active_realm, containing_decl_binding_id)
```

Do not deduplicate only by symbol, and do not report every use-site.

## Hooks

Hook data must be modeled explicitly, not as plain string completion:

```ts
HookSymbol {
  name: string
  gm_path: string
  realm: RealmSet
  callback: Signature
  description: DocPage
  docs_url: string
}
```

For:

```lux
hook.Add("PlayerInitialSpawn", "id", fn(ply, transition) {
  ...
})
```

The LSP should:

- complete hook names in the first string argument
- show `GM:PlayerInitialSpawn` docs on hover
- infer `ply: Player`
- show callback signature help

## Type Data

MVP needs lightweight type data:

- global function return type
- method receiver type
- hook callback parameter type
- constructor return type, for example `vgui.Create("DButton") -> DButton`
- colon receiver type, for example `Player:SteamID`
- constants and enum types

Full static typing is not required at first, but the data must support GMod API
completion and hover.

## Update Commands

Implementation must provide:

```text
Lux: Update Garry's Mod API Database
lux gmod api update
```

The extension should bundle an offline database. Update failure must not break
the basic editor experience.
