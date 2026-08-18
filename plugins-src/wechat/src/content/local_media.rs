use aes::cipher::{BlockDecrypt, KeyInit};
use md5::{Digest as _, Md5};
use serde_json::Value;
use sha2::Sha256;

use super::supplemental::inline_media_value;
use crate::keyinject;
use crate::payload::{new_attrs, Payload};

const V4_HEADER_BYTES: usize = 15;
const AES_BLOCK_BYTES: usize = 16;

pub fn parse(path: &str, data: &[u8], payload: &mut Payload) -> Result<(), String> {
    let mut values = new_attrs();
    values.insert(
        "localPathKind".to_string(),
        Value::String(path_kind(path).to_string()),
    );
    values.insert(
        "encryptedSizeBytes".to_string(),
        Value::from(data.len() as u64),
    );
    values.insert(
        "encryptedSha256".to_string(),
        Value::String(format!("{:x}", Sha256::digest(data))),
    );
    if let Some(key) = storage_key(path) {
        values.insert("storageKey".to_string(), Value::String(key));
    }
    if let Some(key) = sns_cache_key(path) {
        values.insert("cacheKey".to_string(), Value::String(key));
    }

    let decoded = if media_is_plain(data) {
        Some(data.to_vec())
    } else if data.starts_with(b"\x07\x08\x56\x32") {
        keyinject::image_material().and_then(|material| decrypt_v4(data, &material).ok())
    } else {
        None
    };
    let summary = if let Some(decoded) = decoded {
        values.insert("encrypted".to_string(), Value::Bool(false));
        values.insert(
            "plainMd5".to_string(),
            Value::String(format!("{:x}", Md5::digest(&decoded))),
        );
        values.insert("media".to_string(), inline_media_value(&decoded));
        if decoded.starts_with(b"wxgf") || decoded.starts_with(b"wxam") {
            "微信本地媒体（已解密，WxAM 封装待解码）"
        } else {
            "微信本地媒体（已解密并校验格式）"
        }
    } else {
        values.insert("encrypted".to_string(), Value::Bool(true));
        "微信本地媒体（图片密钥缺失或格式无法解码）"
    };
    let mut attrs = new_attrs();
    attrs.insert(
        "table".to_string(),
        Value::String("LocalMediaFile".to_string()),
    );
    attrs.insert("values".to_string(), Value::Object(values));
    payload.artifact(
        "WeChatMedia",
        format!("本地媒体 {}", basename(path)),
        summary,
        attrs,
    );
    Ok(())
}

fn decrypt_v4(data: &[u8], material: &keyinject::ImageMaterial) -> Result<Vec<u8>, String> {
    if data.len() < V4_HEADER_BYTES + AES_BLOCK_BYTES {
        return Err("V4 media header is truncated".to_string());
    }
    let aes_len = u32::from_le_bytes(data[6..10].try_into().unwrap_or_default()) as usize;
    let xor_len = u32::from_le_bytes(data[10..14].try_into().unwrap_or_default()) as usize;
    let body = &data[V4_HEADER_BYTES..];
    let aes_stored_len = aes_len
        .checked_div(AES_BLOCK_BYTES)
        .and_then(|blocks| blocks.checked_mul(AES_BLOCK_BYTES))
        .and_then(|bytes| bytes.checked_add(AES_BLOCK_BYTES))
        .ok_or_else(|| "V4 media AES length overflow".to_string())?;
    if aes_stored_len > body.len() || xor_len > body.len().saturating_sub(aes_stored_len) {
        return Err("V4 media section lengths are invalid".to_string());
    }
    let cipher = aes::Aes128::new_from_slice(material.aes_key.as_ref())
        .map_err(|_| "V4 media AES key is invalid".to_string())?;
    let mut decrypted_prefix = Vec::with_capacity(aes_stored_len);
    for chunk in body[..aes_stored_len].chunks_exact(AES_BLOCK_BYTES) {
        let mut block = aes::cipher::Block::<aes::Aes128>::clone_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        decrypted_prefix.extend_from_slice(&block);
    }
    let mut output = Vec::with_capacity(aes_len + body.len().saturating_sub(aes_stored_len));
    output.extend_from_slice(&decrypted_prefix[..aes_len.min(decrypted_prefix.len())]);
    let xor_start = body.len() - xor_len;
    output.extend_from_slice(&body[aes_stored_len..xor_start]);
    output.extend(body[xor_start..].iter().map(|byte| byte ^ material.xor_key));
    media_is_plain(&output)
        .then_some(output)
        .ok_or_else(|| "V4 media plaintext signature is unsupported".to_string())
}

