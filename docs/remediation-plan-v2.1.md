# Forensics Workbench 剩余 v2.1 Backlog 状态与修补方案

**更新时间**: 2026-06-01 23:23:29 +08:00  
**署名**: Codex  
**基线**: v2 默认门禁已通过；本文件记录 v2.1 backlog 的当前实现状态、部分完成项和剩余修补方案。

## Summary

v2.1 的目标是把内部 alpha 推进到可信 beta。当前本轮已继续补齐一批低风险剩余项：后端 Settings 配置 API、媒体 scoped handle + bounded range command、`artifact-added` 事件接线、frontend workflow 路径修正，并增加对应 targeted tests。真实 Registry/EVTX parser、完整媒体 streaming protocol、formal CycloneDX/SPDX SBOM、`cargo deny`、小型 E01 fixture 策略仍保留为后续工作。

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
| 2.1 Registry parser 接入 | Remaining | 当前系统信息仍为 `notParsed`；下一步应在 `artifacts-windows` 或 analysis adapter 中读取 `SYSTEM` / `SOFTWARE` hive fixture，输出 hostname/version/timezone + provenance |
| 2.2 EVTX parser 策略 | Remaining | 当前 boot/shutdown 不伪造；下一步接入 EVTX fixture 或保留明确 `notParsed` stub |
| 2.3 Analysis provenance | Partial | DTO 已有 `status/warnings`；仍需按每组结果补 data source id、artifact path、parser 名称、解析时间等 provenance 字段 |
| 2.4 Analysis 分类读取一致性 | Completed | classify 走 `FileEntryId + DataSourceKind` helper，仅读取 bounded header |
| 2.5 Markdown summary 修正 | Completed | summary 只基于 parsed/unknown 状态，不输出 `FORENSICS-PC`、`Windows 10` 等伪值 |
| 2.6 UI parser 状态展示 | Completed | DataAnalysis 展示状态、warnings 和空字段“未解析”/`-`，无 active case 有空态 |

## Phase 3: 证据预览、文件动作与调查流闭环

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 3.1 大媒体 scoped streaming | Partial | 本轮 `get_media_url` 对大媒体返回 opaque `handleId + canReadRanges`，新增 `read_media_range` bounded command；前端用 handle 读取首个 chunk 生成 blob 预览。完整 HTTP/Tauri protocol style continuous streaming 仍是后续增强 |
| 3.2 Image preview 限制 | Completed | 小图 data URL，超限 typed error/fallback；不再经 hex lines 反解大图 |
| 3.3 Extract file | Completed | 新增 `extract_file` command + save dialog wrapper，后端通过 reader helper 写出，destination 拒绝 device path/目录 |
| 3.4 FileBrowser -> Timeline | Completed | “在时间线中查看”设置 selection 并导航 `/timeline` |
| 3.5 Timeline -> Source | Completed | Timeline 源对象按钮跳 Files/Artifacts 并设置 selection |
| 3.6 Timeline filters UI | Completed | 增加 timeStart/timeEnd/eventType 控件，hook query key 包含 filters |
| 3.7 Search saved queries | Completed | `localStorage` 保存/覆盖/删除/执行查询，新增 tests |
| 3.8 Settings editable config | Partial | 本轮新增 `get_app_settings` / `save_app_settings` Tauri commands，路径类设置经后端验证后写入 config；theme/dev toggle 仍同步写 localStorage 以便 mock/dev 即时生效 |

## Phase 4: 事件、任务状态与报告可信度

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 4.1 Typed event contract | Completed | `EventTopic` enum 收口 topic，unknown topic serde 拒绝；前端 `EventTopic` 同步 |
| 4.2 Event bridge 扩展 | Partial | 已有 case/job/artifact/timeline/search/partition emit helpers；本轮将 `artifact-added` 接入 post-import artifact store 后循环 emit。更多 artifact/search 细粒度进度仍可继续细化 |
| 4.3 Targeted emit | Completed | event bridge 使用 `emit_to("main", ...)`，payload 不含裸证据绝对路径 |
| 4.4 Capability 对齐 | Completed | `capabilities/default.json` 已包含 `core:event:default` 与保存对话框能力 |
| 4.5 Job partial/warning 语义 | Partial | import/search/artifact 已有 warnings/skipped 基础记录，但 job 状态模型仍需统一 `warning/skipped/failed count` 并在 UI 展示 partial success |
| 4.6 Reports provenance | Partial | v2 已保留 HTML/CSV escaping；报告中还需系统性加入 parser status、warnings、evidence provenance 与 analysis summary |

