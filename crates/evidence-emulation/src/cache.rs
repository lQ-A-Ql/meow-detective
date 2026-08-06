use std::collections::{HashMap, VecDeque};

const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct ClusterLru {
    max_bytes: usize,
    current_bytes: usize,
    entries: HashMap<u64, Vec<u8>>,
    order: VecDeque<u64>,
}

impl ClusterLru {
    pub(crate) fn new(cluster_size: u32) -> Self {
        Self {
            max_bytes: MAX_CACHE_BYTES / cluster_size as usize * cluster_size as usize,
            current_bytes: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub(crate) fn copy_to(&mut self, cluster_index: u64, buffer: &mut [u8]) -> bool {
        let Some(data) = self.entries.get(&cluster_index) else {
            return false;
        };
        if data.len() != buffer.len() {
            return false;
        }
        buffer.copy_from_slice(data);
        self.touch(cluster_index);
        true
    }

    pub(crate) fn insert(&mut self, cluster_index: u64, data: &[u8]) {
        if data.is_empty() || data.len() > self.max_bytes {
            return;
        }
        if let Some(previous) = self.entries.remove(&cluster_index) {
            self.current_bytes = self.current_bytes.saturating_sub(previous.len());
            self.order.retain(|index| *index != cluster_index);
        }
        self.current_bytes = self.current_bytes.saturating_add(data.len());
        self.entries.insert(cluster_index, data.to_vec());
        self.order.push_back(cluster_index);
        while self.current_bytes > self.max_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(data) = self.entries.remove(&oldest) {
                self.current_bytes = self.current_bytes.saturating_sub(data.len());
            }
        }
    }

    fn touch(&mut self, cluster_index: u64) {
        self.order.retain(|index| *index != cluster_index);
        self.order.push_back(cluster_index);
    }
}
