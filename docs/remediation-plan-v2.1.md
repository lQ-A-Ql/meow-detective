# Forensics Workbench 剩余 v2.1 Backlog 状态与修补方案

**更新时间**: 2026-06-02 15:52:16 +08:00
**署名**: Codex  
**基线**: `e7ffa35` -> `codex/beta-forensics-backlog`；本文件记录 v2.1 backlog 在可信 beta 收口实现后的当前状态、验收命令和剩余修补方案。

## Summary

v2.1 的目标是把内部 alpha 推进到可信 beta。当前已补齐 Analysis provenance 契约、Reports analysis provenance、Job partial/warning/skipped/failed 语义、媒体 scoped handle + bounded range command、`evidence-media` Tauri protocol 与 CI guard、Registry 定向字段 parser、EVTX boot/shutdown candidate adapter、EVTX real fixture parser-path regression、`cargo deny`/`cargo audit`/`pnpm audit` 依赖门禁、CycloneDX SBOM CI artifact、coverage report CI artifact、frontend coverage baseline gate、tiny logical/RAW/E01/EVTX fixtures 和前端 Vite/Vitest 安全升级。

剩余风险集中在解析覆盖和架构债：Registry parser 是定向字段解析器，不是完整 hive browser；EVTX adapter 已接入 `evtx` crate 并解析 6005/6006/6008/1074 候选事件，且 `testdata/fixtures/tiny/evtx/system.evtx` 已覆盖真实 parser path。补充复核确认 `evtx 0.11.2` crates.io 发布包通过 `exclude = ["**/*.evtx", "**/*.dat"]` 排除了真实 EVTX 样本，`testdata` 中的 EVTX fixture 因此显式记录了上游仓库 commit、license、SHA-256 和大小；本轮已将 workspace `evtx` 切到本地 `crates/evtx-patched` fork，并把 legacy `encoding = "0.2.33"` 替换为 `encoding_rs`，因此 `RUSTSEC-2021-0153` 已从 `deny.toml` 例外和 `cargo audit` warning 中移除。`docs/evtx-dependency-decision.md` 与 `scripts/check-evtx-dependency-decision.ps1` 现在用于防止重新引入 crates.io `evtx -> encoding` 链路。tiny E01 reader fixture 已入库，用于默认 CI 覆盖 E01 section/table/read/seek 行为；真实 E01 分区/文件系统慢测仍依赖 `FORENSICS_E01_FIXTURE` opt-in。FS root/path/error 公共 helper 已抽出，完整枚举流程/错误转换进一步去重仍是后续质量项；command layer SQL 已通过 CI guard 防回退。

默认产品决策保持不变：`/analysis` 是正式页面；无法真实解析的取证字段必须显示 `notParsed/unavailable + warnings`；证据读取必须走 `FileEntryId + DataSourceKind` reader helper；mock 模式必须可用。

## Phase 0: 基线对账与防回退

| Task | 状态 | 证据 / 验收 |
|---|---|---|
| 0.1 已完成安全项回归 | Completed | v2 已覆盖 `delete_case` sandbox、GPT `entry_size`、case name validation、HTML/CSV escape、CSP、typed `CommandError`；本轮未回退 |
| 0.2 已完成功能项回归 | Completed | Reports export、cancel import UI、Search 跳 Files、FileBrowser preview、Analysis v2 基础链路保留；本轮新增 FileBrowser extract/media/settings API 测试 |
| 0.3 工作流去重前盘点 | Completed | 保留 `.github/workflows/ci-backend.yml` 与 `ci-frontend.yml`；删除旧 `backend-ci.yml` / `frontend-ci.yml`；本轮修正 frontend workflow 的 `pnpm --dir frontend` 路径 |
| 0.4 开发记录准备 | Completed | 开发前已阅读 `docs/remediation-plan-v2.1.md` 与 `development-reports/sessions/2026-06-01.md`，结束追加本轮记录 |

