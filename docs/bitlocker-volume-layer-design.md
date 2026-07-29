# BitLocker 卷层设计

**状态**: Stage 4-7 已完成（2026-07-29）
**范围**: 只读 BitLocker (BDE) 卷解密，作为 `分区 -> 文件系统` 之间的新一层
**事实来源**: 本文件是 Stage 1-6 的唯一范围依据。上游依赖的溯源事实在
`docs/bitlocker-dependency-decision.md`；能力矩阵在 `docs/parser-support-matrix.md`。

---

## 0. 当前状态（代码事实）

BitLocker 仍由引导扇区签名
`crates/app-services/src/datasource_service/fs_magic.rs:93` 的 `-FVE-FS-`
（偏移 3，8 字节）识别并记录为 `ImageFilesystemKind::BitLocker`。Stage 2b
读路径只有在运行时注册了已验证的 `VerifiedUnlock` 后，才会在精确分区窗口内
构造 `BitLockerReader`，重新探测明文 NTFS/FAT/exFAT，并将读者交给预览链路。
未注册密钥时仍返回 typed locked/unsupported。Stage 3 新增 source-scoped inspect、
密码/恢复密码解锁、显式目录导入和锁定命令；初次导入仍先记录锁定分区，不要求凭据
进入导入请求。

Stage 4 已将“已验证密钥包”的持久化接入真实 Windows Credential Manager：解锁成功后
只保存有界、版本化并经过卷身份校验的 FVEK/tweak 包，不保存密码或恢复密码。恢复时
必须重新读取当前卷元数据、校验 metadata fingerprint、加密方法和密钥长度，并重新探测
明文文件系统后才注册运行时；恢复失败不会留下半激活的运行时状态。非 Windows 构建
返回 typed unsupported，不提供内存或文件系统 mock。锁定只清理运行时密钥，遗忘密钥
只删除 Credential Manager 条目，两者分别审计且互不替代。

Stage 7 补齐了案件重开恢复：控制库的 `bitlocker_restore_intents` 仅保存
`dataSourceId`、分区序号、metadata fingerprint、状态和稳定错误码，绝不保存任何 key
或凭据。案件打开后按已启用 intent 逐卷重新读取元数据，严格比较 fingerprint，再从
Credential Manager 恢复并复探测明文文件系统。单卷恢复失败只让该卷保持锁定；缺失、
损坏或 fingerprint 不匹配的安全密钥会禁用 intent，暂时 I/O/平台故障保留为可重试失败。
已存在的 source DB 文件树不会重新导入。内存镜像恢复密钥的后续设计见
`docs/bitlocker-memory-key-recovery-design.md`；它必须经多重明文 oracle 验证，不能
通过随机字节扫描直接解锁。

Stage 5 已将真实解锁流程接入文件浏览器检查器。BitLocker 面板只在当前分区确认为
BitLocker 时出现；密码和恢复密码仅存在于组件的瞬时输入状态，提交后立即清空，不进入
全局 store、查询缓存、持久化表单或 DTO。目录导入后的状态直接采用后端返回的
`BitLockerCatalogImportDto.volume`，前端不自行推导解锁状态。

Stage 6 已将非秘密 BitLocker inventory 接入 HTML/JSON 案件报告。报告仅输出加密方法、
protector 清单、解锁能力、运行时状态、持久化密钥可用性、明文文件系统和稳定错误码；
不输出密码、恢复密码、FVEK、tweak、metadata fingerprint 或 Credential Manager blob。
持久化密钥恢复通过独立性能回归证明不会重新执行密码 KDF。

Stage 2b 的实际接入点如下：

