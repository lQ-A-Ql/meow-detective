//! 文本提取服务
//!
//! 提供文本编码检测、解码、语法高亮支持。

use encoding_rs::{Encoding, GBK, UTF_16BE, UTF_16LE, UTF_8};
use std::io::Read;

/// 编码检测结果
#[derive(Debug, Clone)]
pub struct EncodingInfo {
    pub encoding: &'static Encoding,
    pub name: String,
    pub confidence: f32,
}

/// 文本预览结果
#[derive(Debug, Clone)]
pub struct TextPreview {
    pub content: String,
    pub encoding: String,
    pub is_truncated: bool,
    pub line_count: usize,
    pub is_binary: bool,
    /// Programming language for syntax highlighting
    pub language: Option<String>,
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
                name: "UTF-8 BOM".to_string(),
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
        if let Ok(text) = std::str::from_utf8(data) {
            // 计算 UTF-8 置信度
            let valid_chars = text.chars().count();
            let total_bytes = data.len();
            let confidence = if total_bytes > 0 {
                0.9 + (valid_chars as f32 / total_bytes as f32 * 0.1).min(0.1)
            } else {
                0.95
            };
            return EncodingInfo {
                encoding: UTF_8,
                name: "UTF-8".to_string(),
                confidence,
            };
        }

        // 尝试 GBK (中文环境常见)
        let (decoded, _, errors) = GBK.decode(data);
        if !errors {
            let has_chinese = decoded.chars().any(|c| {
                ('\u{4E00}'..='\u{9FFF}').contains(&c)
                    || ('\u{3400}'..='\u{4DBF}').contains(&c)
                    || ('\u{3000}'..='\u{303F}').contains(&c) // CJK Symbols
                    || ('\u{FF00}'..='\u{FFEF}').contains(&c) // Fullwidth forms
            });

            if has_chinese {
                return EncodingInfo {
                    encoding: GBK,
                    name: "GBK".to_string(),
                    confidence: 0.85,
                };
            }
        }

        // 尝试 Shift-JIS (日文)
        let (decoded_sjis, _, errors_sjis) = encoding_rs::SHIFT_JIS.decode(data);
        if !errors_sjis {
            let has_japanese = decoded_sjis.chars().any(|c| {
                ('\u{3040}'..='\u{309F}').contains(&c) // Hiragana
                    || ('\u{30A0}'..='\u{30FF}').contains(&c) // Katakana
            });
            if has_japanese {
                return EncodingInfo {
                    encoding: encoding_rs::SHIFT_JIS,
                    name: "Shift-JIS".to_string(),
                    confidence: 0.8,
                };
            }
        }

        // 尝试 EUC-KR (韩文)
        let (decoded_kr, _, errors_kr) = encoding_rs::EUC_KR.decode(data);
        if !errors_kr {
            let has_korean = decoded_kr.chars().any(|c| {
                ('\u{AC00}'..='\u{D7AF}').contains(&c) // Hangul syllables
            });
            if has_korean {
                return EncodingInfo {
                    encoding: encoding_rs::EUC_KR,
                    name: "EUC-KR".to_string(),
                    confidence: 0.8,
                };
            }
        }

        // 回退到 Latin-1
        EncodingInfo {
            encoding: encoding_rs::WINDOWS_1252,
            name: "Windows-1252".to_string(),
            confidence: 0.5,
        }
    }

    /// 解码文本
    pub fn decode_text(data: &[u8], encoding: &'static Encoding) -> String {
        let (decoded, _, _) = encoding.decode(data);
        decoded.into_owned()
    }

    /// 检查文件是否可能是文本文件
    pub fn is_likely_text(data: &[u8]) -> bool {
        if data.is_empty() {
            return true;
        }

        // 检查前 8KB 是否包含过多 null 字节
        let check_len = data.len().min(8192);
        let null_count = data[..check_len].iter().filter(|&&b| b == 0).count();

        // 如果 null 字节超过 10%，可能是二进制文件
        (null_count as f64 / check_len as f64) < 0.1
    }

    /// 提取文本预览
    pub fn extract_text_preview(
        reader: &mut dyn Read,
        max_bytes: usize,
    ) -> std::io::Result<TextPreview> {
        let mut buffer = vec![0u8; max_bytes];
        let bytes_read = reader.read(&mut buffer)?;
        buffer.truncate(bytes_read);

        // 检查是否是二进制文件
        if !Self::is_likely_text(&buffer) {
            return Ok(TextPreview {
                content: String::new(),
                encoding: "binary".to_string(),
                is_truncated: false,
                line_count: 0,
                is_binary: true,
                language: None,
            });
        }

        let encoding_info = Self::detect_encoding(&buffer);
        let text = Self::decode_text(&buffer, encoding_info.encoding);
        let is_truncated = bytes_read >= max_bytes;
        let line_count = text.lines().count();

        Ok(TextPreview {
            content: text,
            encoding: encoding_info.name,
            is_truncated,
            line_count,
            is_binary: false,
            language: None,
        })
    }

    /// 根据文件扩展名获取语言标识
    pub fn get_language_from_extension(ext: &str) -> Option<&'static str> {
        match ext.to_lowercase().as_str() {
            "js" | "jsx" | "mjs" => Some("javascript"),
            "ts" | "tsx" | "mts" => Some("typescript"),
            "py" | "pyw" => Some("python"),
            "rs" => Some("rust"),
            "go" => Some("go"),
            "java" | "class" => Some("java"),
            "c" | "h" => Some("c"),
            "cpp" | "cc" | "cxx" | "hpp" => Some("cpp"),
            "cs" => Some("csharp"),
            "html" | "htm" => Some("html"),
            "css" | "scss" | "less" => Some("css"),
            "json" => Some("json"),
            "xml" | "svg" | "xsl" => Some("xml"),
            "yaml" | "yml" => Some("yaml"),
            "toml" => Some("toml"),
            "sql" => Some("sql"),
            "sh" | "bash" | "zsh" => Some("shell"),
            "md" | "markdown" => Some("markdown"),
            "rb" => Some("ruby"),
            "php" => Some("php"),
            "swift" => Some("swift"),
            "kt" | "kts" => Some("kotlin"),
            "r" => Some("r"),
            "lua" => Some("lua"),
            "perl" | "pl" => Some("perl"),
            "ini" | "cfg" | "conf" => Some("ini"),
            "dockerfile" => Some("dockerfile"),
            "makefile" | "mk" => Some("makefile"),
            "bat" | "cmd" => Some("batch"),
            "ps1" => Some("powershell"),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/text_service.rs"]
mod tests;
