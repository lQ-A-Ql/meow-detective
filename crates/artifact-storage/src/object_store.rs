use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use thiserror::Error;

const ARTIFACTS_DIRECTORY: &str = "artifacts";
const OBJECTS_DIRECTORY: &str = "objects";
const DIGEST_HEX_LENGTH: usize = 64;
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum ArtifactStorageError {
    #[error("invalid content address: {0}")]
    InvalidAddress(String),
    #[error("unsafe relative path: {0}")]
    UnsafeRelativePath(String),
    #[error("object does not exist: {0}")]
    ObjectNotFound(PathBuf),
    #[error("object path is a symbolic link: {0}")]
    SymlinkObject(PathBuf),
    #[error("object conflict for {address}: expected {expected_length} bytes, found {actual_length} bytes with digest {actual_digest}")]
    ObjectConflict {
        address: ObjectAddress,
        expected_length: u64,
        actual_length: u64,
        actual_digest: String,
    },
    #[error("object digest mismatch for {address}: found {actual_digest}")]
    DigestMismatch {
        address: ObjectAddress,
        actual_digest: String,
    },
    #[error(
        "object length mismatch for {address}: expected {expected_length}, found {actual_length}"
    )]
    LengthMismatch {
        address: ObjectAddress,
        expected_length: u64,
        actual_length: u64,
    },
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectAddress(String);

pub type ArtifactAddress = ObjectAddress;

