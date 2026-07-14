# PVE BlueStore RocksDB Stage 6.2 SST Entry Stream Foundation 真实样本回归

## 范围

- 原始样本：`E:\pangushi\服务器` 中 `server01-disk02.E01`。
- 派生只读 fixture：活动 MANIFEST 引用的 `000146.sst`。
- fixture 大小：`307,253` bytes。
- fixture SHA-256：
  `e9c21f20d24f29c9594228929a74e0b0114f48443221aa06fc86af5f85a66944`。
- fixture 只用于显式 opt-in 回归，测试结束后删除，不进入仓库。

Stage 6.2 只验证 SST mutation stream。它不合并 SST/WAL latest state，不解释
BlueStore key/value，不持久化 raw key/value，也不创建普通文件树。

## 运行方式

```powershell
$env:FORENSICS_PVE_SST_FIXTURE='D:\private-fixtures\000146.sst'
cargo test -p rocksdb-wire --test sst_real_sample -- --ignored --nocapture
```

环境变量缺失或路径不是文件时测试必须失败，不允许跳过后伪装为通过。fixture
从只读 EWF/LVM/BlueFS 链路导出；导出环境使用只读 loop device，完成后卸载并
删除临时文件。

## Oracle

| 项目 | 期望 |
|---|---:|
| RocksDB table | block-based format v5 |
| Checksum | XXH3 |
| Compression | LZ4 |
| Column family | `default` / ID `0` |
| Data blocks | `148` |
| Entries | `23,364` |
| Deletions / merges / range deletions | `0 / 0 / 0` |
| Raw key bytes | `420,609` |
| Raw value bytes | `298,145` |
| Data / index / filter bytes | `245,834 / 3,106 / 58,437` |

结构值来自 pinned RocksDB `sst_dump` oracle，并已在 Stage 5 与
`inspect_sst` 闭合。Stage 6.2 增加第二条独立断言路径：

- visitor 逐个 data block 读取，不建立全表 entry vector；
- callback 收到借用的 user key/value 和已解码 sequence/type；
- callback 同时收到 raw internal key、block handle、block ordinal 和 entry
  ordinal，后续 reducer 不需要猜测来源；
- visitor entry 数、raw byte 数、smallest/largest sequence 与
  `inspect_sst` 精确一致；
- visitor 累计解压字节与 Stage 5 census 精确一致；
- 当前样本不存在 range tombstone。

## 结果

```text
running 1 test
test representative_pve_sst_matches_independent_sst_dump_oracle ... ok
test result: ok. 1 passed; 0 failed
```

同轮 synthetic 测试还验证：

- value/delete/merge/range callback 分流；
- visitor typed error 立即停止，后续 block 不再读取；
- 跨 data block internal-key regression 被拒绝；
- empty range 保留为合法无效果记录，反向 range、unknown value type、
  external-SST global sequence、restart/checksum/count 错误 fail closed；
- data-block、total-entry、range-delete 和累计解压预算在触发超限 callback
  前失败。

## 取证与资源边界

- raw key/value 只在 callback 生命周期内有效，不写日志、source DB 或报告。
- 常驻内容为 SST layout/index、单个解压 block 和当前 reconstructed key。
- 默认累计解压预算为 `1 GiB`，单 stored/decompressed block 继续受
  `SstReadOptions` 限制；entry stream 另有独立总量限制。
- visitor 在最后仍核对 table properties；调用方必须将输出写入可丢弃 spool
  或事务，在完整成功前不得发布部分状态。
- 三个 BlueStore source 继续保持 `ready_metadata`，普通 `file_entries` 为零。

## 剩余边界

Stage 6.2 证明 live SST 可以安全流式解码，但不代表 RocksDB logical state
已经恢复。当前真实 oracle 只覆盖代表 `000146.sst`，range-only SST 仍 typed
unsupported；下一步必须先对全部 `35/40/33` live SST 建立确定性 digest 与
可回滚 spool，再实现有界 latest-state reducer，正确处理 SST level、sequence、
delete/single-delete/range-delete、Ceph merge operator 和 WAL overlay，随后才能
进入 BlueStore onode/blob/extents、RADOS、RBD 与 VM 文件系统重建。
