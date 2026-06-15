# Lux LSP 架构标准

## 目标

Lux LSP 的目标不是提供一个独立于编译器的小型编辑器插件，而是把 Lux 编译器已经拥有的语义能力开放给编辑器，同时补齐 Garry's Mod 开发者在 GLua Enhanced 中已经习惯的 API 文档、补全、签名和跳转体验。

最终架构分为三层：

```text
Lux compiler analysis API
  -> lux-lsp
  -> vscode-lux

gmod-api-db
  -> compiler realm checker
  -> lux-lsp hover/completion/signature/diagnostics
  -> docs links and API browser
```

## 仓库组成

`lux-lsp` 仓库承载这些组件：

- `crates/lux-lsp`：独立 LSP server，遵循 LSP 3.17。
- `extensions/vscode-lux`：VS Code 扩展壳，负责激活、TextMate grammar、semantic tokens 注册、配置、命令和 server 启动。
- `crates/gmod-api-db`：Garry's Mod API 数据模型、加载器、查询接口和版本信息。
- `tools/gmod-api-update`：从官方文档更新 API 数据库的工具。
- `docs`：语言服务标准和用户文档。

MVP 可以先只落文档和标准，但实现时不应把 VS Code 扩展写成不可复用的一体化插件。LSP server 必须能被其他编辑器复用。

## 编译器分析 API

LSP 不应该运行 `luxc` 并解析 stderr。编译器需要提供稳定分析 API：

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

这些 API 应该被 CLI、LSP 和测试共享。CLI 是其中一个前端，不是语义事实来源。

## Project Model

LSP 必须理解 Lux 的项目模型：

- package 极简，module 自动发现。
- 一个 module 是一个目录，不是单个文件。
- module 目录下所有 part 文件共享同一个逻辑 module scope。
- top-level declaration 在 module 内默认 private，并对同 module part 可见。
- top-level import 是 part-local binding，不会自动变成 module-wide binding。
- top-level `fn` 在整个 module scope 内 hoist。
- top-level 非函数 local 不作为已初始化值 hoist，初始化顺序由稳定 part order 决定。
- use-before-initialization 是错误。
- export 是 module scope binding 到 public API name 的显式映射，不改变内部可见性。
- MVP 0.1 中，重复 module-scope binding name 是错误，即使它们位于不同 realm。

## Realm Model

LSP 和编译器必须共享相同 realm 语义：

```text
Lux symbol       -> strict
known GMod API   -> strict
unknown external -> allow / warn / error
```

内部表示不要把未知外部当作 shared：

```text
RealmAvailability
  = Known(RealmSet)
  | UnknownExternal
```

来源需要保留：

```text
AvailabilitySource
  = LuxBinding
  | GmodApiDb
  | ExternDeclaration
  | UnknownExternal
```

这样 diagnostics 可以明确告诉用户符号来自 Lux、GMod API 数据库、extern 声明，还是无法验证的外部符号。

## 旧 Realm Guard

旧的 realm guard 只是一小部分 API 的手写表，必须被统一 Realm Availability Engine 替代。

新规则：

- `gmod-api-db` 是 GMod API realm 的唯一事实来源。
- 编译器、LSP、hover、completion 和 diagnostics 全部查询同一个数据库。
- 数据库支持 path-level realm，例如 `net`、`net.Start`、`net.Broadcast` 可以有不同 realm。
- 解析符号时使用最长路径优先。
- 未知外部符号不进入 shared/client/server 集合，默认 warning，可配置为 allow 或 error。

## Incremental Analysis

LSP 需要支持文件级增量更新，但语义结果以 module 为单位重新计算：

- 单个 part 文件变化时，重建所属 module 的 parse tree、binding graph、export table 和 diagnostics。
- 跨 module import/export 改变时，重建依赖 module 的 import resolution。
- `lux.toml` 或 part order 改变时，重建 project graph。
- `gmod-api-db` 更新时，重建外部符号 realm 和 hover cache。

## VS Code 责任边界

VS Code 扩展只负责编辑器集成：

- 激活语言服务。
- 提供 TextMate grammar 作为基础高亮。
- 注册 semantic tokens、formatting、diagnostics、completion、hover、signature help、definition、references。
- 提供更新 GMod API 数据库、打开 Lux 文档、打开官方 GMod 文档等命令。

语义判断不应在 TypeScript 扩展里重复实现。
