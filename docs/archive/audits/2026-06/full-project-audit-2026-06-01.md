# Forensics Workbench 全量审计与项目评估

> 归档：2026-06 审计快照，仅用于历史追溯，不代表当前实现状态。

**审计日期**: 2026-06-01  
**审计人**: Codex  
**范围**: Rust workspace、Tauri command layer、React/TypeScript frontend、SQLite migrations、近期新增分析功能、`docs/` 最新文档  
**结论级别**: 当前为功能原型/开发态，不建议作为可信取证工具发布或交付。核心导入、浏览、预览、搜索、时间线已有可用骨架，但质量门禁失败，新增数据源分析功能存在会生成伪取证结论的高风险问题，部分文档明显乐观或已过期。

## 一、总体评估

| 维度 | 评级 | 说明 |
|------|------|------|
| 架构分层 | B | workspace 分层清晰，Tauri command -> app-services -> domain/persistence 的主线合理；但新增分析功能绕过 transport 契约。 |
| 取证可信度 | C- | 文件枚举、MFT、部分 artifact parser 已有基础；但新增系统信息分析返回硬编码事实，导入后搜索/工件抽取存在断链风险。 |
| 安全边界 | C+ | 已有路径校验、错误脱敏、CSP 和参数化 SQL；但分析命令绕过现有安全读取路径，逻辑目录枚举仍可能跟随 symlink/junction。 |
| 前端工程 | B- | React 页面、mock/tauri API 层、测试框架已存在；新增 DataAnalysis 直接 `invoke`，缺 mock、缺 hook、缺共享类型。 |
| 测试/CI 就绪度 | C | 前端 typecheck/test/build 通过；Rust fmt/clippy/test 和前端 lint 均失败。 |
| 文档一致性 | C | 文档数量充足，但新文档中测试数量、性能、安全能力等有未验证或过期表述。 |

## 二、关键发现

### P0-1: 新增 `analysis_service` 会返回伪造的取证事实

**位置**:
- `crates/app-services/src/analysis_service.rs:152-184`
- `crates/app-services/src/analysis_service.rs:190-213`
- `frontend/src/app/pages/DataAnalysis.tsx:183-249`

`extract_system_info` 只检查 `SYSTEM`、`SOFTWARE`、`System.evtx` 文件是否存在，随后 `extract_from_system_hive`、`extract_from_software_hive`、`extract_boot_history` 返回硬编码值，例如 `FORENSICS-PC`、`Windows 10`、`19045`、`User`、固定开机时间。这个问题比“未实现”更严重，因为 UI 和报告会把这些值展示为真实证据。

**影响**: 调查员可能基于虚假主机名、OS 版本和开机记录做错误判断；报告输出不具备证据可信度。

**建议**: 在真实 registry/EVTX parser 接入前，返回 `None`、`unknown` 或显式 warning，不得生成硬编码证据事实。后续应复用或扩展 `artifacts-windows` 的 registry/EVTX 能力，并在 DTO 中携带 provenance 和 confidence。

### P0-2: 数据源分析读取文件方式错误，并绕过既有路径安全模型

**位置**:
- `apps/desktop/src-tauri/src/commands/analysis_commands.rs:53-92`
- `apps/desktop/src-tauri/src/commands/analysis_commands.rs:119-149`

`classify_files` 遍历数据库中的 `FileEntry` 后只保留 `(entry.path, size)`，再用 `case_root.join(path)` 读取文件。这有三类问题：

- 逻辑目录导入的真实数据在原始 `source_path`，不是 case root。
- E01/RAW 背后的文件不能通过 host path 读取。
- 该路径读取没有复用 `file_service::safe_relative_path`、canonical root check、image reader、handle 读取等已有安全路径。

**影响**: 分类结果大概率为空或靠扩展名兜底；恶意/损坏 DB 中的 `../` 路径可能触发越界 host 文件读取尝试。

