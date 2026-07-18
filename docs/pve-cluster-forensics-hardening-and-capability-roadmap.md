# PVE 集群取证加固与能力补齐路线图

**设计日期**: 2026-07-17
**开发基线**: `4247e89d`
**真实样本**: `E:\pangushi\服务器`
**当前重点**: 派生 RBD Catalog 完整性、处理阶段可观测性、只读边界和性能；随后补齐 OSDMap/CRUSH/PG、RBD 高级特性与 CephFS。

## 1. 目标

本设计将 PVE 集群取证能力按四层收敛：

1. 物理成员：六个 E01 成员可独立导入，三个宿主盘解析 EXT4，三个
   BlueStore 盘保存 metadata-only 库存。
2. Ceph 对象：BlueFS、RocksDB、BlueStore、OMAP 与 RADOS 对象读取保持只读，
   且能证明使用的 OSD inventory、对象身份和副本集合。
3. 虚拟磁盘：RBD head image 通过独立派生数据源重建，文件树、预览、搜索、
   时间线和痕迹提取使用统一 source DB 生命周期。
4. 集群语义：逐步补齐 OSDMap epoch、CRUSH、PG、degraded recovery、RBD
   snapshot/clone 和 CephFS；证据不足时必须 typed fail closed。

完成标准不是“能读出部分 VM 字节”，而是：

- 文件树与预览拥有可重复的字节 oracle。
- Catalog 完整性、后处理完整性和导入 readiness 分开表达。
- 原始 E01、LVM、BlueStore 和父 source DB 不被生产重建链路写入。
- 快速 reopen 不重复执行全表摘要，深度审计仍可显式执行。
- 未完成的集群能力不被 UI、报告或文档误标记为 supported。

## 2. 当前真实样本基线

| 维度 | 已验证基线 |
|---|---|
| 集群成员 | 6 个物理数据源 |
| 宿主文件系统 | 3 个 `disk01` EXT4 |
| BlueStore | 3 个 `disk02` metadata source |
| 派生磁盘 | `vm-100-disk-0` |
| 派生文件记录 | 114,260 |
| 派生目录 / 文件 | 15,749 / 98,511 |
| 派生普通文件总大小 | 5,547,104,746 bytes |
| VM 文件系统 | direct XFS、`centos/home`、`centos/root` |
| 首次物化 | 46.28-54.73s |
| ready 幂等物化 | 124-136ms |
| cold runtime + first read | 约 3.186s |
| warm 64 KiB p95 | 0.699ms |
| `4x1MiB` p95 | 234.152ms |
| 614 MiB 文件随机 64 KiB p95 | 73.804ms |
| runtime cache | 117,440,512 bytes |
| RSS delta | 399-448 MiB |

以上数字只适用于当前私有样本和已记录硬件环境。公开支持承诺仍需要 public
fixture、expected JSON 和独立 oracle。

## 3. 已确认缺陷与风险

### 3.1 readiness 与 processing 状态混淆

`data_sources.import_state='ready'` 当前只证明 source DB 的文件 Catalog 可浏览，
不能证明 Graph、Platform、Artifacts、Timeline、Search 已全部完成。若只读取
`import_state`，前端和报告会把“可浏览”误解为“全部分析完成”。

处理原则：

- `import_state` 继续只表达数据源导入/Catalog readiness。
- `data_source_processing_phases` 单独表达后处理 DAG。
- 后端返回聚合 processing 状态、计数和 phase 明细；前端不得自行推导 DAG。

### 3.2 ready reopen 的全表摘要放大

派生 VM source 含 114,260 条 `file_entries`。每次 reopen 若扫描全表、重建父子关系
并重新计算 SHA-256，会把应为毫秒级的幂等操作放大为数据库全量审计。

处理原则：

- 正常 reopen 从 `source_meta` O(1) 加载版本化 Catalog manifest。
- 全表 digest 仅由显式 `verify_derived_source_catalog` 深度审计执行。
- manifest 必须绑定 lineage fingerprint 与 materializer version。

### 3.3 Catalog 摘要不完整

只摘要 path/name/size 会漏掉：

- entry type 漂移。
- hidden/system/deleted 状态漂移。
- MACB 时间漂移。
- partition identity 漂移。
- parent 断链或父节点替换。

Catalog digest 必须覆盖稳定的父路径身份，不包含随机 UUID，否则重导摘要不能稳定。

### 3.4 warning 文案承担正确性判断

