# Lux LSP

Lux LSP 是 Lux 的语言工具仓库。它承载可复用的语言服务器、VS Code 扩展壳设计，以及供编译器和编辑器共同使用的 Garry's Mod API 智能数据标准。

这个仓库的目标不是做一个最低限度的语法插件。Lux 开发者应该获得接近 GLua Enhanced 的成熟 GLua 开发体验，并在此基础上获得 Lux 独有的 module、realm、export 和语法智能。

English documentation: [README.md](README.md).

## 范围

- `lux-lsp`：独立的 Language Server Protocol 实现，当前已接入 `luxc::analysis`。
- `vscode-lux`：轻量 VS Code 扩展壳，负责激活、语法、配置、代码片段、命令和 server 启动。
- `gmod-api-db`：版本化 Garry's Mod API 数据库，供 hover、completion、signature help、diagnostics 和编译器 realm 检查共用。
- 从 Lux 编译器抽取稳定分析 API，而不是让 LSP 解析 CLI stderr。

## 当前实现

Phase 1、Phase 2 和 Phase 3 核心基础已经落地：

- `luxc::analysis` 是 compiler、CLI、LSP 和测试共享的稳定分析入口。
- LSP 通过内存 overlay 分析未保存文件，不解析 `luxc` 命令行输出。
- 支持 LSP 3.17 初始化、全文同步、diagnostics、hover、completion、definition、formatting、semantic tokens 和 code action。
- completion 已接入 Lux module/export 语义：module path、export list、import specifier 和普通 binding 会按上下文返回。
- hover 和 definition 已支持 module 内部 binding、export alias、import binding、unknown external。
- diagnostics 和 quick fix 已由 compiler analysis API 生成，包括 unknown external 的 `extern` 建议。
- `gmod-api-db` 已经内置由 Facepunch 官方 Wiki JSON 页表和单页 markup 生成的离线数据库。
- 当前 bundled manifest 覆盖 6335 个官方页面、6121 个 API 候选页面，其中 6121 个页面结构化解析，0 个页面作为 fallback 文档页保留，生成 10022 个 entry、497 个 hook、186 个 class，失败转换页面为 0。
- 官方 class 和 Derma panel 的 parent metadata 已进入数据库，因此继承方法补全和文档解析沿 Facepunch 官方 markup 查询，而不是依赖人工维护的类型表。
- compiler realm 检查和 LSP hover、completion、signature help、GMod 官方文档 code action 共用同一个 `gmod-api-db` 查询接口。

本仓库还没有发布 VS Code 扩展。下一阶段是 VS Code 扩展壳、数据库更新命令 UX、curated override 支持和发布打包。

## 本地开发

```powershell
cargo test
cargo run -p lux-lsp
```

更新内置官方 GMod API 数据库：

```powershell
luxc gmod api update `
  --out crates\gmod-api-db\data\generated\gmod_api.json `
  --coverage-out crates\gmod-api-db\data\generated\coverage_manifest.json `
  --cache-dir target\gmod-api-cache
```

独立开发入口仍然可用：`cargo run -p gmod-api-update -- ...`。两条路径共用同一个 Rust updater library。updater 以 `https://wiki.facepunch.com/gmod/~pagelist?format=json` 作为覆盖率基准，下载官方单页 JSON，转换 Facepunch markup，并可通过 `--override <json>` 叠加可追溯修正；只要 API 候选页面抓取或转换失败，命令就会失败。开发 parser 时可以显式加 `--allow-failures`。

在 Lux 主仓库中，本仓库作为 `lsp` submodule 存在。`lux-lsp` 依赖相邻的 `../compiler` crate，因此推荐从主仓库克隆并初始化 submodule 后开发。

## 标准文档

- [架构标准](docs/zh-CN/architecture.md)
- [Garry's Mod API 数据库](docs/zh-CN/gmod-api-db.md)
- [Document Hover 标准](docs/zh-CN/document-hover.md)
- [VS Code 体验标准](docs/zh-CN/vscode-experience.md)
- [路线图](docs/zh-CN/roadmap.md)

## 不可降低的体验基线

Lux 编辑器支持必须满足 GLua 开发者已经形成的预期：

- 完整 GMod API 补全
- 文档级 hover
- hook 名称和 callback 签名提示
- 带参数文档的 signature help
- `Player:`、`Entity:`、`Panel:` 等类型的方法补全
- 基于官方 parent metadata 的 Derma panel 继承方法，例如 `DButton` 可以解析 `Panel:SetSize`
- 官方文档链接
- 实时编译和 lint diagnostics

Lux 还必须在此基础上提供 GLua 工具无法提供的增强：

- realm-aware 补全和诊断
- 基于 module public API 的 import/export 智能补全
- 跨多 part module 的定义跳转
- export、alias、内部 binding、realm availability 的 hover
- Lux 语法的格式化和 semantic tokens

## 授权

本仓库采用 MIT 或 Apache-2.0 双授权。生成的官方文档数据必须保留来源署名和授权元数据。