## Phase 1: 剩余安全硬化

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 1.1 导入路径校验 | Completed | `ImportDataSourceRequest::validate()` 拒绝空路径、NUL、Windows device path、extended-length path、reserved device name；command 入口校验存在性和文件/目录类型 |
| 1.2 逻辑目录 symlink/junction 防越界 | Completed | v2 已改 logical enumeration 不下钻 symlink/reparse point，并有回归测试 |
| 1.3 NTFS data run 内存上限 | Completed | v2 已加入 data run/buffer 上限检查，防止恶意 run 触发 OOM |
| 1.4 Artifact 抽取内存限制 | Completed | `ARTIFACT_FILE_LIMIT_BYTES` 集中到 `infrastructure::constants`，artifact reader 使用 bounded `take()` |
| 1.5 Pagination 与 DTO clamp | Completed | `PageRequest`、search/timeline/viewer request 加默认值和 max clamp；transport 单测覆盖 |
| 1.6 Viewer request 校验 | Completed | `ViewerRangeRequestDto` 和本轮新增 `MediaRangeRequestDto` 校验 handle/length 并 clamp 到 1MB |
| 1.7 SQLite migration 幂等复核 | Completed | v2 已覆盖失败 rollback、不写 applied、旧 schema 升级、`0016` rebuild 复核 |
| 1.8 LIKE wildcard escaping | Completed | `find_by_path_prefix` 转义 `%`、`_`、`\\` 并使用 `ESCAPE '\\'`；repository 回归测试已过 |
| 1.9 Recent cases 完整性 | Completed | recent cases 过滤缺失 `case.json` / `app.db` 的坏记录，删除/不可访问路径不会自动打开 |

## Phase 2: 真实取证解析与 Analysis 收口

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 2.1 Registry parser 接入 | Completed / targeted | Analysis 会从 catalog 查找 `Windows/System32/config/SYSTEM` / `SOFTWARE`，通过 `FileEntryId` reader bounded 读取最多 64MB hive；`artifacts-windows::registry::lookup` 支持 `regf` base block、NK/VK、带 cell header 的 value-list、`lf/lh/li/ri` 子键列表和常用值类型，并提取 `ComputerName`、timezone、ProductName、CurrentBuild、InstallDate、RegisteredOwner、Organization、ProductId。字段带 `fieldProvenance`；缺失/损坏/超限为 warning，不生成默认 Windows 文案。最新回归覆盖 value-list cell bounds、inline value 长度、短 `REG_DWORD`、`lh/li/ri` subkey list 导航、UTF-16 NK 名称、`REG_EXPAND_SZ`、`REG_MULTI_SZ` 和 `REG_QWORD`；`REG_MULTI_SZ` 已修正为保留完整 multi-string 列表，奇数长度 UTF-16 会返回 parse error。剩余：不是完整 Registry browser |
| 2.2 EVTX parser 策略 | Completed / real fixture | Analysis 查找 `Windows/System32/winevt/Logs/System.evtx`，通过 `evtx.boot_shutdown` adapter bounded 读取最多 64MB，解析 6005、6006、6008、1074 为候选事件，boot record 带 `eventId`、`recordId`、note 和 provenance。无 EVTX、损坏或超限时保持 `notParsed/unavailable + warnings`。当前单测覆盖 JSON 记录提取、malformed/oversized/truncated 路径，并新增 `testdata/fixtures/tiny/evtx/system.evtx` 真实 fixture 覆盖 parser path；fixture 来自 MIT/Apache-2.0 licensed upstream `evtx` repository commit `38a2d50b21629edb3dd77953a2c02a4b944badf1` |
| 2.3 Analysis provenance | Completed | 新增 `AnalysisProvenanceDto { dataSourceId, artifactPath, parser, parsedAt, status, warnings }`；system info、boot records、file classification 和 classified file 均可携带 provenance |
| 2.4 Analysis 分类读取一致性 | Completed | classify 走 `FileEntryId + DataSourceKind` helper，仅读取 bounded header |
| 2.5 Markdown summary 修正 | Completed | summary 只基于 parsed/unknown 状态，不输出 `FORENSICS-PC`、`Windows 10` 等伪值 |
| 2.6 UI parser 状态展示 | Completed | DataAnalysis 展示状态、warnings 和空字段“未解析”/`-`，无 active case 有空态 |