| 位置 | 当前行为 |
|------|---------|
| `datasource_service/fs_magic.rs:10` | 显示名 `"BitLocker"` |
| `datasource_service/fs_magic.rs:24` | 签名命中即返回该 kind |
| `datasource_service/probe.rs:296` | 映射 `PartitionStatus::EncryptedBitLocker` |
| `datasource_service/probe/gpt.rs:130` | GPT 分区按引导区识别候选；BitLocker 保留为加密候选，等待解锁 |
| `file_service/source_read/bitlocker.rs` | 按 source + partition 路由解密卷 |
| `file_service/viewer/range/*` | Hex/range、文本、图片、文档和媒体范围读取统一走解密读者 |
| `bitlocker_runtime/*` | 仅缓存 `Arc<UnlockedVolume>`，按 case/source/partition/fingerprint 隔离 |
| `bitlocker_service/*` | 案件归属校验、inspect/unlock/lock、审计与解锁后目录枚举 |
| `apps/desktop/src-tauri/src/commands/file_commands/*` | 实际预览命令注入 BitLocker runtime |
| `import_pipeline/partition/*` | 未解锁时排除工作项；解锁后由显式 catalog 用例复用统一分区枚举器 |

文件系统候选和导入 gate 仍保持穷尽处理，预览侧则先依据 source-local 分区元数据
判断是否需要解密，再进入统一的文件系统打开逻辑。这能避免普通 E01/RAW 预览绕过
既有优化读路径，也避免把密文误交给明文文件系统解析器。

两个已在 Stage 0 之前修掉的前置错误：

- `crates/evidence-core/src/volume/mbr.rs` 曾把 MBR 类型字节 `0x42` 标成
  BitLocker。`0x42` 实际是 LDM（Windows 动态磁盘）。MBR 磁盘上的 BitLocker
  卷保留原类型字节（通常 `0x07`），分区表无法识别它。已改为 LDM/`Unsupported`，
  并加了 `no_mbr_type_byte_reports_bitlocker` 回归断言。
- `MbrPartitionStatus::EncryptedBitLocker` 仍然存活，来源是 GPT 类型 GUID
  路径（`probe/gpt.rs`），不是 MBR。

---

## 1. 定版架构

```
E01 / RAW 镜像
  └─ PartitionWindowReader      分区窗口（绝对偏移 -> 卷内偏移 0）
       └─ BitLockerReader       本层新增：明文 Read + Seek 视图
            └─ NTFS / FAT / exFAT Reader
                 └─ 文件树 / 预览 / hex / 媒体 / 分析 / 搜索 / 导出
```

关键性质：

- `BitLockerReader` 只实现 `Read + Seek`，对上层完全透明。已有的 NTFS/FAT/exFAT
  读者不需要知道自己在解密卷上。
- 解密在读路径上按扇区惰性进行，**不产生明文卷副本，不挂载，不修改证据**。
- 原始镜像句柄始终只读。

---

## 2. 核心契约

### 2.1 加密方法与保护器是两个正交维度

原方案把这两者混成一张表，这是必须先纠正的错误，否则 Stage 1 的类型设计会错。

**加密方法**（卷怎么加密，FVE 元数据里的 `encryption_method`）：

| 码 | 算法 | v1 |
|----|------|-----|
| `0x8000` | AES-128-CBC + Elephant Diffuser | 支持 |
| `0x8001` | AES-256-CBC + Elephant Diffuser | 识别但拒绝（上游无 oracle） |
| `0x8002` | AES-128-CBC | 支持 |
| `0x8003` | AES-256-CBC | 支持 |
| `0x8004` | XTS-AES-128 | 支持 |
| `0x8005` | XTS-AES-256 | 支持 |

**保护器**（VMK 怎么被包裹，FVE 元数据里的 protector entry 类型）：

| 码 | 保护器 | v1 |
|----|--------|-----|
| `0x2000` | 密码 | 解锁 |
| `0x0800` | 恢复密码（48 位） | 解锁 |
| `0x0000` | Clear key | 仅清点，不解锁（见 2.2） |
| — | TPM / TPM+PIN | 仅清点 |
| — | 启动密钥（`.BEK`） | 仅清点 |

