# 预览功能开发日志

**项目**: Forensics 数字取证应用 — 预览功能  
**开发周期**: 1 周  
**开发人员**: MiMo AI Assistant  

---

## 📅 Day 1: 文本预览基础 (Phase 1)

### 任务

- [x] 实现编码检测服务 (text_service.rs)
- [x] 支持 UTF-8/GBK/UTF-16/Latin-1 编码
- [x] 实现二进制文件检测
- [x] 添加语言扩展名映射 (20+ 种)

### 技术决策

**选择 encoding_rs 而非 chardet**:
- encoding_rs 是 Mozilla 维护的高质量库
- 性能更好，API 更清晰
- 支持 BOM 检测

**编码检测策略**:
1. BOM 检测 (最高优先级)
2. UTF-8 验证
3. GBK 检测 (中文环境)
4. 回退到 Latin-1

### 代码统计

- 新增文件: `crates/app-services/src/text_service.rs`
- 代码行数: 250
- 测试用例: 16

---

## 📅 Day 2-3: TextViewer 组件 (Phase 1)

### 任务

- [x] 创建 TextViewer 组件
- [x] 实现行号显示
- [x] 实现分页加载
- [x] 实现搜索高亮
- [x] 集成语法高亮 (highlight.js)

### 技术决策

**选择 highlight.js 而非 Prism.js**:
- highlight.js 支持更多语言 (190+)
- 按需加载语言包，减小包体积
- 自动语言检测功能

**分页策略**:
- 每页 1000 行
- 超过 1MB 内容自动截断
- 前端分页，减少后端压力

### 代码统计

- 新增文件: `TextViewer.tsx`, `SyntaxHighlighter.tsx`
- 代码行数: 310
- 支持语言: 17 种

---

## 📅 Day 4: 后端 API 集成 (Phase 1)

### 任务

- [x] 添加 get_text_preview Tauri 命令
- [x] 定义 TextPreviewDto
- [x] 注册命令到 invoke_handler

### 技术细节

**API 设计**:
```rust
#[tauri::command]
pub async fn get_text_preview(
    state: State<'_, AppState>,
    file_id: String,
    max_bytes: Option<usize>,
) -> Result<TextPreviewDto, CommandError>
```

**返回字段**:
- content: 文本内容
- encoding: 编码名称
- is_truncated: 是否截断
- line_count: 行数
- is_binary: 是否二进制
- language: 编程语言

### 遇到的问题

**问题 1**: encoding_rs 的 Encoding 生命周期问题
- 原因: `decode` 方法需要 `'static` 生命周期
- 解决: 修改函数签名为 `encoding: &'static Encoding`

**问题 2**: 类型不匹配 (u32 vs usize)
- 原因: Tauri DTO 使用 u32，Rust API 使用 usize
- 解决: 添加 `as usize` 类型转换

---

## 📅 Day 5: 图片预览 (Phase 2)

### 任务

- [x] 创建 ImageViewer 组件
- [x] 实现缩放功能 (鼠标滚轮)
- [x] 实现拖拽平移
- [x] 实现旋转功能
- [x] 实现适应窗口
- [x] 添加 get_image_preview API

### 技术细节

**缩放实现**:
```tsx
const handleWheel = useCallback((e: React.WheelEvent) => {
  e.preventDefault();
  const delta = e.deltaY > 0 ? 0.9 : 1.1;
  setScale((s) => Math.max(0.1, Math.min(10, s * delta)));
}, []);
```

**拖拽实现**:
- mousedown: 记录起始位置
- mousemove: 更新偏移量
- mouseup: 结束拖拽

**图片加载**:
- Base64 编码传输
- 前端检测图片尺寸
- 自动适应窗口

### 代码统计

- 新增文件: `ImageViewer.tsx`
- 代码行数: 220
- 支持格式: JPG, PNG, GIF, BMP, WebP

---

## 📅 Day 6-7: 视频/音频预览 (Phase 3)

### 任务

- [x] 创建 VideoViewer 组件
- [x] 创建 AudioViewer 组件
- [x] 添加 get_media_url API
- [x] 实现播放控制
- [x] 实现进度条
- [x] 实现音量控制

### 技术细节

**视频播放器功能**:
- 播放/暂停/停止
- 进度条拖拽
- 音量调节
- 全屏播放
- 快进/快退 (10秒)

**音频播放器功能**:
- 播放/暂停
- 进度条拖拽
- 音量调节
- 快进/快退

**媒体 URL 方案**:
- 使用 Tauri 的 asset:// 协议
- 本地文件直接访问
- 无需 Base64 编码

### 代码统计

- 新增文件: `VideoViewer.tsx`, `AudioViewer.tsx`
- 代码行数: 420
- 支持格式: MP4, WebM, MP3, WAV

---

## 📊 总体统计

### 代码统计

| 模块 | 文件数 | 代码行数 | 测试数 |
|------|--------|----------|--------|
| 后端服务 | 1 | 250 | 16 |
| 前端组件 | 5 | 1130 | - |
| DTO 定义 | 1 | 80 | - |
| Tauri 命令 | 1 | 120 | - |
| **总计** | **8** | **~1580** | **16** |

### 功能覆盖

| 类型 | 支持格式 | 功能 |
|------|----------|------|
| 文本 | UTF-8/GBK/UTF-16 | 行号、分页、搜索、语法高亮 |
| 图片 | JPG/PNG/GIF/BMP/WebP | 缩放、拖拽、旋转、适应窗口 |
| 视频 | MP4/WebM | 播放控制、进度条、音量、全屏 |
| 音频 | MP3/WAV | 播放控制、进度条、音量 |

### 技术栈

**后端**:
- encoding_rs: 编码检测
- base64: 图片编码
- rusqlite: 数据库访问

**前端**:
- highlight.js: 语法高亮
- lucide-react: 图标
- React hooks: 状态管理

---

## 🔜 后续计划

### 短期 (1 周)

- [ ] PDF 预览支持
- [ ] 图片 EXIF 信息显示
- [ ] 视频字幕支持

### 中期 (1 个月)

- [ ] 大文件流式加载
- [ ] 图片编辑功能
- [ ] 音频波形显示

### 长期 (3 个月)

- [ ] 3D 模型预览
- [ ] 文档预览 (Word/Excel)
- [ ] 代码对比功能

---

**日志维护人**: MiMo AI Assistant  
**最后更新**: 2026-05-30