fn media_is_plain(bytes: &[u8]) -> bool {
    is_jpeg_header(bytes)
        || bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
        || bytes.starts_with(b"wxgf")
        || bytes.starts_with(b"wxam")
        || is_svg(bytes)
}

fn is_jpeg_header(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\xff\xd8\xff")
        && bytes
            .get(3)
            .is_some_and(|marker| matches!(marker, 0xc0..=0xcf | 0xdb | 0xe0..=0xef | 0xfe))
}

fn is_svg(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(4 * 1024)];
    let Ok(text) = std::str::from_utf8(sample) else {
        return false;
    };
    let trimmed = text.trim_start_matches(|character: char| character.is_whitespace());
    trimmed.starts_with("<svg")
        || (trimmed.starts_with("<?xml") && trimmed.to_ascii_lowercase().contains("<svg"))
}

fn path_kind(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.contains("/sns/img/") {
        "momentCache"
    } else {
        "messageAttachment"
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn storage_key(path: &str) -> Option<String> {
    let name = basename(path).to_ascii_lowercase();
    let candidate = name
        .strip_suffix("_t.dat")
        .or_else(|| name.strip_suffix(".dat"))?;
    (candidate.len() == 32 && candidate.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| candidate.to_string())
}

fn sns_cache_key(path: &str) -> Option<String> {
    let mut segments = path.rsplit('/');
    let name = segments.next()?;
    let parent = segments.next()?;
    let joined = format!("{parent}{name}").to_ascii_lowercase();
    (joined.len() == 32 && joined.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;

    #[test]
    fn recognizes_message_and_moment_storage_keys() {
        assert_eq!(
            storage_key("x/msg/attach/t/2026-04/Img/83d35dbfebf20beff6c1e711168205ee_t.dat")
                .as_deref(),
            Some("83d35dbfebf20beff6c1e711168205ee")
        );
        assert_eq!(
            sns_cache_key("x/cache/2026-03/Sns/Img/05/fa2f7729fffba7ccfd9c2301c36d6f").as_deref(),
            Some("05fa2f7729fffba7ccfd9c2301c36d6f")
        );
    }

    #[test]
    fn decrypts_v4_aes_prefix_and_xor_tail() {
        use aes::cipher::BlockEncrypt;

        let key = [0x2a; 16];
        let plain = b"\xff\xd8\xff\xe0fixture-body\xff\xd9";
        let aes_len = 16usize;
        let xor_key = 0xb0;
        let mut padded = [16u8; 32];
        padded[..aes_len].copy_from_slice(&plain[..aes_len]);
        let cipher = aes::Aes128::new_from_slice(&key).expect("key");
        for chunk in padded.chunks_exact_mut(16) {
            let block = aes::cipher::Block::<aes::Aes128>::from_mut_slice(chunk);
            cipher.encrypt_block(block);
        }
        let tail = plain[aes_len..]
            .iter()
            .map(|byte| byte ^ xor_key)
            .collect::<Vec<_>>();
        let mut encoded = b"\x07\x08\x56\x32\x08\x07".to_vec();
        encoded.extend_from_slice(&(aes_len as u32).to_le_bytes());
        encoded.extend_from_slice(&(tail.len() as u32).to_le_bytes());
        encoded.push(1);
        encoded.extend_from_slice(&padded);
        encoded.extend_from_slice(&tail);
        let material = keyinject::ImageMaterial {
            aes_key: Zeroizing::new(key),
            xor_key,
        };
        assert_eq!(decrypt_v4(&encoded, &material).expect("decode"), plain);
    }
}
