//! Compare reachable physical references with committed persistent-data space maps.
use super::*;
mod bitmap;
mod tree;

pub const VERIFICATION_BYTES: u64 = 256 * 1024 * 1024;
const ENTRIES_PER_BITMAP: u64 = 16_320;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Superblock,
    Roots,
    Mappings,
    Details,
    DataIndex,
    DataOverflow,
    MetadataOverflow,
    Index,
    Bitmap,
}

#[derive(Clone, Copy)]
struct Reference {
    count: u32,
    kind: Option<Kind>,
}

#[derive(Clone, Copy)]
struct IndexEntry {
    block: u64,
    free: u32,
    first_free: u32,
}

struct Graph {
    metadata: Vec<Reference>,
    data: Vec<u64>,
    path: Vec<u64>,
    remaining_bytes: u64,
    data_blocks: u64,
}

pub(super) fn validate(reader: &mut dyn EvidenceReader, sb: &ThinSuperblock) -> Result<()> {
    let _memory = reader.reserve_work(VERIFICATION_BYTES)?;
    let count =
        usize::try_from(sb.nr_metadata_blocks).map_err(|_| LvmError::ThinVerificationBudget)?;
    let bytes = (count as u64)
        .checked_mul(std::mem::size_of::<Reference>() as u64)
        .filter(|bytes| *bytes < VERIFICATION_BYTES)
        .ok_or(LvmError::ThinVerificationBudget)?;
    let mut metadata = Vec::new();
    metadata
        .try_reserve_exact(count)
        .map_err(|_| LvmError::ThinVerificationBudget)?;
    metadata.resize(
        count,
        Reference {
            count: 0,
            kind: None,
        },
    );
    let mut graph = Graph {
        metadata,
        data: Vec::new(),
        path: Vec::new(),
        remaining_bytes: VERIFICATION_BYTES
            .checked_sub(bytes + 1024 * 1024)
            .ok_or(LvmError::ThinVerificationBudget)?,
        data_blocks: sb.nr_data_blocks,
    };
    graph.reference(0, Kind::Superblock)?;
    graph.walk(reader, sb.mapping_root, Kind::Roots, 0, None)?;
    graph.walk(reader, sb.details_root, Kind::Details, 0, None)?;
    if sb.metadata_snapshot != 0 {
        graph.reference(sb.metadata_snapshot, Kind::Superblock)?;
        let mut snapshot = superblock::read_at(reader, sb.metadata_snapshot)?;
        if snapshot.transaction_id > sb.transaction_id
            || snapshot.time > sb.time
            || snapshot.nr_metadata_blocks > sb.nr_metadata_blocks
            || snapshot.data_block_size_sectors != sb.data_block_size_sectors
        {
            return Err(metadata_error(
                "thin held snapshot exceeds committed pool version or capacity".into(),
            ));
        }
        snapshot.nr_data_blocks = sb.nr_data_blocks;
        integrity::validate_trees(reader, &snapshot)?;
        graph.walk(reader, snapshot.mapping_root, Kind::Roots, 0, None)?;
        graph.walk(reader, snapshot.details_root, Kind::Details, 0, None)?;
    }
    let data_entries = graph.data_index(reader, sb.data_space_map.bitmap_root)?;
    let metadata_entries = graph.metadata_index(reader, sb.metadata_space_map)?;
    let data_overflow =
        graph.overflow(reader, sb.data_space_map.overflow_root, Kind::DataOverflow)?;
    let metadata_overflow = graph.overflow(
        reader,
        sb.metadata_space_map.overflow_root,
        Kind::MetadataOverflow,
    )?;
    graph.data.sort_unstable();
    graph.compare(
        reader,
        sb.data_space_map,
        &data_entries,
        &data_overflow,
        false,
    )?;
    graph.compare(
        reader,
        sb.metadata_space_map,
        &metadata_entries,
        &metadata_overflow,
        true,
    )
}

impl Graph {
    fn reference(&mut self, block: u64, kind: Kind) -> Result<bool> {
        if self.path.contains(&block) {
            return Err(metadata_error("thin metadata reference cycle".into()));
        }
        let reference = self
            .metadata
            .get_mut(usize::try_from(block).map_err(|_| LvmError::ThinVerificationBudget)?)
            .ok_or_else(|| {
                metadata_error("thin space-map reference exceeds metadata capacity".into())
            })?;
        if reference.kind.is_some_and(|previous| previous != kind) {
            return Err(metadata_error(
                "thin metadata block has conflicting structural roles".into(),
            ));
        }
        reference.count = reference
            .count
            .checked_add(1)
            .ok_or(LvmError::ThinVerificationBudget)?;
        reference.kind = Some(kind);
        Ok(reference.count == 1)
    }

    fn charge(&mut self, bytes: u64) -> Result<()> {
        self.remaining_bytes = self
            .remaining_bytes
            .checked_sub(bytes)
            .ok_or(LvmError::ThinVerificationBudget)?;
        Ok(())
    }

    fn push_mapping(&mut self, block: u64) -> Result<()> {
        if block >= self.data_blocks {
            return Err(metadata_error(
                "thin mapping exceeds space-map data capacity".into(),
            ));
        }
        if self.data.len() == self.data.capacity() {
            let capacity = 4096;
            self.charge(capacity * 16)?;
            self.data
                .try_reserve_exact(capacity as usize)
                .map_err(|_| LvmError::ThinVerificationBudget)?;
        }
        self.data.push(block);
        Ok(())
    }
}
