# plugins-src — Meow~Detective 解析器插件工程

独立的 cdylib 工作区，**不属于主 workspace**（见
`docs/plugin-system-dev-test-plan.md` §3）：插件以 DLL 形态随 exe 旁的
`plugins/{windows,linux}/` 目录分发，不计入主仓 crate 计数，也不进入主仓
守卫脚本的扫描面。代码质量口径与主仓一致（fmt / clippy 零警告）。

## 成员

| 目录 | plugin_id | 说明 |
|---|---|---|
| `prefetch/` | `meow.plugin.prefetch` | M3 试点：Windows Prefetch (.pf) 解析器的 DLL 化 |
| `bt_panel/` | `meow.plugin.bt_panel` | 二期首个实战插件：宝塔面板 SQLite 库解析（Linux 证据） |

## 构建

```bash
cargo build --manifest-path plugins-src/Cargo.toml --release
```

产物 `meow_plugin_prefetch.dll` 拷贝到 exe 旁 `plugins/windows/`、
`meow_plugin_bt_panel.dll` 拷贝到 exe 旁 `plugins/linux/` 即可被宿主
`plugin_loader` 发现（M4 打包脚本负责该拷贝）。

## 一期复用形态（二期再定）

试点插件直接 path 依赖 `crates/artifacts-windows`（及其传递依赖
`artifacts-core` / `domain`），在 DLL 内部运行与内置轨道 A 完全相同的
`PrefetchExtractor`，再把 `VecSink` 捕获的结果序列化为 ABI payload JSON。
这保证双通道（内置 vs 插件）输出**由构造上深度相等**。

注意边界：

- 这些本项目 crate 均不涉 Tauri / mimalloc，故无全局 allocator 冲突
  （`app-services` 的 mimalloc 是禁区，见 `docs/plugin-abi-contract-design.md`
  §2 与 AGENTS.md Gotcha #22）。
- ABI 文档 §2 的"插件仅依赖 plugin-api"是二期目标形态；一期按
  `docs/plugin-system-dev-test-plan.md` §3.2 显式允许 path 依赖复用解析器，
  二期再评估抽纯解析 crate 或其他复用形态。

## 硬性契约（违反 = 宿主进程 abort）

1. **导出函数内自捕获 panic**：MSVC 下跨 FFI 边界的 unwind 会被宿主判为
   foreign exception 直接 `abort`（0xC0000409），宿主 `catch_unwind`
   拦不住（ABI 文档 §8 实测记录）。本工程 `panic = "unwind"` + 每个导出
   函数经 `guarded_extract` 的 `catch_unwind` 包裹，panic 映射为
   `MeowStatus::InternalError`。
2. **谁分配谁释放**：payload / error_message 由本 DLL 分配，宿主读完后
   调 `meow_plugin_free_buffer` 归还。payload 按显式长度释放；
   error_message 是 NUL 结尾字符串（`CString::into_raw`），宿主按
   strlen+1 回传长度。两者都以"长度 == 容量"的缓冲分配，统一由
   `Vec::from_raw_parts(ptr, len, len)` 回收。
3. **不得留存请求指针**：`MeowExtractRequest` 内的指针仅在调用期间有效。

## crate-type 说明

`prefetch` 声明 `crate-type = ["cdylib", "rlib"]`：只有 DLL 是交付物；
`rlib` 仅用于让插件自身的 `cargo test`（panic 自捕获等单元测试）能够
链接，不改变分发形态。