**建议**: 分类输入应是 `FileEntryId`，通过统一读取 helper 解析 data source 类型，复用 preview/import 的逻辑目录与镜像读取路径，并只读取有限 header 字节。

### P0-3: 导入后工件抽取和全文索引可能与真实文件内容断链

**位置**:
- `apps/desktop/src-tauri/src/commands/import/pipeline.rs:27-60`
- `apps/desktop/src-tauri/src/commands/import/pipeline.rs:93-115`

`run_post_import_pipeline` 传入的是 `FileEntryId`，但 `create_file_reader_fn` 把 `file_id.0` 当作相对路径处理。逻辑目录文件 ID 是 UUID，不是文件路径；E01/RAW 分支直接返回 `None`。

**影响**: artifact extraction 和 Tantivy indexing 可能读取不到任何真实文件内容，但导入流程仍可能以 “Artifacts: 0 / indexed 0” 完成，而不是暴露失败。

**建议**: reader function 应通过 DB 查询 `FileEntry`，根据 `data_source_id` 和 `DataSourceKind` 打开真实文件流。增加一个逻辑目录导入集成测试，断言已知文本文件可被索引并能搜索命中。

### P0-4: SQLite 迁移失败会被标记为已应用

**位置**:
- `crates/persistence-sqlite/src/migrations/runner.rs:81-112`
- `crates/persistence-sqlite/src/migrations/scripts/0016_add_cascade_delete.sql:5-83`

迁移执行失败时，runner 回滚后仍 `INSERT OR IGNORE INTO schema_migrations`。`0016_add_cascade_delete.sql` 包含重建表、创建索引等非幂等操作，一旦部分失败，就可能出现“schema 未达到预期但迁移被记录为已应用”的状态。

**影响**: 级联删除、索引、外键等文档声称的数据库能力可能在真实升级库中不存在，后续迁移也无法自动修复。

**建议**: 失败迁移不得标记为 applied。重建表迁移需要幂等保护、后置 schema verification，以及旧库升级测试。

### P1-1: 新增分析 API 绕过 transport 契约和 mock/tauri API 层

**位置**:
- `crates/app-services/src/analysis_service.rs:9-66`
- `apps/desktop/src-tauri/src/commands/analysis_commands.rs:3-34`
- `crates/transport/src/dto/mod.rs:1-26`
- `crates/transport/src/commands/mod.rs:1-107`
- `frontend/src/app/pages/DataAnalysis.tsx:24-96`

项目约定所有 serializable API types 放在 `crates/transport/src/dto/`，并使用 `#[serde(rename_all = "camelCase")]`。新增 `SystemInfo`、`FileClassification` 等直接定义在 `app-services`，前端又在页面内定义 snake_case interface 并直接调用 `invoke`。

**影响**: mock mode 无法覆盖该页面；frontend `types/models.ts` 没有同步类型；未来重命名/序列化变更不会被现有 API 测试捕获。

**建议**: 新增 `transport/src/dto/analysis.rs` 和 request DTO；前端新增 `src/lib/api/analysis.ts`、`src/features/analysis/hooks.ts`，页面只消费 hook。

### P1-2: 逻辑目录枚举可能跟随 symlink/junction 出 evidence root

**位置**:
- `crates/evidence-core/src/filesystem/logical_fs.rs:13-47`
- `crates/evidence-core/src/filesystem/logical_fs.rs:68-70`

`DirEntry::metadata()` 会跟随 symlink，`read_dir(root.join(relative_path))` 没有对每个子项 canonicalize 并校验 `starts_with(root)`。预览读取后续有 root containment 检查，但枚举阶段仍可能记录外部路径元数据，甚至遍历大型外部目录或循环目录。

**影响**: 逻辑目录导入的取证范围可能被污染，产生 out-of-scope metadata。

**建议**: 枚举使用 `symlink_metadata`，对 symlink/reparse point 只记录不下钻，或 canonicalize child 后强制 root containment。补 Windows junction/symlink 测试。

