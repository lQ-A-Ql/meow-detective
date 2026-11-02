use crate::{
    database::{DatabaseConfig, PersistMode, SingleWriterTxDatabase},
    errors::{FjallStorageError, Result},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

pub const BUILD_PREFIX: &str = "source.fjall.build.";
pub const PUBLISHED_PREFIX: &str = "source.fjall.";
pub const SOURCES_DIRECTORY: &str = "sources";
pub const SOURCE_BUILD_DIRECTORY_PREFIX: &str = "source.fjall.build.";
pub const SOURCE_PUBLISHED_DIRECTORY_NAME: &str = "source.fjall";
pub const MANIFEST_FILE: &str = "storage-manifest.sha256";
pub const SEAL_FILE: &str = "storage-seal";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBuildLayout {
    root: PathBuf,
    source_id: String,
    attempt_id: String,
    build_name: String,
    published_name: String,
}

impl SourceBuildLayout {
    pub fn new(case_root: impl AsRef<Path>, source_id: &str, attempt_id: &str) -> Result<Self> {
        validate_component(source_id)?;
        validate_component(attempt_id)?;
        Ok(Self {
            root: case_root.as_ref().to_path_buf(),
            source_id: source_id.to_owned(),
            attempt_id: attempt_id.to_owned(),
            build_name: format!("{BUILD_PREFIX}{attempt_id}"),
            published_name: format!("{PUBLISHED_PREFIX}{source_id}"),
        })
    }

    pub fn for_case_source(
        case_root: impl AsRef<Path>,
        source_id: &str,
        attempt_id: &str,
    ) -> Result<Self> {
        validate_component(source_id)?;
        validate_component(attempt_id)?;
        Ok(Self {
            root: case_root.as_ref().join(SOURCES_DIRECTORY).join(source_id),
            source_id: source_id.to_owned(),
            attempt_id: attempt_id.to_owned(),
            build_name: format!("{SOURCE_BUILD_DIRECTORY_PREFIX}{attempt_id}"),
            published_name: SOURCE_PUBLISHED_DIRECTORY_NAME.to_owned(),
        })
    }

    pub fn build_path(&self) -> PathBuf {
        self.root.join(&self.build_name)
    }

    pub fn published_path(&self) -> PathBuf {
        self.root.join(&self.published_name)
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn attempt_id(&self) -> &str {
        &self.attempt_id
    }
}

pub struct SourceBuild {
    layout: SourceBuildLayout,
    database: Option<SingleWriterTxDatabase>,
}

impl SourceBuild {
    pub fn create(layout: SourceBuildLayout) -> Result<Self> {
        fs::create_dir_all(layout.build_path())?;
        Ok(Self {
            layout,
            database: None,
        })
    }

    pub fn open_database(&mut self, config: DatabaseConfig) -> Result<&SingleWriterTxDatabase> {
        if self.database.is_none() {
            self.database = Some(SingleWriterTxDatabase::open(
                self.layout.build_path(),
                config,
            )?);
        }
        self.database
            .as_ref()
            .ok_or_else(|| FjallStorageError::InvalidBuildPath("database was not opened".into()))
    }

    pub fn database(&self) -> Option<&SingleWriterTxDatabase> {
        self.database.as_ref()
    }

    pub fn layout(&self) -> &SourceBuildLayout {
        &self.layout
    }

    pub fn write_file(&self, relative: impl AsRef<Path>, bytes: &[u8]) -> Result<PathBuf> {
        let path = safe_child(&self.layout.build_path(), relative.as_ref())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(path)
    }

    pub fn seal_and_publish(mut self) -> Result<PublishReceipt> {
        if let Some(database) = self.database.take() {
            database.persist(PersistMode::SyncAll)?;
            drop(database);
        }
        seal_build(self.layout.build_path())?;
        publish_build(self.layout.build_path(), self.layout.published_path())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReceipt {
    pub path: PathBuf,
    pub manifest_digest: String,
    pub reused: bool,
}

pub fn seal_build(build_path: impl AsRef<Path>) -> Result<String> {
    let build_path = build_path.as_ref();
    if !build_path.is_dir() {
        return Err(FjallStorageError::InvalidBuildPath(
            build_path.display().to_string(),
        ));
    }
    let digest = directory_digest(build_path)?;
    write_metadata(build_path, MANIFEST_FILE, &digest)?;
    write_metadata(build_path, SEAL_FILE, &format!("seal-v1\n{digest}\n"))?;
    sync_directory(build_path)?;
    Ok(digest)
}

pub fn publish_build(
    build_path: impl AsRef<Path>,
    published_path: impl AsRef<Path>,
) -> Result<PublishReceipt> {
    let build_path = build_path.as_ref();
    let published_path = published_path.as_ref();
    if !build_path.is_dir() {
        return Err(FjallStorageError::InvalidBuildPath(
            build_path.display().to_string(),
        ));
    }
    let digest = verify_sealed(build_path)?;
    if published_path.exists() {
        return inspect_existing(published_path, &digest);
    }
    match fs::rename(build_path, published_path) {
        Ok(()) => {
            sync_directory(published_path.parent().unwrap_or_else(|| Path::new(".")))?;
            Ok(PublishReceipt {
                path: published_path.to_path_buf(),
                manifest_digest: digest,
                reused: false,
            })
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            inspect_existing(published_path, &digest)
        }
        Err(error) => Err(FjallStorageError::Io(error)),
    }
}

pub fn verify_sealed(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    let seal_path = path.join(SEAL_FILE);
    let seal = fs::read_to_string(&seal_path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            FjallStorageError::MissingSeal(path.to_path_buf())
        } else {
            FjallStorageError::Io(error)
        }
    })?;
    let expected = seal
        .lines()
        .nth(1)
        .ok_or_else(|| FjallStorageError::MissingSeal(path.to_path_buf()))?;
    let manifest = fs::read_to_string(path.join(MANIFEST_FILE)).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            FjallStorageError::MissingManifest(path.to_path_buf())
        } else {
            FjallStorageError::Io(error)
        }
    })?;
    let manifest = manifest.trim();
    if manifest.is_empty() {
        return Err(FjallStorageError::MissingManifest(path.to_path_buf()));
    }
    if expected != manifest {
        return Err(FjallStorageError::ManifestDigestMismatch {
            expected: expected.to_owned(),
            actual: manifest.to_owned(),
        });
    }
    let actual = directory_digest(path)?;
    if expected != actual {
        return Err(FjallStorageError::ManifestDigestMismatch {
            expected: expected.to_owned(),
            actual,
        });
    }
    Ok(expected.to_owned())
}

