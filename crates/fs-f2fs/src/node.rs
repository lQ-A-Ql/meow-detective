use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::io::{block_offset, read_exact_at, read_u32, SharedReader};
use crate::nat::NatTable;
use crate::{F2fsError, F2fsSuperblock, Result, F2FS_BLOCK_SIZE};

const NODE_FOOTER_OFFSET: usize = 4072;
const VALUES_PER_NODE: usize = NODE_FOOTER_OFFSET / 4;
const DIRECT_NODE_COUNT: usize = 2;
const INDIRECT_NODE_COUNT: usize = 2;
const NODE_CACHE_CAPACITY: usize = 256;

pub(crate) type NodeCache = Arc<Mutex<BoundedNodeCache>>;

pub(crate) fn new_node_cache() -> NodeCache {
    Arc::new(Mutex::new(BoundedNodeCache::default()))
}

#[derive(Default)]
pub(crate) struct BoundedNodeCache {
    entries: HashMap<u32, Arc<[u32]>>,
    order: VecDeque<u32>,
}

impl BoundedNodeCache {
    fn get(&mut self, nid: u32) -> Option<Arc<[u32]>> {
        let values = self.entries.get(&nid).cloned()?;
        self.order.retain(|cached| *cached != nid);
        self.order.push_back(nid);
        Some(values)
    }

    fn insert(&mut self, nid: u32, values: Arc<[u32]>) {
        self.order.retain(|cached| *cached != nid);
        self.entries.insert(nid, values);
        self.order.push_back(nid);
        while self.entries.len() > NODE_CACHE_CAPACITY {
            if let Some(evicted) = self.order.pop_front() {
                self.entries.remove(&evicted);
            }
        }
    }
}

pub(crate) struct F2fsNodeContext {
    source: SharedReader,
    nat: Arc<NatTable>,
    cache: NodeCache,
    volume_offset: u64,
    main_block: u32,
    block_count: u64,
}

impl F2fsNodeContext {
    pub(crate) fn new(
        source: SharedReader,
        nat: Arc<NatTable>,
        cache: NodeCache,
        volume_offset: u64,
        superblock: &F2fsSuperblock,
    ) -> Self {
        Self {
            source,
            nat,
            cache,
            volume_offset,
            main_block: superblock.main_block,
            block_count: superblock.block_count,
        }
    }
}

pub(crate) struct F2fsBlockResolver {
    context: F2fsNodeContext,
    inode: u32,
    inode_addresses: Vec<u32>,
    node_ids: [u32; 5],
}

impl F2fsBlockResolver {
    pub(crate) fn new(
        context: F2fsNodeContext,
        inode: u32,
        inode_addresses: Vec<u32>,
        node_ids: [u32; 5],
        required_blocks: usize,
    ) -> Result<Self> {
        let resolver = Self {
            context,
            inode,
            inode_addresses,
            node_ids,
        };
        if required_blocks > resolver.capacity()? {
            return Err(F2fsError::Unsupported(format!(
                "file block tree for inode {inode} exceeds double-indirect capacity"
            )));
        }
        Ok(resolver)
    }

    pub(crate) fn resolve(&self, logical_block: usize) -> Result<u32> {
        if let Some(address) = self.inode_addresses.get(logical_block) {
            return Ok(*address);
        }
        let after_inode = logical_block - self.inode_addresses.len();
        let direct_capacity = DIRECT_NODE_COUNT * VALUES_PER_NODE;
        if after_inode < direct_capacity {
            let node = after_inode / VALUES_PER_NODE;
            return self.resolve_direct(self.node_ids[node], after_inode % VALUES_PER_NODE);
        }
        let after_direct = after_inode - direct_capacity;
        let indirect_span = VALUES_PER_NODE * VALUES_PER_NODE;
        let indirect_capacity = INDIRECT_NODE_COUNT * indirect_span;
        if after_direct < indirect_capacity {
            let node = after_direct / indirect_span;
            return self.resolve_indirect(
                self.node_ids[DIRECT_NODE_COUNT + node],
                after_direct % indirect_span,
            );
        }
        self.resolve_double(
            self.node_ids[DIRECT_NODE_COUNT + INDIRECT_NODE_COUNT],
            after_direct - indirect_capacity,
        )
    }

