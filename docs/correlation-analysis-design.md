# 关联分析设计说明

## 1. 目标

V2 的关联分析不是自动定罪系统，而是把当前分散的 artifact、文件系统、时间线和调查结果提升为可追溯、可解释、可导出的 investigator 工作流。

## 2. 设计原则

- 仅输出线索，不输出黑盒结论
- 每条线索必须保留 provenance
- 每条关联必须能说明匹配依据和误判边界
- 同一对象在 timeline、artifact、report 中的描述保持一致

## 3. 统一模型

### 3.1 核心对象

- `correlation node`
  - 文件
  - artifact
  - timeline event
- `correlation edge`
  - `sourceReference`
  - `sharedSourceObject`
  - `temporalContext`
  - `pathMatch`
  - `nameMatch`
  - `recoveredOriginalPath`
- `cluster`
  - 围绕同一对象或同一事件聚合的节点与边
- `lead`
  - investigator 可消费的线索摘要
  - 需要额外携带 `matchSignals`，说明这条 lead 到底是由哪些命中依据组成

### 3.2 provenance

每个关联结果至少保留：

- source kind
- source record id
- source label
- producer / version
- guarantee level
- warning summary

### 3.3 confidence

V2 统一为四档：

- `Direct`
- `Strong`
- `Weak`
- `Heuristic`

## 4. 规则集

### 4.1 已落地规则

- `LNK.attrs.target_path / targetPath` → `FileEntry.path`
- `BrowserDownload.attrs.targetPath` → `FileEntry.path`
- `RegistryValue.attrs.data` → 文件路径 / 文件名
- `RecycleBin.attrs.original_path` → 已删除 `FileEntry.path`
- `Prefetch.attrs.executable` → `FileEntry.name`
- `EmailMessage.attrs.attachments[]` → `FileEntry.name`
- `JumpList.attrs.target_path / targetPath` → `FileEntry.path`
- 当规则命中的目标文件本身存在 timeline 事件时，额外挂接 `TemporalContext` 作为辅助上下文
- BrowserDownload 这类带时间戳的规则命中，若 24 小时窗口内存在同路径 timeline，也会追加“邻近时间线命中”信号
- EmailMessage 现在也会利用 `sentAt + subject / attachments` 与邻近 timeline 做辅助命中
- BrowserHistory 现在也会利用 `visitTime + url / title` 与邻近 timeline 做辅助命中

### 4.2 仍保留的基础聚合

- Artifact → File
  - 依据 `ArtifactRowDto.sourceObjectId`
- Timeline → File
  - 依据 `TimelineEventDto.sourceObjectId`
- Artifact → Timeline
  - 依据共享 `sourceObjectId`

### 4.3 当前未完成项

- Prefetch 仍是 basename 级匹配，不是完整执行路径匹配
- Registry 仍主要依赖 `data` 字段字符串抽取，不区分 Run / RecentDocs / UserAssist 的独立规则权重
- Browser 还未把 visit / download 串到 Prefetch / LNK 的更深层规则
- Email 还未形成独立的主题语义规则权重体系
- BrowserHistory / EmailMessage 的邻近时间线命中当前仍属于辅助信号，不等同于直接证据

## 5. 当前 confidence 解释

- `Direct`
  - 同一 `source_object_id` 同时命中 artifact 与 timeline
  - 或者形成明确路径级命中 / Recycle Bin 原路径恢复命中
- `Strong`
  - 当前对象在单侧命中中形成较高数量聚合
  - 或者 `Prefetch executable` 这类较强名称线索命中
- `Weak`
  - 仅形成少量单侧命中
  - 或者 `Email attachment` / `Registry name fallback` 这类弱名称线索命中
- `Heuristic`
  - 当前仅用于保留需要 investigator 进一步复核的提示

## 6. 前端工作流

V2 前端至少提供以下公有能力：

- 线索总览
- 证据聚合视图
- 按 lead drill-down 的详情页
- 与 timeline / file browser / reports 的联动入口

禁止事项：

- 页面内私有重复实现关联视图
- 用营销式文案替代 provenance 与边界说明

## 7. 报告要求

V2 报告导出中的“关联分析”章节至少包含：

- 线索摘要
- 证据链
- confidence
- provenance
- 未保证字段说明

当前已复用同一条关联快照：

- HTML `Correlation Leads` + `Correlation Lead Details`
- JSON `correlation`
- CSV 关联摘要行

## 8. 当前仓库落地事实（2026-06-12）

### 8.1 已实现入口

- `CorrelationSnapshotDto`
- `app_services::correlation_service::get_correlation_snapshot`
- Tauri command `get_correlation_snapshot`
- 前端公有组件 `frontend/src/components/analysis/CorrelationWorkspace.tsx`
- 页面入口 `frontend/src/app/pages/V2Workbench.tsx`
- `CorrelationSnapshotDto.familyCoverage[]`
- `/v2` 中的 jump action 已接入真实选择状态与导航：
  - `查看文件` -> `selectedFileId`
  - `查看痕迹` -> `selectedArtifactId`
  - `查看时间线` -> `selectedTimelineId`

### 8.2 已接住的真实字段

