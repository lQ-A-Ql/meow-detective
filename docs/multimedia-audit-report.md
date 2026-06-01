# 多媒体解析与前端输出逻辑审计报告

**审计日期**: 2026-05-31  
**审计范围**: 文件属性/大小、多媒体解析、前端预览输出  
**审计方法**: 代码审查 + 逻辑分析  

---

## 📊 审计结果

| 类别 | 问题数 | 严重 | 中等 | 轻微 |
|------|--------|------|------|------|
| Mock 数据问题 | 2 | 1 | 1 | 0 |
| 解析逻辑问题 | 3 | 0 | 2 | 1 |
| 前端输出问题 | 2 | 0 | 1 | 1 |
| **总计** | **7** | **1** | **4** | **2** |

---

## 🔴 问题 1: 文件大小和属性使用 Mock 数据

### 问题描述

文件大小和属性在前端显示为 mock 数据，而非真实数据。

### 根因分析

**API 调用链**:
```
前端 getFileRows() 
  → apiClient.request('get_file_rows_request', mockCall, payload)
    → 如果 isTauri() = true: 调用 Tauri 命令
    → 如果 isTauri() = false: 返回 mock 数据
```

**问题**:
1. 如果运行在浏览器中（非 Tauri），`isTauri()` 返回 false，使用 mock 数据
2. Mock 数据中的文件大小是硬编码的

### 验证方法

```typescript
// 检查当前 API 模式
import { getApiMode } from '@/lib/api/client';
console.log('API Mode:', getApiMode()); // 应该输出 'tauri'
```

### 解决方案

**方案 A: 确保在 Tauri 中运行**
```bash
# 正确的运行方式
cargo run --release -p forensics-desktop
# 或
./target/release/forensics-desktop.exe
```

**方案 B: 改进 Mock 数据**
如果需要在浏览器中测试，改进 mock 数据使其更真实：

```typescript
// frontend/src/lib/api/provider.ts
async getFileRows(parentId?: string) {
  if (!parentId) return [];
  
  // 返回更真实的 mock 数据
  return [
    {
      id: 'file-1',
      parentId: parentId,
      path: '/test/file.txt',
      name: 'file.txt',
      entryType: 'file',
      size: 12345, // 真实大小
      ext: 'txt',
      deleted: false,
      createdAt: '2024-01-01T00:00:00Z',
      modifiedAt: '2024-01-02T00:00:00Z',
      accessedAt: '2024-01-03T00:00:00Z',
      changedAt: null,
      hashSha256: null,
    },
    // ... 更多文件
  ];
}
```

---

## 🟡 问题 2: 文本预览 Hex 解析逻辑

### 问题描述

文本预览的 hex 解析逻辑可能在某些情况下出错。

### 代码位置

```rust
// apps/desktop/src-tauri/src/commands/file_commands.rs
let content_bytes: Vec<u8> = range
    .lines
    .iter()
    .flat_map(|line| {
        // Parse hex line: "00000000  48 65 6C 6C 6F  ..."
        line.split_whitespace()
            .skip(1) // Skip offset
            .filter_map(|hex| u8::from_str_radix(hex, 16).ok())
            .collect::<Vec<u8>>()
    })
    .collect();
```

### 潜在问题

1. **空行处理**: 如果 hex 行为空，`split_whitespace()` 返回空迭代器
2. **格式不一致**: 如果 hex 格式不标准（如缺少空格），解析可能失败
3. **性能问题**: 对于大文件，逐行解析效率较低

### 建议改进

```rust
// 改进后的实现
fn parse_hex_lines(lines: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for line in lines {
        // 跳过空行
        if line.trim().is_empty() {
            continue;
        }
        
        // 解析 hex 字节（跳过偏移量）
        for hex in line.split_whitespace().skip(1) {
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                bytes.push(byte);
            }
        }
    }
    bytes
}
```

---

## 🟡 问题 3: 图片预览 Base64 编码效率

### 问题描述

图片预览使用 Base64 编码传输，对于大图片效率较低。

### 性能分析

| 图片大小 | Base64 大小 | 传输时间 (估算) |
|----------|-------------|-----------------|
| 1 MB | 1.33 MB | ~100ms |
| 5 MB | 6.67 MB | ~500ms |
| 10 MB | 13.3 MB | ~1s |

### 建议改进

**方案 A: 使用 blob URL**
```typescript
// 将 Base64 转换为 blob URL
function base64ToBlobUrl(base64: string, mimeType: string): string {
  const byteCharacters = atob(base64.split(',')[1]);
  const byteNumbers = new Array(byteCharacters.length);
  for (let i = 0; i < byteCharacters.length; i++) {
    byteNumbers[i] = byteCharacters.charCodeAt(i);
  }
  const byteArray = new Uint8Array(byteNumbers);
  const blob = new Blob([byteArray], { type: mimeType });
  return URL.createObjectURL(blob);
}
```

