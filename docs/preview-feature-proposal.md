# 文本预览与多媒体预览功能方案

**项目**: Forensics 数字取证应用  
**范围**: 文本预览、图片预览、视频预览、音频预览  
**日期**: 2026-05-30  

---

## 📊 现状分析

### 当前实现

```
┌─────────────────────────────────────────────────────────────┐
│                     文件预览区域                              │
├──────────┬──────────┬──────────┬────────────────────────────┤
│ 十六进制  │   文本   │   预览   │         元数据             │
├──────────┴──────────┴──────────┴────────────────────────────┤
│                                                             │
│  ✅ 十六进制: 已实现，显示文件二进制内容                       │
│  ❌ 文本: 占位符，未实现                                      │
│  ❌ 预览: 占位符，未实现                                      │
│  ✅ 元数据: 已实现，显示文件基本信息                          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 现有 API

| API | 功能 | 状态 |
|-----|------|------|
| `open_file_handle` | 打开文件句柄 | ✅ |
| `read_file_range` | 读取文件范围 | ✅ |

---

## 🎯 功能目标

### 文本预览

| 功能 | 说明 | 优先级 |
|------|------|--------|
| 编码检测 | 自动检测 UTF-8/GBK/UTF-16 | P0 |
| 语法高亮 | 支持常见编程语言 | P1 |
| 行号显示 | 显示行号 | P0 |
| 搜索高亮 | 搜索结果高亮 | P2 |
| 大文件分页 | 分块加载大文件 | P1 |

### 多媒体预览

| 功能 | 说明 | 优先级 |
|------|------|--------|
| 图片预览 | JPG/PNG/GIF/BMP/WebP | P0 |
| 视频预览 | MP4/WebM (基础控制) | P1 |
| 音频预览 | MP3/WAV (播放器) | P1 |
| PDF 预览 | 内嵌 PDF 查看器 | P2 |
| 缩略图 | 文件列表缩略图 | P2 |

---

## 🏗️ 技术架构

### 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        前端 (React)                             │
├─────────────┬─────────────┬─────────────┬───────────────────────┤
│  TextViewer │ ImageViewer │ VideoViewer │     AudioViewer       │
├─────────────┴─────────────┴─────────────┴───────────────────────┤
│                      ViewerContainer                            │
├─────────────────────────────────────────────────────────────────┤
│                      useFileContent Hook                        │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Tauri IPC Commands                           │
├─────────────────┬─────────────────┬─────────────────────────────┤
│ read_file_range │ read_file_chunk │ get_file_thumbnail          │
└─────────────────┴─────────────────┴─────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    后端 (Rust)                                  │
├─────────────────┬─────────────────┬─────────────────────────────┤
│  TextExtractor  │ MediaProcessor  │    ThumbnailGenerator       │
├─────────────────┴─────────────────┴─────────────────────────────┤
│                      file_service                               │
└─────────────────────────────────────────────────────────────────┘
```

---

## 📋 Phase 1: 文本预览 (5 天)

### Task 1.1: 编码检测

**工期**: 1 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.1.1 | 添加 encoding_rs 依赖 | 编码检测库 | 依赖正确 |
| 1.1.2 | 实现 detect_encoding | 自动检测编码 | 检测准确 |
| 1.1.3 | 实现 decode_text | 解码文本 | 正确解码 |
| 1.1.4 | 添加测试 | 单元测试 | 测试通过 |

#### 代码实现

