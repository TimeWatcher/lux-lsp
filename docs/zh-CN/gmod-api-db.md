# Garry's Mod API 数据库标准

## 目标

`gmod-api-db` 是 Lux 对 GMod 生态提供高质量开发体验的基础。它不是简单的补全列表，而是编译器和 LSP 共同使用的 API 语义数据库。

它必须支持：

- GMod API 补全。
- 文档级 hover。
- signature help。
- hook callback 签名。
- class、method、panel、constant、enum 数据。
- realm availability 检查。
- 官方文档链接。
- 官方示例代码。
- Lux 风格补充说明。

## 数据来源

主数据源应是 Facepunch Garry's Mod Wiki 和其他官方/公开来源。

GLua Enhanced 只能作为体验参考，不能复制其 GPL 实现或内置数据。Lux 需要自己生成、校正和维护数据库，并保留来源元数据。

每次生成数据库必须记录：

- source URL
- scraped_at
- source revision 或页面更新时间，如果可获得
- parser version
- override version
- database version

## 数据模型

基础符号模型：

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

文档模型不能被压缩成一句摘要：

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

`DocSection` 需要保留官方页面结构：

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

GMod API 不能只按 global 判断 realm。很多库本身 shared，但成员函数分属不同 realm：

```text
net                shared
net.Start          shared
net.Receive        shared
net.Broadcast      server
net.Send           server
net.SendToServer   client
```

查询时使用最长路径优先：

```text
net.Broadcast
  -> first try net.Broadcast
  -> fallback to net
```

这条规则同时用于编译器 realm 检查、LSP completion 过滤、hover 文档显示和 signature help。

## Extern

数据库无法覆盖所有第三方 addon、binary module 和动态全局。Lux 必须允许用户补充 extern：

```lux
extern server ThirdPartyAddon
extern client FancyHud
extern shared SharedLibrary

extern shared net
extern server net.Send
extern client net.SendToServer
```

也需要支持 package-level 配置：

```toml
[target.gmod.extern]
ThirdPartyAddon = "server"
FancyHud = "client"
SharedLibrary = "shared"

[target.gmod.extern."ThirdPartyAddon.DoSomething"]
realm = "server"
```

extern 和数据库一样使用 path-level annotation，并使用最长路径优先。

## Unknown External

未知外部符号不能被视为 shared-safe。它应该是独立状态：

```text
UnknownExternal
```

默认行为：

```toml
[target.gmod.realm]
unknown_external = "warn"
```

可选值：

- `allow`：不报。
- `warn`：默认，报告风险但不阻止编译。
- `error`：CI 或严格项目使用。

warning 去重 key：

```text
(symbol_path, active_realm, containing_decl_binding_id)
```

不要只按 symbol 去重，也不要按每个 use-site 都报。

诊断示例：

```text
warning[REALM_UNKNOWN]:
cannot verify realm availability of external symbol `ThirdPartyAddon.DoThing`
used in shared context inside `run`

Add an extern declaration to make this strict:
  extern shared ThirdPartyAddon.DoThing
  extern client ThirdPartyAddon.DoThing
  extern server ThirdPartyAddon.DoThing
```

## Hook 数据

Hook 数据必须显式建模，不能只当字符串补全：

```ts
HookSymbol {
  name: string
  gm_path: string        // GM:PlayerInitialSpawn
  realm: RealmSet
  callback: Signature
  description: DocPage
  docs_url: string
}
```

这样 LSP 可以在这些位置提供智能体验：

```lux
hook.Add("PlayerInitialSpawn", "id", fn(ply, transition) {
  ...
})
```

- 在第一个字符串参数内补全 hook 名。
- hover hook 名时显示 `GM:PlayerInitialSpawn` 文档。
- 在 callback 中推断 `ply: Player`。
- signature help 显示 callback 参数。

## 类型数据

MVP 至少需要轻量类型数据：

- global function return type
- method receiver type
- hook callback parameter type
- constructor return type，例如 `vgui.Create("DButton") -> DButton`
- colon call receiver type，例如 `Player:SteamID`
- constant 和 enum 类型

不要求一开始实现完整静态类型系统，但这些数据必须足够支撑 GMod API 补全和 hover。

## 更新命令

实现阶段必须提供：

```text
Lux: Update Garry's Mod API Database
lux gmod api update
```

插件默认内置离线数据库。更新失败不能导致基础编辑器体验失效。
