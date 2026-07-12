//! Parse iOS Photos database (Photos.sqlite), extracting photo asset metadata
//! including filename, dimensions, creation date, and album membership.
//!
//! The modern iOS Photos schema uses CoreData conventions: `ZASSET` for asset
//! records, `ZADDITIONALASSETATTRIBUTES` for extended metadata, and
//! `Z_<N>ALBUMS` / `ZGENERICALBUM` for album relationships.

use crate::{core_data_time_to_dt, open_sqlite_from_bytes, IosArtifactError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A parsed iOS photo (asset) record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosPhoto {
    /// Original filename of the photo or video.
    pub filename: Option<String>,
    /// Width in pixels.
    pub width: Option<f64>,
    /// Height in pixels.
    pub height: Option<f64>,
    /// Creation date (ZASSET.ZDATECREATED as CFAbsoluteTime).
    pub created_at: Option<DateTime<Utc>>,
    /// Names of albums containing this photo.
    pub albums: Vec<String>,
}

/// Parse an iOS `Photos.sqlite` and return extracted photo assets.
///
/// Queries `ZASSET` for basic metadata, `ZADDITIONALASSETATTRIBUTES` for extra
/// fields, and album join tables for album names.
pub fn parse_photos(data: &[u8]) -> Result<Vec<IosPhoto>, IosArtifactError> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    // Build album lookup: Z_PK → title
    let mut album_names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT Z_PK, ZTITLE FROM ZGENERICALBUM") {
        if let Ok(rows) = stmt.query_map([], |row| {
            let pk: i64 = row.get(0)?;
            let title: String = row.get(1)?;
            Ok((pk, title))
        }) {
            for row in rows.flatten() {
                album_names.insert(row.0, row.1);
            }
        }
    }

    // Assets-to-albums mapping. The join table name varies across iOS versions;
    // common forms are Z_5ASSETS (where 5 is a CoreData relationship id) or a
    // generic ZASSETALBUM table. We quote the table name so it can include
    // special characters like angle brackets used in placeholders.
    let mut asset_albums: std::collections::HashMap<i64, Vec<i64>> =
        std::collections::HashMap::new();
    // Try a few common join-table names.
    for table in &["ZASSETALBUMS", "\"Z_<N>ASSETS\""] {
        let sql = format!(
            "SELECT asset_pk, album_pk FROM {} WHERE album_pk IS NOT NULL",
            table
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map([], |row| {
                let asset_pk: i64 = row.get(0)?;
                let album_pk: i64 = row.get(1)?;
                Ok((asset_pk, album_pk))
            }) {
                for row in rows.flatten() {
                    asset_albums.entry(row.0).or_default().push(row.1);
                }
            }
        }
    }

    // Main asset query
    let query_result =
        conn.prepare("SELECT Z_PK, ZFILENAME, ZWIDTH, ZHEIGHT, ZDATECREATED FROM ZASSET");
    let mut results = Vec::new();

    let mut stmt = match query_result {
        Ok(s) => s,
        Err(_) => {
            // Try alternative table name (ZGENERICASSET)
            conn.prepare(
                "SELECT Z_PK, ZFILENAME, ZWIDTH, ZHEIGHT, ZDATECREATED FROM ZGENERICASSET",
            )?
        }
    };

    let rows = stmt.query_map([], |row| {
        let pk: i64 = row.get(0)?;
        let filename: Option<String> = row.get(1).ok();
        let width: Option<f64> = row.get(2).ok();
        let height: Option<f64> = row.get(3).ok();
        let created_raw: Option<f64> = row.get(4).ok();
        Ok((pk, filename, width, height, created_raw))
    })?;

    for row in rows {
        let (pk, filename, width, height, created_raw) = match row {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("skipping photo row: {}", e);
                continue;
            }
        };

        let created_at = created_raw.and_then(core_data_time_to_dt);

        let mut albums = Vec::new();
        if let Some(album_pks) = asset_albums.get(&pk) {
            for apk in album_pks {
                if let Some(name) = album_names.get(apk) {
                    albums.push(name.clone());
                }
            }
        }

        results.push(IosPhoto {
            filename,
            width,
            height,
            created_at,
            albums,
        });
    }

    Ok(results)
}

#[cfg(test)]
#[path = "../tests/unit/photos.rs"]
mod tests;