```rust
// crates/app-services/src/text_service.rs

use encoding_rs::{Encoding, UTF_8, GBK, UTF_16LE, UTF_16BE};
use std::io::Read;

/// 编码检测结果
#[derive(Debug, Clone)]
pub struct EncodingInfo {
    pub encoding: &'static Encoding,
    pub name: String,
    pub confidence: f32,
}

/// 文本提取服务
pub struct TextService;

impl TextService {
    /// 检测文本编码
    pub fn detect_encoding(data: &[u8]) -> EncodingInfo {
        // BOM 检测
        if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
            return EncodingInfo {
                encoding: UTF_8,
                name: "UTF-8 with BOM".to_string(),
                confidence: 1.0,
            };
        }
        if data.starts_with(&[0xFF, 0xFE]) {
            return EncodingInfo {
                encoding: UTF_16LE,
                name: "UTF-16 LE".to_string(),
                confidence: 1.0,
            };
        }
        if data.starts_with(&[0xFE, 0xFF]) {
            return EncodingInfo {
                encoding: UTF_16BE,
                name: "UTF-16 BE".to_string(),
                confidence: 1.0,
            };
        }

        // 尝试 UTF-8
        if let Ok(_) = std::str::from_utf8(data) {
            return EncodingInfo {
                encoding: UTF_8,
                name: "UTF-8".to_string(),
                confidence: 0.95,
            };
        }

        // 尝试 GBK (中文环境常见)
        let (decoded, _, errors) = GBK.decode(data);
        if errors {
            // 回退到 Latin-1
            EncodingInfo {
                encoding: encoding_rs::WINDOWS_1252,
                name: "Windows-1252".to_string(),
                confidence: 0.5,
            }
        } else {
            // 检查是否包含中文字符
            let has_chinese = decoded.chars().any(|c| {
                (c >= '\u{4E00}' && c <= '\u{9FFF}') || // CJK Unified Ideographs
                (c >= '\u{3400}' && c <= '\u{4DBF}')    // CJK Extension A
            });
            
            if has_chinese {
                EncodingInfo {
                    encoding: GBK,
                    name: "GBK".to_string(),
                    confidence: 0.8,
                }
            } else {
                EncodingInfo {
                    encoding: UTF_8,
                    name: "UTF-8 (assumed)".to_string(),
                    confidence: 0.6,
                }
            }
        }
    }

    /// 解码文本
    pub fn decode_text(data: &[u8], encoding: &Encoding) -> String {
        let (decoded, _, _) = encoding.decode(data);
        decoded.into_owned()
    }

    /// 提取文本预览
    pub fn extract_text_preview(
        reader: &mut dyn Read,
        max_bytes: usize,
    ) -> std::io::Result<(String, EncodingInfo)> {
        let mut buffer = vec![0u8; max_bytes];
        let bytes_read = reader.read(&mut buffer)?;
        buffer.truncate(bytes_read);

        let encoding_info = Self::detect_encoding(&buffer);
        let text = Self::decode_text(&buffer, encoding_info.encoding);

        Ok((text, encoding_info))
    }

    /// 检查文件是否可能是文本文件
    pub fn is_likely_text(data: &[u8]) -> bool {
        // 检查前 8KB 是否包含过多 null 字节
        let check_len = data.len().min(8192);
        let null_count = data[..check_len].iter().filter(|&&b| b == 0).count();
        
        // 如果 null 字节超过 10%，可能是二进制文件
        (null_count as f64 / check_len as f64) < 0.1
    }
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.1.1 | UTF-8 检测 | UTF-8 数据 | confidence > 0.9 |
| T1.1.2 | UTF-8 BOM 检测 | BOM 开头 | 检测到 BOM |
| T1.1.3 | GBK 检测 | GBK 中文 | 检测到 GBK |
| T1.1.4 | 二进制检测 | 包含 null | is_likely_text = false |

---

### Task 1.2: 实现 TextViewer 组件

**工期**: 2 天  
**负责**: 前端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.2.1 | 创建组件结构 | 基础布局 | 组件可渲染 |
| 1.2.2 | 实现行号显示 | 左侧行号 | 行号正确 |
| 1.2.3 | 实现编码切换 | 手动切换编码 | 切换生效 |
| 1.2.4 | 实现语法高亮 | 代码高亮 | 高亮正确 |
| 1.2.5 | 实现大文件分页 | 分块加载 | 加载流畅 |

#### 组件结构

```tsx
// frontend/src/components/viewers/TextViewer.tsx

import { useState, useMemo, useRef, useEffect } from 'react';
import { ChevronLeft, ChevronRight, FileText } from 'lucide-react';

interface TextViewerProps {
  /** 文件内容 */
  content: string;
  /** 文件编码 */
  encoding: string;
  /** 文件扩展名 (用于语法高亮) */
  extension?: string;
  /** 是否可编辑 */
  editable?: boolean;
  /** 搜索关键词 */
  searchQuery?: string;
}