impl ObjectAddress {
    pub fn parse(value: &str) -> Result<Self, ArtifactStorageError> {
        if value.len() != DIGEST_HEX_LENGTH
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
            || value.bytes().any(|byte| byte.is_ascii_uppercase())
        {
            return Err(ArtifactStorageError::InvalidAddress(value.to_string()));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn from_digest(digest: [u8; 32]) -> Self {
        Self(hex_encode(&digest))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn first_prefix(&self) -> &str {
        &self.0[..2]
    }

    pub fn second_prefix(&self) -> &str {
        &self.0[2..4]
    }
}

impl std::fmt::Display for ObjectAddress {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for ObjectAddress {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<&str> for ObjectAddress {
    type Error = ArtifactStorageError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub address: ObjectAddress,
    pub length: u64,
    pub path: PathBuf,
    pub reused: bool,
}

#[derive(Debug, Clone)]
pub struct ArtifactStore {
    case_root: PathBuf,
    objects_root: PathBuf,
}

pub type ArtifactStorage = ArtifactStore;
pub type StorageError = ArtifactStorageError;

impl ArtifactStore {
    pub fn new(case_root: impl AsRef<Path>) -> Result<Self, ArtifactStorageError> {
        let case_root = case_root.as_ref().to_path_buf();
        let objects_root = case_root.join(ARTIFACTS_DIRECTORY).join(OBJECTS_DIRECTORY);
        fs::create_dir_all(&objects_root)?;
        Ok(Self {
            case_root,
            objects_root,
        })
    }

    pub fn from_objects_root(objects_root: impl AsRef<Path>) -> Result<Self, ArtifactStorageError> {
        let objects_root = objects_root.as_ref().to_path_buf();
        fs::create_dir_all(&objects_root)?;
        let case_root = objects_root
            .parent()
            .and_then(Path::parent)
            .map_or_else(|| objects_root.clone(), Path::to_path_buf);
        Ok(Self {
            case_root,
            objects_root,
        })
    }

    pub fn case_root(&self) -> &Path {
        &self.case_root
    }

    pub fn objects_root(&self) -> &Path {
        &self.objects_root
    }

    pub fn object_path(&self, address: &ObjectAddress) -> PathBuf {
        self.objects_root
            .join(address.first_prefix())
            .join(address.second_prefix())
            .join(address.as_str())
    }

    pub fn put_bytes(&self, bytes: &[u8]) -> Result<StoredObject, ArtifactStorageError> {
        self.put_reader(io::Cursor::new(bytes))
    }

    pub fn store_bytes(&self, bytes: &[u8]) -> Result<StoredObject, ArtifactStorageError> {
        self.put_bytes(bytes)
    }

    pub fn put_file(&self, source: impl AsRef<Path>) -> Result<StoredObject, ArtifactStorageError> {
        let source = source.as_ref();
        let file = File::open(source)?;
        self.put_reader(file)
    }

    pub fn write(
        &self,
        address: &ObjectAddress,
        bytes: &[u8],
    ) -> Result<StoredObject, ArtifactStorageError> {
        let actual = content_address(bytes);
        if &actual != address {
            return Err(ArtifactStorageError::DigestMismatch {
                address: address.clone(),
                actual_digest: actual.to_string(),
            });
        }
        self.publish_bytes(address, bytes)
    }

    pub fn write_bytes(
        &self,
        address: &ObjectAddress,
        bytes: &[u8],
    ) -> Result<StoredObject, ArtifactStorageError> {
        self.write(address, bytes)
    }

    pub fn put_reader<R: Read>(&self, mut reader: R) -> Result<StoredObject, ArtifactStorageError> {
        let staging_dir = self.objects_root.join(".staging");
        fs::create_dir_all(&staging_dir)?;
        let temporary_path =
            staging_dir.join(format!("{}.{}.tmp", std::process::id(), unique_suffix()));
        let mut temporary = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?;
        let (digest, length) = copy_and_hash(&mut reader, &mut temporary)?;
        temporary.sync_all()?;
        drop(temporary);

        let address = ObjectAddress::from_digest(digest);
        let destination = self.object_path(&address);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let result = self.publish_temporary(&temporary_path, &destination, &address, length);
        if result.as_ref().map_or(true, |stored| stored.reused) {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    fn publish_bytes(
        &self,
        address: &ObjectAddress,
        bytes: &[u8],
    ) -> Result<StoredObject, ArtifactStorageError> {
        let staging_dir = self.objects_root.join(".staging");
        fs::create_dir_all(&staging_dir)?;
        let temporary_path =
            staging_dir.join(format!("{}.{}.tmp", std::process::id(), unique_suffix()));
        let mut temporary = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?;
        temporary.write_all(bytes)?;
        temporary.sync_all()?;
        drop(temporary);
        let destination = self.object_path(address);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let result =
            self.publish_temporary(&temporary_path, &destination, address, bytes.len() as u64);
        if result.as_ref().map_or(true, |stored| stored.reused) {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    pub fn read(&self, address: &ObjectAddress) -> Result<Vec<u8>, ArtifactStorageError> {
        let path = self.object_path(address);
        let mut file = open_object(&path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        self.verify_bytes(address, bytes.len() as u64, &bytes)?;
        Ok(bytes)
    }

    pub fn read_object(&self, address: &ObjectAddress) -> Result<Vec<u8>, ArtifactStorageError> {
        self.read(address)
    }

    pub fn get(&self, address: &ObjectAddress) -> Result<Vec<u8>, ArtifactStorageError> {
        self.read(address)
    }

    pub fn verify(&self, address: &ObjectAddress) -> Result<u64, ArtifactStorageError> {
        let path = self.object_path(address);
        let mut file = open_object(&path)?;
        let length = file.metadata()?.len();
        let (digest, counted_length) = hash_reader(&mut file)?;
        debug_assert_eq!(length, counted_length);
        let actual_digest = hex_encode(&digest);
        if actual_digest != address.as_str() {
            return Err(ArtifactStorageError::DigestMismatch {
                address: address.clone(),
                actual_digest,
            });
        }
        Ok(length)
    }

    pub fn verify_digest(&self, address: &ObjectAddress) -> Result<u64, ArtifactStorageError> {
        self.verify(address)
    }

    pub fn check_digest(&self, address: &ObjectAddress) -> Result<u64, ArtifactStorageError> {
        self.verify(address)
    }

    pub fn resolve_relative_path(
        &self,
        relative: impl AsRef<Path>,
    ) -> Result<PathBuf, ArtifactStorageError> {
        safe_relative_path(&self.case_root, relative)
    }

    fn verify_bytes(
        &self,
        address: &ObjectAddress,
        length: u64,
        bytes: &[u8],
    ) -> Result<(), ArtifactStorageError> {
        let digest = Sha256::digest(bytes);
        let actual_digest = hex_encode(&digest);
        if actual_digest != address.as_str() {
            return Err(ArtifactStorageError::DigestMismatch {
                address: address.clone(),
                actual_digest,
            });
        }
        if length != bytes.len() as u64 {
            return Err(ArtifactStorageError::LengthMismatch {
                address: address.clone(),
                expected_length: length,
                actual_length: bytes.len() as u64,
            });
        }
        Ok(())
    }

    fn publish_temporary(
        &self,
        temporary: &Path,
        destination: &Path,
        address: &ObjectAddress,
        length: u64,
    ) -> Result<StoredObject, ArtifactStorageError> {
        if destination.exists() {
            return self.reuse_existing(destination, address, length);
        }
        match fs::rename(temporary, destination) {
            Ok(()) => {
                sync_directory(destination.parent())?;
                Ok(StoredObject {
                    address: address.clone(),
                    length,
                    path: destination.to_path_buf(),
                    reused: false,
                })
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                self.reuse_existing(destination, address, length)
            }
            Err(error) => Err(ArtifactStorageError::Io(error)),
        }
    }

    fn reuse_existing(
        &self,
        destination: &Path,
        address: &ObjectAddress,
        expected_length: u64,
    ) -> Result<StoredObject, ArtifactStorageError> {
        let mut file = open_object(destination)?;
        let actual_length = file.metadata()?.len();
        let (digest, _) = hash_reader(&mut file)?;
        let actual_digest = hex_encode(&digest);
        if actual_length != expected_length || actual_digest != address.as_str() {
            return Err(ArtifactStorageError::ObjectConflict {
                address: address.clone(),
                expected_length,
                actual_length,
                actual_digest,
            });
        }
        Ok(StoredObject {
            address: address.clone(),
            length: expected_length,
            path: destination.to_path_buf(),
            reused: true,
        })
    }
}

pub fn content_address(bytes: &[u8]) -> ObjectAddress {
    ObjectAddress::from_digest(Sha256::digest(bytes).into())
}

pub fn safe_relative_path(
    root: impl AsRef<Path>,
    relative: impl AsRef<Path>,
) -> Result<PathBuf, ArtifactStorageError> {
    let relative = relative.as_ref();
    let rendered = relative.to_string_lossy();
    if rendered.is_empty()
        || rendered.contains('\0')
        || rendered.contains('\\')
        || rendered.contains(':')
    {
        return Err(ArtifactStorageError::UnsafeRelativePath(
            rendered.into_owned(),
        ));
    }
    if relative.is_absolute() {
        return Err(ArtifactStorageError::UnsafeRelativePath(
            rendered.into_owned(),
        ));
    }
    let mut output = root.as_ref().to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(part) => output.push(part),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(ArtifactStorageError::UnsafeRelativePath(
                    rendered.into_owned(),
                ));
            }
        }
    }
    Ok(output)
}

fn copy_and_hash<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Result<([u8; 32], u64), ArtifactStorageError> {
    let mut hasher = Sha256::new();
    let mut length = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        length = length
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::other("object length overflow"))?;
    }
    Ok((hasher.finalize().into(), length))
}

fn open_object(path: &Path) -> Result<File, ArtifactStorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            ArtifactStorageError::ObjectNotFound(path.to_path_buf())
        } else {
            ArtifactStorageError::Io(error)
        }
    })?;
    if metadata.file_type().is_symlink() {
        return Err(ArtifactStorageError::SymlinkObject(path.to_path_buf()));
    }
    Ok(File::open(path)?)
}

fn hash_reader<R: Read + Seek>(reader: &mut R) -> Result<([u8; 32], u64), ArtifactStorageError> {
    reader.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut length = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        length = length
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::other("object length overflow"))?;
    }
    Ok((hasher.finalize().into(), length))
}

fn sync_directory(directory: Option<&Path>) -> Result<(), ArtifactStorageError> {
    #[cfg(unix)]
    if let Some(directory) = directory {
        File::open(directory)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

fn unique_suffix() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    timestamp ^ u128::from(TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