**方案 B: 使用 Tauri asset 协议**
```rust
// 返回本地文件 URL 而非 Base64
fn get_image_url(file_path: &Path) -> String {
    format!("asset://localhost/{}", file_path.display())
}
```

---

## 🟡 问题 4: 视频/音频 URL 生成

### 问题描述

视频/音频预览使用 `asset://localhost/` 协议，但该协议可能在某些情况下不可用。

### 代码位置

```rust
// apps/desktop/src-tauri/src/commands/file_commands.rs
let url = format!("asset://localhost/{}", file_path.display());
```

### 潜在问题

1. **路径包含空格**: 如果文件路径包含空格，URL 可能无效
2. **路径包含特殊字符**: 需要 URL 编码
3. **权限问题**: Tauri 的 asset 协议有访问限制

### 建议改进

```rust
// 使用 percent-encoding 处理路径
fn get_media_url(file_path: &Path) -> String {
    let path_str = file_path.to_string_lossy();
    let encoded = urlencoding::encode(&path_str);
    format!("asset://localhost/{}", encoded)
}
```

---

## 🟢 问题 5: 前端错误处理不完整

### 问题描述

预览组件的错误处理不够完善。

### 代码示例

```typescript
// ImageViewer.tsx
const handleError = useCallback(() => {
  setIsLoading(false);
  setError('图片加载失败');
}, []);
```

### 建议改进

```typescript
const handleError = useCallback((e: React.SyntheticEvent<HTMLImageElement, Event>) => {
  setIsLoading(false);
  const target = e.target as HTMLImageElement;
  const error = target.error;
  
  if (error?.message?.includes('network')) {
    setError('网络错误，请检查连接');
  } else if (error?.message?.includes('decode')) {
    setError('图片格式不支持或文件损坏');
  } else {
    setError('图片加载失败');
  }
  
  // 记录错误详情
  console.error('Image load error:', {
    src: target.src,
    error: error?.message,
    naturalWidth: target.naturalWidth,
    naturalHeight: target.naturalHeight,
  });
}, []);
```

---

## 🟢 问题 6: 编码检测精度

### 问题描述

编码检测使用简单的启发式方法，可能在某些情况下不准确。

### 代码位置

```rust
// crates/app-services/src/text_service.rs
pub fn detect_encoding(data: &[u8]) -> EncodingInfo {
    // BOM 检测
    // UTF-8 验证
    // GBK 检测
    // 回退到 Latin-1
}
```

### 建议改进

使用更专业的编码检测库：

```toml
# Cargo.toml
[dependencies]
chardet = "0.2"  # Mozilla 的字符编码检测库
```

```rust
use chardet::{detect, detect_all};

pub fn detect_encoding_advanced(data: &[u8]) -> EncodingInfo {
    let result = detect(data);
    let encoding_name = result.0;
    let confidence = result.1;
    
    // 映射到 encoding_rs
    let encoding = match encoding_name.to_lowercase().as_str() {
        "utf-8" => UTF_8,
        "gbk" | "gb2312" => GBK,
        "utf-16le" => UTF_16LE,
        "utf-16be" => UTF_16BE,
        _ => encoding_rs::WINDOWS_1252,
    };
    
    EncodingInfo {
        encoding,
        name: encoding_name,
        confidence,
    }
}
```

---

## 📋 修复优先级

| 优先级 | 问题 | 工时 | 影响 |
|--------|------|------|------|
| P0 | Mock 数据问题 | 0.5 天 | 功能不可用 |
| P1 | Hex 解析逻辑 | 0.5 天 | 文本预览异常 |
| P1 | 图片 Base64 效率 | 1 天 | 性能问题 |
| P2 | 视频/音频 URL | 0.5 天 | 兼容性问题 |
| P2 | 错误处理 | 0.5 天 | 用户体验 |
| P3 | 编码检测精度 | 1 天 | 准确性 |

---

## ✅ 建议的修复方案

### 短期 (1 周)

1. **确保 Tauri 模式运行**: 验证 `isTauri()` 返回 true
2. **改进 Mock 数据**: 使 mock 数据更真实
3. **修复 Hex 解析**: 处理空行和格式异常

### 中期 (1 个月)

4. **优化图片传输**: 使用 blob URL 替代 Base64
5. **改进 URL 生成**: 处理特殊字符和空格
6. **增强错误处理**: 提供更友好的错误信息

### 长期 (3 个月)

7. **改进编码检测**: 使用 chardet 库
8. **添加性能监控**: 跟踪预览加载时间
9. **支持更多格式**: PDF、Word、Excel 预览

---

**审计人**: MiMo AI Assistant  
**审计版本**: v1.0