fn inspect_existing(path: &Path, expected: &str) -> Result<PublishReceipt> {
    let actual = verify_sealed(path)?;
    if actual != expected {
        return Err(FjallStorageError::PublicationConflict(path.to_path_buf()));
    }
    Ok(PublishReceipt {
        path: path.to_path_buf(),
        manifest_digest: expected.to_owned(),
        reused: true,
    })
}

fn directory_digest(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for relative in files {
        let path = root.join(&relative);
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        hasher.update((relative.to_string_lossy().len() as u64).to_be_bytes());
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|error| FjallStorageError::InvalidBuildPath(error.to_string()))?;
        if relative == Path::new(MANIFEST_FILE) || relative == Path::new(SEAL_FILE) {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else if path.is_file() {
            files.push(relative.to_path_buf());
        }
    }
    Ok(())
}

fn write_metadata(root: &Path, name: &str, value: &str) -> Result<()> {
    let path = root.join(name);
    let mut file = File::create(path)?;
    file.write_all(value.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn safe_child(root: &Path, relative: &Path) -> Result<PathBuf> {
    let rendered = relative.to_string_lossy();
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || rendered.contains(['\\', ':', '\0'])
    {
        return Err(FjallStorageError::InvalidBuildPath(
            relative.display().to_string(),
        ));
    }
    let mut output = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(part) => output.push(part),
            _ => {
                return Err(FjallStorageError::InvalidBuildPath(
                    relative.display().to_string(),
                ))
            }
        }
    }
    Ok(output)
}

fn validate_component(value: &str) -> Result<()> {
    if value.is_empty() || value == "." || value == ".." || value.contains(['/', '\\', ':']) {
        return Err(FjallStorageError::InvalidBuildPath(value.to_owned()));
    }
    Ok(())
}

fn sync_directory(_path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(_path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = _path;
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 15) as usize] as char);
    }
    result
}