## Phase 3: 证据预览、文件动作与调查流闭环

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 3.1 大媒体 scoped streaming | Completed / CI guarded / manual smoke pending | `get_media_url` 对大媒体返回 `mode=protocol`、opaque `handleId` 和 `evidence-media://handle/<encoded>` URL；Tauri protocol handler 解析 Range、每次最多读取 1MB、返回 206/416 等标准状态，读取仍走 `open_file_content_by_id`。`read_media_range` command 保留为 mock/unsupported fallback；CSP 只给 `media-src` 增加 `evidence-media:`，不暴露 evidence 宿主绝对路径。新增 `scripts/check-media-protocol-guard.ps1` 并接入 backend CI，固定 CSP/protocol/fallback/host-path URL 边界。剩余：尚未做真实 Windows WebView2 播放器 seek smoke |
| 3.2 Image preview 限制 | Completed | 小图 data URL，超限 typed error/fallback；不再经 hex lines 反解大图 |
| 3.3 Extract file | Completed | 新增 `extract_file` command + save dialog wrapper，后端通过 reader helper 写出，destination 拒绝 device path/目录 |
| 3.4 FileBrowser -> Timeline | Completed | “在时间线中查看”设置 selection 并导航 `/timeline` |
| 3.5 Timeline -> Source | Completed | Timeline 源对象按钮跳 Files/Artifacts 并设置 selection |
| 3.6 Timeline filters UI | Completed | 增加 timeStart/timeEnd/eventType 控件，hook query key 包含 filters |
| 3.7 Search saved queries | Completed | `localStorage` 保存/覆盖/删除/执行查询，新增 tests |
| 3.8 Settings editable config | Completed | `get_app_settings` / `save_app_settings` Tauri commands 持久化 `AppSettingsDto`；路径类设置经后端验证后写入 config，theme/dev event trace 同步写入后端 config，并镜像到 localStorage 以便 mock/dev 即时生效。Settings 页面补齐路径输入 label 和 render tests，覆盖远端加载、保存 API、localStorage fallback、主题应用和非法路径拒绝 |

## Phase 4: 事件、任务状态与报告可信度

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 4.1 Typed event contract | Completed | `EventTopic` enum 收口 topic，unknown topic serde 拒绝；前端 `EventTopic` 同步 |
| 4.2 Event bridge 扩展 | Completed | 已有 case/job/artifact/timeline/search/partition emit helpers；本轮新增 typed `data-source-imported` 与 `job-cancelled` topic，同步 transport enum、frontend `EventTopic`、Tauri bridge 监听列表和 EventBus 测试。Import 完成时发 data source imported payload，仅包含 dataSourceId/name/kind/jobId，不暴露 sourcePath；cancel import 成功时发 job-cancelled |
| 4.3 Targeted emit | Completed | event bridge 使用 `emit_to("main", ...)`，payload 不含裸证据绝对路径 |
| 4.4 Capability 对齐 | Completed | `capabilities/default.json` 已包含 `core:event:default` 与保存对话框能力 |
| 4.5 Job partial/warning 语义 | Completed | `JobSnapshotDto`、SQLite `jobs` 表、JobRepo/JobService 和前端 Jobs panel 已同步 `warningCount/skippedCount/failedCount/partial`；import/search/artifact post-processing 会汇总 recoverable warnings/skips/failures |
| 4.6 Reports provenance | Completed | HTML/CSV/JSON report 已接入当前 Analysis summary、parser status、warnings 和 evidence provenance；HTML escaping 与 CSV formula sanitization 回归保留 |