export function TextViewer({
  content,
  encoding,
  extension,
  searchQuery,
}: TextViewerProps) {
  const [currentPage, setCurrentPage] = useState(0);
  const pageSize = 1000; // 每页行数
  const containerRef = useRef<HTMLDivElement>(null);

  // 分割为行
  const lines = useMemo(() => content.split('\n'), [content]);

  // 分页
  const totalPages = Math.ceil(lines.length / pageSize);
  const currentLines = useMemo(
    () => lines.slice(currentPage * pageSize, (currentPage + 1) * pageSize),
    [lines, currentPage, pageSize]
  );

  // 高亮搜索关键词
  const highlightText = (text: string) => {
    if (!searchQuery) return text;
    const parts = text.split(new RegExp(`(${searchQuery})`, 'gi'));
    return parts.map((part, i) =>
      part.toLowerCase() === searchQuery?.toLowerCase() ? (
        <mark key={i} className="bg-yellow-200">{part}</mark>
      ) : (
        part
      )
    );
  };

  return (
    <div className="flex flex-col h-full">
      {/* 工具栏 */}
      <div className="flex items-center gap-2 px-3 py-1 border-b bg-[#fafafa] text-[11px]">
        <FileText size={12} className="text-[#666]" />
        <span className="text-[#666]">{encoding}</span>
        <span className="text-[#999]">|</span>
        <span className="text-[#666]">{lines.length} 行</span>
        
        {totalPages > 1 && (
          <>
            <span className="text-[#999]">|</span>
            <button
              onClick={() => setCurrentPage((p) => Math.max(0, p - 1))}
              disabled={currentPage === 0}
              className="p-0.5 hover:bg-[#e0e0e0] disabled:opacity-30"
            >
              <ChevronLeft size={12} />
            </button>
            <span className="text-[#666]">
              {currentPage + 1} / {totalPages}
            </span>
            <button
              onClick={() => setCurrentPage((p) => Math.min(totalPages - 1, p + 1))}
              disabled={currentPage === totalPages - 1}
              className="p-0.5 hover:bg-[#e0e0e0] disabled:opacity-30"
            >
              <ChevronRight size={12} />
            </button>
          </>
        )}
      </div>

      {/* 文本内容 */}
      <div
        ref={containerRef}
        className="flex-1 overflow-auto font-mono text-[11px] leading-[18px]"
      >
        <table className="w-full border-collapse">
          <tbody>
            {currentLines.map((line, index) => {
              const lineNum = currentPage * pageSize + index + 1;
              return (
                <tr key={lineNum} className="hover:bg-[#f5f5f5]">
                  <td className="w-12 px-2 text-right text-[#999] select-none border-r border-[#eee]">
                    {lineNum}
                  </td>
                  <td className="px-3 whitespace-pre-wrap break-all">
                    {highlightText(line)}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.2.1 | 渲染文本 | 多行文本 | 正确显示 |
| T1.2.2 | 行号显示 | 100 行文本 | 行号 1-100 |
| T1.2.3 | 分页显示 | 2000 行 | 正确分页 |
| T1.2.4 | 搜索高亮 | 关键词 | 高亮显示 |

---

### Task 1.3: 集成语法高亮

**工期**: 1 天  
**负责**: 前端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.3.1 | 添加 highlight.js | 语法高亮库 | 依赖正确 |
| 1.3.2 | 语言检测 | 根据扩展名 | 检测正确 |
| 1.3.3 | 集成到 TextViewer | 高亮显示 | 高亮正确 |

#### 支持的语言

| 扩展名 | 语言 | 高亮效果 |
|--------|------|----------|
| .js, .jsx | JavaScript | ✅ |
| .ts, .tsx | TypeScript | ✅ |
| .py | Python | ✅ |
| .rs | Rust | ✅ |
| .go | Go | ✅ |
| .java | Java | ✅ |
| .c, .cpp, .h | C/C++ | ✅ |
| .html, .htm | HTML | ✅ |
| .css, .scss | CSS | ✅ |
| .json | JSON | ✅ |
| .xml | XML | ✅ |
| .yaml, .yml | YAML | ✅ |
| .sql | SQL | ✅ |
| .sh | Shell | ✅ |
| .md | Markdown | ✅ |
| .txt | 纯文本 | 无高亮 |

---

### Task 1.4: 添加后端文本提取 API

**工期**: 1 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 1.4.1 | 创建 Tauri 命令 | get_text_preview | 命令可调用 |
| 1.4.2 | 实现文本提取 | 编码检测 + 解码 | 文本正确 |
| 1.4.3 | 添加错误处理 | 二进制文件检测 | 错误处理 |

#### Tauri 命令

```rust
// apps/desktop/src-tauri/src/commands/file_commands.rs

/// 获取文件文本预览
#[tauri::command]
pub async fn get_text_preview(
    state: State<'_, AppState>,
    file_id: String,
    max_bytes: Option<usize>,
) -> Result<TextPreviewDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let db_path = {
            let guard = app_state.active_case.lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Err(CommandError::no_active_case()),
            }
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;

        let handle = file_service::open_file_handle_real(&conn, &file_id)
            .map_err(CommandError::from_service_error)?;

        let max = max_bytes.unwrap_or(1024 * 1024); // 默认 1MB
        let range = file_service::read_file_range_for_case(
            &conn,
            &ViewerRangeRequestDto {
                handle_id: handle.handle_id,
                offset: 0,
                length: max,
            },
        ).map_err(CommandError::from_service_error)?;

        // 检测编码
        let content_bytes = range.lines.join("\n").into_bytes();
        let encoding_info = app_services::text_service::TextService::detect_encoding(&content_bytes);
        let text = app_services::text_service::TextService::decode_text(
            &content_bytes,
            encoding_info.encoding,
        );

        Ok(TextPreviewDto {
            content: text,
            encoding: encoding_info.name,
            is_truncated: content_bytes.len() >= max,
            line_count: text.lines().count(),
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}
```

#### DTO 定义

```rust
// crates/transport/src/dto/viewer.rs

/// 文本预览 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPreviewDto {
    /// 文本内容
    pub content: String,
    /// 编码名称
    pub encoding: String,
    /// 是否截断
    pub is_truncated: bool,
    /// 行数
    pub line_count: usize,
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T1.4.1 | UTF-8 文本 | UTF-8 文件 | 正确解码 |
| T1.4.2 | GBK 文本 | GBK 文件 | 正确解码 |
| T1.4.3 | 二进制文件 | 可执行文件 | 返回错误 |
| T1.4.4 | 大文件截断 | 10MB 文件 | 正确截断 |

---

## 📋 Phase 2: 图片预览 (3 天)

### Task 2.1: 实现 ImageViewer 组件

**工期**: 2 天  
**负责**: 前端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 2.1.1 | 创建组件结构 | 基础布局 | 组件可渲染 |
| 2.1.2 | 实现缩放功能 | 鼠标滚轮缩放 | 缩放流畅 |
| 2.1.3 | 实现拖拽平移 | 鼠标拖拽 | 平移流畅 |
| 2.1.4 | 实现旋转功能 | 旋转图片 | 旋转正确 |
| 2.1.5 | 实现适应窗口 | 自动适应 | 适应正确 |

#### 组件结构

```tsx
// frontend/src/components/viewers/ImageViewer.tsx

import { useState, useRef, useEffect, useCallback } from 'react';
import { ZoomIn, ZoomOut, RotateCw, Maximize, Download } from 'lucide-react';

interface ImageViewerProps {
  /** 图片 URL (data: URL 或 blob: URL) */
  src: string;
  /** 图片 MIME 类型 */
  mimeType?: string;
  /** 文件名 */
  fileName?: string;
}

export function ImageViewer({ src, mimeType, fileName }: ImageViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);

  const [scale, setScale] = useState(1);
  const [rotation, setRotation] = useState(0);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });
  const [imageSize, setImageSize] = useState({ width: 0, height: 0 });

  // 图片加载完成
  const handleLoad = useCallback(() => {
    if (imgRef.current) {
      setImageSize({
        width: imgRef.current.naturalWidth,
        height: imgRef.current.naturalHeight,
      });
    }
  }, []);

  // 鼠标滚轮缩放
  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setScale((s) => Math.max(0.1, Math.min(10, s * delta)));
  }, []);

  // 鼠标拖拽
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button === 0) {
      setIsDragging(true);
      setDragStart({ x: e.clientX - position.x, y: e.clientY - position.y });
    }
  }, [position]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (isDragging) {
      setPosition({
        x: e.clientX - dragStart.x,
        y: e.clientY - dragStart.y,
      });
    }
  }, [isDragging, dragStart]);

  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  // 适应窗口
  const fitToWindow = useCallback(() => {
    if (containerRef.current && imageSize.width > 0) {
      const containerWidth = containerRef.current.clientWidth - 40;
      const containerHeight = containerRef.current.clientHeight - 40;
      const scaleX = containerWidth / imageSize.width;
      const scaleY = containerHeight / imageSize.height;
      setScale(Math.min(scaleX, scaleY, 1));
      setPosition({ x: 0, y: 0 });
    }
  }, [imageSize]);

  // 重置
  const resetView = useCallback(() => {
    setScale(1);
    setRotation(0);
    setPosition({ x: 0, y: 0 });
  }, []);

  return (
    <div className="flex flex-col h-full">
      {/* 工具栏 */}
      <div className="flex items-center gap-2 px-3 py-1 border-b bg-[#fafafa] text-[11px]">
        <button onClick={() => setScale((s) => Math.min(10, s * 1.2))} className="p-1 hover:bg-[#e0e0e0] rounded">
          <ZoomIn size={14} />
        </button>
        <button onClick={() => setScale((s) => Math.max(0.1, s * 0.8))} className="p-1 hover:bg-[#e0e0e0] rounded">
          <ZoomOut size={14} />
        </button>
        <span className="text-[#666] w-16 text-center">{Math.round(scale * 100)}%</span>
        <button onClick={() => setRotation((r) => (r + 90) % 360)} className="p-1 hover:bg-[#e0e0e0] rounded">
          <RotateCw size={14} />
        </button>
        <button onClick={fitToWindow} className="p-1 hover:bg-[#e0e0e0] rounded">
          <Maximize size={14} />
        </button>
        <div className="flex-1" />
        <span className="text-[#999]">{imageSize.width} × {imageSize.height}</span>
        {fileName && (
          <a
            href={src}
            download={fileName}
            className="p-1 hover:bg-[#e0e0e0] rounded"
          >
            <Download size={14} />
          </a>
        )}
      </div>

      {/* 图片容器 */}
      <div
        ref={containerRef}
        className="flex-1 overflow-hidden bg-[#f0f0f0] cursor-grab active:cursor-grabbing"
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
      >
        <div
          className="w-full h-full flex items-center justify-center"
          style={{
            transform: `translate(${position.x}px, ${position.y}px) scale(${scale}) rotate(${rotation}deg)`,
            transition: isDragging ? 'none' : 'transform 0.1s ease-out',
          }}
        >
          <img
            ref={imgRef}
            src={src}
            alt={fileName || 'Preview'}
            onLoad={handleLoad}
            className="max-w-full max-h-full object-contain select-none"
            draggable={false}
          />
        </div>
      </div>
    </div>
  );
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T2.1.1 | 渲染图片 | PNG 图片 | 正确显示 |
| T2.1.2 | 缩放功能 | 滚轮缩放 | 缩放流畅 |
| T2.1.3 | 拖拽平移 | 鼠标拖拽 | 平移流畅 |
| T2.1.4 | 旋转功能 | 点击旋转 | 旋转正确 |
| T2.1.5 | 适应窗口 | 点击适应 | 自动缩放 |

