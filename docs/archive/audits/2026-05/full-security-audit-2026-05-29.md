# Forensics Workbench — 全量安全审计报告

> 归档：2026-05 审计快照，仅用于历史追溯，不代表当前安全状态。

**审计日期**: 2026-05-29  
**审计范围**: 全项目（Rust 后端 18 crates + Tauri 命令层 + React/TypeScript 前端 + 构建配置 + CI/CD）  
**项目版本**: 0.1.0 (v0.1.0, development stage)  
**审计方法**: 静态代码分析 + 6 并行 Agent 深度审计 + 人工交叉验证  

---

## 一、执行摘要

本次审计对 Forensics Workbench 的全部代码进行了系统性安全审查。项目整体安全架构设计合理 — Tauri 2 最小权限配置、全参数化 SQL 查询、结构化错误脱敏、路径遍历防护均到位。但在**二进制解析层**（处理不可信磁盘镜像）和**命令边界校验**上存在若干需优先修复的缺陷。

| 严重等级 | 数量 | 说明 |
|---------|------|------|
| **Critical** | 3 | 可导致任意目录删除、GPT 解析器 panic、E01 无限循环 |
| **High** | 5 | 路径穿越创建目录、任意文件导入、NTFS OOM、HTML XSS、路径遍历 |
| **Medium** | 12 | 内存无界分配、分页无上限、错误泄露、迁移非原子、CSV 注入等 |
| **Low** | 10 | LIKE 通配符、事件广播、日志泄露路径等 |
| **Info** | 6 | 积极发现（参数化查询全覆盖、路径防护到位等） |

---

## 二、关键发现（按严重等级）

### Critical

#### C-01: `delete_case` 可删除任意目录
- **文件**: `apps/desktop/src-tauri/src/commands/case_commands.rs:221-253` + `crates/app-services/src/case_service.rs:96-124`
- **问题**: 接受前端传入的 `case_root` 字符串，直接调用 `fs::remove_dir_all()`。唯一校验是目标目录存在 `case.json` — 任何包含该文件的目录均可被删除。
- **影响**: 完整数据丢失（任意目录）
- **修复**: 定义安全根目录（如 `%APPDATA%/ForensicsWorkbench/cases/`），`canonicalize()` + `starts_with()` 校验。

#### C-02: GPT 解析器 `entry_size` 未校验导致 panic
- **文件**: `crates/evidence-core/src/volume/gpt.rs:65`
- **问题**: `parse_gpt_entries` 中 `entry_size` 直接来自磁盘镜像头，当 `< 128` 时切片 `entry[56..128]` 越界，导致 panic。恶意构造的 GPT 可触发。
- **影响**: 程序崩溃，取证中断
- **修复**: 在切片前校验 `if entry_size < 128 { continue; }`

#### C-03: E01 section walker 无环检测 — 无限循环
- **文件**: `crates/image-e01/src/lib.rs` (section descriptor linked list walk)
- **问题**: `while next_off > 0 && next_off < file_len` 循环遍历 section 链表，但无 visited set。恶意 E01 可构造 `next` 指向已访问 section，造成死循环。
- **影响**: 程序挂起，取证中断
- **修复**: 添加 `HashSet<u64>` 记录已访问偏移，检测到重复即 break。

---

### High

#### H-01: `create_case` 名称未校验 — 路径穿越
- **文件**: `crates/app-services/src/case_service.rs:44-45`
- **问题**: `root.join(name)` 中 `name` 可含 `../../`，在预期根目录外创建目录 + SQLite 数据库。
- **修复**: 正则校验 `^[a-zA-Z0-9_ -]{1,100}$`，拒绝路径分隔符和 `..`。

#### H-02: `import_data_source` 无路径限制
- **文件**: `apps/desktop/src-tauri/src/commands/file_commands.rs:145-160`
- **问题**: 前端传入的 `source_path` 无校验，可用于探测/读取系统任意文件。
- **修复**: 使用 `tauri_plugin_dialog` 文件选择器，或限制到已注册的证据目录。

#### H-03: NTFS `read_file_data` 内存无界分配
- **文件**: `crates/fs-ntfs/src/lib.rs` (`read_file_data`)
- **问题**: 从 data run 计算总大小后直接 `vec![0u8; total_size]`，恶意 NTFS 可声称文件 2GB+。虽然 `open_file` 有 128MB 后置检查，但 OOM 在分配时已发生。
- **修复**: 在累积循环中检查 `if total > 128MB { return Err }`。

#### H-04: HTML 报告生成 XSS
- **文件**: `crates/reports/src/html/exporter.rs:18-30`
- **问题**: `case.name`、`case.number`、`case.examiner`、文件路径直接 `write!` 进 HTML，无 HTML 转义。恶意案例名可注入 `<script>`。
- **修复**: 对所有动态内容进行 HTML 实体编码（`<` → `&lt;` 等）。