## Phase 5: CI、测试资产与依赖治理

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 5.1 Workflow 去重 | Completed | 保留 `ci-backend.yml`、`ci-frontend.yml`；旧重复 workflow 删除；frontend workflow 已修正为 `pnpm --dir frontend ...` |
| 5.2 Dependency gate | Completed / EVTX patched | 后端 CI 保留 `cargo audit` 并新增 `cargo deny check advisories bans licenses sources`；`deny.toml` 对短期 transitive advisories 带 reason/owner/expiry，本轮新增 `scripts/check-deny-exceptions.ps1` 并接入 backend CI，强制 advisory 例外必须包含 owner、expires 和说明文本且不得过期。`RUSTSEC-2021-0153` 已不再是例外：workspace `evtx` 指向 `crates/evtx-patched`，local fork 使用 `encoding_rs`，`Cargo.lock` 不再包含 `encoding`。`docs/evtx-dependency-decision.md` 和 `scripts/check-evtx-dependency-decision.ps1` 现在防止该 legacy 链路回退；前端 CI 执行 `pnpm --dir frontend audit --audit-level high` |
| 5.3 DevTools guard | Completed | 新增 `scripts/check-release-guard.ps1`，CI backend 执行 release guard |
| 5.4 SBOM | Completed | 后端 CI 使用 `cargo-cyclonedx` 生成 `backend-sbom` artifact；前端 CI 使用 `@cyclonedx/cyclonedx-npm` 生成 `frontend-sbom` artifact，并验证 `bomFormat=CycloneDX` |
| 5.5 Fixture 策略 | Completed / real E01 manual | 新增仓库内 tiny logical directory、1024-byte tiny RAW fixture、4405-byte synthetic single-segment `testdata/fixtures/tiny/e01/tiny.E01` 和 1,118,208-byte real `testdata/fixtures/tiny/evtx/system.evtx`；`scripts/generate-tiny-fixtures.ps1` 可重建 RAW/E01 fixture，`crates/testing` 暴露 `tiny_logical_dir()`、`tiny_raw_image()`、`tiny_e01_image()`、`tiny_system_evtx()`。tiny E01 只证明 reader section/table/read/seek 行为，不代表真实文件系统镜像；EVTX fixture 覆盖 `evtx.boot_shutdown` parser path；真实 E01 分区/文件系统慢测仍为 `FORENSICS_E01_FIXTURE` opt-in ignored tests |
| 5.6 覆盖率扩展 | Completed / frontend baseline gated | 已补 Analysis、Reports、Viewer、Jobs、Fixture、CI/dependency 相关 targeted tests；新增 `scripts/run-coverage.ps1`、前端 `test:coverage`、backend/frontend coverage CI artifacts。前端 coverage 现在以全局 lines/statements/functions 45%、branches 35% 作为初始防回退阈值；后端 coverage 仍只生成 LCOV artifact，不设置百分比阈值 |

## Phase 6: 代码质量与文档收尾

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 6.1 FS enum 去重 | Partial | 已在 `evidence-core::filesystem` 抽出 `root_node`、路径拼接、path component normalization、常见 path/file error helper，以及 `node_with_parent_path` / `node_with_parent_path_with_separator`，并让 FAT/NTFS/exFAT reader 复用；targeted tests 与 clippy 通过。剩余：更深层的枚举流程和 FS 特定错误转换仍可继续抽象，但需避免改变 public API、排序、root semantics 和 reader 行为 |
| 6.2 SQL 下沉 repo | Completed / guarded | 复核 `apps/desktop/src-tauri/src/commands` 无原始 SQL 语句或 `rusqlite::params!/prepare/execute` 低层调用；新增 `scripts/check-command-sql-boundary.ps1` 并接入 backend CI，防止后续把业务 SQL 写回 command layer。Command layer 允许打开连接并调用 repository/service/helper，保持 validate -> service/helper -> DTO 边界 |
| 6.3 常量集中 | Completed | preview max、artifact max、pagination max、analysis sample max 等关键限制已集中到 transport/infrastructure 常量 |
| 6.4 Public docs | Completed | `cargo doc --workspace --no-deps` 已通过；同时修复 rustdoc bare URL / invalid intra-doc link warnings |
| 6.5 开发记录与修复计划更新 | Completed | 本文件、`docs/开发记录.md` 和 `development-reports/sessions/` 记录 subagent 分工、主线程统一复核、验证命令与剩余风险 |

## 本轮新增验证

