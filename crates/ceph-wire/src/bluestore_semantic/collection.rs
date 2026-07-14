use crate::{
    bluestore_semantic::{
        denc::{ensure_empty, read_denc_payload},
        types::{
            BlueStoreCnode, BlueStoreCollectionId, BlueStoreCollectionKind,
            BlueStoreCollectionRecord, BlueStoreSemanticLimits,
        },
    },
    codec::CephDecode,
    cursor::CephCursor,
    error::{CephWireError, Result},
};

pub(crate) fn decode_collection(
    logical_key: &[u8],
    value: &[u8],
    _limits: BlueStoreSemanticLimits,
) -> Result<BlueStoreCollectionRecord> {
    let collection = decode_collection_key(logical_key)?;
    let mut cursor = CephCursor::new(value);
    let denc = read_denc_payload(&mut cursor, &[1], "BlueStore cnode")?;
    let mut payload = denc.cursor;
    let bits = u32::decode(&mut payload)?;
    ensure_empty(&payload, "BlueStore cnode DENC payload")?;
    ensure_empty(&cursor, "BlueStore cnode value")?;
    Ok(BlueStoreCollectionRecord {
        collection,
        cnode: BlueStoreCnode {
            denc_version: denc.version,
            bits,
        },
    })
}

fn decode_collection_key(logical_key: &[u8]) -> Result<BlueStoreCollectionId> {
    let key = std::str::from_utf8(logical_key).map_err(|error| CephWireError::InvalidUtf8 {
        context: "BlueStore collection key",
        message: error.to_string(),
    })?;
    if key == "meta" {
        return Ok(BlueStoreCollectionId::Meta);
    }
    let (base, kind) = if let Some(base) = key.strip_suffix("_head") {
        (base, BlueStoreCollectionKind::Head)
    } else if let Some(base) = key.strip_suffix("_TEMP") {
        (base, BlueStoreCollectionKind::Temp)
    } else {
        return Err(invalid_collection_key(
            "unknown canonical collection suffix",
        ));
    };
    decode_pg_collection(base, kind)
}

fn decode_pg_collection(
    base: &str,
    kind: BlueStoreCollectionKind,
) -> Result<BlueStoreCollectionId> {
    let (pool_text, pg_text) = base
        .split_once('.')
        .ok_or_else(|| invalid_collection_key("missing pool/PG separator"))?;
    if pg_text.contains('.') {
        return Err(invalid_collection_key("too many pool/PG separators"));
    }
    let (seed_text, shard_text) = match pg_text.split_once('s') {
        Some((seed, shard)) if !shard.contains('s') => (seed, Some(shard)),
        Some(_) => return Err(invalid_collection_key("invalid shard separator")),
        None => (pg_text, None),
    };
    let pool = parse_canonical_decimal_u64(pool_text, "invalid pool id")?;
    let seed = parse_canonical_hex_u32(seed_text)?;
    let shard = shard_text
        .map(|text| parse_canonical_decimal_u8(text, "invalid shard id"))
        .transpose()?;
    Ok(BlueStoreCollectionId::Pg {
        pool,
        seed,
        shard,
        kind,
    })
}

fn parse_canonical_decimal_u64(text: &str, reason: &'static str) -> Result<u64> {
    if !is_canonical_digits(text, |byte| byte.is_ascii_digit()) {
        return Err(invalid_collection_key(reason));
    }
    text.parse().map_err(|_| invalid_collection_key(reason))
}

fn parse_canonical_decimal_u8(text: &str, reason: &'static str) -> Result<u8> {
    if !is_canonical_digits(text, |byte| byte.is_ascii_digit()) {
        return Err(invalid_collection_key(reason));
    }
    text.parse().map_err(|_| invalid_collection_key(reason))
}

fn parse_canonical_hex_u32(text: &str) -> Result<u32> {
    if !is_canonical_digits(text, |byte| {
        byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
    }) {
        return Err(invalid_collection_key("invalid canonical PG seed"));
    }
    u32::from_str_radix(text, 16).map_err(|_| invalid_collection_key("invalid canonical PG seed"))
}

fn is_canonical_digits(text: &str, valid: impl Fn(u8) -> bool) -> bool {
    !text.is_empty()
        && (text == "0" || !text.starts_with('0'))
        && text.as_bytes().iter().copied().all(valid)
}

fn invalid_collection_key(reason: &'static str) -> CephWireError {
    CephWireError::InvalidBlueStoreSemanticKey {
        key_space: "collection",
        reason,
    }
}
