# Forensics Workbench 剩余 v2.1 Backlog 状态与修补方案

**更新时间**: 2026-06-02 03:10:13 +08:00
**署名**: Codex  
**基线**: `e7ffa35` -> `codex/beta-forensics-backlog`；本文件记录 v2.1 backlog 在可信 beta 收口实现后的当前状态、验收命令和剩余修补方案。

## Summary

v2.1 的目标是把内部 alpha 推进到可信 beta。当前已补齐 Analysis provenance 契约、Reports analysis provenance、Job partial/warning/skipped/failed 语义、媒体 scoped handle + bounded range command、`evidence-media` Tauri protocol、Registry 定向字段 parser、EVTX boot/shutdown candidate adapter、`cargo deny`/`cargo audit`/`pnpm audit` 依赖门禁、CycloneDX SBOM CI artifact、tiny logical/RAW fixtures 和前端 Vite/Vitest 安全升级。

剩余风险集中在解析覆盖和架构债：Registry parser 是定向字段解析器，不是完整 hive browser；EVTX adapter 已接入 `evtx` crate 并解析 6005/6006/6008/1074 候选事件，但仓库内尚无合法 tiny real `.evtx` fixture。补充复核确认 `evtx 0.11.2` crates.io 发布包通过 `exclude = ["**/*.evtx", "**/*.dat"]` 排除了真实 EVTX 样本，且 `encoding = "0.2.33"` 是直接非可选依赖，不能通过关闭 feature 消除 `RUSTSEC-2021-0153`；当前通过 `deny.toml` 临时例外跟踪。tiny E01 fixture 尚未入库；FS enum 去重和 command layer SQL 下沉仍是后续质量项。

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
| 2.1 Registry parser 接入 | Completed / targeted | Analysis 会从 catalog 查找 `Windows/System32/config/SYSTEM` / `SOFTWARE`，通过 `FileEntryId` reader bounded 读取最多 64MB hive；`artifacts-windows::registry::lookup` 支持 `regf` base block、NK/VK、`lf/lh/li/ri` 子键列表和常用值类型，并提取 `ComputerName`、timezone、ProductName、CurrentBuild、InstallDate、RegisteredOwner、Organization、ProductId。字段带 `fieldProvenance`；缺失/损坏/超限为 warning，不生成默认 Windows 文案。剩余：不是完整 Registry browser |
| 2.2 EVTX parser 策略 | Completed / fixture gap | Analysis 查找 `Windows/System32/winevt/Logs/System.evtx`，通过 `evtx.boot_shutdown` adapter bounded 读取最多 64MB，解析 6005、6006、6008、1074 为候选事件，boot record 带 `eventId`、`recordId`、note 和 provenance。无 EVTX、损坏或超限时保持 `notParsed/unavailable + warnings`。剩余：仓库内尚无合法 tiny real EVTX fixture；当前单测覆盖 JSON 记录提取、malformed/oversized 路径，并新增 truncated EVTX magic 输入的 warning/not-panic 回归。复核确认 `evtx 0.11.2` crates.io 包排除 `*.evtx` fixture，不能直接复用其上游测试样本 |
| 2.3 Analysis provenance | Completed | 新增 `AnalysisProvenanceDto { dataSourceId, artifactPath, parser, parsedAt, status, warnings }`；system info、boot records、file classification 和 classified file 均可携带 provenance |
| 2.4 Analysis 分类读取一致性 | Completed | classify 走 `FileEntryId + DataSourceKind` helper，仅读取 bounded header |
| 2.5 Markdown summary 修正 | Completed | summary 只基于 parsed/unknown 状态，不输出 `FORENSICS-PC`、`Windows 10` 等伪值 |
| 2.6 UI parser 状态展示 | Completed | DataAnalysis 展示状态、warnings 和空字段“未解析”/`-`，无 active case 有空态 |

## Phase 3: 证据预览、文件动作与调查流闭环

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 3.1 大媒体 scoped streaming | Completed / smoke pending | `get_media_url` 对大媒体返回 `mode=protocol`、opaque `handleId` 和 `evidence-media://handle/<encoded>` URL；Tauri protocol handler 解析 Range、每次最多读取 1MB、返回 206/416 等标准状态，读取仍走 `open_file_content_by_id`。`read_media_range` command 保留为 mock/unsupported fallback；CSP 只给 `media-src` 增加 `evidence-media:`，不暴露 evidence 宿主绝对路径。剩余：尚未做真实桌面播放器 smoke |
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
| 5.2 Dependency gate | Completed | 后端 CI 保留 `cargo audit` 并新增 `cargo deny check advisories bans licenses sources`；`deny.toml` 对短期 transitive advisories 带 reason/owner/expiry，本轮新增 `scripts/check-deny-exceptions.ps1` 并接入 backend CI，强制 advisory 例外必须包含 owner、expires 和说明文本且不得过期；`RUSTSEC-2021-0153` 例外用于 `evtx -> encoding` transitive unmaintained advisory，expires 2026-09-01；前端 CI 执行 `pnpm --dir frontend audit --audit-level high` |
| 5.3 DevTools guard | Completed | 新增 `scripts/check-release-guard.ps1`，CI backend 执行 release guard |
| 5.4 SBOM | Completed | 后端 CI 使用 `cargo-cyclonedx` 生成 `backend-sbom` artifact；前端 CI 使用 `@cyclonedx/cyclonedx-npm` 生成 `frontend-sbom` artifact，并验证 `bomFormat=CycloneDX` |
| 5.5 Fixture 策略 | Partial | 新增仓库内 tiny logical directory 和 1024-byte tiny RAW fixture、生成脚本与 `crates/testing` helper；真实 E01 测试改为 `FORENSICS_E01_FIXTURE` opt-in ignored slow tests。tiny E01 fixture 尚未入库 |
| 5.6 覆盖率扩展 | Partial | 已补 Analysis、Reports、Viewer、Jobs、Fixture、CI/dependency 相关 targeted tests；仍未建立自动覆盖率指标或严格计数门槛 |