v1 解锁面 = {密码, 恢复密码} × {`0x8000`, `0x8002`, `0x8003`, `0x8004`, `0x8005`}。
其余组合返回 typed unsupported，但**元数据解析始终报告它找到的全部保护器和方法**
（protector inventory），这是取证价值所在：调查员需要知道"这卷能用什么解锁"。

### 2.2 v1 明确不做

- Clear key 解锁。上游有实现，我们 v1 不启用：它意味着无凭据即可解密，
  取证上应当是一个显式的、被记录的调查员动作，不能是自动 fallback。
  留作 Stage 6+ 的独立决策。
- 启动密钥 / TPM 解锁。
- 密码破解、字典攻击、内存取密钥。
- 写回、挂载、生成明文卷文件。

### 2.3 密码算法

BitLocker 原生：UTF-16LE 编码 -> SHA-256 -> 迭代 stretch -> AES-CCM 解包 VMK
-> AES-CCM 解包 FVEK。**不使用 DPAPI**（DPAPI 是 Windows 用户态凭据保护，
与 BDE 卷密钥推导无关）。恢复密码走 48 位分组 / mod-11 校验后的独立推导路径。

### 2.4 凭据边界

| 事项 | 规则 |
|------|------|
| 密码 / 恢复密码落盘 | 禁止。不入 SQLite、job 参数、事件、日志、错误详情、报告、前端缓存 |
| Credential Manager | 只保存已验证的 FVEK/tweak 密钥包，绝不保存密码或恢复密码 |
| Credential target | `Meow_Detective/BitLocker/v1/<metadataFingerprint>` |
| 运行时注册键 | `caseId + dataSourceId + partitionIndex + metadataFingerprint` |
| 密钥类型 | 禁止派生 `Debug` / `Clone` / `Serialize`；`Drop` 必须 zeroize |
| Stage 3 传参 | 凭据作为独立 secret 参数进入，不进现有可 `Debug`/`Clone` 的导入请求 |
| Stage 5 前端 | 密码不进 Zustand / TanStack Query；锁定与遗忘密钥是两个独立动作 |
| "锁定"语义 | 只清除读取密钥。已入库的文件名、artifact、索引属于案件派生数据，不自动清除 |

### 2.5 读路径形态

- 每次读取用独立 `BitLockerReader`，共享只读的密钥/布局快照。
- 有界扇区缓存，约 128 KB；I/O 合并上限 1 MB。
- 重复的 1 MB 区间读取**不得重跑 KDF**——密钥推导结果按运行时注册键缓存。
- 活跃 reader 数量与内存都必须有上界。
- 锁定后所有新的证据读取返回 `BITLOCKER_LOCKED`，已有 read lease 先排空。

---

## 3. 分阶段计划

每个 Stage 结束后必须单独复审：模块边界、错误分类、凭据泄漏、文件长度、
测试覆盖。**存在 High/Critical 问题不得进入下一阶段。**

| Stage | 内容 | 退出条件 |
|-------|------|---------|
| 0 ✅ | 边界冻结 | 本文件 + 依赖决策定稿；两个 guard 上线；crate 骨架编译通过 |
| 1 ✅ | 元数据与密钥层 | FVE 解析 + 密码/恢复密码推导；protector inventory 可枚举 |
| 2a ✅ | 扇区 cipher 与明文读者 | 五种方法往返、区域映射、有界缓存、合并 I/O、端到端解密 |
| 2b ✅ | 预览读路径 | 运行时 verified unlock registry；分区窗口；Hex/文本/图片/文档/媒体统一路由；锁定返回 typed Unsupported |
| 3 ✅ | 服务与导入编排 | inspect/解锁/锁定；凭据以独立 secret 参数进入；显式 catalog 导入与审计记录 |
| 4 ✅ | 持久化与凭据存储 | 只存已验证密钥包；metadata fingerprint 稳定；Windows Credential Manager 真实读写；恢复复探测；锁定/遗忘独立 |
| 5 ✅ | 前端解锁流程 | 密码不进前端状态层；锁定与遗忘密钥分离；BitLocker 面板按真实分区显示 |
| 6 ✅ | 报告、性能、文档 | HTML/JSON 含非秘密 protector inventory；KDF 不重跑；恢复 oracle 与文档已同步 |