- `pnpm --dir frontend test -- settings files`: 通过，3 个文件 / 15 个测试。
- `cargo test -p transport`: 通过，26 个测试。
- `cargo test -p forensics-desktop commands::settings_commands`: 通过，2 个测试。
- `cargo test -p forensics-desktop commands::file_commands::tests::oversized_media_preview_returns_scoped_handle_and_range_reads`: 通过。
- `cargo check -p forensics-desktop`: 通过。
- `pnpm --dir frontend typecheck`: 通过。
- `cargo test -p transport analysis jobs viewer`: 通过。
- `cargo test -p app-services --lib analysis_service`: 通过。
- `cargo test -p app-services report_service`: 通过。
- `cargo test -p artifacts-windows registry`: 通过。
- `cargo test -p artifacts-windows evtx`: 通过；7 个 EVTX targeted tests，包含 real `System.evtx` parser-path regression、JSON helper、malformed、oversized 和 truncated EVTX magic warning/not-panic 回归。
- `cargo test -p transport analysis`: 通过。
- `cargo test -p forensics-desktop media_protocol`: 通过。
- `powershell -ExecutionPolicy Bypass -File scripts\check-media-protocol-guard.ps1`: 通过；验证 `evidence-media` CSP/protocol registration/range fallback wiring，扫描禁止 `asset://localhost` / `convertFileSrc` 回退。
- `cargo test -p transport events`: 通过。
- `cargo clippy -p forensics-desktop --all-targets -- -D warnings`: 通过。
- `pnpm --dir frontend test -- events`: 通过，1 file / 1 test。
- `powershell -ExecutionPolicy Bypass -File scripts\check-command-sql-boundary.ps1`: 通过。
- `cargo test -p reports`: 通过。
- `cargo test -p testing`: 通过。
- `pnpm --dir frontend test -- Settings`: 通过，3 files / 11 tests。
- `pnpm --dir frontend typecheck`: 通过。
- `pnpm --dir frontend lint`: 通过，0 error / 0 warnings；已清理 MCP、DenseDataTable、SyntaxHighlighter、tauri-bridge 的既有 lint warning。
- `cargo deny check advisories bans licenses sources`: 通过。
- `powershell -ExecutionPolicy Bypass -File scripts\check-deny-exceptions.ps1`: 通过。
- `powershell -ExecutionPolicy Bypass -File scripts\check-evtx-dependency-decision.ps1`: 通过。
- `cargo audit`: 通过，仍报告 19 个 warning-class transitive advisories，短期由 `deny.toml` 例外追踪；`RUSTSEC-2021-0153` 已移除。
- `pnpm --dir frontend audit --audit-level high`: 通过；Vite/Vitest 安全升级后无 high/critical 漏洞。
- Final gate 复核 `pnpm --dir frontend test`: 通过，16 个测试文件 / 59 个测试；新增 route lazy-split 防回退测试；ErrorBoundary 用例打印预期 jsdom stack，退出码为 0。
- Final gate 统一 grep 复核：伪取证事实命中仅存在于审计文档或负向测试；页面/feature 无 direct Tauri `invoke`；新增事件 payload 不含裸 evidence host path。
- FS helper 去重 targeted tests：`cargo test -p evidence-core filesystem`、`cargo test -p fs-fat`、`cargo test -p fs-exfat`、`cargo test -p fs-ntfs`、`cargo test -p app-services file_service`、`cargo test -p app-services --test file_service_real_test` 均通过；`cargo clippy --workspace --all-targets -- -D warnings` 通过。
- Tiny E01 fixture targeted tests：`powershell -ExecutionPolicy Bypass -File scripts\generate-tiny-fixtures.ps1` 通过，生成 `testdata/fixtures/tiny/e01/tiny.E01` 4405 bytes；`cargo test -p image-e01` 通过，4 个默认 regression tests + 5 个 ignored real-E01 slow tests；`cargo test -p testing` 通过，4 个 fixture helper tests；`cargo clippy -p image-e01 --all-targets -- -D warnings` 通过。
- Tiny EVTX fixture targeted tests：`cargo test -p artifacts-windows evtx` 通过，real `System.evtx` fixture 能解析出 6005/6006/6008/1074 boot/shutdown candidate；`cargo test -p testing tiny_system_evtx` 通过，验证 fixture 存在、大小小于 2MB 且 EVTX header 正确。
- Coverage report targeted tests：`pnpm --dir frontend test:coverage` 通过，生成 `frontend/coverage/coverage-summary.json` 和 `lcov.info`，并通过前端初始 coverage 阈值；`powershell -ExecutionPolicy Bypass -File scripts\run-coverage.ps1 -Frontend` 通过，验证统一 coverage runner 的前端路径。
- FS path component helper targeted tests：`cargo test -p evidence-core filesystem`、`cargo test -p fs-fat`、`cargo test -p fs-exfat`、`cargo test -p fs-ntfs`、`cargo test -p app-services file_service`、`cargo test -p app-services --test file_service_real_test`、`cargo clippy -p evidence-core -p fs-fat -p fs-exfat -p fs-ntfs --all-targets -- -D warnings`、`cargo fmt --all -- --check` 均通过。

