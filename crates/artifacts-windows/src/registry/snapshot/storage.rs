use std::{borrow::Cow, sync::Arc};

pub(super) enum SnapshotBytes<'a> {
    Borrowed(&'a [u8]),
    Shared(Arc<Vec<u8>>),
}

impl<'a> From<Cow<'a, [u8]>> for SnapshotBytes<'a> {
    fn from(bytes: Cow<'a, [u8]>) -> Self {
        match bytes {
            Cow::Borrowed(bytes) => Self::Borrowed(bytes),
            Cow::Owned(bytes) => Self::Shared(Arc::new(bytes)),
        }
    }
}

impl SnapshotBytes<'_> {
    pub(super) fn bytes(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Shared(bytes) => bytes,
        }
    }

    pub(super) fn shared(&self) -> Arc<Vec<u8>> {
        match self {
            Self::Borrowed(bytes) => Arc::new(bytes.to_vec()),
            Self::Shared(bytes) => bytes.clone(),
        }
    }

    pub(super) fn into_owned(self) -> SnapshotBytes<'static> {
        SnapshotBytes::Shared(self.into_shared())
    }

    pub(super) fn into_shared(self) -> Arc<Vec<u8>> {
        match self {
            Self::Borrowed(bytes) => Arc::new(bytes.to_vec()),
            Self::Shared(bytes) => bytes,
        }
    }
}
