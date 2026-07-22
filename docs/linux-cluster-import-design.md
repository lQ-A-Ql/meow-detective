# Linux 集群导入与分析设计

## Summary

本方案将 Linux / PVE 集群作为一个 case-level evidence set 建模。导入时用户选择一个文件夹，文件夹中包含该集群的全部 E01/RAW 镜像；后端扫描文件夹、生成集群 manifest、注册 `data_source_clusters` 控制记录，并把成员镜像通过统一有界调度器导入到各自独立的 `source.db`。默认最多两个成员同时进行，单成员仍复用单源链路。

本轮只落地“集群导入与成员隔离”。集群级分析、PVE thin-pool 重建、跨节点关联与 VM 磁盘重组不在本轮执行范围内。

## Development Baseline

- 单源 Linux 基线：`D:\獬豸杯\检材3.E01`
- 集群样本目录：`E:\pangushi\服务器`
- 当前已复用能力：
  - E01/RAW 读取
  - 分区探测
  - LVM direct LV 展开
  - XFS 文件树枚举与预览
  - Linux artifact extraction
  - 数据源独立 `source.db`
  - 全局重 IO 导入串行 gate；集群内部成员由 admission 控制并发

## Engineering Boundary

- 前端只提交用户意图：`sourceKind=linuxCluster`、`platform=linux`、`sourcePath=<folder>`。
- 前端不得扫描文件夹、推断镜像成员、计算 cluster manifest 或参与 LVM/PVE 解析。
- 后端负责文件夹扫描、成员分类、manifest、控制库登记、成员导入与状态流转。
- 每个成员镜像仍拥有独立 `sources/<dataSourceId>/source.db`。
- 集群整体只保存在 `app.db` 控制库与 `clusters/<clusterId>/cluster-manifest.json`，不把多个成员数据混写到同一个 source DB。
- 初版只扫描所选目录第一层文件，不递归进入子目录，避免误导入大目录。
- E01 多段镜像只选 `.E01` / `.ewf` 起始段，跳过 `.E02` 等后续段。
- 目录扫描不得静默吞掉 `read_dir` 单项错误；不可读成员必须让导入计划失败。
- cluster profile/name 由后端统一 trim 归一化，不能只依赖前端输入清洗。
- manifest 采用同目录临时文件 + rename 写入，避免崩溃时留下半截 JSON。
- job、cluster state、member metadata 三处状态必须闭环：启动阶段失败也要把 job 标记为 `failed`。
- repository 更新必须检查 affected rows；更新 0 行视为链路状态错误。

## Stage Design

### Stage 1 - Cluster Import Modeling

Tasks:
- 新增 `ImportSourceKindDto::LinuxCluster`。
- 新增 `data_source_clusters` 控制表。
- `data_sources` 增加 `cluster_id`、`cluster_member_index`、`cluster_member_count`。
- `cluster_service` 负责扫描目录并生成 `LinuxClusterImportPlan`。
- manifest 写入 `clusters/<clusterId>/cluster-manifest.json`。
- `data_source_clusters.import_state` 使用限定状态集合：`pending/importing/ready/failed/cancelled`。
- `cluster_id` membership 更新必须命中真实数据源，禁止静默成功。

Expected result:
- 一个文件夹可被显式登记为一个 Linux cluster evidence set。
- 成员镜像具备稳定排序、成员序号和总数。

### Stage 2 - Bounded-Parallel Member Import

Tasks:
- 一个 cluster import 对应一个后台 job。
- job 内部通过统一调度器有界并行执行现有单镜像 import pipeline，默认最多两个成员。
- 每个成员独立 attach、enumerate、post-import、finalize。
- 集群总 CPU 权重不超过六，单成员 import/analysis worker 上限为三；内存 admission 默认按两个成员分摊 4096 MiB 预算，分析 worker 在 RSS 接近软上限时动态降档。
- attach 后更新成员 `cluster_id/member_index/member_count`。
- 任一成员失败时 cluster 进入 `failed`，保留已完成成员状态。
- register / manifest / state 初始化任一失败时，job 必须进入 `failed` 并发出失败事件。
- cancellation 事件与普通单源导入保持一致，避免前端 Jobs 抽屉停留在 running/cancelling。

Expected result:
- 集群导入可以在不超过资源预算的情况下并发写多个彼此独立的 source DB，不破坏已有单源导入路径。

### Stage 3 - Cluster-Level Analysis Boundary

Tasks:
- 后续 Linux 分析命令增加 clusterId 聚合入口。
- 聚合只读取成员 source DB，不移动或复制成员数据。
- 节点身份从 hostname、machine-id、corosync、PVE config 中提取。
- PVE thin-pool / VM disk reconstruction 作为独立 stage 处理。

Expected result:
- 调查员能以“集群整体”查看节点配置、日志与关键痕迹。

## Test Matrix

| Area | Case | Expected |
|---|---|---|
| DTO | `sourceKind=linuxCluster` + `platform=linux` | 反序列化、校验、序列化稳定 |
| DTO | `sourceKind=linuxCluster` + `platform=windows` | validation error |
| Cluster scan | 文件夹包含 `node-a.E01`, `node-a.E02`, `node-b.raw`, `notes.txt` | 只导入 `node-a.E01` 与 `node-b.raw` |
| Cluster scan | 文件夹只有 1 个镜像 | `InsufficientSources` |
| Manifest | 有效 cluster plan | 写入 `clusters/<clusterId>/cluster-manifest.json` |
| Manifest | 写入过程 | 使用临时文件 + rename，成功后无 `.tmp` 残留 |
| Repository | 插入 cluster 记录并更新状态 | `pending -> importing -> ready/failed` |
| Repository | 更新不存在的 cluster/source | 返回错误，不静默成功 |
| Repository | 非法 cluster state | CHECK constraint 拒绝 |
| Import | cluster job | 有界并行调用成员单源导入，成员绑定 cluster metadata，父状态由协调线程顺序归并 |
| Import | 启动阶段失败 | job 标记 `failed`，cluster 如已注册也标记 `failed` |
| Frontend | Linux 集群模式 | 发送 `{ sourceKind: 'linuxCluster', platform: 'linux' }` |

## Acceptance Criteria

- 用户可以在导入弹窗选择 Linux 集群模式，并选择集群文件夹。
- 后端不会把集群文件夹当普通 logical directory 导入。
- cluster manifest 落盘，控制库有 cluster 记录。
- 成员镜像按现有 source DB 隔离策略导入。
- 导入任务对用户表现为一个 cluster job，内部成员最多两个同时执行，取消时等待/运行成员都会收敛。
- 启动阶段、成员导入阶段、取消阶段都有明确 job/cluster 状态，不残留 running 假状态。
- 解析/分析阶段仍不承诺 PVE thin-pool、VM disk、跨节点结论。

## Evaluation

- 默认门禁：
  - `cargo fmt --all -- --check`
  - `cargo check -p forensics-desktop`
  - `cargo test -p app-services cluster_service -- --nocapture`
  - `cargo test -p persistence-sqlite datasource_cluster_repo -- --nocapture`
  - `pnpm --dir frontend typecheck`
  - `pnpm --dir frontend test -- ImportDataSourceDialog`
  - `git diff --check`
- 真实样本手动验证：
  - 选择 `E:\pangushi\服务器`
  - 确认识别到全部首段 E01/RAW 成员
  - 确认 app.db 中 `data_source_clusters` 与成员 `data_sources.cluster_id` 正确
  - 确认每个成员 source DB 可独立浏览文件树与预览
