# Document Hover 标准

## 核心原则

Lux 的 hover 必须是文档效果，而不是结构信息提示。

对已知 GMod API，hover 应该把官方文档加工成编辑器内可读文档页。签名、realm、类型只是文档的一部分，不能替代完整说明、参数、返回值、warning、note 和官方示例。

产品标准：

```text
如果一个常用 GMod API 在 GLua Enhanced 中能显示文档级 hover，
Lux VS Code 也必须提供等价或更好的效果。
```

Lux 的增强是叠加在这个基线之上：

- 当前 Lux realm 上下文。
- API 是否可在当前上下文使用。
- import/export 来源。
- Lux 风格写法。
- unknown external 风险说明。

## 渲染格式

LSP 使用 `MarkupContent(markdown)` 返回 hover。VS Code 扩展不应私自实现第二套 hover 渲染逻辑。

标准 hover 结构：

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

## API Hover 示例

`net.Start` 的 hover 不合格示例：

```text
net.Start(messageName: string) -> boolean
Realm: shared
```

合格示例应该接近：

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

Hover 必须支持语义位置。对于：

```lux
hook.Add("PlayerInitialSpawn", "welcome", fn(ply, transition) {
  print(ply:Nick())
})
```

在 `"PlayerInitialSpawn"` 上 hover 时，显示 hook 文档，而不是字符串类型：

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

Lux 自身符号 hover 也要有文档效果：

- binding kind：function、local、import、export、extern、type。
- declaring module 和 part。
- internal binding name。
- public export name，如果有。
- available realms。
- inferred type 或 signature。
- definition link。
- import 来源。

示例：

```lux
export { p_inv = player_inventory }
```

对 `p_inv` hover：

```text
public export `p_inv`
from binding `player_inventory`
module: inventory
realm: shared

Import with:
  import { p_inv } from "inventory"
```

对 `player_inventory` hover：

```text
module-private binding `player_inventory`
exported as `p_inv`
visible to all parts in module `inventory`
```

## Realm Diagnostics In Hover

hover 可以显示当前上下文的 realm 检查结果：

```lux
shared {
  net.Broadcast()
}
```

在 `net.Broadcast` 上 hover：

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

Hover 空间有限，但不应删掉文档内容。实现阶段可以采用两层：

1. Hover 显示完整可读摘要页，包含官方说明、参数、返回、warning、至少一个官方示例。
2. Hover 底部提供 `Open Full Documentation` 命令，打开 VS Code Webview，显示完整官方页面、所有示例、Lux 示例、相关 API 和数据库元数据。

Webview 是增强，不是 hover 简化的借口。