### Stage 1 交付物（已完成 2026-07-27）

`crates/volume-bitlocker` 的元数据、解锁和 cipher 测试，以及 app-services 的
运行时注册和预览路由测试：

- `bytes.rs` — 越界即返回零值的小端读取器。输入是攻击者可控的卷，谎报的长度必须
  在上层变成解析失败，不能在证据读路径上 panic。
- `guid.rs` — 混合端序 GUID 渲染，与 libbde/pybde 输出一致。
- `header.rs` — 三种卷头布局。**`MSWIN4.1` 不能单独证明是 BitLocker**：Windows
  格式化的普通 FAT 卷带同样签名，所以该变体只是候选，必须由 FVE 元数据块确认。
  `HeaderVariant::is_self_identifying()` 把这个区别做成类型上的事实。
- `metadata.rs` — FVE 块与条目解析，三份副本、protector inventory、被 metadata_size
  限界的条目游走。
- `kdf.rs` — UTF-16LE + 双 SHA-256 + 0x100000 轮 stretch + AES-CCM 解包；48 位
  恢复密码的 mod-11 校验先于推导执行（打错一位若漏过去，会产生与"密码错误"
  无法区分的错误密钥）。
- `fingerprint.rs` — 凭据无关的卷身份，用于 Credential target 与运行时注册键。
  保护器集合参与哈希，所以重新加密过的卷不会复用旧密钥包。
- `unlock.rs` — 编排：定位 VMK → stretch → 解包 VMK → 解包 FVEK。两次 AES-CCM
  tag 校验都通过才返回，这是"已验证"的含义。

Stage 2b 前阻断复审已加固元数据冗余副本语义：元数据按声明长度有界精读，拒绝
非 v2 block、短 header、超过 `0x80000` 的 entries、截断 entry 尾部和不完整读取；
密码/恢复密码解锁会对每个结构完整副本继续执行 VMK、FVEK 与 cipher 验证，只有整条
链路成功才选中副本。单个副本的 seek/read/parse/unwrap 失败不会遮蔽后续健康副本。
`VolumeKeyPackage` 构造和 metadata-level 派生不再是 crate 外部 API，外部只能取得
`VerifiedUnlock`，避免调用方伪造“已验证”密钥包。

Stage 1 的取舍记录：

- stretch 轮数在 crate 内部函数上是参数，两个公开入口恒定传
  `STRETCH_ITERATIONS`。测试用小轮数跑编排，另有一个测试走真实轮数的生产路径，
  以及一个反向测试确认小轮数解不开真实卷。
- 不支持的加密方法在任何凭据运算之前就被拒绝，所以不支持的卷不会先花掉一百万轮
  SHA-256 才失败。
- 恢复密码"结构非法"与"就是错的"都映射为 `CredentialRejected`，不向调用方区分。

### Stage 2a 交付物（已完成 2026-07-27）

`crates/volume-bitlocker` 192 个 lib 测试（Stage 1 为 112）。Stage 2b 已在
app-services 读路径接入，不改变该 crate 的只读边界。

- `diffuser.rs` — vendor 进来的 Elephant Diffuser,只有解密方向。带上游回归向量,
  这是唯一能抓到"旋转常量写错"的检查:往返测试抓不到,因为两个方向会一起错。
- `cipher.rs` — 五种方法的扇区变换。三个**错了不报错**的点已各自钉住测试:
  CBC 的 IV 是 `ECB(FVEK, LE128(offset))` 而非裸偏移;diffuser sector key 是
  tweak 对同一块做两次 ECB(第二次 byte[15]=0x80);**XTS 按扇区号定址而 CBC 按
  字节偏移** —— 两轴交叉会解出貌似合理的垃圾。
