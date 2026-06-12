# 文件树根节点与排序修复暂停记录（2026-06-11）

> **恢复进展（2026-06-11，第二轮）：**
> 第 6 节恢复顺序中的「排序闭环」已落地并通过验证：
> - 后端 `file_service` 已实现服务层统一比较器（目录优先 → 状态后置 → 指定键含方向 → 名称自然排序兜底）。
> - `get_file_rows_for_request` 改为「取完整可见集合 → 服务层排序 → 再分页切片」。
> - `get_file_children_lazy_with_visibility` 与根树 `get_file_tree_real_with_visibility` 改为目录集合统一自然排序后再切片。
> - 前端 `sortKey/sortDirection` 已从 `FileBrowser` → `hooks` → `api/files.ts` 真实接线（含 query key 与排序变化回到首页），并删除页内二次排序；`file-sort.ts` 改为与后端同构、仅用于 mock 与兜底。
> - 测试：新增后端比较器单测、`file-sort.test.ts`，修正既有集成测试与 `FileBrowser.test.tsx`。
> - 已验证：`cargo test -p app-services`（200 通过）、`cargo clippy -p app-services -p forensics-desktop --all-targets -D warnings`、`pnpm typecheck/lint/test`（197 通过）、`pnpm build`、command-SQL-boundary guard。
>
> **恢复进展（2026-06-11，第三轮）：** 第 5 节风险点「MFT/staging 裸根折叠」已按四阶段方案落地并通过验证：
> - **Stage A（身份绑定地基）：** placeholder root 的 path 改为 `__partition_placeholder__/{index}/{status}`，`insert_partition_placeholder_root` 增加 `partition_index` 入参（两处 pipeline 调用已传入）；merge 改为 `find_partition_placeholder_root_id_by_index`，**删除按名字匹配与「同数据源第一个 placeholder」回退**；解析器向后兼容旧 `{status}`-only 历史值。
> - **Stage B（根折叠收口）：** merge SQL 的 staging 根识别从「`name IN ('\\','/','.')`」扩展为「(NULL-parent 或 自引用) 且 marker 名」统一折叠；NULL-parent 真实顶层目录（FAT `EFI`）改为 reparent 保留而非裸插；无 placeholder 时在 merge 事务内合成分区根（与合并同原子，回滚一致），杜绝裸 `\`/`EFI` 入主库首层。
> - **Stage C（读侧防御归一）：** `get_file_tree_real_with_visibility` 对残留裸根按 MFT 实体 ID（`mft:{idx}:5`）或唯一分区记录重命名为 `分区x（…）`，无法归属时标 `UNKNOWN` 而非透出原名；新增 `looks_like_raw_fs_root_name` 集中判定；最终复核后补上按 data source 批量预取 partitions，避免裸根兜底路径 per-root 查询。
> - **Stage D（回归与产物）：** 新增免外部 fixture 的端到端测试 `partition_root_folding_e2e_test`（seed 双分区 staging → 真实 merge → 断言树首层只有分区根、无裸 `\`/`EFI`、幂等）。
> - 测试矩阵 T-A1~A4 / T-B1~B5 / T-C1~C3 / T-D1~D2 全部落地通过。
> - 已验证：`cargo test --workspace`（全绿，含新 e2e）、`cargo clippy --workspace --all-targets -D warnings`、`pnpm typecheck/lint/test/build`（lint 0 error/2 pre-existing warnings，test 197 通过）、command-SQL-boundary guard、stage5 regression guard、`cargo build -p forensics-desktop --release`。
>
> **仍未执行/未通过环境项：** `cargo tauri build` 未完成，当前环境缺少 `cargo-tauri` 子命令（`error: no such command: tauri`）；已用 `cargo build -p forensics-desktop --release` 覆盖桌面 Rust release crate 构建。未构建新的 `demo.exe` 产物。

## 1. 当前目标

本轮目标有两件事：

1. 让文件树真实链路稳定显示 `分区x（NTFS/FAT/EXFAT/RECOVERY/BITLOCKER/UNKNOWN）`，不再把真实文件系统根 `\` 或顶层目录如 `EFI` 直接暴露成首层树根。
2. 让文件浏览排序更接近 Windows Explorer 直觉，并将 `hidden/system/deleted` 统一后置，保证列表与树的懒加载链路都能接住。

当前任务已暂停，尚未完成测试、构建和 demo 产物验证。

---

## 2. 已确认的事实

### 2.1 文件树首层没有吃到前端改动的真实根因

问题不是单纯前端 formatter 没生效，而是后端真实数据结构还不稳定：

- 并行导入链路里，`placeholder root` 的插入、manifest 中的分区根名、merge 时查找 `placeholder root` 的名字来源没有完全统一。
- 这会导致 merge 后主库顶层出现混合根：
  - 顶层目录如 `EFI`
  - NTFS MFT 根 `\`
  - 其他 `parent_id IS NULL` 的直接节点
- 当前截图现象与这一链路一致：左树首层仍然出现 `EFI` 和裸 `\`。

### 2.2 文件列表排序仍然没有真实接入

当前排序仍未完全收口到真实链路：

- 后端 repo 查询仍主要依赖简单 SQL 排序：
  - `ORDER BY entry_type ASC, name ASC`
  - `ORDER BY name ASC`
- 前端 `frontend/src/lib/file-sort.ts` 仍是本地简单排序，尚未完全具备：
  - `hidden/system/deleted` 后置
  - Windows 风格自然排序
  - 与分页一致的真实链路排序保证

---

## 3. 本轮已落地的修改

> 下面只列本轮确认并实际落地的修改，不把工作区中其他既有未提交改动混在一起。

### 3.1 并行导入插入 placeholder root 时统一分区根命名

文件：

- `apps/desktop/src-tauri/src/commands/import/pipeline.rs`

已落地内容：

- 在 probe 完成后，先从 `probe.candidates` 生成 `partition_index -> format_partition_root_name(candidate)` 的统一映射。
- 插入 placeholder root 时：
  - 优先使用 candidate 对应的统一根名
  - 回退时才使用 `format_partition_record_root_name(partition)`

目的：

- 避免“主库插入的 placeholder root 名字”和“manifest/merge 使用的分区根名”不一致。
- 这是导致首层树根无法稳定挂回的重要原因之一。

### 3.2 merge 查找 placeholder root 增加兜底回退

文件：

- `crates/app-services/src/staging.rs`

已落地内容：

- `find_partition_placeholder_root_id(...)` 现在先按：
  - `data_source_id`
  - `parent_id IS NULL`
  - `name = partition_name`
  - `path GLOB '__partition_placeholder__/*'`
  精确查找。
- 若精确查找不到，则临时回退为：
  - 在同一 `data_source_id` 下查找任意首层 placeholder root。

目的：

- 在当前工作区尚未把“分区根命名事实源”彻底统一前，先给 merge 一层防御，减少继续把 `EFI` / `\` 暴露到首层的概率。

注意：

- 这个回退策略是暂停前的兜底，不是最终最优设计。
- 最终更稳妥的做法仍应基于“分区索引/分区身份”绑定 placeholder root，而不是依赖名字匹配或“同数据源第一个 placeholder”。

---

## 4. 已审阅但尚未完成实现的相关工作区改动

以下文件已经审阅，且与本次问题直接相关，但本轮尚未完成最终收口：

- `crates/transport/src/commands/mod.rs`
  - 已存在 `GetFileRowsRequest.sort_key`
  - 已存在 `GetFileRowsRequest.sort_direction`
- `crates/persistence-sqlite/src/repositories/file_repo.rs`
  - 已存在 `find_children_visible(...)`
  - 已存在 `find_root_entries_visible(...)`
  - 已存在 `find_child_directories_visible(...)`
  - 但 `file_service` 还没完全切到这些接口做统一排序后分页
- `crates/app-services/src/file_service/mod.rs`
  - 仍主要走 repo 直接分页查询
  - 尚未落地“目录优先 + 状态后置 + 自然排序”的服务层统一比较器
- `frontend/src/lib/api/files.ts`
  - 尚未把 `sortKey/sortDirection` 真实传给 Tauri
- `frontend/src/features/files/hooks.ts`
  - query key 尚未完整接入排序参数
- `frontend/src/app/pages/FileBrowser.tsx`
  - 仍对真实链路返回的 `rows` 做本地 `sortFileEntries(...)`
  - 容易与后端分页排序形成双排序或页内排序假象
- `frontend/src/lib/file-sort.ts`
  - 仍是旧版简单排序
- `frontend/src/lib/api/provider.ts`
  - mock 仍然是旧根模型与旧排序路径
- `frontend/src/lib/api/mock-data.ts`
  - 当前 mock 树根还是 `System32`，与真实“分区根模型”不一致
- `frontend/src/app/pages/FileBrowser.test.tsx`
  - 现有测试能覆盖隐藏/删除图标与分区名显示
  - 但还未覆盖真实链路排序一致性

---

## 5. 当前客观评估

### 已经解决到什么程度

- 根因已经基本定位清楚，不再是“前端没渲染”的模糊状态。
- 已经对“分区根命名不一致”这个关键断点落了两刀：
  - 插入时统一根名
  - merge 时加防御性回退

### 还没有解决的部分

- 这还不是完整闭环。
- 当前还没有完成以下关键收口：
  1. `file_service` 读取侧对坏根形态的统一折叠
  2. 后端主排序 + 前端同构兜底排序
  3. 排序参数从前端到 Tauri 的真实接线
  4. 针对性测试与构建验证

### 风险判断

- 目前的 `placeholder root` 回退查找是“先止血”的兜底，不是最终形态。
- 如果一个数据源下存在多个 placeholder root，而精确名称匹配仍失败，则“回退到任意 placeholder root”有潜在误挂接风险。
- 因此恢复工作后，建议优先把“基于分区索引/身份绑定 placeholder root”的方案做实，而不是长期依赖名字与兜底回退。

---

## 6. 建议的恢复顺序

恢复后建议按下面顺序继续：

1. 完成 `file_service` 的服务层排序比较器
   - 目录优先
   - 状态后置：
     - 正常
     - hidden/system
     - deleted
     - hidden/system + deleted
   - 名称自然排序
2. 将 `get_file_rows_for_request(...)` 切到：
   - 先取完整可见集合
   - 服务层统一排序
   - 再分页切片
3. 将 `get_file_children_lazy_with_visibility(...)` 切到：
   - 目录集合统一排序
   - 再分页切片
4. 前端接通 `sortKey/sortDirection`
   - `api/files.ts`
   - `features/files/hooks.ts`
   - `FileBrowser.tsx`
5. 将前端 `sortFileEntries(...)` 改成仅用于：
   - mock mode
   - 极小范围展示兜底
6. 补定向测试：
   - 根节点折叠
   - `file2 < file10`
   - hidden/system/deleted 后置
   - 分页一致性
7. 再跑：
   - `cargo test`
   - `pnpm test`
   - `pnpm build`
   - `cargo tauri build`

---

## 7. 本轮涉及的文件

### 本轮实际新落地修改

- `apps/desktop/src-tauri/src/commands/import/pipeline.rs`
- `crates/app-services/src/staging.rs`

### 本轮重点审阅但未完成修改的相关文件

- `crates/transport/src/commands/mod.rs`
- `crates/persistence-sqlite/src/repositories/file_repo.rs`
- `crates/app-services/src/file_service/mod.rs`
- `frontend/src/lib/api/files.ts`
- `frontend/src/features/files/hooks.ts`
- `frontend/src/app/pages/FileBrowser.tsx`
- `frontend/src/lib/file-sort.ts`
- `frontend/src/lib/api/provider.ts`
- `frontend/src/lib/api/mock-data.ts`
- `frontend/src/app/pages/FileBrowser.test.tsx`
- `frontend/src/lib/api/files.test.ts`

---

## 8. 未执行项

截至暂停时，以下动作尚未执行：

- 未运行 `cargo test`
- 未运行 `pnpm test`
- 未运行 `pnpm build`
- 未运行 `cargo tauri build`
- 未构建新的 `demo.exe`

---

## 9. 备注

- 当前工作区本身是 dirty tree，存在大量与本议题并行的未提交改动。
- 本记录只聚焦本轮“文件树根节点与 Windows 风格排序修复”相关内容，不代表整个工作区的全部变更。