若 Catalog completeness 依赖 warning 字符串匹配，文案修改、国际化或日志格式变化
会改变取证结果。

处理原则：

- filesystem reader 返回 typed `FileSystemDiagnostic`。
- `DirectoryPartial`、`DirectoryUnreadable`、`EntryUnavailable` 明确影响完整性。
- 普通物理源可保留部分结果并展示诊断；派生 RBD Catalog 必须 fail closed。

### 3.5 父 source DB 可被重建读取路径写入

使用 writable open helper 会执行 migration、WAL pragma 或产生 `-wal/-shm`，
破坏“父数据源仅作为只读事实源”的边界。

处理原则：

- RADOS/RBD reconstruction 统一使用 `open_existing_source_read_only`。
- reconstruction route 同时校验案件归属、readiness、platform、控制库 schemaVersion
  与物理 source DB migration version。
- 缺失 source DB、版本漂移或非法 lineage 不允许自动创建或升级。

### 3.6 phase 中断、重复执行和并发 ownership

Graph、Artifacts、Timeline 和 Search 是昂贵投影。没有持久 phase identity 时，
进程中断会留下“状态不明”，重试可能重复写入或覆盖新一轮结果。

处理原则：

- phase 保存 version、input fingerprint、owner、attempt、lease 和 heartbeat。
- 同 identity 的 ready phase 幂等复用。
- identity 变化重置为 pending。
- case open 将旧进程遗留 running phase 收敛为 failed，并允许重试。
- stale attempt 不得提交新 owner 的结果。

### 3.7 OSD inventory 完整集合尚未证明

当前样本显式加载三个 OSD，三副本字节一致性已验证，但“已加载三个 inventory”
不等于“这就是集群在目标 epoch 的完整 acting set”。

风险：

- 漏载第四个 OSD。
- 使用过期 OSDMap。
- 把缺失副本当作 sparse hole。
- 对 EC pool 错用 replicated-pool 语义。

在 OSDMap/CRUSH/PG 闭合前，degraded/missing-inventory 必须保持 unsupported。

### 3.8 RBD 高级特性会改变读取语义

snapshot、parent/clone、object-map、fast-diff、data-pool、journaling、encryption、
multi-PV LVM 均可能改变对象选择、零块语义或虚拟块设备组合。只按 head image
object sequence 读取会产生静默错误。

### 3.9 CephFS presence 与重建未闭合

当前样本 strongly leaning absent，但尚未用具有 freshness proof 的 FSMap 完成
权威 absent 证明。不得因为存在 Ceph 集群就创建 CephFS 文件树。

## 4. 架构基线

```text
physical E01 source DBs (read-only)
  -> BlueStore inventory
  -> RocksDB latest state
  -> BlueStore semantic object plans
  -> RADOS replica provider
  -> RBD virtual block reader
  -> partition/LVM/filesystem readers
  -> derived source.db Catalog
  -> processing phase DAG
  -> Graph / Platform / Artifacts / Timeline / Search
```

数据状态分离：

```text
data_sources.import_state
  pending -> importing -> ready | ready_metadata | failed

data_source_processing_phases
  pending -> running -> ready | failed | deferred
```

契约解释：

- `ready_metadata`：BlueStore 元数据库存已持久化，不提供普通文件树。
- `ready`：该数据源 Catalog 可浏览。
- `processing.state=ready`：当前版本和 input fingerprint 下所有已注册 phase 完成。
- `processing.state=failed/deferred`：Catalog 仍可浏览，但后续投影不完整。

Phase DAG：

```text
Catalog
  ├─> Graph
  └─> Platform
        └─> Artifacts
              ├─> Timeline
              └─> Search
```

实际 fingerprint 还包含各 phase policy version，Timeline/Search 分别绑定所需的
Platform/Artifact output identity，不能只比较数据源 ID。

## 5. Stage Plan

### Stage 0 - 基线冻结与测试拆分

#### stage_design

冻结字节、Catalog、文件树、性能和内存 oracle。后续优化必须证明没有改变证据结果。

#### phase tasks

**Phase 0.1 真实样本事实冻结**

- 固定六成员相对路径和串行导入顺序。
- 固定 OSD `0/1/2`、BlueFS `44/49/42`、live SST `35/40/33`。
- 固定 RBD image ID/name/pool、114,260 Catalog 记录和分区布局。
- 固定代表文件 first/middle/last/random range SHA-256。

**Phase 0.2 性能口径拆分**

