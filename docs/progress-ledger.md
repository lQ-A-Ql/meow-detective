# Meow~Detective 项目进度台账

> 2026-07-13：新增 PVE 六成员串行导入门禁
> `scripts/check-pve-cluster-import.ps1`。生产后台 runner 在单成员失败后继续尝试
> 后续成员，并在最终 cluster/job 中保存 ready/failed partial 计数。真实样本通过
> `FORENSICS_PVE_CLUSTER_ROOT` opt-in；BlueStore label、BlueFS
> superblock/layout、bounded transaction-log metadata replay，以及 RocksDB
> CURRENT/IDENTITY/活动 MANIFEST control-plane inventory、live-SST 物理结构库存，
> active WAL/WriteBatch metadata 恢复、全部 live-SST 单次 mutation streaming，
> digest-only RocksDB latest-state summary，以及 `S/C/O/X` BlueStore
> onode/blob/shard/extent/checksum/shared-ref semantic snapshot 已完成。
> OMAP catalog、source-bound RADOS range reader、bounded RBD head-reader、
> 持久化 RBD lineage 和派生 VM source DB 已完成。真实 PVE 样本已生成
> 114,260 条 VM 文件记录，并通过五个代表文件、614 MiB 大文件、连续/随机
> bounded range 性能门禁；通用 PG/CRUSH/EC、
> degraded replica recovery、CephFS 和跨节点语义分析仍保持 unsupported。
>
> 同轮结构债务从 17 个模块基线降至 0；另有 5 个
> 501-800 行普通生产模块按 `owner/reason/expires=2026-09-30` 登记正式临时例外。
> 函数债务从 17 降至 9。例外只用于本轮无法安全拆分的 parser/repository 边界，
> 到期前必须完成后续能力族拆分。

本文档记录当前可执行进度和下一开发边界。它只登记已经由代码、提交和验证结果证明的状态；详细能力承诺仍以 `docs/parser-support-matrix.md`、`docs/known-unsupported-formats.md` 和真实样本回归记录为准。

## 当前焦点

