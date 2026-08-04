use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{
    DirectoryPage, MountAccess, MountError, MountFileHandle, MountFileSystem, MountNode, MountPath,
    MountPlan, MountReadPolicy,
};

type SharedMountFileHandle = Arc<Mutex<Box<dyn MountFileHandle>>>;
type MountHandleRegistry = HashMap<u64, SharedMountFileHandle>;

#[derive(Clone)]
pub struct MountSession {
    inner: Arc<MountSessionInner>,
}

struct MountSessionInner {
    plan: MountPlan,
    filesystem: Arc<dyn MountFileSystem>,
    policy: MountReadPolicy,
    handles: Mutex<MountHandleRegistry>,
    next_handle: Mutex<u64>,
}

impl MountSession {
    pub fn new(
        plan: MountPlan,
        filesystem: Arc<dyn MountFileSystem>,
        policy: MountReadPolicy,
    ) -> Self {
        Self {
            inner: Arc::new(MountSessionInner {
                plan,
                filesystem,
                policy,
                handles: Mutex::new(HashMap::new()),
                next_handle: Mutex::new(1),
            }),
        }
    }

    pub fn plan(&self) -> &MountPlan {
        &self.inner.plan
    }

    pub fn root(&self) -> Result<MountNode, MountError> {
        self.lookup(&MountPath::root())
    }

    pub fn lookup(&self, path: &MountPath) -> Result<MountNode, MountError> {
        self.inner.filesystem.lookup(path)
    }

    pub fn read_directory(
        &self,
        path: &MountPath,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<DirectoryPage, MountError> {
        self.inner.policy.validate_directory_page(limit)?;
        self.inner.filesystem.read_directory(path, cursor, limit)
    }

    pub fn open(&self, path: &MountPath, access: MountAccess) -> Result<u64, MountError> {
        access.validate()?;
        let node = self.inner.filesystem.lookup(path)?;
        if node.is_dir {
            return Err(MountError::IsDirectory(path.to_string()));
        }
        let mut handles =
            self.inner.handles.lock().map_err(|_| {
                MountError::Filesystem("mount handle registry is poisoned".to_string())
            })?;
        if handles.len() >= self.inner.policy.max_open_handles {
            return Err(MountError::HandleLimit);
        }
        let handle = self.inner.filesystem.open_read(path)?;
        let id = self.allocate_handle_id()?;
        handles.insert(id, Arc::new(Mutex::new(handle)));
        Ok(id)
    }

    pub fn read_at(
        &self,
        handle_id: u64,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, MountError> {
        let handle = {
            let handles = self.inner.handles.lock().map_err(|_| {
                MountError::Filesystem("mount handle registry is poisoned".to_string())
            })?;
            handles
                .get(&handle_id)
                .cloned()
                .ok_or(MountError::HandleNotFound(handle_id))?
        };
        let mut handle = handle
            .lock()
            .map_err(|_| MountError::Filesystem("mount file handle is poisoned".to_string()))?;
        let bounded_length = self
            .inner
            .policy
            .validate_read(offset, length, handle.size())?;
        handle
            .read_at(offset, bounded_length)
            .map_err(|error| MountError::Filesystem(error.to_string()))
    }

    pub fn close(&self, handle_id: u64) -> Result<(), MountError> {
        let mut handles =
            self.inner.handles.lock().map_err(|_| {
                MountError::Filesystem("mount handle registry is poisoned".to_string())
            })?;
        handles
            .remove(&handle_id)
            .map(|_| ())
            .ok_or(MountError::HandleNotFound(handle_id))
    }

    pub fn active_handle_count(&self) -> Result<usize, MountError> {
        self.inner
            .handles
            .lock()
            .map(|handles| handles.len())
            .map_err(|_| MountError::Filesystem("mount handle registry is poisoned".to_string()))
    }

    fn allocate_handle_id(&self) -> Result<u64, MountError> {
        let mut next =
            self.inner.next_handle.lock().map_err(|_| {
                MountError::Filesystem("mount handle counter is poisoned".to_string())
            })?;
        let id = *next;
        *next = next.checked_add(1).unwrap_or(1);
        Ok(id)
    }
}