### P1-3: image/media preview 与文档表述不一致

**位置**:
- `apps/desktop/src-tauri/src/commands/file_commands.rs:288-345`
- `apps/desktop/src-tauri/src/commands/file_commands.rs:364-398`

`get_image_preview` 仍通过 `read_file_range_for_case` 读取 hex lines，再解码回 bytes 并 base64 成 data URL。该路径受 `MAX_RANGE_LENGTH` 限制，大图可能被截断；`handle.size as u32` 也有大文件截断风险。`get_media_url` 返回 `asset://localhost/{absolute_path}`，只替换空格，没有完整 URL encoding，也会暴露 host path。

**影响**: 大图预览可能损坏；media URL 对 `#`、`?`、`%`、非 ASCII、Windows separator 不稳；renderer 看到绝对路径。

**建议**: 用 handle keyed streaming command 或 Tauri 受控 asset scope，避免裸 host path。图片缩略图应明确 size limit，超限返回 typed fallback。

### P1-4: 测试断言与实际迁移版本过期

**位置**:
- `crates/app-services/tests/case_service_test.rs:27-30`
- `crates/persistence-sqlite/tests/connection_test.rs`

测试仍断言当前迁移为 `0010_job_partition_progress`，但当前实际版本为 `0017_add_missing_indexes`。`cargo test --workspace --no-fail-fast` 中 `case_service_test` 和 `connection_test` 均因此失败。

**影响**: 后端全量测试不能作为可合并门禁；迁移测试无法证明真实最新 schema。

**建议**: 测试不要硬编码旧版本，或集中从 migration registry 获取最新版本；同时增加 schema shape 断言。

### P1-5: 当前 clippy/fmt/lint 门禁失败

**验证结果**:
- `cargo fmt --all -- --check`: failed，多个 Rust 文件需要格式化。
- `cargo clippy --workspace --all-targets -- -D warnings`: failed。
- `pnpm lint`: failed，1 error + 9 warnings。

主要 clippy 失败:
- `crates/domain/src/timestamp.rs:29`: manual range contains。
- `crates/domain/src/error.rs:105`: `io_other_error`。
- `crates/domain/src/timestamp.rs:240/249/257`: identity op。
- `crates/evidence-core/src/volume/gpt.rs:211`: unused mut。

新增分析代码也产生 warning:
- `crates/app-services/src/analysis_service.rs:191/199/207/279`: unused variables。

前端 lint:
- `frontend/src/stores/mcp-store.ts:486`: `preserve-caught-error` error。
- `frontend/src/app/pages/DataAnalysis.tsx:155`: `any` warning。

**影响**: CI 若按 AGENTS.md 命令执行会失败。

**建议**: 先恢复 fmt/clippy/test/lint，再继续扩展分析功能。

## 三、最新文档评审

### `docs/architecture-model.md`

优点:
- 分层架构、crate 职责、前端组件结构描述基本符合项目方向。
- 明确了 Tauri/Rust/React/SQLite 的主架构。

问题:
- `测试: 269 个单元测试`、各 crate 测试数量、CI 门禁 9 项等表述没有反映当前失败状态。
- 性能指标如 `文件枚举 ~1000 文件/秒`、`MFT 扫描 ~5000 记录/秒`、`搜索 <100ms` 未见本轮可复现实测依据。
- 安全架构写 `无 unsafe 代码`、`权限检查`、`审计日志`，但真实项目仍存在未闭合边界：分析命令绕过安全读取、迁移失败标记 applied、media URL 裸路径。

建议:
- 改为“设计目标/当前实现状态/待验证指标”三栏，不把未验证数据写成事实。
- 在文档顶部增加“最后验证命令与结果”。

### `docs/algorithm-complexity-analysis.md`

优点:
- 对 BFS 枚举、排序、hash、hex、编码检测、MFT 扫描给出了合理的静态复杂度框架。
- 已识别 MFT 路径重建 O(n²) 问题，并且当前代码已尝试用递归缓存改到 O(n)。