- `layout.rs` — 纯地址运算,无密钥,可独立测试。三条重塑规则:卷头重定位、
  元数据块置零、`encrypted_volume_size` 之后为明文。已钉住的两个反直觉语义:
  **blanking 优先于重定位**,以及**加密边界按物理偏移判定而非逻辑偏移**。
- `reader.rs` — 明文 `Read + Seek`。`UnlockedVolume`(cipher + layout)用 `Arc`
  共享,每个 reader 只自带证据句柄、位置和 128 KiB 直接映射缓存。缓存按构造有界
  而非靠淘汰策略,所以内存上限在建 reader 那一刻就固定了。

Stage 2a 期间发现并修掉的一个真实缺陷:读到镜像末尾之外时,全零缓冲被送进 cipher,
解出**看起来像数据的垃圾**。在证据路径上这比报错更糟 —— 越界读会拿到貌似合理的
内容且无人报告。现在按实际读到的字节数判定,完全不存在的扇区直接返回零。

I/O 合并已落地并被测试量化:连续 64 KiB 读取只发出 ≤4 次 seek(未合并需 128 次)。
重复读同一区间不触碰证据句柄 —— 这是"重复 1MB 区间不重跑 KDF"验收项的实测形式,
KDF 本来只在解锁时跑一次。

一条已接受的限制:`aes 0.8` 没有 `zeroize` feature,所以 `SectorCipher` 里展开的
AES key schedule 不会在 drop 时擦除(FVEK 字节会)。升到 `aes 0.9` 能解决,但会
拆掉 `xts-mode 0.5`。理由与边界记在 `docs/bitlocker-dependency-decision.md`。

### Stage 3 交付物（已完成 2026-07-27）

- 新增 `inspect_bitlocker_volume`、密码/恢复密码解锁、
  `import_unlocked_bitlocker_catalog` 与 `lock_bitlocker_volume` 六个真实 Tauri command。
- 凭据不定义 transport request DTO；command 收到独立字符串后立即转为不可
  `Debug/Clone/Serialize` 的 `Passphrase`，KDF 返回后不进入目录枚举作用域。
- inspect 响应只包含加密方法、protector inventory、metadata fingerprint、解锁状态
  与明文文件系统，不包含凭据或密钥材料。
- catalog 导入只接受 case/source/partition，依赖已验证运行时密钥，复用统一
  `enumerate_partition_with_fs` 写入 source DB；重复调用遇到真实分区根时幂等返回。
- inspect、unlock 和 catalog 读取都持有 preview scope lease。锁定先 retire source、等待
  活跃读操作排空，再只失效目标分区密钥，最后恢复 source preview 路由；并发 unlock
  不能在锁定窗口内重新注册密钥。
- unlock、lock、catalog import 写入案件审计日志；审计只含 source、partition、
  metadata fingerprint、方法、结果和稳定错误码。
- 若解锁验证成功但明文复探测失败，立即撤销刚注册的运行时密钥，避免半成功状态。
- `check-bitlocker-credential-guard.ps1` 同时禁止 transport DTO 新增 secret 字段。

Stage 3 有意把“解锁”和“目录导入”分成两个命令。这样百万轮 KDF 完成后凭据即可
离开作用域，可能长时间运行的文件树枚举只使用已验证 cipher state；初次镜像导入
仍可在无凭据时完成并保留锁定分区节点。Stage 5 UI 应按 inspect -> unlock -> catalog
顺序编排，但不得把凭据放入 Zustand、TanStack Query 或持久化表单状态。

### Stage 4 交付物（已完成 2026-07-28）

- `PersistedKeyBlob` — v1 有界二进制包，包含 magic、版本、加密方法、metadata
  fingerprint、FVEK/tweak 长度和密钥材料；解析拒绝未知版本、身份不匹配、错误长度、
  截断和尾随数据。包及内部密钥字节均使用 zeroize，且不进入 transport DTO。