| 日期 | 类型 | 范围 | 状态 | 结果 | 下一边界 |
|---|---|---|---|---|---|
| 2026-07-18 | Linux/Ceph | RBD Catalog 与 Artifacts 冷路径性能加固 | Completed for private retained-case gate | Artifacts 最终采用 8MiB read-plan cache 且关闭 mmap，冷重放 `130.62s / 715MiB`；Catalog 使用隐藏 build DB、4,000 行 / 16MiB 双上限和 64MiB WAL checkpoint，真实零派生源重建 `83.574s`，114,260 行、三 XFS 分区、预览、deep manifest 和零 sidecar 通过 | 持久化 Catalog frontier/cursor、inventory 完整集合证明 |
| 2026-07-17 | Linux/Ceph | PVE 派生 Catalog 与 processing 加固 | Implemented / final gate pending | 父 source DB 只读路由、O(1) Catalog manifest、显式 deep audit、typed completeness diagnostic、XFS v1/v2/v3 inode/MACB、六阶段 processing ledger/lease/heartbeat/recovery 及 `get_data_sources` processing DTO 已落地 | 补 retained-case 只读/manifest/deep-audit 门禁后进入 OSDMap epoch 与 inventory 完整集合证明 |
| 2026-07-12 | Backend/Stage 7 | 文档、最终工程审计、全量门禁与真实样本验收 | Completed | 结构守卫、Rust/frontend 全量门禁、检材3 20 项、双顺序隔离和检材2性能门禁通过；工程评分 99/100 | 继续按 baseline 单调清理剩余结构债务 |
| 2026-07-13 | Backend/Post-Stage 7 cleanup | 历史 module-root 清理与 PVE BlueStore 边界分类 | Completed | `fs-fat`、`fs-exfat`、`image-e01` facade 均降至 200 行内，module baseline 清零；随后 BlueStore 从 typed unsupported 升级为 metadata-only inventory | 清理 5 个临时 module exception 与 9 个函数债务 |
| 2026-07-12 | Backend/Stage 5-6 | Parser/core 能力拆分与测试物理隔离 | Completed | parser/filesystem 能力族完成；非 vendored `src/` 测试债务降至 0 | Stage 7 最终验收 |
| 2026-07-11 | Backend/Stage 3-4 | Transport/command 与 app-services 拆分 | Completed | command/service 边界守卫通过，command raw SQL 为 0，service 保持 Tauri-free | Stage 5 parser/core 拆分 |
| 2026-07-13 | Backend/Stage 4 closure | 清理 app-services 剩余上帝模块与函数债务 | Completed | app-services 模块基线 7→0、函数基线 20→0；全 workspace 门禁、双顺序检材2/检材3隔离、报告/Registry/临时文件失败路径回归通过 | 保持 app-services 零债务并继续清理 parser/core 历史基线 |
| 2026-07-11 | Backend/Stage 2 | Windows/Linux 平台域与多源读写隔离 | Completed | 双顺序真实 E01 回归通过；ready-source、报告归属、Graph 分页、前端切源均加固 | Stage 3 transport/command 拆分 |
| 2026-07-10 | Backend/Stage 0 | 模块、函数、测试物理边界基线 | Completed | 三项结构守卫、单调 baseline、进程树/路径 identity 加固、数据源删除两阶段恢复与真实样本冻结完成 | Stage 1 移除 macOS 生产支持 |
| 2026-07-10 | Linux/PVE | 集群成员导入建模 | Completed | 文件夹发现 6 个 E01 成员，成员保持独立数据源与独立数据库 | 集群级语义关联 |
| 2026-07-10 | Linux/LVM | direct LV 与 dm-thin 只读映射 | Partial | direct root LV 与基础 thin metadata/block mapping 已实现并 fail closed | metadata checksum、更多 thin 变体 |
| 2026-07-10 | Linux/EXT4 | PVE 宿主文件系统 | Completed for private baseline | 三个 `disk01` 的 `pve/root` 均可枚举和预览；代表成员导入 56,471 文件、5,931 目录 | 公开 fixture、更多 incompat feature |
| 2026-07-13 | Linux/Ceph | BlueStore / BlueFS Stage 3 | Metadata replay completed / content and object reconstruction unsupported | 三个真实 `disk02` 完成有界 BlueFS transaction replay，原子持久化 4 个事务、5 个目录及 44/49/42 个文件 metadata；保持 `ready_metadata` 和零普通文件行 | 后续为 RocksDB 内容解析、RADOS/PG/object 与 VM disk reconstruction |
| 2026-07-13 | Linux/Ceph | BlueStore Stage 4 | RocksDB control-plane inventory completed / SST and object content unsupported | 三个真实 `disk02` 读取 CURRENT/IDENTITY/活动 MANIFEST，完成 39 个 VersionEdit、12 个 column family 和 35/40/33 个 live SST metadata 的确定性回放与原子持久化 | 后续为 SST/WAL 内容、BlueStore object key/value、RADOS/PG/RBD reconstruction |
| 2026-07-14 | Linux/Ceph | BlueStore Stage 5 | Live-SST structure inventory completed / semantic reconstruction unsupported | 三个真实 `disk02` 的 35/40/33 个 live SST 全部完成 BlueFS identity、footer v5、XXH3、LZ4、properties、index、data-block/entry 计数和有界脱敏 key-space census，并与 OSD/BlueFS/MANIFEST 在 source DB 中原子持久化 | 后续为 WAL/latest-state、onode/blob/value、RADOS/PG/RBD/VM reconstruction |
| 2026-07-14 | Linux/Ceph | BlueStore Stage 6.1 | WAL/WriteBatch metadata recovery completed / latest-state unsupported | 三个真实 `disk02` 完成 active WAL 定位、physical log/WriteBatch 解码和 source-local provenance 持久化；WAL 142/120/127 与独立 `ldb` oracle 闭合，六成员串行门禁通过 | 后续为 SST entry stream、RocksDB latest-state reducer、onode/blob/value 与 RADOS/RBD/VM reconstruction |
| 2026-07-14 | Linux/Ceph | BlueStore Stage 6.2 | Entry-stream parser foundation validated / full live-set and latest-state pending | 新增逐 block、借用 raw slice 且带 block/entry provenance 的 fallible visitor；external-SST sequence fail closed，point order、range 语义和独立资源预算闭合；代表 `000146.sst` 的 148 blocks、23,364 entries 和 raw byte oracle 通过 | 先补全 `35/40/33` live-set digest 与可回滚 spool，再实现 latest-state reducer |
| 2026-07-14 | Linux/Ceph | BlueStore Stage 6.3 | Full live-set + WAL latest-state summary completed / BlueStore semantics pending | 三个真实 OSD 的 `35/40/33` live SST 与 active WAL 进入 source-local disposable spool；value/delete/single-delete/range-delete、Ceph `T`/`b` merge 完成有界 reduction，每个 OSD 持久化 12 个 digest-only CF summary，canonical aggregate oracle 闭合，六成员回归 `50.31s` | Stage 6.4 在 reducer 生命周期内解析 BlueStore `S/C/O/X` 与后续 OMAP，不持久化 raw key/value |
| 2026-07-15 | Linux/Ceph | BlueStore Stage 6.4 | `S/C/O/X` semantic snapshot completed / RADOS content pending | 三个真实 OSD 完成 super/collection/onode/shard/blob/logical+physical extent/checksum/shared-ref 规范化；semantic snapshot 与 latest-state、OSD/device 原子绑定，三组精确 count/digest oracle 闭合；六成员串行回归 6/6 ready，BlueStore 普通文件行仍为零 | Stage 6.5 先完成 OMAP 与 object content reader，再进行 PG/replica/RBD 重建 |
| 2026-07-15 | Linux/Ceph | BlueStore Stage 6.4 performance hardening | Full E01 rerun passed | 修复两处 object/blob child `O(n^2)` 路径，checksum 改为 compact numeric row + canonical object ordinal，semantic child rows 使用运行时 bind limit 批量写入；单 `server01-disk02` 从 544.105s 降至 92.673s，peak RSS 从 589MB 降至 537MB，count/digest oracle 不变 | 六成员完整性能复跑按发布门禁需要执行；Stage 6.5 继续 RADOS/OMAP |
| 2026-07-15 | Linux/Ceph | BlueStore Stage 6.5/6.6 | RADOS/RBD foundation completed / real image oracle pending | 修复真实 `PerPg`/`PgMeta` 无 Header OMAP scope 阻断；OMAP catalog、source-bound RADOS range reader、显式配置 inventory 集合的副本冲突拒绝、RBD striping/head reader 与 filesystem probe foundation 已通过 37 项重建测试和真实六成员回归；六成员耗时 353.71s，`ready=6`、`failed=0` | 建立真实 RBD image byte oracle，之后进入 VM partition/filesystem integration；PG/replica placement 与 CephFS 仍不做 |
| 2026-07-16 | Linux/Ceph | Stage 6 RBD derived VM | Private real-sample VM tree and preview completed | 从显式加载的三个 OSD inventory 重建 `vm-100-disk-0`；派生独立 source DB 枚举直接 XFS 与 `centos/home`、`centos/root`，得到 114,260 条文件记录并通过真实 `/etc/passwd` range 预览。文件树事务、SQLite 集合式 Graph 投影和 XFS 有界目录缓存将未完成的 `>1h47min / 6,828 rows` 路径降至两次首次物化实测 `46.28s / 54.73s`；幂等物化实测 `124ms / 136ms` | 通用 PG/CRUSH/EC、degraded replica、multi-PV RBD LVM 与 CephFS 仍 fail closed；当前尚未独立证明已加载 inventory 等于集群完整副本集合 |
| 2026-07-17 | Linux/Ceph | RBD VM preview performance hardening | Completed for private three-source gate | 修复 XFS 整文件 materialize，建立 source-scoped runtime、opaque preview session、scope generation、retire/read-drain、前端异常 close、target-only XFS path lookup 和最多 256 KiB 请求内页合并。统一门禁绑定 RBD、检材3原生 XFS、PVE 宿主 EXT4 固定 oracle，并验证 viewer/media 字节一致、source/case invalidation 与 `1 -> 2 -> 3` 冷重建。提交 `db49698a` 的 RBD 三轮中位：cold file read `349.992ms`、warm 64 KiB p95 `0.699ms`、`4x1 MiB` p95 `234.152ms`、大文件随机 64 KiB p95 `73.804ms`；对 `1ms` 噪声下限后的 native 比值 `0.699x` | 首次三 OSD runtime 初始化中位 `3.186s` 继续单独优化；补浏览器端 media 时序、容量 LRU eviction 与 inventory 完整集合证明 |
| 2026-07-19 | Linux/Ceph | PVE 六成员完整串行复跑与 CephFS 设计启动 | Cluster regression passed / CephFS design only | `ready=6`、`failed=0`；三宿主 EXT4 为 `62,403/62,380/62,405` 条记录，三 BlueStore 为 `ready_metadata`，RBD 派生 VM 为 `114,260` 条；内部耗时 `712.968s`，测试进程墙钟 `805.22s`。RBD 局部 metadata diagnostic 与宿主 legacy partition-root fallback 记录为非阻断风险；CephFS 仍为 `indeterminate (strongly leaning absent)`，未创建 CephFS source | 先执行 CephFS Stage 0 presence proof，再进入 FSMap/MDSMap、metadata pool、journal 和 namespace 重建；RBD 路径保持冻结 |