- Cargo 编译不计入运行时阈值。
- 分别记录 source open、runtime construction、first read、warm read、深审计。
- 分别记录墙钟、CPU、I/O bytes、SQLite pages、cache hit、RSS peak。

**Phase 0.3 只读基线**

- 记录三个父 `source.db` 的 size、mtime、SHA-256、sidecar 状态。
- 重建前后比较，不允许出现内容变化或新 `-wal/-shm`。

#### expected result

优化前后可以按相同 oracle 比较，不再用混合了编译、导入和深审计的单个耗时数字。

#### stage review

- 检查样本路径只存在于 ignored test/runner。
- 检查固定 hash 不进入生产分支。
- 检查测试失败信息能区分字节错误、性能退化和 fixture 缺失。

### Stage 1 - Catalog 完整性与快速复用

#### stage_design

正常 reopen 走 O(1) manifest；完整性验证走显式 O(n) 深审计，两者不可混用。

#### phase tasks

**Phase 1.1 Manifest**

- 持久化 materializer version、lineage fingerprint、record/directory/size/MACB counts。
- 持久化 canonical Catalog digest。
- ready reopen 只读取单个 `source_meta` row。

**Phase 1.2 Canonical digest**

- 覆盖 path、name、type、size、status、MACB、partition。
- 覆盖父节点存在状态和稳定父路径身份。
- 不摘要随机 file UUID。

**Phase 1.3 Deep audit**

- `verify_derived_source_catalog` 全表重算并比较 manifest。
- 缺失 manifest、版本漂移、lineage 漂移返回 false/typed error。
- 深审计不得作为普通文件浏览前置步骤。

#### expected result

- ready 幂等物化保持约 124-136ms。
- 父子关系或 metadata 漂移可被深审计检出。
- 普通 reopen 不读取 114,260 行。

#### stage review

- SQL 查询具有确定性排序。
- digest 字段有长度前缀，避免拼接歧义。
- manifest 更新、checkpoint 和 Catalog ready 顺序明确。

### Stage 2 - Typed completeness 与 XFS correctness

#### stage_design

普通源允许“部分可用 + 明确诊断”；派生 VM Catalog 要求完整且 fail closed。

#### phase tasks

**Phase 2.1 Diagnostic model**

- 定义 `DirectoryPartial`、`DirectoryUnreadable`、`EntryUnavailable`、
  `MetadataDegraded`、`TypeConflict`。
- diagnostic 带 path/inode/message，但正确性只依赖 kind。

**Phase 2.2 XFS inode/timestamp**

- v1/v2 inode core 固定 100 bytes，v3 固定 176 bytes。
- 支持已验证的 v1/v2/v3 timestamp、BIGTIME 和 crtime。
- MACB 传播到 `FileEntry`、Catalog 和 Timeline。

**Phase 2.3 Partial directory policy**

- 可解析 siblings 继续返回。
- 部分目录记录 typed diagnostic。
- 派生 Catalog 遇到 completeness-affecting diagnostic 失败，不把缺失子树标记为完整。

#### expected result

现有 114,260 文件树和固定 range oracle 不变，同时 malformed/partial XFS 不再静默丢树。

#### stage review

- parser 不依赖 app-services/SQLite/Tauri。
- 错误路径无 `unwrap`、无 warning 文案匹配。
- extent/btree 目录能力按单一职责拆分。

### Stage 3 - Processing Phase DAG 与可观测契约

#### stage_design

将 Catalog readiness 和后处理 completeness 物理分表、逻辑分层、契约分字段。

#### phase tasks

**Phase 3.1 Ledger**

- 新增 case migration `0038_data_source_processing_phases.sql`。
- phase 状态支持 pending/running/ready/failed/deferred。
- 保存 version、input fingerprint、owner、attempt、lease、heartbeat 和 stats。

**Phase 3.2 Runner**

- claim 使用单 owner；lease 到期才能被新 attempt 接管。
- file-backed case DB 每 30 秒用独立 connection heartbeat。
- stale completion、heartbeat 和跨 source transition 必须拒绝。

**Phase 3.3 Recovery**

- case open 将旧 running phase 标记 failed。
- failed/deferred phase 保留诊断并可重试。
- 相同 identity ready phase不重复执行。

**Phase 3.4 DTO/UI**

- `DataSourceSummary.processing` 返回 aggregate state、各状态计数、lastError 和 phase list。
- aggregate state 由后端计算。
- 前端只显示 DTO，不根据 phase 顺序重新实现业务规则。

#### expected result