- LNK: `target_path`
- BrowserDownload: `targetPath`
- BrowserHistory: `url` / `title` / `visitTime`
- RegistryValue: `data`
- RecycleBin: `original_path`
- Prefetch: `executable`
- EmailMessage: `attachments` / `subject` / `sentAt`

### 8.2.1 当前治理快照已接住的关联运行信号

`get_v2_governance_snapshot` 现在不再只展示静态治理项，还会从真实 `get_correlation_snapshot` 派生以下运行信号：

- `correlationSnapshotAvailable`
- `correlationLeadCount`
- `correlationHighConfidenceLeadCount`
- `correlationReviewLeadCount`
- `correlationClusterCount`

这些字段会进入：

- `/v2` 的 `运行信号`
- `releaseScorecard`
- `governance.runtimeSignals`

这意味着 V2 评分卡已经开始把“有没有线索、线索强度如何、待复核比例如何”纳入调查工作流可用性的真实评分。

### 8.2.2 当前治理快照已接住的规则家族覆盖信号

在上一层统计之上，治理快照现在还会按规则家族输出：

- `correlationRuleFamilyCount`
- `correlationCoveredFamilyCount`
- `correlationHighConfidenceFamilyCount`
- `correlationFamilyCoverage[]`

`correlationFamilyCoverage[]` 当前按以下家族统计：

- `LNK`
- `Prefetch`
- `Registry`
- `RecycleBin`
- `BrowserDownload`
- `BrowserHistory`
- `EmailMessage`
- `JumpList`

每个家族当前会带：

- `status = covered / review / missing`
- `leadCount`
- `highConfidenceLeadCount`
- `reviewLeadCount`
- `clusterCount`
- `sampleSignals`

这让 `/v2` 可以直接回答一个更接近 investigator 真实问题的判断：不是“有没有关联分析”，而是“哪些规则家族已经形成可用线索，哪些还只是待复核或尚未命中”。

### 8.2.3 当前关联快照已直接带出规则家族覆盖

为避免前端工作台只能绕经治理快照读取规则覆盖，`get_correlation_snapshot` 现在会直接返回：

- `familyCoverage[]`
- `lead.families[]`
- `cluster.families[]`

每个条目与治理快照保持同构：

- `family`
- `displayName`
- `status = covered / review / missing`
- `leadCount`
- `highConfidenceLeadCount`
- `reviewLeadCount`
- `clusterCount`
- `sampleSignals`

这样 `CorrelationWorkspace` 可以直接展示：

- 规则家族覆盖状态
- 家族级 lead / cluster 数量
- 高置信与待复核分布
- 示例命中信号

治理快照中的 `runtimeSignals.correlationFamilyCoverage[]` 继续保留，但其来源改为复用关联快照自身的 `familyCoverage[]`，不再重复派生第二套规则家族统计逻辑。

### 8.2.4 规则家族归属已下沉到 lead / cluster

`CorrelationLeadDto` 与 `CorrelationClusterDto` 现在都带有 `families[]`。该字段由后端关联服务在生成 source group / rule group 时根据 artifact type 结构化派生，当前映射为：

- `LNK`
- `Prefetch`
- `RegistryValue -> Registry`
- `RecycleBin`
- `BrowserDownload`
- `BrowserHistory`
- `EmailMessage`
- `JumpList`

`familyCoverage[]` 会优先使用 `lead.families[]` 与 `cluster.families[]` 统计覆盖情况，再保留 provenance / signal 文本作为兼容兜底。前端 `CorrelationWorkspace` 的 lead、cluster 与选中详情均直接展示 families，报告 JSON / HTML / 文本摘要也输出对应规则家族。

### 8.3 当前边界

- 线索仍用于 investigator 导航与 evidence drill-down，不输出结论型判定
- 名称类匹配默认低于路径类匹配，需要 investigator 复核
- 时间线命中可能来自 projection 层，解释时需回跳原始事件
- 当前治理评分仍未根据“规则家族覆盖率”细分到 Prefetch / LNK / Registry / Browser / Email 各家族，只先统计 lead / cluster 与高置信 / 待复核数量

更新：治理快照现已开始按规则家族输出覆盖状态，但当前判定仍是基于 lead / provenance / signal 聚合推导，不是独立的规则执行追踪流水。

### 8.2.4 当前 `/v2` 如何与 benchmark 门禁联动

当前 V2 工作台已经不再只展示 benchmark 场景列表，还会把发布门禁依赖的必需项逐条展开为 `requiredChecks`：

- `datasetLevel`
- `scenario`
- `thresholdP95Ms`
- `measuredP95Ms`
- `status = covered / missing / exceeded`

这意味着 investigator / release reviewer 在同一页面里就能同时看到：

- 关联规则家族覆盖情况
- benchmark 必需项覆盖情况
- `releaseScorecard.breakdown.performance`

从而判断“当前线索工作流可解释性”与“当前性能门禁是否真实可发布”是不是同时成立。

## 9. 验收要求

- 至少 6 类核心规则稳定可回溯 provenance
- 任一 lead 都能说明来源、匹配依据、置信度和未保证字段
- timeline、artifact、报告对同一线索的描述保持一致
- 至少 3 个真实案例 walkthrough 能复现同一调查路径

## 10. 真实案例关联 Walkthrough

### 10.1 Walkthrough 目的