## 代码里程碑

| 提交 | 日期 | 类型 | 状态 | 说明 |
|---|---|---|---|---|
| `72493fce` | 2026-07-12 | Stage 6 | Completed | Rust 测试正文与生产 `src/` 物理隔离 |
| `4c2bd3a7` | 2026-07-12 | Stage 5 | Completed | Parser 与 filesystem 能力族拆分 |
| `49561c9a` | 2026-07-11 | Stage 4 | Completed | Application service 上帝模块拆分 |
| `c3ae351b` | 2026-07-11 | Stage 3 | Completed | Transport 与 desktop command 模块拆分 |
| `7ac7e695` | 2026-07-11 | Stage 2 | Completed | Windows/Linux 平台同层与隔离 |
| `aed82c02` | 2026-07-11 | Stage 1 | Completed | 移除 macOS 生产支持 |
| `7f783497` | 2026-07-10 | 数据隔离 | Completed | 加固 source database isolation |
| `0498b4e7` | 2026-07-10 | 集群导入 | Completed | 增加 Linux cluster import modeling |
| `8d2f84e2` | 2026-07-10 | 生命周期 | Completed | 加固 Linux cluster import lifecycle |
| `1b60ded1` | 2026-07-10 | LVM/PVE | Completed | 加固 Linux E01 cluster parsing 与诊断 |
| `bddef98c` | 2026-07-10 | dm-thin | Partial | 增加只读 LVM thin reader，保留 checksum/repair 边界 |
| `38940702` | 2026-07-10 | EXT4/PVE | Completed | 修复 64-byte group descriptor、高 inode 定位和有界 inode cache |