---

### Task 2.2: 添加图片读取 API

**工期**: 1 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 2.2.1 | 创建 Tauri 命令 | get_image_preview | 命令可调用 |
| 2.2.2 | 实现 Base64 编码 | 图片转 Base64 | 编码正确 |
| 2.2.3 | 添加 MIME 检测 | 检测图片类型 | 检测正确 |

#### Tauri 命令

```rust
/// 获取图片预览
#[tauri::command]
pub async fn get_image_preview(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<ImagePreviewDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let db_path = {
            let guard = app_state.active_case.lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Err(CommandError::no_active_case()),
            }
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;

        let handle = file_service::open_file_handle_real(&conn, &file_id)
            .map_err(CommandError::from_service_error)?;

        // 检查是否是图片
        let mime = handle.mime.as_deref().unwrap_or("");
        if !mime.starts_with("image/") {
            return Err(CommandError::from_service_error("Not an image file"));
        }

        // 读取文件内容
        let range = file_service::read_file_range_for_case(
            &conn,
            &ViewerRangeRequestDto {
                handle_id: handle.handle_id,
                offset: 0,
                length: handle.size as usize,
            },
        ).map_err(CommandError::from_service_error)?;

        // Base64 编码
        let content_bytes = range.lines.join("").into_bytes();
        let base64 = base64::encode(&content_bytes);

        Ok(ImagePreviewDto {
            data_url: format!("data:{};base64,{}", mime, base64),
            mime_type: mime.to_string(),
            width: 0,  // 前端检测
            height: 0, // 前端检测
            size: handle.size,
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T2.2.1 | PNG 图片 | PNG 文件 | 返回 data URL |
| T2.2.2 | JPG 图片 | JPG 文件 | 返回 data URL |
| T2.2.3 | 非图片文件 | 文本文件 | 返回错误 |

---

## 📋 Phase 3: 视频/音频预览 (3 天)

### Task 3.1: 实现 VideoViewer 组件

**工期**: 1.5 天  
**负责**: 前端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 3.1.1 | 创建组件结构 | 基础布局 | 组件可渲染 |
| 3.1.2 | 实现播放控制 | 播放/暂停/停止 | 控制正确 |
| 3.1.3 | 实现进度条 | 拖拽进度 | 进度正确 |
| 3.1.4 | 实现音量控制 | 音量调节 | 控制正确 |
| 3.1.5 | 实现全屏功能 | 全屏播放 | 全屏正确 |

#### 组件结构

```tsx
// frontend/src/components/viewers/VideoViewer.tsx

