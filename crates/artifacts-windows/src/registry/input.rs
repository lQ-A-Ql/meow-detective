use std::{borrow::Cow, sync::Arc};

use super::{reader::HiveView, RegistryError, RegistrySnapshot};

/// Registry input whose validated immutable view can be shared by field queries.
pub trait RegistryHiveSource {
    fn registry_view(&self) -> Result<Cow<'_, HiveView>, RegistryError>;
}

impl RegistryHiveSource for [u8] {
    fn registry_view(&self) -> Result<Cow<'_, HiveView>, RegistryError> {
        complete(Cow::Owned(HiveView::open(Arc::new(self.to_vec()))?))
    }
}

impl RegistryHiveSource for Vec<u8> {
    fn registry_view(&self) -> Result<Cow<'_, HiveView>, RegistryError> {
        self.as_slice().registry_view()
    }
}

impl<const N: usize> RegistryHiveSource for [u8; N] {
    fn registry_view(&self) -> Result<Cow<'_, HiveView>, RegistryError> {
        self.as_slice().registry_view()
    }
}

impl RegistryHiveSource for Arc<Vec<u8>> {
    fn registry_view(&self) -> Result<Cow<'_, HiveView>, RegistryError> {
        complete(Cow::Owned(HiveView::open(self.clone())?))
    }
}

impl RegistryHiveSource for HiveView {
    fn registry_view(&self) -> Result<Cow<'_, HiveView>, RegistryError> {
        complete(Cow::Borrowed(self))
    }
}

impl RegistryHiveSource for RegistrySnapshot<'_> {
    fn registry_view(&self) -> Result<Cow<'_, HiveView>, RegistryError> {
        complete(Cow::Borrowed(self.view()?))
    }
}

fn complete(view: Cow<'_, HiveView>) -> Result<Cow<'_, HiveView>, RegistryError> {
    if view.is_partial() {
        return Err(super::reader::BrowserError::Corrupt.into());
    }
    Ok(view)
}