文件树 ready 后即使 Search/Timeline 失败，也能准确显示“可浏览但处理不完整”，且重试不会覆盖新 attempt。

#### stage review

- command 仍为薄 wrapper。
- repository 不暴露底层 connection。
- UI 无 direct SQL、无 Tauri 旁路、无 mock fallback。

### Stage 4 - OSD inventory 完整集合证明

#### stage_design

在执行通用 replica/degraded 判断前，先建立“目标 epoch 下集群成员全集”的证据链。

#### phase tasks

**Phase 4.1 OSDMap inventory**

- 从 monitor store/OSD metadata 恢复 FSID、epoch、pool、OSD up/in、weight、address。
- 每个记录保存 source provenance、epoch 和 canonical digest。
- 多份 map 按 epoch 单调排序，拒绝冲突的同 epoch payload。

**Phase 4.2 Freshness proof**

- 选择与目标证据时间匹配的最高可信 epoch。
- 记录 map 来源和时间边界。
- 无法证明 freshness 时 aggregate state 为 indeterminate，不进入 degraded recovery。

**Phase 4.3 Inventory closure**

- 证明 map 中目标 pool 的相关 OSD 均已导入，或明确记录缺失成员。
- 校验 OSD ID、FSID、device binding、inventory identity 唯一。
- 将“全部缺失可视为 sparse hole”限制在完整集合已证明且对象语义允许的情况。

#### expected result

系统能区分：

- 完整三副本集合。
- 合法 degraded 集合。
- inventory 不完整。
- map 过期或不可判定。

#### stage review

- 不扫描案件目录猜测 OSD。
- 不用文件名推断 OSD ID。
- OSDMap 原始敏感 payload 不进入日志或报告。

### Stage 5 - CRUSH / PG / Replica / Degraded Recovery

#### stage_design

复现固定 Ceph revision 的 placement 语义，所有 pool 类型和 tunable 必须显式支持。

#### phase tasks

**Phase 5.1 CRUSH**

- 解码 buckets、items、weights、rules、choose args 和 tunables。
- 实现 straw2 等样本所需算法。
- 对未知 bucket/rule/step typed unsupported。

**Phase 5.2 PG mapping**

- 实现 object hash、raw PG、pgp_num、up/acting set 和 primary。
- 结果绑定 pool ID、OSDMap epoch、CRUSH digest 和 object identity。
- 与独立 `ceph osd map` oracle 对比。

**Phase 5.3 Replica selection**

- 首选 acting primary，允许同 epoch 的完整副本一致性验证。
- 副本不一致时保留冲突证据并 fail closed。
- 缺失副本只有在 OSDMap/acting set 证明 degraded 时才允许读取现存副本。

**Phase 5.4 EC boundary**

- 首版只识别 EC pool 并 typed unsupported。
- 后续独立实现 stripe/chunk/parity，不复用 replicated-pool 快速路径。

#### expected result

当前样本不再依赖“手工给定三个副本就是全集”，并可安全区分一致、副本缺失和冲突。

#### stage review

- 算法以固定 Ceph revision 和独立 CLI oracle 双重证明。
- placement 结果有 deterministic property tests。
- 无网络访问运行中 Ceph 集群的生产依赖。

### Stage 6 - RBD 能力补齐

#### stage_design

从“单 head image + 默认 striping”扩展到显式 feature matrix；任何改变读语义的 feature
必须先解析再启用。

#### phase tasks

**Phase 6.1 Snapshot**

- 解析 snap context、snapshot ID/name/size/features。
- snapshot reader 固定 snapshot object map，不回落到 head。
- first/middle/last object 建立独立 byte oracle。

**Phase 6.2 Parent/clone**

- 解析 parent pool/image/snap overlap。
- overlap 内缺失 child object 才允许读取 parent。
- parent lineage 必须在同案件中显式注册并通过完整性校验。

**Phase 6.3 Object map / fast-diff**

- object-map 只作为读取优化提示，不作为唯一事实源。
- object-map 与实际 object presence 冲突时 fail closed 或降级到完整验证。
- fast-diff 只用于差异枚举，不改变 byte reader 正确性。

**Phase 6.4 Journaling/encryption**

- 未实现 replay/key management 前保持 typed unsupported。
- 不调用宿主 qemu-nbd/rbd map 作为生产读取后门。

**Phase 6.5 Multi-PV LVM**

- 支持同一 VM 的多 RBD/PV 组合。
- PV identity、VG/LV segment 和跨设备 extent 映射必须完整。
- thin/cache/RAID/snapshot LV 独立裁定，不能按 linear LV 猜测。