问题:
- `路径重建` 在总览仍写 O(n²)，而代码已修改为递归缓存，文档前后不一致。
- `MFT 批量扫描 O(n/p)` 的表述忽略 reader/writer/SQLite 批量写入瓶颈，实际更接近 pipeline throughput，而不是纯 CPU 并行复杂度。
- `Magic 检测` 示例读取完整文件，和“实际 O(1)”不完全一致；当前 `std::fs::read` 会读取整个文件。

建议:
- 将复杂度分成 CPU matching、I/O bytes read、DB writes 三部分。
- 对新增 `analysis_service` 先标为实验/不可用于证据报告。

## 四、验证记录

| 命令 | 结果 | 备注 |
|------|------|------|
| `cargo fmt --all -- --check` | failed | 多文件格式不一致，包括新增 `analysis_commands.rs`、`transport/src/dto/mod.rs` 等。 |
| `cargo clippy --workspace --all-targets -- -D warnings` | failed | domain/evidence-core 旧问题 + 新增 analysis unused variable warnings。 |
| `cargo test --workspace` | failed | 首个失败为 `app-services --test case_service_test` 迁移版本断言过期。 |
| `cargo test --workspace --no-fail-fast` | failed | 失败 targets: `app-services case_service_test`、`app-services e01_full_pipeline_test`、`persistence-sqlite connection_test`、`domain --doc`。其中 E01 慢测长时间无输出后被本轮手动停止。 |
| `pnpm typecheck` | passed | TypeScript 类型检查通过。 |
| `pnpm test` | passed | 5 files / 22 tests passed。 |
| `pnpm build` | passed | Vite production build passed。 |
| `pnpm lint` | failed | 1 error + 9 warnings。 |

## 五、建议修复路线

### Phase 0: 恢复可信门禁

1. 跑 `cargo fmt --all` 并提交格式化。
2. 修复 clippy 当前错误和 analysis unused warnings。
3. 修复迁移测试旧版本断言与 domain doctest import。
4. 将 E01 慢测拆成 `#[ignore]` 或单独 CI job，避免全量单元测试被长时样本阻塞。
5. 修复 `pnpm lint` 的 `preserve-caught-error` error，并消除新增 `DataAnalysis` 的 `any`。

### Phase 1: 禁止伪取证输出

1. 删除或禁用 `analysis_service` 中硬编码系统信息。
2. UI 明确显示 “未解析/待实现/无证据来源”，不要显示默认主机名和默认 OS。
3. 报告生成加入 provenance、source file、parser status、warnings。

### Phase 2: 统一文件读取与 API 契约

1. 抽出 data-source-aware file reader helper，供 preview、analysis、search indexing、artifact extraction 共用。
2. `analysis` DTO 迁入 transport，统一 camelCase。
3. 前端通过 `apiClient` 和 feature hook 访问分析接口，补 mock provider 和测试。

### Phase 3: 数据库迁移可靠性

1. 迁移失败立即返回 error，不标记 applied。
2. 对 `0016` 这类 table rebuild migration 补幂等性和 schema verification。
3. 增加旧库升级 fixture 测试。

### Phase 4: 取证能力增强

1. Registry/EVTX 解析接入真实 Windows artifact pipeline。
2. Search/artifact post-import pipeline 用真实文件内容集成测试兜底。
3. 逻辑目录 symlink/junction 边界测试。
4. media/image preview 改为 handle scoped streaming，不暴露绝对路径。

## 六、当前状态判定

当前项目适合作为继续开发的桌面取证工作台原型，不适合生成可信调查报告或对外展示“已完成取证分析能力”。最需要优先处理的不是扩展新页面，而是恢复工程门禁、去除伪证据输出、统一文件读取路径和修复迁移可靠性。完成 Phase 0-2 后，项目可进入“内部 alpha”；完成 Phase 3-4 并补足真实样本验证后，再考虑“可信 beta”。
