# 2026-04-28 逐模块深度审计摘要

**总体等级：B- (70/100)**

## 关键数字
- 审计范围：37 crate + 1 Tauri app + 1 React 前端 = ~217K 行代码
- Critical：3 项（app-services SQL 泄露、MFT 重复、unimplemented! 生产路径）
- High：12 项（架构违规、类型错误缺失、上帝函数、unwrap/expect、竞态、文档漂移、NTFS加密无检测、XFS/APFS限制）
- Medium：17 项（组件过大、DTO 不匹配、空桩函数、GPT分区类型不足等）
- 所有 8 个审计维度已完成（Evidence+FS 已补充）

## 最需关注的模块
1. app-services (59/100) — SQL 泄露、重复代码、Result<String> 泛滥
2. fs-xfs (65/100) — inode 硬编码、B+tree 仅 leaf level
3. fs-ntfs (75/100) — 加密文件无检测、压缩降级静默
4. 文档 (45/100) — design.md 严重过时

## 取证完整性关键缺口
- NTFS: 加密文件无提示，压缩失败静默返回垃圾数据
- XFS: inode 基址硬编码，真实镜像可能失败
- APFS: 多 block extent 截断
- GPT: 仅识别 Windows 分区类型

## 审计报告路径
- 详细报告：`development-reports/sessions/deep-module-audit-2026-04-28.md`
- 摘要：`development-reports/summaries/deep-module-audit-2026-04-28.md`
