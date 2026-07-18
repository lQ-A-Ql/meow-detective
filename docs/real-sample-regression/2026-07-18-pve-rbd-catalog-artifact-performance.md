# PVE RBD Catalog 与 Artifacts 冷路径回归

**日期**: 2026-07-18  
**样本**: `E:\pangushi\服务器`  
**范围**: `vm-100-disk-0` 派生 Catalog 与 Linux Artifacts 冷重放

## 验证结论

- Artifacts 最终采用 `8MiB` SQLite read-plan cache、关闭 mmap。
- 严格三副本逐字节校验保持不变。
- 冷重放扫描 `15,323` 个候选，读取 `119,326,220` 证据字节。
- 测试体耗时 `130.62s`，峰值 RSS `715MiB`。
- Catalog 最终采用 `4,000` 行或估算 `16MiB` 双上限，以及约 `64MiB`
  WAL 自动 checkpoint 阈值。
- 零派生源案件副本的真实 Catalog 物化耗时 `83.574s`。
- 输出为 `114,260` 条记录、`15,749` 个目录、`98,511` 个文件和三个 XFS 分区。
- `/etc/passwd` bounded range 预览、deep manifest 与 `PRAGMA quick_check` 通过。
- 发布完成后没有 `.build`、`-wal` 或 `-shm` 残留。

## 采用与拒绝

| 实验 | 结果 | 决策 |
|---|---:|---|
| Artifacts 4MiB cache、无 mmap | `157.66s / 701MiB` | 拒绝，时间退化 |
| Artifacts 8MiB cache、无 mmap | `130.62s / 715MiB` | 采用 |
| Catalog 500 行、默认 checkpoint | `100.314s` | 安全基线 |
| Catalog 4,000 行、默认 checkpoint | `86.087s` | 有效 |
| Catalog 4,000 行 / 16MiB、64MiB checkpoint | `83.574s` | 采用 |
| Catalog 16,000 行 / 16MiB、64MiB checkpoint | `83.234s` | 拒绝，收益不足 |

## 边界

- Graph、Platform、Artifacts、Search 和 Timeline 在 Catalog 可浏览后由
  `TaskManager` 受管后台执行，不计入 Catalog rebuild 时间。
- 当前 checkpoint 只保证有界事务和隐藏发布；未实现持久化目录 frontier。
- 进程崩溃后会丢弃未发布 build DB 并从头重建，不会暴露部分文件树。
- 该结果只适用于当前私有三副本样本，不扩展为通用 PG/CRUSH/EC、degraded
  replica、CephFS 或 inventory 完整集合证明。