## 真实样本基线

| 样本 | 测试面 | 当前结果 | 记录 |
|---|---|---|---|
| `D:\獬豸杯\检材2.E01` + `D:\獬豸杯\检材3.E01` | Windows/Linux 双顺序串行导入、独立 source DB、分区、文件树、预览、分析 ID 隔离 | 通过，Windows -> Linux 96.92s；Linux -> Windows 94.63s | `docs/real-sample-regression/2026-07-11-backend-refactor-stage2.md` |
| `D:\獬豸杯\检材3.E01` | LVM direct LV -> XFS -> 文件树/预览/Linux artifacts | 通过私有 Stage 0 baseline | `docs/real-sample-regression/2026-07-05-linux-stage0-jiancai3.md` |
| `E:\pangushi\服务器` | 6 成员发现、PVE root EXT4、BlueStore/RBD/VM 重建 | 宿主文件系统通过；三个 BlueStore OSD 完成 semantic/OMAP/RADOS；显式加载的三 OSD inventory 已重建 `vm-100-disk-0`，派生独立 source DB 含 114,260 条 VM 文件记录、直接 XFS 与两个 XFS LV。五个代表文件覆盖 `1,019 B` 至 `614,794,240 B`、连续/随机/文件尾 range，并通过 RBD/原生 XFS/宿主 EXT4 三源门禁、viewer/media parity 和 source/case invalidation 冷重建。通用 PG/CRUSH/EC、degraded replica、集群完整副本集合证明与 CephFS 未验收 | `docs/real-sample-regression/2026-07-16-pve-rbd-derived-vm.md` |

样本路径只用于本地 opt-in 回归，不得进入生产逻辑。

## 当前验收事实