- `BitLockerKeyStore` — app-services 的平台无关存储契约。应用层只接收已验证包，
  不知道 Credential Manager 细节，也不把密钥写入 SQLite、案件目录或任务参数。
- `WindowsCredentialBitLockerKeyStore` — 桌面端真实调用 `CredReadW`、`CredWriteW`、
  `CredDeleteW`、`CredFree`；读取完成后在释放 Credential Manager 分配前清零 blob。
  非 Windows 分支只返回 typed unsupported。
- `restore_persisted_bitlocker_key` 与 `forget_persisted_bitlocker_key` — 恢复重新
  读取元数据并复探测明文文件系统后才注册 runtime；`lock` 只清 runtime，`forget`
  只删持久化条目；失败、恢复和遗忘动作写入审计日志。
- `BitLockerVolumeStatusDto.storedKeyAvailable` 及 frontend API 镜像 — 只暴露是否有
  可恢复的持久化条目，不暴露 fingerprint 之外的密钥材料或凭据。
- 验证覆盖：volume-bitlocker 192 项、app-services 770 项、Credential Manager
  Windows 读写删测试、命令注册完整性、credential/module/function/test-layout guards。

### Stage 5 交付物（已完成 2026-07-28）

- 文件浏览器检查器新增 BitLocker 专用真实数据面板，按显式 `dataSourceId + partitionIndex`
  调用 inspect/unlock/restore/import/lock/forget API；非 BitLocker 分区不会渲染该面板。
- 支持密码和 48 位恢复密码选择；输入框使用密码类型、关闭自动完成，提交动作先清空
  输入再进入后端调用；前端只编排请求和展示 DTO，不参与卷解密或状态推导。
- `BitLockerCatalogImportDto.volume` 是目录导入成功后的唯一状态来源，面板显示的明文
  文件系统、保护器和持久化密钥可用性与后端返回保持一致。
- 前端验证覆盖 typecheck、lint、87 个测试文件共 591 项测试、生产构建和 runtime guard。

### Stage 6 交付物（已完成 2026-07-28）

- HTML/JSON 报告命令注入 `BitLockerReportContext`，报告从 ready source DB 读取真实分区
  inventory；CSV 保持 artifact-oriented，不伪装为 BitLocker 专用报告。
- 报告输出脱敏边界由 app-services 单元测试锁定，明确拒绝 credential、FVEK、tweak 和
  fingerprint 字段。
- 32 次连续 persisted-key restore 性能回归通过；恢复路径只解析有界 key envelope、
  重建 cipher，并不调用密码或恢复密码 KDF。
- 新增 `FORENSICS_BITLOCKER_RECOVERY_ORACLE` 与
  `FORENSICS_BITLOCKER_RECOVERY_PASSWORD` 环境门控的 ignored 真实恢复密码 oracle；不
  提交镜像或凭据。

### Stage 0 交付物

1. `docs/bitlocker-dependency-decision.md` — 上游 commit、tree hash、逐文件
   SHA-256、许可与归属、被排除的上游部分、新增 crates.io 依赖。
2. `crates/volume-bitlocker/` 骨架 — 冻结的类型契约（加密方法、保护器、
   错误分类、secret 类型），`#![forbid(unsafe_code)]`，无实现。
3. `scripts/check-bitlocker-credential-guard.ps1` — 禁止凭据进日志/错误/序列化，
   禁止 secret 类型派生 `Debug`/`Clone`/`Serialize`，禁止明文卷临时文件。
4. 公开 oracle 清单（下节）。
5. 注册：`Cargo.toml`、`CLAUDE.md` guard 表、`docs/documentation-index.md`、
   `README.md`、`scripts/check-doc-drift.ps1` crate 计数。

---

## 4. 测试 oracle

