use super::formats::Backend;
impl super::RawImageReader {
    /// Retained native mapping tables; independent readers own separate tables.
    pub fn metadata_bytes(&self) -> u64 {
        backend_bytes(&self.backend)
    }
}
fn backend_bytes(backend: &Backend) -> u64 {
    let (bytes, parent) = match backend {
        Backend::Flat { .. } => return 4096,
        Backend::DynamicVhd { bat, parent, .. } => (bat.capacity() * 4, parent),
        Backend::Qcow2 { l1, parent, .. } => (l1.capacity() * 8, parent),
        Backend::Vhdx { bat, parent, .. } => (bat.capacity() * 8, parent),
    };
    (bytes as u64)
        .saturating_add(4096)
        .saturating_add(parent.as_deref().map_or(0, backend_bytes))
}