- `pve_cluster_` 四项真实样本回归全部通过。
- Windows/Linux 双源严格串行导入通过，两个数据源的 source DB、平台、文件树、预览与全局 ID 保持隔离。
- `fs-ext4` 32 项单元/文档测试通过，`fs-lvm` 75 项测试通过。
- 代表 PVE 宿主导入结果为 `files=56471`、`dirs=5931`、`totalBytes=5250350224`。
- `/etc/passwd`、`/etc/os-release`、`/etc/hostname`、`/var/lib/pve-cluster/config.db` 可通过 `FileEntryId` 预览。
- BlueStore label、BlueFS replay、RocksDB control-plane/live-SST/WAL/latest-state、`S/C/O/X` semantic snapshot、OMAP catalog、source-bound RADOS range reader、bounded RBD head-reader、持久化 lineage、派生 VM source DB、114,260 条文件记录与真实预览已完成私有样本验收。通用 PG/CRUSH/EC、degraded replica、multi-PV RBD LVM、CephFS 和跨节点语义分析仍不得标记为完成。
- RBD 物化旧路径运行 `>1h47min` 仅产生约 6,828 条记录。持久 source connection、每副本 read-plan LRU、64 KiB verified page cache、文件树单事务、SQLite 集合式 Graph 投影和 XFS 有界目录缓存完成后，两次 retained-case 首次物化实测为 `46.28s` 与 `54.73s`；其中最新一次 phase 为 probe `3.084s`、LVM `0.251s`、小 XFS `0.797s`、`centos/root` XFS `46.586s`、Graph `3.704s`、checkpoint `0.008s`。已完成 source 的幂等物化观测为 `124ms` 与 `136ms`，ready tree + preview 测试体观测为 `6.05s` 与 `6.54s`。深度 isolation/oracle 测试 `131.53s` 主要用于读取三个约 1 GiB source DB 并执行完整性检查；历史 `507.19s` 混合 Cargo、六成员导入和深审计，不作为生产物化或发布性能基线。
- VM 预览性能加固已完成 bounded-range、source-scoped runtime、opaque session、scope generation、retire/read-drain、固定 SHA-256 oracle、XFS target-only path lookup 和请求内 256 KiB 页合并。提交 `db49698a` 的三轮统一门禁同时覆盖 RBD、检材3原生 XFS 与 PVE 宿主 EXT4；RBD 中位为 cold file read `349.992ms`、cold runtime + file `3,185.749ms`（仅报告）、warm 64 KiB p95 `0.699ms`、连续 `16x64 KiB` p95 `16.741ms`、连续 `4x1 MiB` p95 `234.152ms`、614 MiB 文件随机 64 KiB p95 `73.804ms`。原生 XFS warm/`4x1 MiB` 为 `0.058ms / 11.165ms`，宿主 EXT4 为 `0.077ms / 7.991ms`；RBD/native 门禁比值按 `1ms` 分母噪声下限为 `0.699x`。viewer/media 字节一致，source/case invalidation 后 provider 精确 `1 -> 2 -> 3` 且 session 归零；runtime cache `117,440,512 B`，RSS delta `399-448 MiB`。当前剩余性能债务集中在首次三 OSD E01/LVM runtime 初始化、浏览器端 media 时序和容量 LRU eviction。
- 派生 RBD readiness 已拆为两层：`import_state=ready` 仅代表 Catalog 可浏览，`data_source_processing_phases` 单独记录 Catalog/Graph/Platform/Artifacts/Timeline/Search 的 pending/running/ready/failed/deferred、version、input fingerprint、owner/attempt、lease 与 heartbeat。ready reopen 从 `source_meta` O(1) 读取版本化 manifest；完整 114,260 行 digest 只由显式 deep audit 执行。Catalog digest 覆盖稳定父路径身份、type/status/MACB/partition，typed filesystem diagnostic 决定完整性。父 BlueStore source DB 经 reconstruction route 只读打开，不执行 migration 或创建 WAL/SHM。
- 2026-07-18 retained-case 冷路径门禁：Artifacts 在严格三副本校验下由 `144.74s / 730MiB` 降至 `130.62s / 715MiB`；真实零派生源 Catalog 从 500 行提交的 `100.314s` 降至最终 4,000 行 / 16MiB 双上限和 64MiB WAL checkpoint 的 `83.574s`。16,000 行方案仅再快 `0.34s`，因取消延迟与最坏内存成本被拒绝。Catalog 发布前保持 quick-check、WAL 收敛、原子 rename；受控失败立即清理隐藏 build，崩溃后仍需整树重做，持久化 frontier 尚未实现。
- BlueStore semantic persistence 的保留真实 source DB phase benchmark 已由
  真实数据验证为 `68.34..77.69s / 311MB`。`E:` 重新挂载后的单成员全链复跑为
  `92.673s / 537MB`，相同命令的本轮优化前结果为 `544.105s / 589MB`；
  semantic digest、精确行数、单事务提交和零普通文件行均保持不变。
- Stage 7 后续清理事实：模块 baseline 0 行、正式临时例外 5 行、函数 baseline 9 行（其中 1 个历史函数超过 150 行）、test-layout baseline 0 行；`app-services` 模块与函数 baseline 均为 0，所有 baseline 只允许减少，临时例外不得无审查延期。
- 检材2三次性能回归：total median `13.479s`、enumeration median `8.488s`、RSS `582MB`、每次 `91,737` rows、最低 `9,892 rows/s`。

## 更新规则

- 每个可交付 stage 完成后新增一条日期记录，不覆盖历史记录。
- `Completed` 必须同时具备代码提交和自动化或真实样本验证。
- `Partial` 必须写明剩余边界；不能用编译通过替代功能验收。
- 历史计划和审计移动到 `docs/archive/<type>/<YYYY-MM>/`，不继续在本台账累积过程细节。
