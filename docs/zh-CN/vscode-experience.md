# VS Code 体验标准

## 体验基线

Lux VS Code 插件必须以成熟 GLua 开发者的预期为基线。很多 GMod 开发者长期依赖 GLua Enhanced 提供的补全、hover、hook 签名和官方文档链接。如果 Lux 插件缺少这些能力，即使语言本身更好，真实开发体验也会倒退。

因此 Lux 插件的最低目标是：

```text
GLua Enhanced 级 GMod API 体验
+ Lux module/export/realm 语义
+ Lux 语法、格式化和 diagnostics
```

## 语法高亮

VS Code 扩展需要两层高亮：

- TextMate grammar：提供未启动 LSP 时的基础高亮。
- Semantic tokens：由 Lux parser/resolver 提供准确语义高亮。

Semantic tokens 需要区分：

- keyword
- realm marker：`server`、`client`、`shared`
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

语法高亮颜色不能被站点或主题的品牌色污染。扩展应提供稳定 token scopes，但尊重用户主题。

## Completion

Completion 必须按语义上下文工作，而不是纯文本补全。

### Import Completion

```lux
import { | } from "inventory"
```

应只显示目标 module 对当前 realm 可见的 exports。

如果 module export 为：

```lux
local player_inventory = ...
export { p_inv = player_inventory }
```

则 import 补全只显示 `p_inv`，不显示内部名 `player_inventory`。

### Export Completion

```lux
export { | }
```

显示当前 module scope 中可导出的 binding，不显示：

- part-local import binding
- 已重复导出的 public name
- 不满足 realm 收窄规则的 binding

### GMod API Completion

```lux
net.
```

根据 active realm 过滤或降权显示成员：

- server context：server + shared
- client context：client + shared
- shared context：shared，server/client-only 项显示为不可用或隐藏，取决于设置

```lux
hook.Add("|
```

补全 hook 名，并显示 realm、callback 签名和文档摘要。

```lux
vgui.Create("|
```

补全 panel class，并显示 panel 文档。

```lux
ply:
```

如果 `ply` 被推断为 `Player`，只显示 `Player` 方法。

## Signature Help

Signature help 必须来自同一份 `gmod-api-db`：

- 函数参数名、类型、默认值。
- 参数详细说明。
- 多重签名。
- hook callback 签名。
- method receiver 类型。

Lux 函数也必须显示 signature，包括跨 part module 的 top-level hoisted function。

## Definition And References

Lux 符号：

- 跳转到声明。
- import 跳转到 export。
- export public name 跳转到内部 binding。
- 跨 part module 跳转。
- references 能找到同 module 内部引用和跨 module import 使用。

GMod API：

- definition 默认打开官方文档。
- hover 和 completion 中保留官方文档链接。
- 可以提供 `Open in Lux API Browser` 命令。

Unknown external：

- 不伪造 definition。
- hover 显示无法验证，并建议 extern。

## Diagnostics

Diagnostics 来自编译器分析 API 和 `gmod-api-db`：

- syntax errors
- parse recovery errors
- unresolved import/export
- duplicate module-scope binding
- use-before-initialization
- realm mismatch
- unknown external realm risk
- export realm widening
- imported export not available in current realm
- stale API database warning

错误信息要包含可操作建议。例如：

```text
`net.Broadcast` is server-only, but this code runs in shared context.

Wrap the call in:
  server { ... }

or move it into a server-only function:
  server fn sendUpdate(...) { ... }
```

## Code Actions And Quick Fixes

Code actions 是 Lux VS Code 支持的一等功能。每个可恢复 diagnostic 都应尽量提供 quick fix；每个 quick fix 必须来自编译器分析 API 或 `gmod-api-db`，不能由 VS Code 扩展用脆弱字符串规则猜测。

Quick fix 分为三档：

- safe fix：不改变语义，可以直接应用。
- guided fix：可能改变可见性、realm 或 API，需要用户明确选择。
- refactor action：跨文件或跨 module 修改，进入单独 action 流程。

### 必须支持的 Quick Fix

Realm mismatch：

```lux
shared {
  net.Broadcast()
}
```

提供：

- Wrap in `server { ... }`
- Move call to `server fn`
- Hide unavailable realm completion items
- Open official docs for `net.Broadcast`

Unknown external：

```lux
ThirdPartyAddon.DoThing()
```

提供：

- Add `extern shared ThirdPartyAddon.DoThing`
- Add `extern server ThirdPartyAddon.DoThing`
- Add `extern client ThirdPartyAddon.DoThing`
- Add package-level extern entry
- Change `unknown_external` to `allow/warn/error`

Unresolved import：

```lux
import { player_inventory } from "inventory"
```

如果目标 module 只有：

```lux
export { p_inv = player_inventory }
```

提供：

- Replace import with `p_inv`
- Import as local alias: `import { p_inv as player_inventory } from "inventory"`
- Open target module exports

Missing export：

```lux
import { grant } from "permissions"
```

如果目标 module 存在 private binding `grant`：

- Export `grant`
- Export `grant` as alias
- Export `grant` for server/client/shared，按 binding realm 收窄

Export realm widening：

```lux
server fn grant() { ... }
export shared { grant }
```

提供：

- Change export realm to `server`
- Move declaration to shared realm，如果依赖允许
- Show blocking server-only dependencies

Duplicate module binding：

- Rename current binding
- Rename all references in module
- Convert one binding to part-local import alias，如果适用

Use before initialization：

- Move declaration earlier in part order
- Add or update module part order entry
- Convert top-level non-function initializer into `fn` 或 lazy initializer

Formatting diagnostics：

- Format document
- Format selection
- Normalize import/export list order

### Source Actions

保存或命令面板可用：

```text
Lux: Fix All Safe Issues
Lux: Organize Imports
Lux: Sort Exports
Lux: Add Missing Externs
Lux: Update Part Order
Lux: Convert Lua Callback To Lux Fn
Lux: Wrap Selection In server/client/shared Block
```

`Fix All Safe Issues` 只能应用 safe fix，不得自动扩大 export、改变 realm 或新增不确定 extern。

### UX 要求

- Quick fix 标题必须具体，例如 `Add extern server net.Broadcast`，不要写成 `Fix realm issue`。
- action 预览必须显示会修改哪些文件。
- 跨文件 action 需要使用 workspace edit。
- 对官方 GMod API 的 quick fix 需要附带 `Open official docs`。
- 对 Lux import/export 的 action 需要使用 resolver 结果，不能只按文本路径猜测。
- 对 realm 修复，必须解释当前 context 和目标 API realm。

## Formatting

Formatter 由 Lux compiler formatting API 提供。VS Code 扩展只调用 LSP formatting。

必须支持：

- format document
- format selection
- format on save
- range formatting
- stable formatting for match、箭头函数、隐式表达式返回、domain block、export/import、part order declaration

## Commands

VS Code 命令：

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

建议设置：

```json
{
  "lux.lsp.serverPath": null,
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

## 验收标准

第一阶段实现不算完成，除非满足：

- 打开真实 GMod Lux 项目时，无需配置即可获得语法高亮和 diagnostics。
- 常用 GMod API 有补全、文档级 hover、signature help 和官方链接。
- hook 名和 callback 参数可补全、可 hover。
- import/export completion 能正确遵守 realm 和 alias。
- multi-part module 的定义跳转可用。
- 常见 diagnostics 提供具体 quick fix 或 source action。
- 旧 realm guard 不再是独立白名单，realm 检查来自 `gmod-api-db`。
