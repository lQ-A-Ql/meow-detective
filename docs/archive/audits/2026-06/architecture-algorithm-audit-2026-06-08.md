# Forensics Workbench 架构与算法审计报告

> 归档：2026-06 审计快照，仅用于历史追溯，不代表当前架构状态。

> 审计日期:2026-06-08
> 范围:全仓库模型、算法与架构评估(8 个子系统集群并行深审 + 综合)
> 方法:逐 crate 阅读真实源码(lib.rs/mod.rs + 各最大文件),所有结论附 `file:line` 锚点

## 目录

1. [整体架构评估](#1-整体架构评估)
2. [数据模型](#2-数据模型)
3. [核心算法](#3-核心算法)
4. [设计模式与抽象](#4-设计模式与抽象)
5. [优势](#5-优势)
6. [风险与技术债](#6-风险与技术债)
7. [成熟度评级](#7-成熟度评级)
8. [建议](#8-top-建议)

---

## 1. 整体架构评估

整体采用 **backend-led(后端主导)** 的分层架构,与 PRD/spec 中"Rust 拥有工作流,React 消费快照与事件"的设计意图一致,且实现层面基本忠实于这一原则。

**依赖方向**(自核心向外):

- `crates/domain` + `crates/transport` 位于内核,定义实体与 IPC 契约;
- `crates/app-services` 编排用例(case 生命周期、import、enumerate、timeline projection、search、reports);
- `crates/persistence-sqlite`、`crates/infrastructure`、各 evidence/fs/artifacts crate 提供存储与外部能力;
- `apps/desktop/src-tauri` 的命令层是薄 IPC 适配器,验证/翻译 DTO 后委派(`scripts/check-command-sql-boundary.ps1` 守护"命令层无裸 SQL")。

**端到端数据流**(典型 import):

```
image (RAW/E01)  →  volume detect (MBR/GPT)  →  FileSystemReader (NTFS/FAT/exFAT)
   →  parallel_enum 多线程枚举到 per-partition staging DB
   →  staging→main ATTACH + INSERT OR IGNORE 合并
   →  post-import analysis (MACB timeline / artifacts / text index)
   →  persistence (file_entries / timeline_events / artifacts / tantivy)
   →  lazy SQL projections (timeline)  →  transport DTO  →  EventBus  →  React Query / Zustand
```

**架构成熟度:中高。** 两条干净的 reader seam(`EvidenceReader` 字节源 / `FileSystemReader` 命名空间)使 RAW 与 E01 对文件系统读取器完全可替换,是整个底层最成功的抽象。`app-services` 函数式服务层 + 注入闭包(`reader_fn`/`read_header_fn`/`progress_cb`)使核心逻辑可单测。

主要架构性隐忧集中在**两处"双实现"**:

- **生产路径与"理想抽象"分叉**:`crates/ingest` 的 `IngestPipeline`/`IngestSink` trait 完全未被生产路径使用,真实引擎在 `apps/desktop/src-tauri/src/commands/import/pipeline.rs`(~3,100 行)自带一套类型;`crates/catalog` 整个 crate 无任何消费者(dead code)。
- **同一资源的双访问模型**:`app_services::active_case::ActiveCase` 持 `Mutex<Connection>`,而 desktop `AppState` 另持 r2d2 连接池(`app_state.rs:56`)指向同一 `app.db`,二者并发语义未定义。

数据流本身设计成熟(staging-merge、resumable manifest、backpressure、内存治理器),但若干正确性假设只在简单 fixture 下成立(见第 6 节)。

---

## 2. 数据模型

### 核心实体(`crates/domain`)

- `CaseMeta`、`DataSource`(含 `DataSourceProvenance` + `DataSourceHashStatus`)、`FileEntry`(自引用 `parent_id` 树,4 个 MACB 时间戳)、`TimelineEvent`、`Artifact`。
- `Artifact` 与 `TimelineEvent` 均带 provenance 字段(`parser_id`/`version`/`confidence`/`source_attribution`),但 artifacts-windows 抽取器目前大多留 `None`(`artifacts-core/src/lib.rs:107`)。

### SQLite schema(migrations 0001–0021)

- **一案一库**:case 作用域主要靠物理文件隔离,而非 `WHERE case_id`。所有 ID 为 TEXT UUID,所有时间戳为 TEXT(写入 `to_rfc3339()`,读取 `util.rs` 双格式宽松解析)。
- **关系**:cases 1-N data_sources 1-N file_entries(自引用树);artifacts/timeline_events 通过 `source_object_id`(裸 TEXT,**非声明式 FK**)挂到 file_entries,并冗余携带 `case_id`。
- **分区三重表示**(技术债热点):`data_source_partitions`(0009,实际使用)、`partitions`(0013,创建但从不读写)、以及 legacy `data_sources.partitions` JSON 列;迁移器 `partition_migration.rs:20` 从不被调用,0014 的 `migration_log` 行永久 'pending'。
- **索引**:timeline 在 0021 增加 `(case_id, ts DESC, id ASC)` 复合覆盖索引,匹配规范 ORDER BY;但 `entry_type = 'directory' COLLATE NOCASE` 与 BINARY collation 的 `idx_file_entries_type_deleted` 不匹配,目录过滤无法用索引。
- **级联**:artifacts/timeline 无 FK,靠 `case_repo`/`datasource_repo` 手写 cascade delete,两条删除路径语义不一致(timeline 仅经 JOIN 子查询、缺 `OR case_id=?`),导致非文件来源事件(registry/EVTX 派生)成为孤儿。
- **审计**:`audit_log` 为可变明文 INSERT,无 hash 链/签名,`cleanup_old()` 可无条件删历史记录——对取证工具是防御性短板。

### Transport DTO 契约(`crates/transport`)

单一事实源、无 codegen,两侧手工同步。DTO 用 `#[serde(rename_all="camelCase")]`,可选字段 `skip_serializing_if`;命令返回 `Result<T,String>`,跨 crate 错误为 `ApiErrorDto`,命令边界再 sanitize 为 `CommandError`(刻意不实现 `From<String>`)。事件主题为字符串常量 + `EventTopic` enum 双编码,镜像到 TS union——`search-index_progress` 这类混合连字符/下划线命名需手工 serde rename,任何漂移无法编译期捕获。MCP DTO(`dto/mcp.rs`)刻意用 snake_case,与其余 camelCase 契约不一致。

---

## 3. 核心算法

### 镜像 / 文件系统读取层

| 算法 | 位置 | 复杂度 |
|---|---|---|
| E01 section 链表遍历 + chunk-table 构建(visited-set 防环,10MB section 上限) | `image-e01/src/lib.rs:65,367,437` | O(sections+chunks) 构建,O(1) 摊还查找 |
| E01 chunk 解码 + 顺序预取缓存(zlib inflate,16MB 有界 VecDeque,线性扫描) | `image-e01/src/lib.rs:173,230,270,299` | 解码 O(chunk),缓存查找 O(cache_len) |
| NTFS MFT 记录读取(沿 $MFT data runs 映射 + USA fixup) | `fs-ntfs/src/lib.rs:110,127,808` | O(runs)/记录 + O(record/sector) fixup |
| NTFS 目录列举($INDEX_ROOT + $INDEX_ALLOCATION 合并,按 mft_ref 去重) | `fs-ntfs/src/lib.rs:180,500,885` | O(entries);路径解析 O(depth×dir) |
| NTFS data-run 解析 + LZNT1 解压(100k run / 128MB 上限) | `fs-ntfs/src/lib.rs:1030,334,1074,1146` | O(total_clusters) / O(output) |
| Bulk MFT scan(reader/parser-pool/writer,crossbeam) | `fs-ntfs/src/mft_scanner.rs:36,240`;`file_service/mod.rs:1092,1159` | O(records) 并行 |
| FAT cluster-chain 遍历 + LFN 解析(FAT12 半字节;**无环检测/无上限**) | `fs-fat/src/lib.rs:166,116,206,307` | O(chain) |
| exFAT entry-set fold + 时间戳解码(HashSet 防环,100M 上限) | `fs-exfat/src/dir.rs:290,372`;`fat.rs:83` | O(entries)/O(chain) |

### Windows Artifacts + EVTX

| 算法 | 位置 | 复杂度 |
|---|---|---|
| BinXML 单遍 token 流 + IR 构建(bump arena) | `evtx-patched/src/binxml/ir.rs:661` | O(n) tokens |
| 模板 clone-and-resolve 实例化(`Rc<IrTree>` flyweight by GUID) | `evtx-patched/src/binxml/ir.rs:991` | O(template_nodes)/实例 |
| Array-substitution 元素复制(MS-EVEN6 3.1.4.7.5,cross-product,**无总量上限**) | `evtx-patched/src/binxml/array_expand.rs:32,160` | O(N×element),嵌套乘性 |
| String cache 链表填充 | `evtx-patched/src/string_cache.rs:13` | O(names) 构建,O(1) 查找 |
| Registry hive cell 遍历(regf/hbin 校验,lf/lh/li/ri,深度上限 8) | `artifacts-windows/src/registry/lookup.rs:337,598` | O(subkeys)/段(线性) |
| EVTX dirty-tail 裁剪 + 16MiB 有界抽取 | `artifacts-windows/src/evtx/parser.rs:118` | O(1) header |
| Boot/shutdown 分类(6005/6006/6008/1074) | `artifacts-windows/src/evtx/parser.rs:172` | O(records) |

### Search(tantivy)

| 算法 | 位置 | 复杂度 |
|---|---|---|
| BOM 编码探测文本抽取(仅靠 mime_hint,≤10MiB 全量缓冲) | `search/src/extractor/text_extractor.rs:14` | O(n) |
| 文档索引/commit(硬编码 15MB heap,**无 delete_term/upsert**) | `search/src/indexer/tantivy_writer.rs:54` | O(d+m),每次全量 commit |
| staging→tantivy 分页合并(LIMIT 50,每页新 writer+commit) | `app-services/src/staging.rs:637` | O(N) docs,O(N/50) commits |
| 查询解析 + phrase fallback + (TopDocs,Count) 精确计数 | `search/src/indexer/tantivy_writer.rs:112` | ~O(postings) |
| Snippet 聚类高亮(256KiB/512B/5 上限,字符边界) | `search/src/highlighter/mod.rs:8` | O(t×n)/hit |

### Timeline + Catalog

| 算法 | 位置 | 复杂度 |
|---|---|---|
| `project_file_macb` 每文件 MACB 投影(随机 UUID id) | `timeline/src/lib.rs:6,48` | O(1)/文件 |
| `ensure_macb_timeline_projected` 惰性幂等门(meta 表) | `app-services/src/timeline_service.rs:42` | O(1) 首跑后 |
| 集合式 SQL 投影(4×INSERT OR IGNORE...SELECT + NOT EXISTS,确定性 id) | `app-services/src/timeline_service.rs:254` | O(n)×4,依赖索引 |
| `TimelineRepo::query` 有序 OFFSET 分页(ts DESC,id ASC) | `persistence-sqlite/.../timeline_repo.rs:50` | 深页 O(offset+limit) |

### Ingest 管道

| 算法 | 位置 | 复杂度 |
|---|---|---|
| 相位编排 + profiling-string 协议(解析回 DTO) | `commands/import/pipeline.rs:408` | O(phases) |
| 并行分区枚举(crossbeam work/result/progress,25ms recv_timeout) | `app-services/src/parallel_enum.rs:67` | O(entries) 并行 |
| NTFS MFT fast-path(25k 记录/chunk,memoized DFS 路径重建,shape 校验回退) | `parallel_enum.rs:438,1086` | O(records)+O(N) |
| 路径解析规模切换(>100k 转 SQLite TEMP 表) | `parallel_enum.rs:545,1390` | O(N) |
| 分析 producer/consumer + RSS 内存治理器(软节流/硬中止) | `import_analysis.rs:452,904` | O(files) 并行 |
| staging→main ATTACH + INSERT OR IGNORE 合并 | `app-services/src/staging.rs:393` | O(rows)/分区 |

### Persistence / Reports

| 算法 | 位置 | 复杂度 |
|---|---|---|
| 幂等事务式迁移 runner(name-keyed,per-script BEGIN/COMMIT/ROLLBACK) | `persistence-sqlite/src/migrations/runner.rs:87` | O(M) |
| 手写 cascade delete(case/data source,5–9 表有序) | `case_repo.rs:125`、`datasource_repo.rs:98` | O(N_files+...) |
| 批量子目录计数(N+1 规避,IN placeholder) | `file_repo.rs:191` | O(children) |
| CSV 公式注入消毒(`= + - @` 前缀加 tab) | `reports/src/csv/exporter.rs:7` | O(bytes) |

---

## 4. 设计模式与抽象

- **Reader 双缝抽象(核心抽象之一)**:`EvidenceReader`(字节源:RAW/E01)与 `FileSystemReader`(命名空间:NTFS/FAT/exFAT)正交分离,使任意镜像格式 × 任意文件系统可自由组合,是本项目最干净的设计。
- **插件注册 + Sink/Visitor**:artifacts 抽取器经注册表挂载,通过 `ArtifactSink` 回写;ingest 经 `IngestSink` trait 解耦投影写入。
- **Arena / Flyweight**:EVTX BinXML 用 bump arena 分配 IR 节点,模板按 GUID 以 `Rc<IrTree>` 共享后 clone-resolve——经典 flyweight。
- **投影读模型(CQRS-lite)**:timeline/catalog 是从 file_entries/artifacts 派生的只读投影,惰性幂等生成。
- **Producer/Consumer + Staging-merge**:ingest 用 crossbeam 有界通道做背压,每分区先写 staging DB 再 ATTACH 合并入主库,兼顾并行与崩溃可恢复。
- **Repository**:persistence 层每域一个 repo,SQL 封装其内,命令层不碰 SQL。
- **Job/Task 模型**:长任务以 `AtomicBool` 协作式取消 + job 快照/事件对外;但仅协作式、从不 join 线程(见 H10)。
- **事件总线双编码**:Rust 字符串常量主题 ↔ TS `EventTopic` union,前端 `EventBus` 订阅。
- **前端**:`apiClient` facade 在 mock/tauri 间切换;Zustand store 经 `useSyncExternalStore` 暴露;`mcp.ts` 对后端响应做强规范化(防御 snake_case 漂移)。

---

## 5. 优势

- **底层干净且防御性强**:E01/NTFS/FAT/exFAT 读取层普遍使用 `checked_mul`、有界缓存、run/cluster 上限,溢出与 OOM 防护到位。
- **NTFS reader 可用度高**:data-runs、USA fixup、LZNT1 解压、$INDEX 合并去重均实现完整,是取证核心能力的扎实基座。
- **EVTX 核心成熟**:vendored `evtx-patched` 的 BinXML 单遍解析 + 模板 flyweight + array-substitution 语义贴合 MS-EVEN6 规范。
- **Registry `lookup.rs` 接近生产级**:regf/hbin 校验、lf/lh/li/ri 全分支、深度上限,质量明显高于其他 artifact 解析器。
- **Ingest 工程扎实**:并行枚举、MFT fast-path、RSS 内存治理、staging 可恢复合并,体现真实大镜像工程经验。
- **Timeline 可靠**:集合式 SQL 投影 + 确定性 id + 惰性幂等门 + 0021 覆盖索引,正确且高效。
- **安全/取证意识**:命令层 SQL 边界守护、media 协议守护、CSV 公式注入消毒、错误 sanitize、只读 evidence 路径。
- **前端状态/事件成熟**:React Query + Zustand + EventBus + `useSyncExternalStore`,契约规范化防御到位。
- **迁移 runner 正确**:name-keyed 幂等、per-script 事务、ROLLBACK 隔离。

---

## 6. 风险与技术债

按严重度排序。编号 H=高、M=中、L=低。

### 高(High)

| # | 风险 | 位置 |
|---|---|---|
| H1 | staging→main 合并用 `INSERT OR IGNORE`,主键/唯一冲突时**静默丢弃文件**,取证完整性受损且无计数告警 | `app-services/src/staging.rs:465` |
| H2 | bulk MFT scanner **假设 $MFT 数据连续**(单 run),碎片化 $MFT 会漏读记录 | `fs-ntfs/src/mft_scanner.rs:204` → `file_service/mod.rs:1159` |
| H3 | exFAT `NoFatChain` 标志解析了但**从不使用**,连续簇文件遍历会错误地走 FAT 链 | `fs-exfat/src/lib.rs:77,87` vs `dir.rs:337` |
| H4 | exFAT 在镜像路径下**不可达**(datasource 探测/分发未挂 exFAT) | `datasource_service.rs:25,541` |
| H5 | SRU 按 **SQLite** 解析,实则为 ESE/JET 数据库,格式错误 | `artifacts-windows/src/sru/mod.rs:53` |
| H6 | Prefetch **不解压 MAM**(Win10+ 压缩),无法解析现代 prefetch | `artifacts-windows/src/prefetch/parser.rs:74` |
| H7 | EVTX WEVT-template 渲染与多线程被 **feature-gate 关闭**,默认产物缺消息字符串 | `Cargo.toml:68` |
| H8 | Search 无 upsert/dedup,重复 import 产生**重复文档** | `search/src/indexer/tantivy_writer.rs:54` |
| H9 | Search 每次调用 **create-on-every-call**(重建 writer/index) | `app-services/src/search_service.rs:83` |
| H10 | 仅协作式取消,**从不 join 工作线程**,取消后线程可能继续写库 | `task_manager.rs:78`、`pipeline.rs:196` |
| H11 | 逻辑目录枚举为**单事务、不可取消**,大目录树阻塞 | `enumeration.rs:63` |
| H12 | 死分区 schema 0013/0014 + 从不调用的迁移器,`migration_log` 永久 pending | `partition_migration.rs:20` |
| H13 | 迁移 0016 表重建存在 **FK-violation 风险**(rebuild 期间外键悬空) | `0016_add_cascade_delete.sql`、`runner.rs:111` |
| H14 | **双 DB 访问模型**并存(`active_case` vs `app_state` pool),易现状态不一致 | `active_case.rs:8`、`app_state.rs:17` |
| H15 | MCP **每次调用新建 Tokio runtime** + 跨 runtime I/O | `mcp_commands.rs:163` |
| H16 | MCP **Mutex 跨阻塞 I/O 持有**,可致死锁/串行化 | `mcp_commands.rs:343` |
| H17 | MCP **任意命令/URL 执行无校验**(stdio 启动外部进程) | `stdio.rs:132`、`mcp_commands.rs:99` |

### 中(Medium,节选高代表性)

| # | 风险 | 位置 |
|---|---|---|
| M1 | E01 多段(multi-segment)寻址未完整支持 | `image-e01/src/lib.rs` |
| M2 | FAT cluster chain **无环检测**,恶意/损坏镜像致 OOM/死循环 | `fs-fat/src/lib.rs:166` |
| M3 | `LogicalFsReader` 存在**路径穿越**风险 | logical reader |
| M4 | GPT 分区表 entry 数**未校验**即分配 | partition 解析 |
| M5 | E01 **无完整性校验**(不校验 stored CRC/hash) | `image-e01` |
| M6 | Registry 注册的是较弱的 `parser.rs` 而非 `lookup.rs` | registry 注册 |
| M10 | Search 二进制检测仅靠 mime,误判文本/二进制 | `text_extractor.rs` |
| M11 | 高亮 offset 基于 lowercase 文本,多字节错位 | `highlighter/mod.rs` |
| M13 | 合并碎片化 + `IMPORT_TEXT_INDEX_LIMIT=100` + 256KiB cap **静默丢弃语料** | staging/search 配置 |
| M15 | 双 timeline 投影路径(crate vs service SQL)可能**分叉** | timeline |
| M17 | content budget **双重递减** | content 预算 |
| M31 | MCP 配置**从不加载** | `mcp_commands.rs` |
| M32 | MCP server capabilities 被**丢弃** | mcp client |
| M33 | 双事件路径并存 | events |
| M34 | `invalidateQueries` **未带 key**,过度失效 | 前端 hooks |

### 低(Low,节选)

- 每文件 reopen reader(无句柄复用);FAT LFN 解析脆弱;Thumbcache/SRU 为 stub;Prefetch UTF16 O(n²);year-filter 误丢合法 FILETIME;search 深页在 1000 处截断;HTML 报告 `width:100%%` bug;`verify_sha256` 大小写敏感比较;FileBrowser O(n×e) 渲染。

---

## 7. 成熟度评级

| 子系统 | 评级 | 说明 |
|---|---|---|
| 镜像 / E01 | 高 | 链表+chunk-table、防环、有界缓存;缺多段与完整性校验 |
| NTFS reader | 高 | data-runs/USA/LZNT1/$INDEX 完整 |
| FAT / exFAT | 低–中 | exFAT 镜像路径不可达(H4)、NoFatChain 未用(H3);FAT 无环检测 |
| Bulk MFT scanner | 中 | 并行扎实,但假设 $MFT 连续(H2) |
| EVTX 核心 | 高(配置项中) | BinXML/模板/array 语义到位;WEVT/多线程被 gate 关(H7) |
| Registry `lookup.rs` | 高 | 接近生产级;但注册的是较弱 `parser.rs`(M6) |
| 其他 artifacts(SRU/Prefetch/Thumbcache 等) | 低 | 格式错误/stub(H5/H6) |
| Search | 中 | 可用但无 dedup/upsert、每调用重建、语料丢弃(H8/H9/M13) |
| Timeline | 高 | 集合式投影 + 覆盖索引 + 幂等门 |
| Catalog | 低(含死代码) | 投影存在但集成薄弱 |
| Ingest | 中–高 | 并行/内存治理/可恢复合并强;取消语义与合并丢弃为短板(H1/H10) |
| Persistence | 中 | runner 正确;分区三重表示与 0016 重建风险(H12/H13) |
| App-services + transport | 中–高 | 契约清晰;手工同步与双 DB 模型(H14) |
| 前端状态 / 事件 | 高 | React Query + Zustand + EventBus 成熟 |
| MCP client | 低 | runtime/锁/安全多重问题(H15/H16/H17) |

---

## 8. Top 建议

1. **修复取证保真核心(H1+H2)**:合并改用显式冲突检测 + 丢弃计数告警,杜绝静默丢文件;bulk MFT scanner 沿 $MFT data-runs 读取以支持碎片化 $MFT。这是取证完整性的底线。
2. **打通 exFAT 端到端(H3+H4)**:在 datasource 探测/分发中挂载 exFAT,并正确实现 `NoFatChain` 连续簇遍历。
3. **重写 MCP 层(H15/H16/H17 + M30/31/32)**:复用单一 Tokio runtime、锁不跨阻塞 I/O、对命令/URL 加白名单与校验、加载配置并保留 capabilities。
4. **修正 3 个 artifact 解析器格式正确性(H5/H6/M6)**:SRU 改 ESE/JET 解析、Prefetch 解压 MAM、Registry 注册 `lookup.rs`。
5. **统一 ingest 取消语义(H10/H11/M18)**:协作取消后 join 线程并停止写库;逻辑枚举改可取消的分批事务。
6. **修 Search dedup/覆盖(H8/M13)**:引入 `delete_term`/upsert 去重,放宽语料上限或显式记录被丢弃文档。
7. **统一 DB 访问模型(H14)**:收敛到单一 case-scoped pool,移除并行的 `active_case` 通路。
8. **清理死/分叉代码**:catalog 死代码、ingest trait、`streaming.rs`、search stub、分区 schema 0013/0014、EVTX feature-gate 决策——减少认知负担与误导。

---

*报告结束。所有结论基于 2026-06-08 仓库快照的真实源码阅读,`file:line` 锚点可直接跳转复核。*