#### expected result

RBD capability 由 metadata feature matrix 驱动；head/snapshot/clone/multi-PV 的读取边界可解释、可测试。

#### stage review

- RBD reader 保持 `Read + Seek`/bounded-range，不整盘 materialize。
- cache key 包含 image/snapshot/parent/epoch identity。
- derived source lineage 可追溯到全部父 source 和 object-map 决策。

### Stage 7 - CephFS Presence 与文件系统重建

#### stage_design

先做 presence proof，再做 MDS metadata 和 data object 重建。不存在或不可判定时不创建伪文件树。

#### phase tasks

**Phase 7.1 Presence proof**

- 恢复 FSMap/MDSMap、filesystem list、metadata pool 和 data pools。
- freshness proof 成立且 filesystems=0 才标记 absent。
- filesystems>0 标记 present；其余为 indeterminate。

**Phase 7.2 Metadata reconstruction**

- 解码 inode、dirfrag、dentry、backtrace、layout 和 snap realm。
- 保存 source/pool/object/epoch provenance。
- 目录 fragment 不完整时输出 typed diagnostic。

**Phase 7.3 Data mapping**

- 根据 file layout 映射 stripe unit/count/object size 和 data pool。
- 文件内容走 RADOS bounded-range。
- sparse、inline、xattr、symlink 和 hard link 分别验证。

**Phase 7.4 Derived CephFS source**

- 每个 filesystem 创建独立 derived source DB。
- Catalog/processing/read-only/manifest 规则复用 RBD 派生源。
- MDS journal 未闭合时只能标记 crash-consistency boundary，不能宣称最新状态。

#### expected result

产品可以可靠给出 present/absent/indeterminate，并在 present 样本上重建可预览的 CephFS 文件树。

#### stage review

- RBD 与 CephFS 模型、DTO、source kind 分离。
- 不把 monitor 配置、PMXCFS storage 配置当作唯一 presence 事实。
- MDS journal、snapshot realm 和 encryption 未完成时保持 fail closed。

## 6. 测试矩阵

| 测试层 | 场景 | 必须断言 |
|---|---|---|
| Unit | phase aggregate | 聚合状态、计数、顺序和 lastError 由后端生成 |
| Unit | phase lease | 单 owner、过期接管、stale attempt 拒绝 |
| Unit | Catalog digest | metadata/parent 漂移改变 digest，随机 UUID 不改变 digest |
| Unit | XFS inode | v1/v2/v3 core、BIGTIME、crtime、invalid bounds |
| Unit | diagnostic policy | typed completeness kind 控制 fail closed，不匹配文案 |
| Integration | ready reopen | 不执行文件表全量 digest；读取 manifest |
| Integration | deep audit | 114,260 Catalog 全量校验成功；人为漂移失败 |
| Integration | read-only parent | 父 source DB hash/mtime/sidecar 前后不变 |
| Integration | recovery | case reopen 将 running phase 收敛 failed，随后可重试 |
| Real sample | physical members | 6 source、3 EXT4、3 BlueStore metadata |
| Real sample | derived VM | 1 RBD、114,260 rows、固定分区与文件计数 |
| Real sample | preview | 五个代表文件和大文件固定 range SHA-256 |
| Real sample | processing | 6 个 phase 均存在，期望状态与 output count 一致 |
| Performance | ready reopen | p95 不高于 250ms |
| Performance | first materialize | 不高于当前 55s 基线上浮 10% |
| Performance | cold runtime | 当前 3.186s 先设报告线，Stage 4 后设硬门禁 |
| Performance | warm range | 64 KiB p95 <= 2ms；4x1MiB p95 <= 300ms |
| Resource | RSS | 不高于当前 448 MiB 上浮 10% |
| Future | OSDMap/CRUSH | 与固定 epoch `ceph osd map` oracle 一致 |
| Future | degraded | 仅在 acting set 和缺失 OSD 已证明时恢复 |
| Future | RBD features | head/snapshot/clone 分别具备 byte oracle |
| Future | CephFS | present/absent/indeterminate 三态与文件预览 |

## 7. 性能评估

### 7.1 指标

- Wall clock：phase 和端到端分别记录。
- CPU：parser/reducer 与 I/O wait 分开。
- Disk I/O：E01 read bytes、SQLite read/write bytes、WAL bytes。
- Cache：plan/page/runtime hit/miss、eviction、construction count。
- Memory：RSS peak、cache capacity、单 request temporary allocation。

