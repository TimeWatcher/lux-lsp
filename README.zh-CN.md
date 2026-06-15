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

Phase 1 和 Phase 2 的基础已经落地：

- `luxc::analysis` 是 compiler、CLI、LSP 和测试共享的稳定分析入口。
- LSP 通过内存 overlay 分析未保存文件，不解析 `luxc` 命令行输出。
- 支持 LSP 3.17 初始化、全文同步、diagnostics、hover、completion、definition、formatting、semantic tokens 和 code action。
- completion 已接入 Lux module/export 语义：module path、export list、import specifier 和普通 binding 会按上下文返回。
- hover 和 definition 已支持 module 内部 binding、export alias、import binding、unknown external。
- diagnostics 和 quick fix 已由 compiler analysis API 生成，包括 unknown external 的 `extern` 建议。

本仓库还没有发布 VS Code 扩展和完整 GMod API 数据库。下一阶段是在这个基础上接入 `gmod-api-db`、signature help、文档级 GMod API hover 和 VS Code 扩展打包。

## 本地开发

```powershell
cargo test
cargo run -p lux-lsp
```

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