## Phase 6: 代码质量与文档收尾

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 6.1 FS enum 去重 | Remaining | NTFS/FAT/exFAT 共用枚举/错误转换仍可抽出，需保持 public API 不破坏 |
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
- `cargo test -p artifacts-windows evtx`: 通过；新增 truncated EVTX magic warning/not-panic 回归。
- `cargo test -p transport analysis`: 通过。
- `cargo test -p forensics-desktop media_protocol`: 通过。
- `cargo test -p transport events`: 通过。
- `cargo clippy -p forensics-desktop --all-targets -- -D warnings`: 通过。
- `pnpm --dir frontend test -- events`: 通过，1 file / 1 test。
- `powershell -ExecutionPolicy Bypass -File scripts\check-command-sql-boundary.ps1`: 通过。
- `cargo test -p reports`: 通过。
- `cargo test -p testing`: 通过。
- `pnpm --dir frontend test -- Settings`: 通过，3 files / 11 tests。
- `pnpm --dir frontend typecheck`: 通过。
- `pnpm --dir frontend lint`: 通过，0 error / 7 existing warnings。
- `cargo deny check advisories bans licenses sources`: 通过。
- `powershell -ExecutionPolicy Bypass -File scripts\check-deny-exceptions.ps1`: 通过。
- `cargo audit`: 通过，仍报告 warning-class transitive advisories，短期由 `deny.toml` 例外追踪。
- `pnpm --dir frontend audit --audit-level high`: 通过；Vite/Vitest 安全升级后无 high/critical 漏洞。

## Final Gate 结果

可信 beta 收口后默认门禁已执行：

```bash
cargo fmt --all -- --check                         # 通过
cargo clippy --workspace --all-targets -- -D warnings # 通过
cargo test --workspace                             # 通过；真实 E01 tests ignored
pnpm --dir frontend typecheck                      # 通过
pnpm --dir frontend lint                           # 通过；0 error / 7 existing warnings
pnpm --dir frontend test                           # 通过；13 files / 53 tests
pnpm --dir frontend build                          # 通过；Vite chunk size warning
cargo build -p forensics-desktop                   # 通过
powershell -ExecutionPolicy Bypass -File scripts\check-release-guard.ps1 # 通过
powershell -ExecutionPolicy Bypass -File scripts\check-command-sql-boundary.ps1 # 通过
cargo doc --workspace --no-deps                    # 通过
powershell -ExecutionPolicy Bypass -File scripts\check-deny-exceptions.ps1 # 通过
cargo deny check advisories bans licenses sources  # 通过；duplicate dependency warnings
cargo audit                                        # 通过；warning-class advisories reported
pnpm --dir frontend audit --audit-level high       # 通过；No known vulnerabilities
```

Slow E01 保持手动验收，且现在都需要 `--ignored` 和 `FORENSICS_E01_FIXTURE`：

```bash
cargo test -p app-services --test e01_full_pipeline_test -- --ignored --nocapture
cargo test -p app-services --test e01_mft_scan_test -- --ignored --nocapture
```

## 后续优先级

1. 为 EVTX adapter 增加合法 tiny real `.evtx` fixture，覆盖真实 parser path，不只覆盖 JSON extraction helper。已确认 `evtx 0.11.2` crates.io 包排除 `*.evtx`，需要自行生成、引入可再分发 fixture，或改用/维护其他 parser 路径。
2. 深化 Registry parser 覆盖更多 hive cell/list 变体，并补真实 fixture；当前只承诺 Analysis 所需定向字段。
3. 做 Tauri 桌面 smoke，确认 `evidence-media://` 在 Windows WebView2 中对 `<video>/<audio>` seek 行为稳定；fallback command 已保留。
4. 引入合法 tiny E01 fixture 或可生成 fixture，替代私有样本慢测依赖。
5. 逐步消除 `cargo audit` warning-class transitive advisories，尤其是本轮 `evtx -> encoding` 临时例外；`encoding` 是 `evtx 0.11.2` 的直接非可选依赖，需替换 parser、vendor/patch parser 或在 2026-09-01 前重新评审例外。
6. 收口 FS enum 去重与 command layer 残留业务 SQL 下沉。