### 7.2 预算

| 路径 | 当前基线 | 门禁 |
|---|---:|---:|
| ready manifest reopen | 124-136ms | p95 <= 250ms |
|首次 RBD Catalog 物化 | 46.28-54.73s | <= 60s |
| cold runtime + first read | 3.186s | Stage 3 仅报告；Stage 4 目标 <= 2.5s |
| warm 64 KiB | 0.699ms | p95 <= 2ms |
| 4x1MiB | 234.152ms | p95 <= 300ms |
| random 64 KiB | 73.804ms | p95 <= 100ms |
| RSS delta | 399-448MiB | <= 493MiB |

### 7.3 退化判定

- 字节 oracle 变化：Critical，立即阻断。
- Catalog count/digest 变化：Critical，除非 expected oracle 经独立工具复核更新。
- 只读父库变化：Critical。
- 首次物化或 warm range 退化 >10%：High，必须分析。
- RSS 超预算：High。
- 仅 cold construction 抖动且 warm 路径稳定：Medium，记录并继续拆分初始化成本。

## 8. 工程评审标准

每个 Stage 完成后单独 review：

| 维度 | 权重 | 通过标准 |
|---|---:|---|
| 取证正确性 | 25 | oracle、provenance、fail-closed 无缺口 |
| 模块化 | 15 | 单文件单能力，生产文件/函数守卫通过 |
| 只读与安全 | 15 | 原始证据和父 source DB 零写入 |
| 契约 | 15 | transport 单一事实源，前端只消费 DTO |
| 健壮性 | 10 | typed error/diagnostic、恢复、幂等 |
| 测试 | 10 | unit/integration/real sample 覆盖 |
| 性能 | 10 | 时间和内存预算通过 |

总分低于 90、任一维度低于 80 或存在 Critical/High 未整改项，不进入下一 Stage。

## 9. 验收标准

### 当前加固阶段

- ready reopen 不扫描 114,260 行重算摘要。
- 显式 deep audit 能验证完整 Catalog 并检出父子关系漂移。
- typed filesystem diagnostic 决定完整性，warning 文案不参与正确性。
- XFS v1/v2/v3 inode core 与 MACB 传播通过回归。
- 六个 processing phase 持久化，支持 lease/heartbeat/recovery/幂等重试。
- `get_data_sources` 明确返回 processing aggregate 和 phase 明细。
- 三个父 BlueStore source DB 在重建与预览后无内容或 sidecar 变化。
- Rust、frontend、结构 guard 和 `git diff --check` 全部通过。

### OSDMap/CRUSH/PG 阶段

- 已加载 inventory 集合可由目标 epoch 证明完整或明确 degraded。
- PG/acting set 与独立 Ceph oracle 一致。
- 缺失对象、sparse hole、副本冲突不再依赖猜测。

### RBD/CephFS 能力阶段

- snapshot/clone/object-map/multi-PV 各自有 feature gate 和 byte oracle。
- encryption/journaling 未实现时 typed unsupported。
- CephFS 有权威 present/absent/indeterminate 判定。
- CephFS 文件树、预览和 provenance 在 present 样本上闭合。

## 10. 明确开发边界

- 不修改已修复的 Hex preview。
- 不引入 mock 数据或宿主已挂载 Ceph 的生产旁路。
- 不运行 repair、compact、fsck repair 或写模式 RocksDB。
- 不为性能跳过副本一致性和证据完整性校验。
- 不把 private fixture 路径写入生产逻辑。
- 不在 OSDMap/CRUSH/PG 完成前宣称通用 degraded recovery。
- 不在 FSMap freshness proof 完成前宣称 CephFS absent。
- 不在 snapshot/clone/encryption/journaling 语义闭合前读取相关 image。

## 11. 当前实施状态

截至 2026-07-17：

- Stage 0-3 的主要代码已落地并进入最终门禁：read-only parent source、
  typed diagnostic、XFS inode/MACB、Catalog manifest/deep audit、processing ledger、
  lease/heartbeat/recovery 和 processing DTO。
- Stage 4-7 仍为后续能力范围，当前生产路径继续 fail closed。
- 下一实现优先级：
  1. 真实 PVE retained-case 门禁补齐 manifest/deep audit/phase/read-only 断言。
  2. OSD inventory 完整集合证明与 OSDMap epoch。
  3. CRUSH/PG/replica/degraded。
  4. RBD snapshot/clone/object-map/multi-PV。
  5. CephFS presence proof 与重建。
