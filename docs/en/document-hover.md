# Document Hover Standard

## Core Principle

Lux hover must feel like documentation, not a structural tooltip.

For known GMod APIs, hover should render an editor-friendly documentation page
derived from the official docs. Signatures, realms, and types are part of the
document, but they cannot replace full descriptions, parameters, returns,
warnings, notes, and official examples.

Product standard:

```text
If a common GMod API has documentation-level hover in GLua Enhanced,
Lux VS Code must provide equivalent or better behavior.
```

Lux-specific enhancements are added on top:

- current Lux realm context
- whether the API is available in that context
- import/export origin
- Lux-style equivalent examples
- unknown external risk notes

## Rendering Format

The LSP returns hover using `MarkupContent(markdown)`. The VS Code extension
should not implement a separate semantic hover renderer.

Standard hover layout:

```text
Title
Signature block
Realm / availability
Full description
Parameters
Returns
Warnings
Notes
Official examples
Lux equivalent or Lux note
Related APIs
Official docs link
Source link if available
Database version/update time
```

## API Hover Example

Bad `net.Start` hover:

```text
net.Start(messageName: string) -> boolean
Realm: shared
```

Acceptable hover should look closer to:

````markdown
### net.Start

```lua
net.Start(messageName: string, unreliable?: boolean) -> boolean
```

**Realm:** shared

Begins a new net message.

The net library is used to send data between server and client. This entry is
available in shared code, but follow-up send calls may be server-only or
client-only.

#### Parameters

| Name | Type | Description |
|---|---|---|
| messageName | string | The message name registered with `util.AddNetworkString`. |
| unreliable | boolean | Whether the message may be sent unreliably. |

#### Returns

| Type | Description |
|---|---|
| boolean | Whether the message was started successfully. |

#### Warnings

Existing unsent messages may be discarded. Net messages have size limits.

#### Official Lua example

```lua
util.AddNetworkString("my_message")

net.Start("my_message")
net.WriteString("hello")
net.Broadcast()
```

#### Lux note

`net.Start` is shared, but `net.Broadcast` is server-only and
`net.SendToServer` is client-only. Lux checks those follow-up calls separately.

[Official docs](https://wiki.facepunch.com/gmod/net.Start)
````

## Hook Hover

Hover must understand semantic positions. For:

```lux
hook.Add("PlayerInitialSpawn", "welcome", fn(ply, transition) {
  print(ply:Nick())
})
```

Hovering `"PlayerInitialSpawn"` should show hook docs, not the string type:

````markdown
### GM:PlayerInitialSpawn

```lua
GM:PlayerInitialSpawn(player: Player, transition: boolean)
```

**Realm:** server

Called when a player spawns for the first time.

#### Callback used by `hook.Add`

```lux
fn(player: Player, transition: boolean) -> nil
```

#### Parameters

| Name | Type | Description |
|---|---|---|
| player | Player | Player who spawned. |
| transition | boolean | Whether this spawn comes from a transition. |

#### Official Lua example

```lua
hook.Add("PlayerInitialSpawn", "Welcome", function(ply)
    print(ply:Nick())
end)
```

#### Lux equivalent

```lux
hook.Add("PlayerInitialSpawn", "Welcome", fn(ply) {
  print(ply:Nick())
})
```

[Official docs](https://wiki.facepunch.com/gmod/GM:PlayerInitialSpawn)
````

## Lux Symbol Hover

Lux symbols should also have rich hover:

- binding kind: function, local, import, export, extern, type
- declaring module and part
- internal binding name
- public export name, if any
- available realms
- inferred type or signature
- definition link
- import origin

Example:

```lux
export { p_inv = player_inventory }
```

Hover on `p_inv`:

```text
public export `p_inv`
from binding `player_inventory`
module: inventory
realm: shared

Import with:
  import { p_inv } from "inventory"
```

Hover on `player_inventory`:

```text
module-private binding `player_inventory`
exported as `p_inv`
visible to all parts in module `inventory`
```

## Realm Diagnostics In Hover

Hover may show the current realm check result:

```lux
shared {
  net.Broadcast()
}
```

Hover on `net.Broadcast`:

```text
net.Broadcast
Realm: server
Current context: shared

This API is not available in shared context.

Fix:
  server {
    net.Broadcast()
  }
```

## Full Documentation Panel

Hover space is limited, but content should not be removed. Implementation can
use two layers:

1. Hover shows a complete readable summary page with official description,
   parameters, returns, warnings, and at least one official example.
2. The bottom of the hover provides `Open Full Documentation`, opening a VS Code
   Webview with the full official page, all examples, Lux examples, related APIs,
   and database metadata.

The Webview is an enhancement, not an excuse to simplify hover.
