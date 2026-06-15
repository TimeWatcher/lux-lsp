# 路线图

## Phase 0：标准和仓库

- 建立 `lux-lsp` 仓库。
- 明确 LSP、VS Code、GMod API 数据库和 Document Hover 标准。
- 明确旧 realm guard 被 `gmod-api-db` 驱动的 Realm Availability Engine 取代。

## Phase 1：Compiler Analysis API

- 已完成：从 Lux 编译器抽出稳定分析 API。
- 已完成：提供 parse、resolve、module graph、part order、realm stack、diagnostics、formatting、hover、completion、definition、semantic tokens 和 code action 数据。
- 已完成：CLI、LSP 和测试共享同一套语义入口。
- 已完成：测试覆盖 multi-part module、export alias、unknown external、UTF-16 position、realm domain block、use-before-init。

## Phase 2：LSP Server 基础层

- 已完成：实现 LSP 3.17 server。
- 已完成：支持 initialize、text sync、diagnostics、hover、completion、definition、formatting、semantic tokens、code action。
- 已完成：先接 Lux 自身符号，不依赖 GMod API DB 即可运行。
- 已完成：使用 workspace root + in-memory overlay 分析未保存文件。
- 已完成：import/export completion、跨 part definition、export alias hover、unknown external quick fix 走 compiler analysis API。

## Phase 3：GMod API Database

- 已完成：定义 compiler 和 LSP 共用的数据库 schema。
- 已完成：实现 `gmod-api-update`，从 Facepunch 官方 Wiki 页表抓取全量地址，下载单页 JSON，解析 Facepunch markup，并生成 coverage manifest。
- 已完成：内置 generated 离线数据库。
- 已完成：以官方页表作为覆盖率基准。当前 generated manifest 覆盖 6335 个官方页面和 6122 个 API 候选页面，其中 6121 个页面结构化解析、1 个页面作为 fallback 文档页保留，失败转换为 0。
- 已完成：解析官方 class 和 Derma panel 的 parent metadata，因此方法补全、hover 和 signature help 会沿官方 class parent chain 查询，而不是依赖人工维护的继承表。
- 已完成：加入 curated 轻量 JSON override 层，用于修正官方文档中已知的不精确信息。
- 已完成：通过主 compiler CLI 提供 `luxc gmod api update`。
- VS Code 更新命令归入 Phase 6 的扩展壳交付。

## Phase 4：Document Hover 和 GLua Experience Baseline

- 已完成：基于 generated GMod API 数据提供文档级 hover。
- 已完成：官方页面提供的说明、参数、返回、warning、note、示例代码和链接会进入 hover。
- 已完成：支持 hook 名 hover 和 callback 签名。
- 已完成：API root/member completion 和 signature help 已接入同一份数据库。
- 已完成：为 `LocalPlayer()`、`vgui.Create("DButton")` 等常见模式提供 receiver/constructor 感知的方法补全。
- 已完成：receiver 类型传播已扩展到 local alias 和简单函数返回事实，并用于方法补全、hover 和 signature help。
- 已完成：使用官方 class/panel parent metadata 提供继承方法补全和文档解析，例如 `DButton` 可以解析继承来的 `Panel:SetSize`。

## Phase 5：Realm Availability Engine

- 已完成：用 `gmod-api-db` 替代旧 realm guard。
- 已完成：compiler 和 LSP 共用同一查询接口。
- 已完成：从 generated 官方数据支持 path-level realm。
- 已完成：支持 extern 源码声明和 unknown external allow/warn/error。
- 已完成：支持 `lux.toml` package-level extern 配置。
- 已完成：为 source extern、package-level extern 和 realm mismatch 官方文档 action 提供 quick fix。
- 已完成：为 export realm widening 提供 quick fix；在可明确判断时，会把无效的 `export shared` 收窄到 binding 实际 realm。

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