- 演示每条关联规则在真实调查场景下如何从 artifact 出发，经过规则引擎，命中到目标文件，最终形成可追溯的 lead
- 说明 provenance 链条的每一环分别由谁产生、携带哪些保证信息
- 明确 confidence 判定逻辑，以及何时 investigator 需要自己复核
- 标注当前实现无法保证的字段和已知局限

以下场景基于 `correlation_service` 的单元测试数据构造，具备与真实案例一致的数据形态和命名规范。

### 10.2 场景设定：疑似持久化与数据窃取调查

调查员正在分析一份 Windows 10 宿主机的磁盘镜像（E01 挂载）。已运行完整导入流水线：文件树、artifact 提取（LNK / Prefetch / Registry / RecycleBin / Browser / Email / JumpList）、MACB 时间线投影。

调查假设：
- 主机上可能存在通过快捷方式触发的载荷执行
- 存在浏览器下载的可疑文件
- 有邮件提交了涉案附件
- 回收站中有被删除的敏感文件

---

### 10.3 规则 Walkthrough

#### 10.3.1 LNK 目标路径命中文件（Direct）

**调查发现**：在 `C:\Users\Admin\Desktop\cmd.lnk` 中提取到 `target_path = "C:\Windows\System32\cmd.exe"`。

**规则行为**：

```
Artifact(LNK).attrs.target_path → normalize → path suffix 匹配 → FileEntry.path
```

1. `build_artifact_rule_matches` 进入 `LNK` 分支
2. 调用 `first_string_attr(attrs, ["target_path", "targetPath"])` 读取 `"C:/Windows/System32/cmd.exe"`
3. 经 `normalize_path` 处理为 `"c:/windows/system32/cmd.exe"`
4. `find_best_file_by_path` 在 file_entries 中精确匹配到 `FileEntry { path: "C:/Windows/System32/cmd.exe", deleted: false }`

**关联结果**：

| 字段 | 值 |
|------|-----|
| Lead ID | `lead:rules:file-cmd` |
| Primary File | `C:/Windows/System32/cmd.exe` |
| Confidence | `Direct` |
| Edge Kind | `PathMatch` |
| Edge Summary | "LNK 目标路径命中文件路径" |
| Match Signals | `["LNK 目标路径命中文件路径"]` |

**Provenance**：

```
source_kind: "artifact"
source_record_id: "artifact-lnk"
source_label: "LNK"
producer: "lnk"
producer_version: "1.0.0"
guarantee_level: "bestEffort"
```

**Confidence 分析**：
- 判定为 `Direct` 的依据：`CorrelationEdgeKindDto::PathMatch` → 在 `rule_group_confidence` 中匹配到 `PathMatch` 分支，无条件返回 `Direct`
- 路径类匹配属于最可靠的一档，因为文件系统路径是树状唯一标识
- 但需注意：LNK 内嵌的 `target_path` 是创建 LNK 时的声明，不等于 LNK 被双击时的实际文件

**Caveats**:

```
"路径类匹配依赖工件字段规范化，必要时需回跳原始 LNK 字段复核。"
```

**Investigator 下一步**：
- 在 `/v2` 工作台点击"查看痕迹" → 跳转到 LNK artifact 详情，复核 `creation_time`、`drive_serial` 等字段
- 点击"查看文件" → 跳转到 `cmd.exe` 文件详情，检查 MACB 时间戳
- 若有同一 `file-cmd` 的 timeline 事件，会在 `TemporalContext` 边中展示

---

#### 10.3.2 BrowserDownload 目标路径命中文件（Direct）

**调查发现**：浏览器下载记录中存在 `targetPath = "C:/Temp/payload.exe"`，下载时间为 `2026-06-12T10:00:00Z`。

**规则行为**：

```
Artifact(BrowserDownload).attrs.targetPath → normalize → find_best_file_by_path → FileEntry
```

1. `build_artifact_rule_matches` 进入 `BrowserDownload` 分支
2. 读取 `targetPath` → `"C:/Temp/payload.exe"`
3. 在 file_entries 中匹配到文件 `C:/Temp/payload.exe`
4. 同时在 `rule_match_timestamps` 中提取 `startTime`，用于后续邻近时间线关联

**关联结果**：

| 字段 | 值 |
|------|-----|
| Lead ID | `lead:rules:file-payload` |
| Primary File | `C:/Temp/payload.exe` |
| Confidence | `Direct` |
| Edge Kind | `PathMatch` |

**Provenance**：

```
source_kind: "artifact"
source_label: "BrowserDownload"
producer: "browserdownload"
guarantee_level: "experimental"  ← BrowserDownload 的保证级别低于 LNK/Prefetch/Registry
```

**Caveats**：

```
"下载路径来自浏览器数据库记录，仍需结合文件内容与时间线复核。"
```

**邻近时间线追加上下文**：

当下载记录的 `startTime`（`2026-06-12T10:00:00Z`）与另一条 timeline 事件的时间差在 24 小时窗口（`RULE_TIMELINE_PROXIMITY_WINDOW_SECS = 86400`）内，且该 timeline 的 `source_attribution` 或 `attrs.path` 的路径后缀命中 `"Temp/payload.exe"` 时，会在 lead 的 `match_signals` 中追加：