上游全部 oracle 都是环境变量门控、不提交 fixture，与本仓库
`check-private-real-sample-tests.ps1` 的私有样本纪律一致。公开可获取的镜像：

| 镜像 | 方法 | 凭据 | 来源 |
|------|------|------|------|
| `bdetogo.raw` | `0x8000` | 密码 `bde-TEST` | dfvfs 测试数据 |
| `bitlocker-1.dd` | `0x8002` | 密码 `jacqueline` | picoCTF 2025 |
| `vault.raw` | `0x8004` | 恢复密码（已公开） | BelkaCTF #6 |
| `m8003.raw` | `0x8003` | 恢复密码（自铸） | 自铸，需自行生成 |
| `m8004.raw` | `0x8004` | 恢复密码（自铸） | 自铸，需自行生成 |
| `m8005.raw` | `0x8005` | 恢复密码（自铸） | 自铸，需自行生成 |

约束：

- 不提交任何 BitLocker 镜像到仓库。体积和来源许可都不允许。
- oracle 测试用 `FORENSICS_BITLOCKER_*_ORACLE` 命名，这样
  `check-private-real-sample-tests.ps1` 的 `FORENSICS_*_ORACLE` 规则会自动
  要求 `#[ignore]`，无需改 guard。
- 私有 E01 门控：`FORENSICS_BITLOCKER_E01_FIXTURE`、
  `FORENSICS_BITLOCKER_PARTITION_INDEX`、`FORENSICS_BITLOCKER_EXPECTED_PATH`、
  `FORENSICS_BITLOCKER_EXPECTED_SHA256`。
- **恢复密码不通过环境变量传给 CI**。它是凭据；本地调查员自己提供。测试断言
  的是解密后扇区的 SHA-256，不是凭据本身。

---

## 5. 验收标准

1. 密码、恢复密码不进入 SQLite、job 参数、事件、日志、错误详情、报告或前端缓存。
2. 原始证据保持只读，不产生明文卷副本。
3. 重复的 1 MB 区间读取不重跑 KDF。
4. 内存占用与活跃 reader 数量有上界。
5. 锁定后新的证据读取返回 `BITLOCKER_LOCKED`，已有 read lease 先排空。
6. 11 个 gate point 全部显式处理，无静默跳过。
7. 支持的方法逐一通过 oracle 校验；不支持的组合返回 typed unsupported 且
   仍报告 protector inventory。
8. `unsafe_code` 保持 forbid；`unwrap`/`expect` 不出现在生产路径。

---

## 6. 待办与已知风险

- **FVEK 落盘的取证风险**：Credential Manager 里的 FVEK 密钥包等价于该卷的
  永久解密能力。当前策略是不自动过期，只提供显式 `forget` 删除动作；Stage 5 面板已
  将“锁定运行时卷”和“删除安全存储”分成两个动作，Stage 6 报告只披露
  `storedKeyAvailable`，不披露密钥材料。调查员仍需明确执行 forget 才能删除密钥包。
- **`catalog_manifest.rs` 版本号**：`unlock_hint` 已经被序列化进 manifest，
  新增解锁状态字段需要 bump manifest 版本，否则旧案件的指纹会失配。
- **descriptor cache 版本升级**：原方案假设存在这个机制，代码里没有。
  Stage 2 若需要缓存失效，得先建机制或改用 `data_source_processing_phases`
  的 input fingerprint。
- **`MSWIN4.1` 误判**：当前检测只看 `-FVE-FS-`，没有这个问题。若 Stage 1
  为了兼容旧版 BDE 而加 `MSWIN4.1` 签名，必须同时校验 FVE 元数据块，
  否则普通 FAT 卷会被误判成 BitLocker。
- **unsafe 代码文档同步**：新 crate 是 forbid，需在相关安全文档中登记。
- **读租约排空**：复用 `preview_runtime` 已有的 retire / read-drain 机制，
  不要新造一套。