#### H-05: 无 CSP（Content Security Policy）
- **文件**: `apps/desktop/src-tauri/tauri.conf.json`
- **问题**: `app.security` 完全缺失。WebView 可加载任意脚本/连接任意 URL。
- **修复**: 添加严格 CSP：`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost`

---

### Medium

| # | 发现 | 文件 | 说明 |
|---|------|------|------|
| M-01 | unsafe 生命周期转写 | `file_service.rs:915-917` | `&AtomicBool` 裸指针转 `'static`，应传 `Arc<AtomicBool>` 所有权 |
| M-02 | artifact 提取无大小限制 | `artifact_service.rs:30-33` | `read_to_end()` 无上限，多 GB 文件可 OOM |
| M-03 | 导入时全量内存收集 | `file_service.rs` (run_post_import_pipeline) | 所有 FileEntry 收集到单个 Vec，大镜像 OOM |
| M-04 | 分页 limit 无上限 | `transport/paging.rs:7` + `commands/mod.rs:87` | u32 无最大值校验，可请求 4B 行 |
| M-05 | `get_file_tree` 无分页 | `file_commands.rs:487-502` | 返回整棵树，大镜像内存爆炸 |
| M-06 | `From<String> for CommandError` | `transport/errors.rs:95-98` | 绕过 `from_service_error()` 脱敏，泄露内部信息 |
| M-07 | DTO 字段无输入校验 | `commands/mod.rs:13-27` | `case_root`、`source_path`、`ViewerRangeRequestDto.length` 无边界 |
| M-08 | 迁移非原子 | `persistence-sqlite/migrations/runner.rs:31-45` | `0010` 的 ALTER TABLE 非幂等，部分失败后 schema 损坏 |
| M-09 | CSV 公式注入 | `reports/csv/exporter.rs` | 双引号转义正确，但未防 `=`, `+`, `-`, `@` 开头的公式注入 |
| M-10 | DevTools 未显式守护 | `Cargo.toml:42` | 当前未启用，但无 CI 断言阻止意外添加 |
| M-11 | 缺少 `event:default` capability | `capabilities/default.json` | 前端 `listen()` 可能静默失败 |
| M-12 | CI 无依赖审计 | `.github/workflows/ci-backend.yml` | 无 `cargo audit`、无前端 CI、无 SBOM |

---

### Low

| # | 发现 | 文件 |
|---|------|------|
| L-01 | LIKE 通配符注入 | `file_repo.rs:145` — `%`/`_` 未转义 |
| L-02 | artifact_repo 硬编码空 case_id | `artifact_repo.rs:20-21` |
| L-03 | 事件广播到所有窗口 | `event_bridge.rs:8` — `app.emit()` 非定向 |
| L-04 | 日志泄露证据路径 | `main.rs:2-11` — stderr info 级别记录路径 |
| L-05 | Error Display 泄露路径/SQL | `connection.rs:8`, `case_service.rs:17` |
| L-06 | 事件 topic 无校验 | `events/mod.rs:16` — `EventEnvelope.topic` 为任意 String |
| L-07 | 手动 cascade + unchecked_transaction | `case_repo.rs:101`, `datasource_repo.rs:40` |
| L-08 | 无代码签名配置 | `tauri.conf.json` — bundle.active: false |
| L-09 | `tauri-plugin-fs` 传递依赖 | `Cargo.lock:4004` — 未初始化但存在于 lockfile |
| L-10 | mock 数据含硬编码证据路径 | `mock-data.ts` — `E:/evidence/FINCH-1.E01` 等 |

---

### Info（积极发现 ✅）

| # | 发现 | 说明 |
|---|------|------|
| I-01 ✅ | 零 SQL 注入 | 全部查询使用 `rusqlite::params![]` 参数化 |
| I-02 ✅ | 路径遍历防护到位 | `safe_relative_path()` + `canonicalize()` + `starts_with()` 教科书级实现 |
| I-03 ✅ | 零 unsafe 块（解析层） | 所有二进制解析代码无 unsafe（除 file_service.rs 一处） |
| I-04 ✅ | Tauri capabilities 最小化 | 仅 `core:default` + `dialog:default` + `dialog:allow-open` |
| I-05 ✅ | WAL + busy_timeout | SQLite 配置适合桌面单用户场景 |
| I-06 ✅ | 无硬编码密钥/凭证 | 全仓库无真实 secrets（仅测试 fixture 中的 "secret"/"credential" 字面量） |

---

## 三、攻击面分析