```
"邻近时间线命中 FILE_CREATED @ 2026-06-12T10:05:00Z"
```

**Investigator 下一步**：
- 检查 `payload.exe` 的文件内容 hash 与下载 URL 是否关联
- 查看邻近 timeline 事件，确认文件创建时间是否与下载时间一致
- 若发现不一致（如文件创建时间早于下载记录），则需进一步复核浏览器数据库完整性

---

#### 10.3.3 BrowserHistory 标题或 URL 命中文件名（Weak → 邻近时间线可为 Strong）

**调查发现**：Edge 历史记录中存在 `url = "https://intranet.local/reports/browser-incident-report"`，`title = "browser-incident-report.docx draft"`，访问时间 `2026-06-12T12:00:00Z`。

**规则行为**：

```
Artifact(BrowserHistory).attrs.url / title → extract_file_name_candidates → find_best_file_by_name → FileEntry
```

1. `build_browser_history_rules` 从 `title` 提取文件名候选 `["browser-incident-report.docx"]`，从 `url` 也提取候选
2. 使用 `find_best_file_by_name` 按 `file.name`（大小写无关）匹配到 `C:/Cases/browser-incident-report.docx`
3. 这是一条 `NameMatch` + `Weak` 的线索
4. 但 `rule_match_timestamps` 提取了 `visitTime`，进入邻近时间线查找流程
5. 发现 24 小时内存在一条 `REPORT_OPENED` timeline 事件，其 `title` 包含 `"browser-incident-report.docx draft"`（文本 needle 命中）

**关联结果**：

| 字段 | 值 |
|------|-----|
| Lead ID | `lead:rules:file-report` |
| Primary File | `C:/Cases/browser-incident-report.docx` |
| Confidence | `Strong`（名称匹配 + 邻近时间线佐证） |
| Edge Kind | `NameMatch` |

**Confidence 分析**：
- 单条 `BrowserHistory → NameMatch` 的 individual rule confidence 是 `Weak`
- 但 `rule_group_confidence` 在第 1270-1276 行发现 `has_proximity_timeline == true && matches 不为空` → 整体提升为 `Strong`
- 这是"邻近时间线命中"作为辅助信号升档的核心机制

**Provenance**：

```
guarantee_level: "experimental"  ← BrowserHistory 作为浏览器数据，保证级别低于原生系统工件
```

**Caveats**：

```
"BrowserHistory 命中基于标题或 URL 文本，需要结合访问时间与原始记录复核。"
```

**Investigator 下一步**：
- 回跳原始 BrowserHistory artifact，确认 `url` 是内部站点还是外网
- 检查 timeline 中的文件访问记录，判断文件是被"查看"还是被"编辑/导出"
- 特别注意：名称匹配可能命中同名文件（如多个目录都有同名 report），需确认路径

---

#### 10.3.4 Registry 值数据命中文件路径（Strong / Weak）

**调查发现**：Registry artifact 的 `data` 字段值为：

```
"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -nop
```

**规则行为（路径优先）**：

```
Artifact(RegistryValue).attrs.data → extract_path_candidates → find_best_file_by_path → FileEntry
```

1. `build_registry_rules` 调用 `first_string_attr(attrs, ["data"])` 获得原始值
2. `extract_path_candidates` 识别 `"C:\...\powershell.exe"` 为合法 Windows 路径（引号内的路径段）
3. 归一化后 `find_best_file_by_path` 精确匹配到 `C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`
4. 路径命中返回 `Confidence = Strong`，`EdgeKind = PathMatch`

**规则行为（名称兜底）**：

若 data 字段不含合法路径（如 `"cmd.exe -c ..."`），则走名称匹配 fallback：

```
extract_file_name_candidates("cmd.exe -c ...") → ["cmd.exe"]
find_best_file_by_name → FileEntry { name: "cmd.exe" }
→ EdgeKind: NameMatch, Confidence: Weak
```

**关联结果（路径命中）**：

| 字段 | 值 |
|------|-----|
| Confidence (rule group) | `Direct`（因为存在 PathMatch） |
| Edge Kind | `PathMatch` |

**Provence**：

```
guarantee_level: "bestEffort"  ← Registry 属于原生系统工件，保证级别较高
```

**Caveats**：

```
"Registry 值可能包含环境变量或启动参数，命中后仍需回跳原始值复核。"
```

**已知局限**（与设计文档第 4.3 节对齐）：
- Registry 当前不区分 `Run` / `RecentDocs` / `UserAssist` 的独立规则权重
- 所有 Registry artifact 走同一套 `data` 字段路径/名称抽取逻辑
- 这可能导致 Run key 中的命令行参数被误拆分为路径候选

---

#### 10.3.5 RecycleBin 原路径恢复命中已删除文件（Direct）

**调查发现**：回收站记录中 `original_path = "C:/Users/Admin/Desktop/secrets.txt"`。

**规则行为**：

```
Artifact(RecycleBin).attrs.original_path → normalize → prefer_deleted=true → find_best_file_by_path → FileEntry (deleted)
```

1. `build_artifact_rule_matches` 进入 `RecycleBin` 分支
2. 读取 `original_path`，调用 `build_single_path_rule` 时传入 `prefer_deleted = Some(true)`
3. `find_best_file_by_path` 中的 `deleted_preference_score` 优先匹配 `deleted == true` 的文件
4. 匹配到 `FileEntry { path: "C:/Users/Admin/Desktop/secrets.txt", deleted: true }`