    fn resolve_direct(&self, nid: u32, index: usize) -> Result<u32> {
        if nid == 0 {
            return Ok(0);
        }
        Ok(self.node_values(nid)?[index])
    }

    fn resolve_indirect(&self, nid: u32, index: usize) -> Result<u32> {
        if nid == 0 {
            return Ok(0);
        }
        let values = self.node_values(nid)?;
        let direct_nid = values[index / VALUES_PER_NODE];
        self.resolve_direct(direct_nid, index % VALUES_PER_NODE)
    }

    fn resolve_double(&self, nid: u32, index: usize) -> Result<u32> {
        if nid == 0 {
            return Ok(0);
        }
        let indirect_span = VALUES_PER_NODE * VALUES_PER_NODE;
        let values = self.node_values(nid)?;
        let indirect_nid = values[index / indirect_span];
        self.resolve_indirect(indirect_nid, index % indirect_span)
    }

    fn node_values(&self, nid: u32) -> Result<Arc<[u32]>> {
        if let Some(values) = self
            .context
            .cache
            .lock()
            .map_err(|_| F2fsError::Invalid("node cache lock is poisoned".to_string()))?
            .get(nid)
        {
            return Ok(values);
        }
        let entry =
            self.context
                .nat
                .lookup(&self.context.source, self.context.volume_offset, nid)?;
        if entry.inode != self.inode {
            return Err(F2fsError::Invalid(format!(
                "node {nid} belongs to inode {}, expected {}",
                entry.inode, self.inode
            )));
        }
        if entry.block < self.context.main_block
            || u64::from(entry.block) >= self.context.block_count
        {
            return Err(F2fsError::Invalid(format!(
                "node {nid} block {} is outside the main area",
                entry.block
            )));
        }
        let bytes = read_exact_at(
            &self.context.source,
            block_offset(self.context.volume_offset, entry.block)?,
            F2FS_BLOCK_SIZE,
        )?;
        validate_footer(&bytes, nid, self.inode)?;
        let values: Arc<[u32]> = (0..VALUES_PER_NODE)
            .map(|index| read_u32(&bytes, index * 4, "node value"))
            .collect::<Result<Vec<_>>>()?
            .into();
        self.context
            .cache
            .lock()
            .map_err(|_| F2fsError::Invalid("node cache lock is poisoned".to_string()))?
            .insert(nid, Arc::clone(&values));
        Ok(values)
    }

    fn capacity(&self) -> Result<usize> {
        VALUES_PER_NODE
            .checked_mul(VALUES_PER_NODE)
            .and_then(|square| square.checked_mul(VALUES_PER_NODE))
            .and_then(|double| {
                double.checked_add(INDIRECT_NODE_COUNT * VALUES_PER_NODE * VALUES_PER_NODE)
            })
            .and_then(|value| value.checked_add(DIRECT_NODE_COUNT * VALUES_PER_NODE))
            .and_then(|value| value.checked_add(self.inode_addresses.len()))
            .ok_or_else(|| F2fsError::Unsupported("F2FS block tree capacity overflows".to_string()))
    }
}

fn validate_footer(bytes: &[u8], expected_nid: u32, expected_inode: u32) -> Result<()> {
    let nid = read_u32(bytes, NODE_FOOTER_OFFSET, "node footer nid")?;
    let inode = read_u32(bytes, NODE_FOOTER_OFFSET + 4, "node footer inode")?;
    if nid != expected_nid || inode != expected_inode {
        return Err(F2fsError::Invalid(format!(
            "node footer mismatch for nid {expected_nid}: nid={nid}, ino={inode}"
        )));
    }
    Ok(())
}
