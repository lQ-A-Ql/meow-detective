# PVE 集群真实导入复跑记录

## 基线

- 日期：2026-07-19
- 样本：`E:\pangushi\服务器`
- 模式：六成员、串行导入、`--test-threads=1`
- 命令：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-pve-cluster-import.ps1 `
  -FixtureRoot 'E:\pangushi\服务器' `
  -RequireFixture `
  -TimeoutSeconds 7200
```

- 证据访问：E01、LVM、BlueStore、RADOS、RBD 全程只读。
- 派生数据：写入案件控制库和独立 source DB，不修改原始 E01。

## 结果

```text
test                                  = passed
cluster state                         = ready
ready members                         = 6
failed members                       = 0
total internal timing                = 712.968s
test process wall clock              = 805.22s
```

成员结果：

| 成员 | 状态 | 文件记录 |
|---|---|---:|
| `server01-disk01.E01` | `ready` | 62,403 |
| `server01-disk02.E01` | `ready_metadata` | 0 |
| `server02-disk01.E01` | `ready` | 62,380 |
| `server02-disk02.E01` | `ready_metadata` | 0 |
| `server03-disk01.E01` | `ready` | 62,405 |
| `server03-disk02.E01` | `ready_metadata` | 0 |
| `vm-100-disk-0` | `ready` | 114,260 |

三份 BlueStore semantic oracle 与本次导入一致：

```text
server01-disk02.E01
  semantic_sha256=794ab1ea6632d809bac456d9cd5e5e54c3a46b93977d2224f98c0d564a46c73
  collections=34 objects=2924 blobs=116135 shards=18971
  logical=116487 physical=134148 checksums=1839658
  shared=23316 shared_refs=27897

server02-disk02.E01
  semantic_sha256=441e1a48ec5ca51e5ff2caa94eac106d283d9375bbbc08d841196eb84fbe78e9
  collections=34 objects=2927 blobs=116135 shards=18970
  logical=116487 physical=134154 checksums=1839666
  shared=23316 shared_refs=27900

server03-disk02.E01
  semantic_sha256=d5eb02ba6e77a66476a2c84f010bca75ec77d870858d15e6b57681fb075028bc
  collections=34 objects=2930 blobs=116135 shards=18974
  logical=116487 physical=134150 checksums=1839646
  shared=23316 shared_refs=27911
```

## 告警与解释

### RBD 局部元数据诊断

RBD filesystem candidate 在分区索引 `2` 产生 `23` 条 localized metadata
diagnostic，但测试仍通过，Catalog 和文件树完整性判定未被 warning 文案绕过。
该问题不阻断本次集群验收，但在 CephFS 设计中必须保留同样的 typed diagnostic
边界，不能把局部缺失静默折叠为完整文件树。

### 宿主 EXT4 legacy partition-root fallback

宿主文件预览多次记录 `legacy partition-root name fallback`。本次回归仍通过，
说明现有 inode/partition 路由可工作，但该路径仍是兼容回退，不应成为 CephFS
对象到文件的正式路由模型。CephFS 设计要求使用 inode、dentry、filesystem
identity 和 object locator，不使用显示名称推断文件来源。

## CephFS 结论

本次复跑没有把 CephFS 从 `indeterminate (strongly leaning absent)` 升级为
`absent` 或 `present`。现有样本仍缺少可证明新鲜的 FSMap/MDSMap 快照和 MDS
元数据对象闭合证据，因此不创建 CephFS 数据源、不生成空文件树。

下一步按
`docs/cephfs-stepwise-reconstruction-design.md`
执行 presence proof、元数据池建模、MDS journal 复放、namespace graph 和
bounded data reader；RBD 现有路径不改变。