**关联结果**：

| 字段 | 值 |
|------|-----|
| Confidence | `Direct` |
| Edge Kind | `RecoveredOriginalPath` |
| Edge Summary | "Recycle Bin 原路径命中已删除文件" |

**Confidence 分析**：
- `rule_group_confidence` 第 1262-1266 行：只要存在 `RecoveredOriginalPath`，无条件返回 `Direct`
- 这是所有规则中置信度最高的分支，因为回收站原路径直接说明文件的"来源路径"

**Caveats**：

```
"回收站原路径反映删除前路径声明，需结合 deleted 文件与删除时间复核。"
```

**Investigator 下一步**：
- 点击"查看文件"跳转到已删除文件详情，检查删除时间
- 若文件系统元数据还保留了文件内容，尝试预览恢复
- 注意：回收站的原路径是"文件被删除时声明的路径"，不等于"文件被删除时的实际路径"（如果 MFT 中存在硬链接或 junction）

---

#### 10.3.6 Prefetch 可执行名命中文件名（Strong）

**调查发现**：Prefetch artifact 中 `executable = "CMD.EXE"`。

**规则行为**：

```
Artifact(Prefetch).attrs.executable → basename("CMD.EXE") → "cmd.exe" → find_best_file_by_name → FileEntry
```

1. `build_artifact_rule_matches` 进入 `Prefetch` 分支
2. 调用 `basename("CMD.EXE")` 归一化为 `"cmd.exe"`
3. `find_best_file_by_name` 大小写无关匹配到 `FileEntry { name: "cmd.exe" }`

**关联结果**：

| 字段 | 值 |
|------|-----|
| Confidence (rule) | `Strong` |
| Edge Kind | `NameMatch` |

**Confidence 分析**：
- `build_name_rules` 中 Prefetch 传入的 `confidence` 参数为 `Strong`（代码第 778 行）
- Prefetch executable 是较强线索，因 Windows Prefetch 记录的是最近执行的可执行文件完整路径
- 但当前实现只做 basename 匹配（非完整路径），这是 `Strong` 而非 `Direct` 的原因

**Caveats**：

```
"名称匹配可能命中同名文件，需要结合路径与时间进一步复核。"
```

**已知局限**（与设计文档第 4.3 节对齐）：
- Prefetch 仍是 basename 级匹配，不是完整执行路径匹配
- `cmd.exe` 可能命中的是 `System32` 下的系统文件，也可能是某目录下同名的恶意文件
- 当前不利用 Prefetch 的 `file_references`（MFT 引用号）去精确定位

---

#### 10.3.7 Email 附件名 / 主题命中文件名（Weak）

**调查发现**：邮件 artifact 中：
- `attachments = ["triage.csv"]`
- `subject = "Initial triage notes"`
- `sentAt = "2026-06-12T11:00:00Z"`

**规则行为（附件匹配）**：

```
Artifact(EmailMessage).attrs.attachments[] → basename("triage.csv") → "triage.csv" → find_best_file_by_name → FileEntry
```

→ `EdgeKind: NameMatch, Confidence: Weak`

**规则行为（主题匹配 + 邻近时间线）**：

```
Artifact(EmailMessage).attrs.subject → extract_file_name_candidates → find_best_file_by_name → FileEntry
→ EdgeKind: NameMatch, Confidence: Weak
```

同时：
- `rule_match_timestamps` 提取 `sentAt` → `2026-06-12T11:00:00Z`
- 24 小时内 timeline 中出现 `REPORT_UPDATED` 事件，其 `title` 包含 `"Initial triage notes"`、`source_attribution` 为 `"C:/Cases/triage.csv"`
- 邻近时间线信号追加到 lead 的 `match_signals`

**关联结果**：

| 字段 | 值 |
|------|-----|
| Confidence (rule group) | `Strong`（邻近时间线升档） |
| Edge Kind | `NameMatch` |

**Provenance**：

```
guarantee_level: "experimental"  ← Email 解析依赖邮件格式与编码，保证级别低于原生系统工件
```

**Caveats**：

```
"附件名匹配只提供弱线索，需要结合时间、路径与邮件上下文复核。"
"主题命名匹配只提供弱线索，需要结合 sentAt 与附件/时间线复核。"
```

**Investigator 下一步**：
- 点击"查看痕迹"检查邮件 artifact 的 `from` / `to` / `message_id` 字段
- 若附件 `triage.csv` 已从邮件中导出为独立文件，检查其内容 hash
- 主题名称匹配不可靠（"triage notes" 可能命中文档库中任何同名文件），需要交叉验证

---

#### 10.3.8 JumpList 目标路径命中文件路径（Direct）

**调查发现**：JumpList artifact 中 `target_path = "C:/Users/Admin/Documents/report.docx"`。

**规则行为**：

```
Artifact(JumpList).attrs.target_path / targetPath → normalize → find_best_file_by_path → FileEntry
```

→ `EdgeKind: PathMatch, Confidence: Direct`

**Caveats**：

```
"JumpList 命中依赖嵌入式 LNK 提取结果，需结合原始 JumpList 复核。"
```

