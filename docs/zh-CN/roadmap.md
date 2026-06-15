# 路线图

## Phase 0：标准和仓库

- 建立 `lux-lsp` 仓库。
- 明确 LSP、VS Code、GMod API 数据库和 Document Hover 标准。
- 明确旧 realm guard 被 `gmod-api-db` 驱动的 Realm Availability Engine 取代。

## Phase 1：Compiler Analysis API

- 从 Lux 编译器抽出稳定分析 API。
- 提供 parse、resolve、module graph、part order、realm stack、diagnostics、formatting。
- CLI 和 LSP 共享同一套语义入口。
- 建立 fixture 测试，覆盖 multi-part module、export alias、realm domain block、use-before-init。

## Phase 2：LSP Server MVP

- 实现 LSP 3.17 server。
- 支持 initialize、text sync、diagnostics、hover、completion、definition、formatting、semantic tokens、code action。
- 先接 Lux 自身符号，不依赖 GMod API DB 即可运行。
- 使用 module 粒度增量分析。

## Phase 3：GMod API Database

- 定义数据库 schema。
- 实现官方文档抓取和解析工具。
- 加入 curated overrides。
- 内置离线数据库。
- 提供 `lux gmod api update` 和 VS Code 更新命令。

## Phase 4：Document Hover 和 GLua Experience Baseline

- 对常用 GMod API 提供文档级 hover。
- 支持官方说明、参数、返回、warning、note、示例代码和链接。
- 支持 hook 名 hover、callback 签名、panel class hover、class method hover。
- 补齐 signature help 和 completion。

## Phase 5：Realm Availability Engine

- 用 `gmod-api-db` 替代旧 realm guard。
- compiler 和 LSP 共用同一查询接口。
- 支持 path-level realm。
- 支持 extern 源码声明和 package-level extern 配置。
- 支持 unknown external allow/warn/error。
- 为 realm mismatch、unknown external、export realm widening 提供 quick fix。

## Phase 6：VS Code Extension

- TextMate grammar。
- Semantic token theme scopes。
- snippets。
- settings。
- commands。
- quick fix、source action 和 workspace edit UX。
- server distribution。
- VSIX package。

## Phase 7：Release

- GitHub Actions 构建 LSP server 二进制。
- GitHub Release 附带预构建 server。
- 发布 VSIX。
- 文档站加入 LSP 和 VS Code 安装页面。
- Lux 主仓库 README 链接到 LSP 仓库。

## 不做的事

这些不进入第一阶段：

- 复制 GLua Enhanced 的 GPL 数据或实现。
- 在 VS Code TypeScript 扩展里重复实现 Lux resolver。
- 把未知外部符号默认为 shared-safe。
- 只做语法高亮就宣称 VS Code 支持完成。