import { useRef, useState, useEffect } from 'react';
import { Play, Pause, Volume2, VolumeX, Maximize, Minimize } from 'lucide-react';

interface VideoViewerProps {
  /** 视频 URL */
  src: string;
  /** MIME 类型 */
  mimeType?: string;
}

export function VideoViewer({ src, mimeType }: VideoViewerProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [isMuted, setIsMuted] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);

  // 播放/暂停
  const togglePlay = () => {
    if (videoRef.current) {
      if (isPlaying) {
        videoRef.current.pause();
      } else {
        videoRef.current.play();
      }
      setIsPlaying(!isPlaying);
    }
  };

  // 进度更新
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const handleTimeUpdate = () => setCurrentTime(video.currentTime);
    const handleLoadedMetadata = () => setDuration(video.duration);
    const handleEnded = () => setIsPlaying(false);

    video.addEventListener('timeupdate', handleTimeUpdate);
    video.addEventListener('loadedmetadata', handleLoadedMetadata);
    video.addEventListener('ended', handleEnded);

    return () => {
      video.removeEventListener('timeupdate', handleTimeUpdate);
      video.removeEventListener('loadedmetadata', handleLoadedMetadata);
      video.removeEventListener('ended', handleEnded);
    };
  }, []);

  // 格式化时间
  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  return (
    <div className="flex flex-col h-full bg-black">
      {/* 视频 */}
      <div className="flex-1 flex items-center justify-center">
        <video
          ref={videoRef}
          src={src}
          className="max-w-full max-h-full"
          onClick={togglePlay}
        />
      </div>

      {/* 控制栏 */}
      <div className="flex items-center gap-3 px-4 py-2 bg-[#1a1a1a] text-white text-[12px]">
        <button onClick={togglePlay} className="hover:text-gray-300">
          {isPlaying ? <Pause size={16} /> : <Play size={16} />}
        </button>

        {/* 进度条 */}
        <input
          type="range"
          min={0}
          max={duration}
          value={currentTime}
          onChange={(e) => {
            const time = parseFloat(e.target.value);
            if (videoRef.current) {
              videoRef.current.currentTime = time;
            }
            setCurrentTime(time);
          }}
          className="flex-1 h-1 bg-gray-600 rounded-lg appearance-none cursor-pointer"
        />

        <span className="w-20 text-center">
          {formatTime(currentTime)} / {formatTime(duration)}
        </span>

        {/* 音量 */}
        <button
          onClick={() => setIsMuted(!isMuted)}
          className="hover:text-gray-300"
        >
          {isMuted ? <VolumeX size={16} /> : <Volume2 size={16} />}
        </button>
        <input
          type="range"
          min={0}
          max={1}
          step={0.1}
          value={isMuted ? 0 : volume}
          onChange={(e) => {
            const vol = parseFloat(e.target.value);
            setVolume(vol);
            setIsMuted(vol === 0);
            if (videoRef.current) {
              videoRef.current.volume = vol;
            }
          }}
          className="w-20 h-1 bg-gray-600 rounded-lg appearance-none cursor-pointer"
        />
      </div>
    </div>
  );
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T3.1.1 | 渲染视频 | MP4 文件 | 正确显示 |
| T3.1.2 | 播放控制 | 点击播放 | 播放开始 |
| T3.1.3 | 进度拖拽 | 拖拽进度条 | 跳转正确 |
| T3.1.4 | 音量控制 | 调节音量 | 音量变化 |
| T3.1.5 | 全屏功能 | 点击全屏 | 全屏播放 |