JumpList 本质上是内嵌的 LNK 流集合，因此规则与 LNK 相同（路径匹配 + Direct），但 caveat 提醒 investigator 回跳原始 JumpList 文件确认内嵌数据完整性。

---

### 10.4 完整调查路径（investigator view）

以下展示在 `/v2` 工作台中 investigator 看到的完整信息流：

```
步骤 1: 打开 /v2 → 看到"运行信号"面板
  - correlationSnapshotAvailable: true
  - correlationLeadCount: 5
  - correlationHighConfidenceLeadCount: 3
  - correlationReviewLeadCount: 2

步骤 2: 展开"规则家族覆盖"
  - LNK:           covered (1 lead, 1 high)
  - Prefetch:      covered (1 lead, 0 high → 1 review)
  - Registry:      covered (1 lead, 1 high)
  - RecycleBin:    covered (1 lead, 1 high)
  - BrowserDownload: review (1 lead, 1 high but experimental guarantee)
  - BrowserHistory:  review (1 lead, 0 high → 1 review + has proximity)
  - EmailMessage:    covered (1 lead, 0 high → 1 review + has proximity)
  - JumpList:        covered (1 lead, 1 high)

步骤 3: 点击 BrowserHistory 的 lead → 查看详情
  - 标题: "browser-incident-report.docx 形成规则型关联线索"
  - Confidence: Strong
  - Match Signals:
    · "BrowserHistory 标题或 URL 命中文件名"
    · "邻近时间线命中 REPORT_OPENED @ 2026-06-12T12:15:00Z"
  - Provenance chain:
    · artifact:artifact-browser-history-proximity (BrowserHistory, experimental)
    · timeline:timeline-near-browser-history (REPORT_OPENED, bestEffort)
  - Caveats:
    · "BrowserHistory 命中基于标题或 URL 文本，需要结合访问时间与原始记录复核。"
    · "当前规则命中尚未获得同文件时间线佐证。"
  - Jumps:
    · 查看文件 → /files?selected=file-report
    · 查看痕迹 → /artifacts?selected=artifact-browser-history-proximity

步骤 4: 点击"查看文件" → 进入 FileBrowser
  - 定位到 C:/Cases/browser-incident-report.docx
  - 显示文件 MACB 时间戳、大小、hash
  - 可在此预览文件内容（若文件类型支持）

步骤 5: 回到 /v2，展开同一文件的"规则命中" cluster
  - 看到该文件被哪些 artifact 族命中
  - 每条命中边标注了 edge kind、confidence、summary
  - 可逐条 drill-down 到各自的 artifact 详情

步骤 6: 导出报告
  - HTML 报告的 "Correlation Leads" 章节列出所有 lead
  - "Correlation Lead Details" 展开每条 lead 的 provenance
  - JSON 导出包含完整 correlation 快照
  - CSV 导出包含关联摘要行
```

---

### 10.5 置信度决策树速查

以下速查表帮助 investigator 理解每条 lead 的置信度是如何判定的：

| 条件 | 最终 Confidence |
|------|----------------|
| 存在 `RecoveredOriginalPath`（回收站原路径） | `Direct` |
| 存在 `PathMatch`（LNK/Registry/BrowserDownload/JumpList 路径命中） | `Direct` |
| 同一 source object 同时有 artifact + timeline | `Direct` |
| Prefetch basename 匹配 + timeline 佐证 | `Strong` |
| Browser/Email name 匹配 + 邻近 timeline 佐证 | `Strong` |
| 同一 source object 有 3 条以上 artifact/timeline 但无双侧 | `Strong` |
| Prefetch basename 匹配（无 timeline 佐证） | `Strong` |
| Registry 名称 fallback 匹配（无 path 命中） | `Weak` |
| Email attachment 匹配（无 timeline 佐证） | `Weak` |
| BrowserHistory 名称匹配（无 timeline 佐证） | `Weak` |
| Registry name fallback（无 path 命中，无 timeline 佐证） | `Weak` |
| 仅 1-2 条单侧命中且无规则命中 | `Weak` |
| 保留给 investigator 复核的提示（当前仅用于占位） | `Heuristic` |

**重要提示**：`rule_group_confidence` 在 `PathMatch` 或 `RecoveredOriginalPath` 存在时无条件返回 `Direct`，不附加考虑 timeline 数量。这意味着即使文件没有 timeline 事件，LNK/RecycleBin 的路径命中仍然是 `Direct`——路径本身就是最强的证据信号。

---

### 10.6 Guarantee Level 与证据强度对照

| Artifact 类型 | Guarantee Level | 原因 |
|--------------|----------------|------|
| Prefetch / LNK / Registry / RecycleBin | `bestEffort` | 直接来自 Windows 原生系统工件，解析器有成熟测试覆盖 |
| BrowserDownload / BrowserHistory / EmailMessage | `experimental` | 依赖浏览器数据库/SQLite 解析和邮件格式解析，受编码和版本影响 |
| JumpList | `bestEffort` | 内嵌 LNK 流的结构化提取，与 LNK 共享底层解析逻辑 |

