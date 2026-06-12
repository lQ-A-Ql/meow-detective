# 导出与媒体安全边界

## 1. 目标

明确文件提取、报告导出、媒体预览三条链路的安全边界，避免：

- 静默覆盖
- 路径越界
- 宿主真实路径泄露
- 可推导、长期有效的媒体访问标识

## 2. 文件提取

当前实现：

- `ExtractFileRequest.overwrite` 默认 `false`
- 目标为目录时直接拒绝
- 目标已存在且未显式允许覆盖时返回冲突
- 父目录按需创建
- 写入走临时文件再 rename 的原子流程
- 提取动作写入审计日志

要求：

- 先校验路径，再写入
- 错误返回统一走脱敏 `CommandError`
- 不记录完整宿主绝对路径到审计日志

## 3. 报告导出

当前实现：

- `ExportScopeDto.overwrite` 默认 `false`
- 输出目录由 case workspace 下的 `reports/` 承接
- 导出先写临时文件，再 rename 到目标文件
- 覆盖时需要显式 `overwrite=true`

要求：

- 默认拒绝覆盖
- 导出失败必须清理临时文件
- 错误信息不得把内部路径、SQL、堆栈直接暴露到前端

## 4. 媒体预览

当前实现：

- 小文件可 inline data URL
- 大文件走 protocol / range
- 协议 URL 不直接暴露宿主真实路径
- 媒体 handle 使用 runtime-cache 生成的临时 token
- handle 与当前 case 绑定，并带 TTL
- case close / delete 后对应 handle 失效

要求：

- 媒体 handle 不能再用可推导的 `file:<id>` 形式对外暴露
- protocol 错误信息统一脱敏
- 前端仅把 handle 当作短期、受限凭据使用

## 5. overwrite 口径

- 默认拒绝覆盖
- 如前端提供“覆盖”选项，必须是显式用户操作
- 文档、测试和验收都必须覆盖“默认不覆盖”的真实行为

## 6. 最低测试要求

- 文件提取到已存在文件时返回冲突
- 报告导出默认不覆盖
- 导出与提取都使用原子写入
- 媒体协议与 range 响应不泄露宿主路径
- 大媒体文件使用短期 handle，不暴露 `file:<id>`