---

### Task 3.2: 实现 AudioViewer 组件

**工期**: 1 天  
**负责**: 前端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 3.2.1 | 创建组件结构 | 基础布局 | 组件可渲染 |
| 3.2.2 | 实现波形显示 | 音频波形 | 波形显示 |
| 3.2.3 | 实现播放控制 | 播放/暂停 | 控制正确 |

#### 组件结构

```tsx
// frontend/src/components/viewers/AudioViewer.tsx

import { useRef, useState, useEffect } from 'react';
import { Play, Pause, Volume2, VolumeX, Music } from 'lucide-react';

interface AudioViewerProps {
  /** 音频 URL */
  src: string;
  /** MIME 类型 */
  mimeType?: string;
  /** 文件名 */
  fileName?: string;
}

export function AudioViewer({ src, mimeType, fileName }: AudioViewerProps) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [isMuted, setIsMuted] = useState(false);

  // 播放/暂停
  const togglePlay = () => {
    if (audioRef.current) {
      if (isPlaying) {
        audioRef.current.pause();
      } else {
        audioRef.current.play();
      }
      setIsPlaying(!isPlaying);
    }
  };

  // 进度更新
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;

    const handleTimeUpdate = () => setCurrentTime(audio.currentTime);
    const handleLoadedMetadata = () => setDuration(audio.duration);
    const handleEnded = () => setIsPlaying(false);

    audio.addEventListener('timeupdate', handleTimeUpdate);
    audio.addEventListener('loadedmetadata', handleLoadedMetadata);
    audio.addEventListener('ended', handleEnded);

    return () => {
      audio.removeEventListener('timeupdate', handleTimeUpdate);
      audio.removeEventListener('loadedmetadata', handleLoadedMetadata);
      audio.removeEventListener('ended', handleEnded);
    };
  }, []);

  // 格式化时间
  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  return (
    <div className="flex flex-col items-center justify-center h-full bg-[#1a1a1a] text-white p-6">
      {/* 音频图标 */}
      <div className="mb-6">
        <Music size={64} className="text-[#666]" />
      </div>

      {/* 文件名 */}
      {fileName && (
        <div className="text-[14px] font-medium mb-4 text-center truncate max-w-full">
          {fileName}
        </div>
      )}

      {/* 波形占位 */}
      <div className="w-full h-24 bg-[#2a2a2a] rounded mb-4 flex items-center justify-center">
        <canvas ref={canvasRef} className="w-full h-full" />
      </div>

      {/* 进度条 */}
      <div className="w-full flex items-center gap-3 mb-4">
        <span className="text-[11px] w-12 text-right">{formatTime(currentTime)}</span>
        <input
          type="range"
          min={0}
          max={duration}
          value={currentTime}
          onChange={(e) => {
            const time = parseFloat(e.target.value);
            if (audioRef.current) {
              audioRef.current.currentTime = time;
            }
            setCurrentTime(time);
          }}
          className="flex-1 h-1 bg-gray-600 rounded-lg appearance-none cursor-pointer"
        />
        <span className="text-[11px] w-12">{formatTime(duration)}</span>
      </div>

      {/* 控制按钮 */}
      <div className="flex items-center gap-4">
        <button
          onClick={togglePlay}
          className="w-12 h-12 rounded-full bg-white text-black flex items-center justify-center hover:bg-gray-200"
        >
          {isPlaying ? <Pause size={20} /> : <Play size={20} className="ml-1" />}
        </button>
      </div>

      {/* 音量 */}
      <div className="flex items-center gap-2 mt-4">
        <button
          onClick={() => setIsMuted(!isMuted)}
          className="hover:text-gray-300"
        >
          {isMuted ? <VolumeX size={16} /> : <Volume2 size={16} />}
        </button>
        <input
          type="range"
          min={0}
          max={1}
          step={0.1}
          value={isMuted ? 0 : volume}
          onChange={(e) => {
            const vol = parseFloat(e.target.value);
            setVolume(vol);
            setIsMuted(vol === 0);
            if (audioRef.current) {
              audioRef.current.volume = vol;
            }
          }}
          className="w-24 h-1 bg-gray-600 rounded-lg appearance-none cursor-pointer"
        />
      </div>

      {/* 隐藏的音频元素 */}
      <audio ref={audioRef} src={src} preload="metadata" />
    </div>
  );
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T3.2.1 | 渲染音频 | MP3 文件 | 正确显示 |
| T3.2.2 | 播放控制 | 点击播放 | 播放开始 |
| T3.2.3 | 进度拖拽 | 拖拽进度条 | 跳转正确 |
| T3.2.4 | 音量控制 | 调节音量 | 音量变化 |

---

### Task 3.3: 添加视频/音频读取 API

**工期**: 0.5 天  
**负责**: 后端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 3.3.1 | 创建 Tauri 命令 | get_media_url | 命令可调用 |
| 3.3.2 | 实现 URL 生成 | 本地文件 URL | URL 可访问 |

#### Tauri 命令

```rust
/// 获取媒体文件 URL (用于视频/音频播放)
#[tauri::command]
pub async fn get_media_url(
    state: State<'_, AppState>,
    file_id: String,
) -> Result<MediaUrlDto, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let db_path = {
            let guard = app_state.active_case.lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            match guard.as_ref() {
                Some(active) => active.db_path(),
                None => return Err(CommandError::no_active_case()),
            }
        };

        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;

        let handle = file_service::open_file_handle_real(&conn, &file_id)
            .map_err(CommandError::from_service_error)?;

        // 对于媒体文件，返回本地文件路径
        // Tauri 会自动处理本地文件 URL
        let file_path = file_service::get_file_path(&conn, &file_id)
            .map_err(CommandError::from_service_error)?;

        Ok(MediaUrlDto {
            url: format!("asset://localhost/{}", file_path.display()),
            mime_type: handle.mime.unwrap_or_default(),
            size: handle.size,
        })
    })
    .await
    .map_err(CommandError::from_join_error)?
}
```

#### 测试用例

| 测试 ID | 测试名称 | 输入 | 预期结果 |
|---------|----------|------|----------|
| T3.3.1 | 视频 URL | MP4 文件 | 返回有效 URL |
| T3.3.2 | 音频 URL | MP3 文件 | 返回有效 URL |

---

## 📋 Phase 4: 集成与优化 (2 天)

### Task 4.1: 集成到 FileBrowser

**工期**: 1 天  
**负责**: 前端  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 4.1.1 | 更新 ViewerTabs | 添加新标签页 | 标签页显示 |
| 4.1.2 | 实现预览切换 | 根据文件类型 | 切换正确 |
| 4.1.3 | 实现懒加载 | 按需加载 | 性能优化 |

---

### Task 4.2: 性能优化

**工期**: 1 天  
**负责**: 全栈  

#### 子任务

| ID | 子任务 | 描述 | 验收标准 |
|----|--------|------|----------|
| 4.2.1 | 图片压缩 | 大图片压缩 | 加载更快 |
| 4.2.2 | 视频转码 | 视频预览优化 | 播放流畅 |
| 4.2.3 | 缓存策略 | 缓存预览数据 | 减少请求 |

---

## 📊 测试用例汇总

| Phase | 测试数量 | 通过标准 |
|-------|---------|----------|
| Phase 1 (文本) | 16 | 100% |
| Phase 2 (图片) | 10 | 100% |
| Phase 3 (视频/音频) | 14 | 100% |
| Phase 4 (集成) | 8 | 100% |
| **总计** | **48** | **100%** |

---

## 📋 交付物清单

| 交付物 | 文件路径 | 说明 |
|--------|----------|------|
| TextService | `crates/app-services/src/text_service.rs` | 文本提取服务 |
| TextViewer | `frontend/src/components/viewers/TextViewer.tsx` | 文本预览组件 |
| ImageViewer | `frontend/src/components/viewers/ImageViewer.tsx` | 图片预览组件 |
| VideoViewer | `frontend/src/components/viewers/VideoViewer.tsx` | 视频预览组件 |
| AudioViewer | `frontend/src/components/viewers/AudioViewer.tsx` | 音频预览组件 |
| get_text_preview | Tauri 命令 | 文本提取 API |
| get_image_preview | Tauri 命令 | 图片预览 API |
| get_media_url | Tauri 命令 | 媒体 URL API |

---

## 📅 甘特图

```
Week 1                    Week 2
│                         │
├─ Phase 1 (5d) ──────────┤
│  ├─ Task 1.1 (1d)       │
│  ├─ Task 1.2 (2d)       │
│  ├─ Task 1.3 (1d)       │
│  └─ Task 1.4 (1d)       │
│                         │
│  ├─ Phase 2 (3d) ───────┤
│  │  ├─ Task 2.1 (2d)    │
│  │  └─ Task 2.2 (1d)    │
│  │                      │
│  │  ├─ Phase 3 (3d) ────┤
│  │  │  ├─ Task 3.1 (1.5d)
│  │  │  ├─ Task 3.2 (1d) │
│  │  │  └─ Task 3.3 (0.5d)
│  │  │                   │
│  │  │  ├─ Phase 4 (2d) ─┤
│  │  │  │  ├─ Task 4.1 (1d)
│  │  │  │  └─ Task 4.2 (1d)
└──┴──┴──┴────────────────┘
```

---

## ✅ 验收标准

### 文本预览

- [ ] 自动检测 UTF-8/GBK/UTF-16 编码
- [ ] 正确显示行号
- [ ] 支持语法高亮 (15+ 语言)
- [ ] 大文件分页加载 (>1MB)
- [ ] 搜索关键词高亮

### 图片预览

- [ ] 支持 JPG/PNG/GIF/BMP/WebP
- [ ] 鼠标滚轮缩放
- [ ] 拖拽平移
- [ ] 旋转功能
- [ ] 适应窗口

### 视频预览

- [ ] 支持 MP4/WebM
- [ ] 播放/暂停/停止
- [ ] 进度条拖拽
- [ ] 音量控制
- [ ] 全屏播放

### 音频预览

- [ ] 支持 MP3/WAV
- [ ] 播放/暂停
- [ ] 进度条拖拽
- [ ] 音量控制

---

**方案版本**: v1.0  
**制定人**: MiMo AI Assistant  
**日期**: 2026-05-30