## Final Gate 结果

可信 beta 收口后默认门禁已执行：

```bash
cargo fmt --all -- --check                         # 通过
cargo clippy --workspace --all-targets -- -D warnings # 通过
cargo test --workspace                             # 通过；真实 E01 tests ignored
pnpm --dir frontend typecheck                      # 通过
pnpm --dir frontend lint                           # 通过；0 error / 0 warnings
pnpm --dir frontend test                           # 通过；16 files / 59 tests
pnpm --dir frontend build                          # 通过；route-level lazy split 后无 chunk size warning，主 chunk 约 315KB
cargo build -p forensics-desktop                   # 通过
powershell -ExecutionPolicy Bypass -File scripts\check-release-guard.ps1 # 通过
powershell -ExecutionPolicy Bypass -File scripts\check-command-sql-boundary.ps1 # 通过
cargo doc --workspace --no-deps                    # 通过
powershell -ExecutionPolicy Bypass -File scripts\check-deny-exceptions.ps1 # 通过
cargo deny check advisories bans licenses sources  # 通过；duplicate dependency warnings
cargo audit                                        # 通过；20 个 warning-class advisories reported
pnpm --dir frontend audit --audit-level high       # 通过；No known vulnerabilities
pnpm --dir frontend test:coverage                  # 通过；生成 coverage report，并执行前端初始阈值
```

Slow E01 保持手动验收，且现在都需要 `--ignored` 和 `FORENSICS_E01_FIXTURE`：

```bash
cargo test -p app-services --test e01_full_pipeline_test -- --ignored --nocapture
cargo test -p app-services --test e01_mft_scan_test -- --ignored --nocapture
```

## 后续优先级

1. 深化 Registry parser 覆盖更多 hive cell/list 变体，并补真实 fixture；当前已修正 value-list cell 语义和数值长度边界，但仍只承诺 Analysis 所需定向字段。
2. 做 Tauri 桌面 manual smoke，确认 `evidence-media://` 在 Windows WebView2 中对 `<video>/<audio>` seek 行为稳定；CI 已有 media protocol guard，但不替代真实播放器 seek 验证。
3. 若需要真实 E01 分区/文件系统验收，继续使用 `FORENSICS_E01_FIXTURE` 手动慢测；当前提交的 tiny E01 只覆盖 reader 层 section/table/read/seek，不替代真实样本。
4. 逐步消除 `cargo audit` 剩余 19 个 warning-class transitive advisories；`evtx -> encoding` 已通过 `crates/evtx-patched` + `encoding_rs` 移除，后续只需维护 local fork、跟踪上游 maintained release，并继续防止 `encoding` 回归。
5. 继续积累 coverage baseline 并逐步抬高前端阈值；后端 coverage 目前仍只上传 LCOV artifact，不以百分比阻塞默认门禁。
6. 继续收口 FS 枚举流程和 FS 特定错误转换去重；当前公共 root/path/path-components/error helper 已落地。Command layer SQL 已通过 guard 防回退，后续若有 repository/service SQL 行为变更仍需补 targeted tests。
