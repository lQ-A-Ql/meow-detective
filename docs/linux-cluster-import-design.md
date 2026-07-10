# Linux 集群导入与分析设计

## Summary

本方案将 Linux / PVE 集群作为一个 case-level evidence set 建模。导入时用户选择一个文件夹，文件夹中包含该集群的全部 E01/RAW 镜像；后端扫描文件夹、生成集群 manifest、注册 `data_source_clusters` 控制记录，并把每个成员镜像按现有单源链路串行导入到独立 `source.db`。

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
  - 全局重 IO 导入串行 gate

## Engineering Boundary

- 前端只提交用户意图：`sourceKind=linuxCluster`、`platform=linux`、`sourcePath=<folder>`。
- 前端不得扫描文件夹、推断镜像成员、计算 cluster manifest 或参与 LVM/PVE 解析。
- 后端负责文件夹扫描、成员分类、manifest、控制库登记、成员导入与状态流转。
- 每个成员镜像仍拥有独立 `sources/<dataSourceId>/source.db`。
- 集群整体只保存在 `app.db` 控制库与 `clusters/<clusterId>/cluster-manifest.json`，不把多个成员数据混写到同一个 source DB。
- 初版只扫描所选目录第一层文件，不递归进入子目录，避免误导入大目录。
- E01 多段镜像只选 `.E01` / `.ewf` 起始段，跳过 `.E02` 等后续段。

## Stage Design

### Stage 1 - Cluster Import Modeling

Tasks:
- 新增 `ImportSourceKindDto::LinuxCluster`。
- 新增 `data_source_clusters` 控制表。
- `data_sources` 增加 `cluster_id`、`cluster_member_index`、`cluster_member_count`。
- `cluster_service` 负责扫描目录并生成 `LinuxClusterImportPlan`。
- manifest 写入 `clusters/<clusterId>/cluster-manifest.json`。

Expected result:
- 一个文件夹可被显式登记为一个 Linux cluster evidence set。
- 成员镜像具备稳定排序、成员序号和总数。

### Stage 2 - Serial Member Import

Tasks:
- 一个 cluster import 对应一个后台 job。
- job 内部串行执行现有单镜像 import pipeline。
- 每个成员独立 attach、enumerate、post-import、finalize。
- attach 后更新成员 `cluster_id/member_index/member_count`。
- 任一成员失败时 cluster 进入 `failed`，保留已完成成员状态。

Expected result:
- 集群导入不会并发写多个 source DB，也不会破坏已有单源导入路径。

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
| Repository | 插入 cluster 记录并更新状态 | `pending -> importing -> ready/failed` |
| Import | cluster job | 串行调用成员单源导入，成员绑定 cluster metadata |
| Frontend | Linux 集群模式 | 发送 `{ sourceKind: 'linuxCluster', platform: 'linux' }` |

## Acceptance Criteria

- 用户可以在导入弹窗选择 Linux 集群模式，并选择集群文件夹。
- 后端不会把集群文件夹当普通 logical directory 导入。
- cluster manifest 落盘，控制库有 cluster 记录。
- 成员镜像按现有 source DB 隔离策略导入。
- 导入任务对用户表现为一个 cluster job，内部成员串行执行。
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
