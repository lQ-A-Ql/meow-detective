use crate::{
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::ScanDirection,
    source_store::SourceStore,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::fs::File;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const SOURCE_GENERATION_MANIFEST_FILE: &str = "source-generation.manifest.json";
pub const SOURCE_GENERATION_SEAL_FILE: &str = "source-generation.seal";
const SOURCE_GENERATION_VERSION: u32 = 1;
const SEALED_KEYSPACES: [KeyspaceName; 4] = [
    KeyspaceName::CatalogRecords,
    KeyspaceName::CatalogIndexes,
    KeyspaceName::PartitionRecords,
    KeyspaceName::PartitionIndexes,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceGenerationManifest {
    pub version: u32,
    pub source_id: String,
    pub attempt_id: String,
    pub immutable_data_digest: String,
    pub projection_batch_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceGenerationReceipt {
    pub path: PathBuf,
    pub manifest_digest: String,
    pub reused: bool,
}

pub fn seal_source_generation(
    source: &SourceStore,
    source_id: &str,
    attempt_id: &str,
    projection_batch_digest: &str,
) -> Result<String> {
    validate_identity(source_id)?;
    validate_identity(attempt_id)?;
    validate_digest(projection_batch_digest)?;
    let path = source.database().path();
    let manifest_path = path.join(SOURCE_GENERATION_MANIFEST_FILE);
    let seal_path = path.join(SOURCE_GENERATION_SEAL_FILE);
    if manifest_path.exists() || seal_path.exists() {
        let manifest = verify_source_generation(source, source_id)?;
        if manifest.attempt_id == attempt_id
            && manifest.projection_batch_digest == projection_batch_digest
        {
            return manifest_digest(path);
        }
        return Err(FjallStorageError::PublicationConflict(path.to_path_buf()));
    }

    write_source_generation_markers(source, source_id, attempt_id, projection_batch_digest)
}

pub fn replace_source_generation(
    source: &SourceStore,
    source_id: &str,
    attempt_id: &str,
    projection_batch_digest: &str,
) -> Result<String> {
    validate_identity(source_id)?;
    validate_identity(attempt_id)?;
    validate_digest(projection_batch_digest)?;
    write_source_generation_markers(source, source_id, attempt_id, projection_batch_digest)
}

fn write_source_generation_markers(
    source: &SourceStore,
    source_id: &str,
    attempt_id: &str,
    projection_batch_digest: &str,
) -> Result<String> {
    let path = source.database().path();
    let manifest_path = path.join(SOURCE_GENERATION_MANIFEST_FILE);
    let seal_path = path.join(SOURCE_GENERATION_SEAL_FILE);
    let manifest = SourceGenerationManifest {
        version: SOURCE_GENERATION_VERSION,
        source_id: source_id.to_owned(),
        attempt_id: attempt_id.to_owned(),
        immutable_data_digest: immutable_data_digest(source)?,
        projection_batch_digest: projection_batch_digest.to_owned(),
    };
    let bytes = serde_json::to_vec(&manifest)
        .map_err(|error| FjallStorageError::Serialization(error.to_string()))?;
    let digest = digest_hex(&bytes);
    write_atomic(&manifest_path, &bytes)?;
    write_atomic(
        &seal_path,
        format!("source-generation-v1\n{digest}\n").as_bytes(),
    )?;
    sync_directory(path)?;
    Ok(digest)
}

pub fn verify_source_generation(
    source: &SourceStore,
    expected_source_id: &str,
) -> Result<SourceGenerationManifest> {
    validate_identity(expected_source_id)?;
    let path = source.database().path();
    let (manifest, _) = read_manifest(path)?;
    if manifest.version != SOURCE_GENERATION_VERSION
        || manifest.source_id != expected_source_id
        || !valid_identity(&manifest.attempt_id)
    {
        return Err(FjallStorageError::InvalidEnvelope(
            "source generation manifest identity is invalid".to_owned(),
        ));
    }
    validate_digest(&manifest.projection_batch_digest)?;
    let actual = immutable_data_digest(source)?;
    if actual != manifest.immutable_data_digest {
        return Err(FjallStorageError::ManifestDigestMismatch {
            expected: manifest.immutable_data_digest,
            actual,
        });
    }
    Ok(manifest)
}

pub fn publish_source_generation(
    build_path: impl AsRef<Path>,
    published_path: impl AsRef<Path>,
) -> Result<SourceGenerationReceipt> {
    let build_path = build_path.as_ref();
    let published_path = published_path.as_ref();
    if !build_path.is_dir() || build_path.parent() != published_path.parent() {
        return Err(FjallStorageError::InvalidBuildPath(
            build_path.display().to_string(),
        ));
    }
    let (_, digest) = read_manifest(build_path)?;
    if published_path.exists() {
        let (_, existing_digest) = read_manifest(published_path)?;
        if existing_digest != digest {
            return Err(FjallStorageError::PublicationConflict(
                published_path.to_path_buf(),
            ));
        }
        return Ok(SourceGenerationReceipt {
            path: published_path.to_path_buf(),
            manifest_digest: digest,
            reused: true,
        });
    }
    match fs::rename(build_path, published_path) {
        Ok(()) => {
            sync_directory(published_path.parent().unwrap_or_else(|| Path::new(".")))?;
            Ok(SourceGenerationReceipt {
                path: published_path.to_path_buf(),
                manifest_digest: digest,
                reused: false,
            })
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let (_, existing_digest) = read_manifest(published_path)?;
            if existing_digest != digest {
                return Err(FjallStorageError::PublicationConflict(
                    published_path.to_path_buf(),
                ));
            }
            Ok(SourceGenerationReceipt {
                path: published_path.to_path_buf(),
                manifest_digest: digest,
                reused: true,
            })
        }
        Err(error) => Err(FjallStorageError::Io(error)),
    }
}

pub fn source_generation_manifest_digest(path: impl AsRef<Path>) -> Result<String> {
    read_manifest(path.as_ref()).map(|(_, digest)| digest)
}

fn immutable_data_digest(source: &SourceStore) -> Result<String> {
    let snapshot = source.database().read_snapshot();
    let mut hasher = Sha256::new();
    for keyspace in SEALED_KEYSPACES {
        let name = keyspace.as_str().as_bytes();
        hasher.update((name.len() as u64).to_be_bytes());
        hasher.update(name);
        for entry in snapshot.prefix(keyspace, &[], ScanDirection::Forward)? {
            hasher.update((entry.key.len() as u64).to_be_bytes());
            hasher.update(&entry.key);
            hasher.update((entry.value.len() as u64).to_be_bytes());
            hasher.update(&entry.value);
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

fn read_manifest(path: &Path) -> Result<(SourceGenerationManifest, String)> {
    let manifest_path = path.join(SOURCE_GENERATION_MANIFEST_FILE);
    let bytes = fs::read(&manifest_path).map_err(|error| marker_error(error, path, false))?;
    let manifest = serde_json::from_slice(&bytes)
        .map_err(|error| FjallStorageError::Deserialization(error.to_string()))?;
    let digest = digest_hex(&bytes);
    let seal = fs::read_to_string(path.join(SOURCE_GENERATION_SEAL_FILE))
        .map_err(|error| marker_error(error, path, true))?;
    let mut lines = seal.lines();
    if lines.next() != Some("source-generation-v1") {
        return Err(FjallStorageError::MissingSeal(path.to_path_buf()));
    }
    let expected = lines
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| FjallStorageError::MissingSeal(path.to_path_buf()))?;
    if expected != digest {
        return Err(FjallStorageError::ManifestDigestMismatch {
            expected: expected.to_owned(),
            actual: digest,
        });
    }
    Ok((manifest, expected.to_owned()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| FjallStorageError::InvalidBuildPath(path.display().to_string()))?;
    let temporary_path = path.with_file_name(format!(".{}.tmp", file_name.to_string_lossy()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary_path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&temporary_path, path)?;
    Ok(())
}

fn marker_error(error: io::Error, path: &Path, seal: bool) -> FjallStorageError {
    if error.kind() == io::ErrorKind::NotFound {
        if seal {
            FjallStorageError::MissingSeal(path.to_path_buf())
        } else {
            FjallStorageError::MissingManifest(path.to_path_buf())
        }
    } else {
        FjallStorageError::Io(error)
    }
}

fn manifest_digest(path: &Path) -> Result<String> {
    let bytes = fs::read(path.join(SOURCE_GENERATION_MANIFEST_FILE))?;
    Ok(digest_hex(&bytes))
}

fn digest_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn validate_identity(value: &str) -> Result<()> {
    if valid_identity(value) {
        Ok(())
    } else {
        Err(FjallStorageError::InvalidBuildPath(value.to_owned()))
    }
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty() && value != "." && value != ".." && !value.contains(['/', '\\', ':', '\0'])
}

fn validate_digest(value: &str) -> Result<()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(FjallStorageError::InvalidEnvelope(
            "source generation digest is invalid".to_owned(),
        ))
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
