use std::sync::{Arc, Mutex};

use super::SourceDbRadosObjectProvider;
use crate::ceph_reconstruction::{
    RbdObjectProvider, RbdObjectProviderError, RbdObjectReadOutcome, RbdObjectReadRequest,
};

#[derive(Clone)]
pub(crate) struct SharedRadosObjectProvider {
    inner: Arc<Mutex<SourceDbRadosObjectProvider>>,
}

impl SharedRadosObjectProvider {
    pub(crate) fn new(provider: SourceDbRadosObjectProvider) -> Self {
        Self {
            inner: Arc::new(Mutex::new(provider)),
        }
    }
}

impl RbdObjectProvider for SharedRadosObjectProvider {
    fn read_object_range(
        &mut self,
        request: &RbdObjectReadRequest,
        output: &mut [u8],
    ) -> Result<RbdObjectReadOutcome, RbdObjectProviderError> {
        let mut provider = self
            .inner
            .lock()
            .map_err(|_| RbdObjectProviderError::Unavailable {
                object_identity: request.object_identity.clone(),
                reason: "shared RBD provider lock is poisoned".to_string(),
            })?;
        provider.read_object_range(request, output)
    }
}