```
┌─────────────────────────────────────────────────────────────┐
│  前端 (React/TypeScript)                                     │
│  攻击面: 用户输入 → API 调用, 事件监听, 搜索查询              │
│  风险: 中 — XSS (报告渲染), 输入未校验传递后端                │
├─────────────────────────────────────────────────────────────┤
│  Tauri IPC 层                                                │
│  攻击面: 30+ 命令暴露, 文件路径参数, 分页参数                  │
│  风险: 高 — C-01 任意删除, H-01 路径穿越, M-4 分页 DoS        │
├─────────────────────────────────────────────────────────────┤
│  证据处理层 (解析不可信磁盘镜像)                               │
│  攻击面: E01/RAW 镜像, MBR/GPT 头, NTFS/FAT/exFAT 结构,     │
│          Prefetch/LNK/Registry 二进制格式                     │
│  风险: 高 — C-02 panic, C-03 死循环, H-03 OOM                │
├─────────────────────────────────────────────────────────────┤
│  持久化层 (SQLite)                                           │
│  攻击面: SQL 查询, 文件系统操作, 迁移脚本                     │
│  风险: 低 — 全参数化查询, FK 启用                            │
└─────────────────────────────────────────────────────────────┘
```

---

## 四、优先修复路线图

### 🔴 立即修复（Release 前必须）

1. **C-01**: `delete_case` 添加安全根目录沙箱
2. **C-02**: GPT `parse_gpt_entries` 添加 `entry_size >= 128` 校验
3. **C-03**: E01 section walker 添加 `HashSet<u64>` 环检测
4. **H-01**: `create_case` 名称正则校验
5. **H-02**: `import_data_source` 路径白名单或对话框限制
6. **H-04**: HTML 报告输出转义
7. **H-05**: 添加 CSP 配置

### 🟡 近期修复（Next Sprint）

8. **H-03**: NTFS read_file_data 添加 128MB 前置检查
9. **M-01**: 修复 unsafe 生命周期转写 → 传 Arc 所有权
10. **M-02/M-03**: 添加内存使用限制（artifact 50MB 上限，流式批处理）
11. **M-04/M-05**: 分页上限（limit ≤ 1000），废弃 get_file_tree
12. **M-06**: 移除 `From<String> for CommandError`
13. **M-07**: DTO 字段校验层
14. **M-08**: 迁移脚本添加 IF NOT EXISTS / 事务包裹
15. **M-09**: CSV 公式前缀净化
16. **M-12**: CI 添加 `cargo audit` + 前端构建检查

### 🟢 加固项（Hardening）

17. **L-01 ~ L-10**: 通配符转义、topic 校验、事件定向发送等
18. **M-10/M-11**: DevTools CI 守护、补充 event capability
19. 前端测试框架搭建（vitest + @testing-library/react）
20. 依赖版本锁定 + SBOM 生成

---

## 五、依赖安全

### Rust 关键依赖

| 依赖 | 版本 | 状态 |
|------|------|------|
| rusqlite | 0.31 (bundled) | ✅ 当前无已知 CVE |
| serde | ~1.0 | ✅ 稳定 |
| chrono | ~0.4 | ✅ 稳定 |
| tauri | ~2 | ✅ v2 当前维护版 |
| thiserror | ~2.0 | ✅ 稳定 |
| uuid | 1 | ✅ 稳定 |

### NPM 关键依赖

| 依赖 | 版本 | 状态 |
|------|------|------|
| react | 18.3.1 | ✅ 稳定 |
| vite | 6.3.5 | ✅ 当前版 |
| @tauri-apps/api | ^2.8.0 | ✅ 匹配 Tauri v2 |
| zustand | 5.0.8 | ✅ 稳定 |
| recharts | 2.15.2 | ✅ 稳定 |

**结论**: 未发现已知高危 CVE，但建议定期运行 `cargo audit` 和 `npm audit`。

---

## 六、与上次审计对比

上次审计 (2026-05-27) 的主要修复情况：

| 上次发现 | 状态 |
|---------|------|
| `create_file_reader_fn` 路径遍历 | ✅ 已修复 — `safe_relative_path()` + `canonicalize()` |
| 错误信息泄露 | ✅ 大部分修复 — `CommandError` 结构化 + `from_service_error()` |
| 近案例文件权限 | ⚠️ 仍为明文 JSON，无完整性保护 |
| tracing 日志框架 | ✅ 已引入 |
| 事件发射错误处理 | ⚠️ 仍为 `let _ = emit_event(...)` |

**新增发现**: C-01 (任意删除)、C-02/C-03 (解析器 crash/hang)、H-04 (XSS)、H-05 (无 CSP) 等为本次审计新发现。

---

## 七、总结

项目安全基础**扎实**：
- ✅ 零 SQL 注入（全参数化查询）
- ✅ 路径遍历防护教科书级实现
- ✅ 错误脱敏机制设计良好
- ✅ Tauri 最小权限配置
- ✅ 零硬编码密钥

最大风险集中在**处理不可信输入的边界**：
- 🔴 Tauri 命令层对用户输入的校验不足（C-01, H-01, H-02）
- 🔴 二进制解析器对恶意构造镜像的防御不足（C-02, C-03, H-03）
- 🟡 报告生成层未转义输出（H-04）和 WebView 缺少 CSP（H-05）

按优先级修复 Critical 和 High 项后，项目安全态势将达到发布标准。

---

*报告由 Codex Security Auditor 自动生成 — 2026-05-29T15:00:00+08:00*