**解释**：
- `bestEffort` 不等于"不会出错"，而是"我们对该解析器做了最充分的测试和验证"
- `experimental` 意味着"当前测试覆盖有限，可能存在未发现的解析错误"
- 报告中 `guarantee_level` 字段直接写入每个 provenance 条目，investigator 可据此判断证据可信度

---

### 10.7 边界与未保证字段

以下字段在当前关联引擎中**不做保证**：

1. **Prefetch 完整执行路径** — 当前只做 `basename(executable)` 匹配，不携带原始的 `executable_path` 完整路径进行精确匹配
2. **Registry key path 上下文** — 当前从 `data` 字段抽取路径/文件名，不区分 key 是 `Run`、`RecentDocs`、`UserAssist` 还是其他
3. **Browser visit/download 的精确 URL→文件映射** — 当前只做名称/路径匹配，不判断 downloaded file 的 hash 是否与 URL 指向的资源一致
4. **Email 发件人身份验证** — 当前不检查 SPF/DKIM/DMARC 头，主题匹配仅做文本级名称抽取
5. **多版本同名文件的歧义消除** — `find_best_file_by_name` 多命中时取 path 最短的，但这不一定是正确的目标文件
6. **软链接 / junction / 硬链接追踪** — 当前 file_entries 的 path 字段是 NTFS 解析器给定的声明路径，不追踪重解析点目标

这些未保证字段在 lead 的 `caveats` 中已经给出对应提示。若调查中发现了由未保证字段导致的错误关联，应回退到原始 artifact/timeline 手动核实。

---

### 10.8 当前报告输出验证

以单元测试中的 BrowserHistory + proximity timeline 场景为例，关联快照的报告输出为：

```json
{
  "generatedAt": "2026-06-12T...",
  "leadCount": 6,
  "clusterCount": 4,
  "familyCoverage": [
    {
      "family": "BrowserHistory",
      "displayName": "Browser History",
      "status": "covered",
      "leadCount": 1,
      "highConfidenceLeadCount": 1,
      "reviewLeadCount": 1,
      "sampleSignals": ["BrowserHistory 标题或 URL 命中文件名", "邻近时间线命中 REPORT_OPENED @ ..."]
    }
  ],
  "leads": [
    {
      "id": "lead:rules:file-report",
      "title": "browser-incident-report.docx 形成规则型关联线索",
      "confidence": "strong",
      "families": ["BrowserHistory"],
      "primaryFileId": "file-report",
      "matchSignals": [
        "BrowserHistory 标题或 URL 命中文件名",
        "邻近时间线命中 REPORT_OPENED @ 2026-06-12T12:15:00Z"
      ],
      "jumps": [
        {"route": "/files", "targetId": "file-report", "label": "查看文件"},
        {"route": "/artifacts", "targetId": "artifact-browser-history-proximity", "label": "查看痕迹"}
      ],
      "provenance": [
        {
          "sourceKind": "artifact",
          "sourceRecordId": "artifact-browser-history-proximity",
          "sourceLabel": "BrowserHistory",
          "producer": "browserhistory",
          "producerVersion": "1.0.0",
          "guaranteeLevel": "experimental",
          "warningSummary": []
        }
      ],
      "caveats": [
        "BrowserHistory 命中基于标题或 URL 文本，需要结合访问时间与原始记录复核。",
        "当前规则命中尚未获得同文件时间线佐证。"
      ]
    }
  ]
}
```

---

### 10.9 待真实样本验证

以下条目需在加载真实 E01 样本（如 `E:\pangushi\刘洋\20202101-刘洋-涉Win-检材.E01`）并完成完整导入/解析流程后进行验证：

- [ ] **待验证-1**：LNK 规则在真实样本上的路径匹配率（预期：≥ 80% 的 LNK 能找到目标文件）
- [ ] **待验证-2**：Registry 规则的误报率（预期：Run key 的数据字段应产生路径命中，RecentDocs 应产生名称命中，UserAssist 应产生路径命中——需逐类统计）
- [ ] **待验证-3**：Prefetch basename 匹配在同名文件场景下的歧义比例（预期：< 5% 的 Prefetch 线索因同名文件导致 investigator 复核后判定为误关联）
- [ ] **待验证-4**：BrowserDownload 与邻近 timeline 的时间窗口是否合理（当前 `24h` 窗口在真实调查中是否过宽或过窄）
- [ ] **待验证-5**：Email 附件名匹配是否因 base64 编码/带引号文件名导致失败（当前 normalize 已处理引号，但未验证 MIME 定界符场景）
- [ ] **待验证-6**：RecycleBin 原路径恢复后文件 `deleted` 标记是否与真实文件系统状态一致
- [ ] **待验证-7**：`find_best_file_by_path` 的 suffix fallback 在嵌套目录场景下是否产生误命中（如 `C:\Temp\a\cmd.exe` vs `C:\Temp\b\cmd.exe`）
- [ ] **待验证-8**：关联快照在 ≥ 1000 artifacts 场景下的性能（当前限流 `MAX_CORRELATION_ARTIFACTS = 250`）
- [ ] **待验证-9**：报告输出中 8 个规则家族在真实样本上的 `familyCoverage` 覆盖状态分布

**验证记录应追加到此 Walkthrough 末尾**，格式为：

