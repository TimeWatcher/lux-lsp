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

## Official Data Pipeline

The primary source must be the official Facepunch Garry's Mod Wiki JSON, not a
hand-maintained curated API table.

Lux uses `https://wiki.facepunch.com/gmod/~pagelist?format=json` as the
coverage baseline, then fetches every official page as `?format=json` and parses
the Facepunch markup in that payload.

Generation pipeline:

```text
official pagelist JSON
  -> per-page official JSON
  -> Facepunch markup parser
  -> gmod_api.json
  -> coverage_manifest.json
```

Handwritten data may exist only as test fixtures or override patches. It must
not be the main database source. Overrides must be traceable and must not
replace the official scraping pipeline.

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

Current updater command:

```powershell
luxc gmod api update `
  --out crates\gmod-api-db\data\generated\gmod_api.json `
  --coverage-out crates\gmod-api-db\data\generated\coverage_manifest.json `
  --cache-dir target\gmod-api-cache
```

For updater development inside the LSP workspace, the same implementation is
also available as `cargo run -p gmod-api-update -- ...`.

Default rules:

- The official pagelist is the source of truth.
- The update command fails when an API candidate page cannot be fetched or
  parsed.
- `--allow-failures` is only for parser development.
- The generated database and coverage manifest must be committed together.
- Curated corrections are applied with `--override <json>` files after official
  data generation. They must be reviewed and traceable.
- The coverage manifest must report official page count, API candidate count,
  structured conversions, fallback documentation pages, skipped pages, and
  failed pages.

The current bundled manifest covers 6,335 official pages and 6,122 API candidate
pages. It has 6,121 structured conversions, 1 fallback documentation page,
10,023 entries, 497 hooks, 186 classes, and zero failed conversions.

Override files are lightweight JSON patches. They do not need to repeat the full
generated database metadata:

```json
{
  "version": "2026-06-local-corrections",
  "entries": [
    {
      "path": "net.Start",
      "kind": "function",
      "realm": "shared",
      "summary": "Corrected summary from reviewed project knowledge."
    }
  ]
}
```

Entries are merged by `path`, hooks by `name`, and classes by `name`.

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

Class and panel metadata is also generated from official markup, including
`<type parent="...">` and `<panel><parent>...</parent></panel>`:

```ts
ClassSymbol {
  name: string
  kind: "class" | "panel"
  realm?: "shared" | "server" | "client" | "menu"
  parent?: string
  doc: DocPage
  methods: ApiSymbol[]
  docs_url: string
}
```

The parent chain must come from the official generated data. Lux may not ship a
hand-maintained inheritance table as the primary source.

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

## Class And Panel Parent Chains

GMod object documentation is not flat. Player, Weapon, NPC, Vehicle, Derma
panels, and many other API surfaces inherit methods from a documented parent.
The generated database must preserve that parent metadata and expose shared
queries:

```text
method_for_class_or_base("DButton", "SetSize")
  -> DButton
  -> DLabel
  -> Label
  -> Panel
  -> Panel:SetSize
```

LSP completion, hover, and signature help must use the same class query API.
For example, `local button = vgui.Create("DButton")` followed by `button:`
should include both DButton methods such as `SetImage` and inherited Panel
methods such as `SetSize` and `Dock`.

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
