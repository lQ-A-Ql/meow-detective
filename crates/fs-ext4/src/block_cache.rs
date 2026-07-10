use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub(crate) struct BlockCache {
    capacity: usize,
    blocks: HashMap<u64, Arc<[u8]>>,
    insertion_order: VecDeque<u64>,
}

impl BlockCache {
    pub(crate) fn with_byte_budget(block_size: u64, byte_budget: usize) -> Self {
        let block_size = usize::try_from(block_size).unwrap_or(usize::MAX);
        let capacity = byte_budget.checked_div(block_size).unwrap_or(0).max(1);
        Self {
            capacity,
            blocks: HashMap::with_capacity(capacity),
            insertion_order: VecDeque::with_capacity(capacity),
        }
    }

    pub(crate) fn get(&self, block: u64) -> Option<Arc<[u8]>> {
        self.blocks.get(&block).cloned()
    }

    pub(crate) fn insert(&mut self, block: u64, data: Vec<u8>) -> Arc<[u8]> {
        if let Some(cached) = self.blocks.get(&block) {
            return cached.clone();
        }
        while self.blocks.len() >= self.capacity {
            let Some(evicted) = self.insertion_order.pop_front() else {
                break;
            };
            self.blocks.remove(&evicted);
        }
        let data: Arc<[u8]> = data.into();
        self.blocks.insert(block, data.clone());
        self.insertion_order.push_back(block);
        data
    }
}

#[cfg(test)]
mod tests {
    use super::BlockCache;

    #[test]
    fn cache_respects_byte_derived_capacity() {
        let mut cache = BlockCache::with_byte_budget(4, 8);
        cache.insert(1, vec![1; 4]);
        cache.insert(2, vec![2; 4]);
        assert!(cache.get(1).is_some());

        cache.insert(3, vec![3; 4]);
        assert!(cache.get(1).is_none());
        assert!(cache.get(2).is_some());
        assert!(cache.get(3).is_some());
    }
}