## Phase 5: CI、测试资产与依赖治理

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 5.1 Workflow 去重 | Completed | 保留 `ci-backend.yml`、`ci-frontend.yml`；旧重复 workflow 删除；frontend workflow 已修正为 `pnpm --dir frontend ...` |
| 5.2 Dependency gate | Partial | backend 有 `cargo audit`，frontend 有 `pnpm audit --audit-level high`；仍需新增 `cargo deny` 配置和带到期日期的例外机制 |
| 5.3 DevTools guard | Completed | 新增 `scripts/check-release-guard.ps1`，CI backend 执行 release guard |
| 5.4 SBOM | Partial | 当前生成 lightweight `cargo metadata` / `pnpm list` JSON artifacts；正式 CycloneDX/SPDX SBOM 仍需替换 |
| 5.5 Fixture 策略 | Partial | 默认测试不依赖私有 E01；真实 E01 slow test ignored 并可手动运行。仍需仓库内 tiny logical/RAW/E01 或生成脚本 |
| 5.6 覆盖率扩展 | Partial | 已补 transport/file/settings/search/timeline/analysis 等 targeted tests；仍未严格完成“后端 +20 / 前端 +15 全覆盖 backlog”的量化目标审计 |

## Phase 6: 代码质量与文档收尾

| Task | 状态 | 当前结果 / 剩余动作 |
|---|---|---|
| 6.1 FS enum 去重 | Remaining | NTFS/FAT/exFAT 共用枚举/错误转换仍可抽出，需保持 public API 不破坏 |
| 6.2 SQL 下沉 repo | Partial | 新增命令遵守 validate -> service/helper -> DTO；既有 command layer 残留 SQL 仍需逐步下沉 |
| 6.3 常量集中 | Completed | preview max、artifact max、pagination max、analysis sample max 等关键限制已集中到 transport/infrastructure 常量 |
| 6.4 Public docs | Partial | touched public DTO/commands 有基础注释；仍需跑 `cargo doc --workspace --no-deps` 并处理 warnings |
| 6.5 开发记录与修复计划更新 | Completed | 本文件和 `development-reports/sessions/2026-06-01.md` 已更新；architecture/complexity 同步本轮状态 |

## 本轮新增验证

- `pnpm --dir frontend test -- settings files`: 通过，3 个文件 / 15 个测试。
- `cargo test -p transport`: 通过，26 个测试。
- `cargo test -p forensics-desktop commands::settings_commands`: 通过，2 个测试。
- `cargo test -p forensics-desktop commands::file_commands::tests::oversized_media_preview_returns_scoped_handle_and_range_reads`: 通过。
- `cargo check -p forensics-desktop`: 通过。
- `pnpm --dir frontend typecheck`: 通过。

## Final Gate 待跑

完整默认门禁仍需在本轮收尾执行并记录：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir frontend typecheck
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend build
cargo build -p forensics-desktop
```

Slow E01 仍保持手动验收：

```bash
cargo test -p app-services --test e01_full_pipeline_test -- --ignored --nocapture
cargo test -p app-services --test e01_mft_scan_test -- --nocapture
```

## 后续优先级

1. 接 Registry hive fixture parser，给 Analysis system info 加字段级 provenance。
2. 接 EVTX boot/shutdown parser 或正式 notParsed adapter，保留“不伪造事实”约束。
3. 将大媒体预览升级为真正 continuous streaming protocol 或 seek-aware command abstraction。
4. 引入 `cargo deny` 与 CycloneDX/SPDX SBOM。
5. 建仓库内 tiny RAW/E01 fixture 或生成脚本，替代私有路径慢测依赖。
6. 统一 job partial/warning 语义并让 Reports 输出 provenance。