```
[YYYY-MM-DD] 样本: <case-name>, Artifacts: <N>, Leads: <M>, 验证人: <name>
  - LNK: covered/review/missing, 路径匹配率: X%
  - Registry: covered/review/missing, 路径匹配率: X%, 名称匹配率: Y%
  - ...
```

## 11. 案件级跨源关系图投影（2026-08-02）

### 11.1 存储与所有权边界

案件级跨源图是可重建派生数据，不是新的事实源：

- `app.db` 继续保存案件与数据源注册，不保存跨源图明细。
- 每个 `sources/<dataSourceId>/source.db` 继续独立保存源内 `graph_nodes`、`graph_edges`、artifact 和文件树。
- `indexes/case-graph.db` 只保存参与确定性跨源匹配的镜像实体节点、案件实体 hub、跨源边和构建 manifest。
- 构建过程只读打开所有 ready source DB；不会把跨源边反写任一 source DB，也不会复制完整文件树。
- `CaseGraphRepo::replace_projection` 在一个 SQLite 事务内替换投影。构建或写入失败时，已有完整投影保持可读。

对应源码边界：

- `crates/app-services/src/graph_service/case_graph/manifest.rs`
- `crates/app-services/src/graph_service/case_graph/projection.rs`
- `crates/app-services/src/graph_service/case_graph/traversal.rs`
- `crates/persistence-sqlite/src/repositories/case_graph_repo.rs`
- `crates/persistence-sqlite/src/migrations/scripts/case_graph_001.sql`
- `crates/persistence-sqlite/src/migrations/scripts/source_029_case_graph_entity_index.sql`

### 11.2 确定性匹配规则

当前只在以下条件全部成立时建立案件实体 hub：

1. 节点类型为 `Entity`。
2. entity type 相同，例如 `person`、`account` 或 `device`。
3. 使用既有 `EntityMergeEngine` 规范化后的值完全一致。
4. 匹配成员至少来自两个不同的 data source。

规范化沿用既有实体规则：trim、lowercase、Unicode NFKD，并按 entity type 去除已支持的 `mailto:` 或 `sid:` 前缀。系统不会为案件图执行编辑距离、近似字符串、名称包含、时间邻近或其他启发式匹配。相似但不相同的实体不会被连接。

案件 hub ID 和边 ID 均由 SHA-256 确定性生成。跨源边固定使用 `CorrelatesWith`、置信度 `1.0`，provenance 至少记录匹配策略、entity type、canonical hash、两端数据源和 projection version；UI 展示的关联仍需回跳源节点与原始 artifact 复核。

### 11.3 新鲜度与发布

投影 manifest 绑定：

- case ID 与 projection version
- ready data source ID 与 source schema version
- source DB 文件大小和修改时间
- source DB WAL 文件大小和修改时间

查询前会比较 manifest。输入变化时重建投影；重建前后再次采集 manifest，若源库在构建期间继续变化，则不发布该结果并要求稍后重试。进程内构建锁避免并发查询重复发布，SQLite WAL 和单事务替换保证已有读取者继续看到上一个完整快照。

### 11.4 混合查询与资源预算

`query_graph_for_case` 使用有界混合 BFS，同时遍历 `case-graph.db` 与相关 source DB：

- 案件 hub 可展开到多个数据源中的精确实体成员。
- source-scoped 实体可继续进入本数据源的 artifact、file 和其他源内邻域。
- 查询允许混合 `ds:<dataSourceId>:<localId>` 种子以及案件 hub 种子。
- 最大深度为 5，节点上限为 500，边上限为 2000，起始节点上限为 64。
- 每次 SQLite 邻接查询都有独立 LIMIT，confidence 在发现节点前过滤。
- 达到节点或边预算时返回 `truncated=true`，不会用静默截断伪装完整结果。
- 非法 edge type、未作用域源节点 ID 和非 ready 数据源均返回明确 `InvalidInput`。

快照会返回 ready 数据源数、案件 hub 数、跨源边数、后端选择的稳定 seed IDs 与投影时间。`largestComponentSize=0` 明确表示未物化全图连通分量，前端显示“未计算”，不再用节点总数冒充最大分量。

### 11.5 前端契约

`GraphVisualizationSection` 只使用后端 `GraphSnapshot.seedIds` 启动关系图，不再从文件树前若干根节点猜测种子。图布局初始坐标由节点 ID 确定性生成，达到稳定阈值或 240 tick 后停止模拟；查询被预算截断时显示明确提示。

### 11.6 验证与当前边界

自动化测试 `crates/app-services/tests/case_graph_cross_source.rs` 当前覆盖：

- Windows/Linux 精确实体跨源连通并进入两端源内 artifact 邻域
- 混合数据源种子
- edge budget 截断
- 相似但不相同实体不误连
- source graph 变化后自动重建
- 非法 edge type 拒绝

持久化测试覆盖独立 schema、事务失败保留旧投影、必要索引和只读重开。前端测试覆盖后端 seed 驱动、截断提示和确定性节点位置。

当前不承诺模糊实体消歧、跨案件匹配、全图连通分量物化、后台预构建或超出预算的无界全图导出。真实 E01 双源的实体命中率与误关联率仍需单独建立 opt-in 样本基线；现有自动化证明的是算法边界和 source DB 隔离，不把合成测试数量冒充真实样本结果。
