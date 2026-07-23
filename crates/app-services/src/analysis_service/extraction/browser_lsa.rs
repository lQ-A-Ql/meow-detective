//! Offline LSA secret and Chrome App-Bound material collection for the
//! browser preload pipeline. All failures degrade to warnings so the base
//! v10/v11 decryption path keeps working.

use super::browser_preload::{locate_by_suffix, locator_from_row, read_locator};
use super::candidate_processing::CandidateSource;
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use artifacts_windows::dpapi::{
    derive_user_prekeys, derive_user_prekeys_from_password_sha1, parse_cng_system_key_file,
};
use artifacts_windows::{decrypt_lsa_secrets, DpapiSystemKeys, LsaDecryptedSecrets, TbalSecret};
use rusqlite::{params, Connection};
use std::sync::atomic::AtomicBool;

/// Inputs needed to seed DPAPI prekeys from offline LSA secrets.
pub(super) struct PrekeySeed<'a> {
    pub boot_key: &'a [u8; 16],
    pub user_nt_hashes: &'a [(String, [u8; 16])],
}

/// NT-hash prekeys plus the SID/hash pairs kept for TBAL cross-checking.
pub(super) type SamPrekeys = (Vec<[u8; 20]>, Vec<(String, [u8; 16])>);

/// Derive NT-hash prekeys for every SAM user, keeping the SID/hash pairs for
/// TBAL cross-checking.
pub(super) fn derive_sam_prekeys(sam_info: artifacts_windows::SamInfo) -> SamPrekeys {
    let mut prekeys = Vec::new();
    let mut user_nt_hashes = Vec::new();
    for user in sam_info.users {
        let Some(password_hash) = user.password_hash else {
            continue;
        };
        let Some((_, nt_hash)) = password_hash.rsplit_once(':') else {
            continue;
        };
        let Ok(decoded) = hex::decode(nt_hash) else {
            continue;
        };
        let Ok(nt_hash) = <[u8; 16]>::try_from(decoded.as_slice()) else {
            continue;
        };
        prekeys.extend(derive_user_prekeys(&user.sid, &nt_hash));
        user_nt_hashes.push((user.sid.clone(), nt_hash));
    }
    (prekeys, user_nt_hashes)
}

/// Extend user prekeys with offline LSA secrets: `DPAPI_SYSTEM` machine/user
/// prekeys and TBAL-provisioned password-SHA1 prekeys cross-checked against
/// SAM NT hashes. All failures degrade to warnings.
pub(super) fn extend_prekeys_from_lsa_secrets(
    conn: &Connection,
    data_source_id: &str,
    seed: &PrekeySeed<'_>,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
    prekeys: &mut Vec<[u8; 20]>,
    warnings: &mut Vec<String>,
) {
    let Some(security) =
        locate_by_suffix(conn, data_source_id, "/windows/system32/config/security")
    else {
        return;
    };
    let security_bytes = match read_locator(&security, cancel_token, file_reader) {
        Ok(bytes) => bytes,
        Err(error) => {
            warnings.push(format!(
                "SECURITY hive could not be read for offline LSA secrets: {error}"
            ));
            return;
        }
    };
    match decrypt_lsa_secrets(&security_bytes, seed.boot_key) {
        Ok(secrets) => apply_lsa_secrets(&secrets, seed.user_nt_hashes, prekeys, warnings),
        Err(error) => warnings.push(format!(
            "offline LSA secret decryption was not available: {error}"
        )),
    }
}

fn apply_lsa_secrets(
    secrets: &LsaDecryptedSecrets,
    user_nt_hashes: &[(String, [u8; 16])],
    prekeys: &mut Vec<[u8; 20]>,
    warnings: &mut Vec<String>,
) {
    match secrets
        .secret("DPAPI_SYSTEM")
        .map(DpapiSystemKeys::from_secret)
    {
        Some(Ok(keys)) => {
            prekeys.push(keys.machine_key);
            prekeys.push(keys.user_key);
        }
        Some(Err(error)) => warnings.push(format!("DPAPI_SYSTEM secret was malformed: {error}")),
        None => {}
    }
    for entry in &secrets.secrets {
        let Some(tbal) = TbalSecret::from_secret(&entry.name, &entry.secret) else {
            continue;
        };
        match user_nt_hashes.iter().find(|(_, nt)| *nt == tbal.nt_hash) {
            Some((sid, _)) => {
                prekeys.extend(derive_user_prekeys_from_password_sha1(
                    sid,
                    &tbal.password_sha1,
                ));
            }
            None => warnings.push(format!(
                "TBAL secret {} did not match any SAM user NT hash",
                entry.name
            )),
        }
    }
}

/// Read the CNG system key file whose description identifies the Chrome
/// `Google Chromekey1` key, if present.
pub(super) fn read_cng_chromekey_file(
    conn: &Connection,
    data_source_id: &str,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Option<Vec<u8>> {
    for locator in locate_files_by_like(conn, data_source_id, "%/crypto/systemkeys/%") {
        let Ok(bytes) = read_locator(&locator, cancel_token, file_reader) else {
            continue;
        };
        let Ok(cng) = parse_cng_system_key_file(&bytes) else {
            continue;
        };
        if cng.description.to_ascii_lowercase().contains("chromekey") {
            return Some(bytes);
        }
    }
    None
}

/// Read the Chrome/Edge `elevation_service.exe` used to bind the App-Bound XOR
/// constant to the browser build, if present.
pub(super) fn read_elevation_service(
    conn: &Connection,
    data_source_id: &str,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Option<Vec<u8>> {
    let locator = locate_files_by_like(
        conn,
        data_source_id,
        "%/application/%/elevation_service.exe",
    )
    .into_iter()
    .find(|locator| {
        let path = normalize_evidence_path(&locator.path);
        path.contains("/google/chrome/application/")
            || path.contains("/microsoft/edge/application/")
    })?;
    read_locator(&locator, cancel_token, file_reader).ok()
}

fn locate_files_by_like(
    conn: &Connection,
    data_source_id: &str,
    like_pattern: &str,
) -> Vec<EvidenceCandidate> {
    let Ok(mut statement) = conn.prepare(
        "SELECT id, path, COALESCE(size, 0), partition_index
         FROM file_entries
         WHERE data_source_id = ?1 AND entry_type = 'file' COLLATE NOCASE
           AND REPLACE(LOWER(path), '\\', '/') LIKE ?2
         ORDER BY LENGTH(path) ASC
         LIMIT 32",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map(params![data_source_id, like_pattern], |row| {
        let id: String = row.get(0)?;
        let path: String = row.get(1)?;
        let size: u64 = row.get(2)?;
        let partition_index = row
            .get::<_, Option<i64>>(3)?
            .and_then(|value| usize::try_from(value).ok());
        Ok((id, path, size, partition_index))
    }) else {
        return Vec::new();
    };
    rows.flatten()
        .map(|row| locator_from_row(data_source_id, row, "BrowserPreload"))
        .collect()
}
