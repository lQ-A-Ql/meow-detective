//! Priority-ordered task queue for import analysis scheduling.
//!
//! Workers consume tasks from highest to lowest priority so that derived
//! analysis (PST attachments, carved files) completes before artifact
//! extraction, and artifact extraction completes before plain file
//! enumeration.

use std::collections::VecDeque;

/// Priority categories for import analysis tasks.
///
/// Higher-priority items are [`pop`](PriorityTaskQueue::pop)ped before
/// lower-priority ones.  The variant names document which task kinds map
/// to each priority level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskPriority {
    /// Derived analysis tasks — PST attachment extraction, carved-file
    /// processing, and other downstream work that depends on earlier
    /// analysis results.
    High,
    /// Artifact extraction tasks — running artifact parsers (registry,
    /// prefetch, LNK, EVTX, etc.) against candidate files.
    Normal,
    /// File enumeration tasks — per-file timeline projection, content
    /// extraction, and text indexing.
    Low,
}

/// A priority-ordered task queue backed by three [`VecDeque`]s.
///
/// Items pushed with [`TaskPriority::High`] are popped first, then
/// [`TaskPriority::Normal`], then [`TaskPriority::Low`].  Within each
/// level the order is FIFO (the underlying deque insertion order).
///
/// # Example
///
/// ```
/// use app_services::import_analysis::priority_queue::{PriorityTaskQueue, TaskPriority};
///
/// let mut q = PriorityTaskQueue::new();
/// q.push("file-a", TaskPriority::Low);
/// q.push("artifact-1", TaskPriority::Normal);
/// q.push("derived-x", TaskPriority::High);
///
/// assert_eq!(q.pop(), Some("derived-x"));
/// assert_eq!(q.pop(), Some("artifact-1"));
/// assert_eq!(q.pop(), Some("file-a"));
/// assert_eq!(q.pop(), None);
/// ```
#[derive(Debug, Clone)]
pub struct PriorityTaskQueue<T> {
    high: VecDeque<T>,
    normal: VecDeque<T>,
    low: VecDeque<T>,
}

impl<T> Default for PriorityTaskQueue<T> {
    fn default() -> Self {
        Self {
            high: VecDeque::new(),
            normal: VecDeque::new(),
            low: VecDeque::new(),
        }
    }
}

impl<T> PriorityTaskQueue<T> {
    /// Creates an empty priority queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes an item onto the deque corresponding to `priority`.
    pub fn push(&mut self, item: T, priority: TaskPriority) {
        match priority {
            TaskPriority::High => self.high.push_back(item),
            TaskPriority::Normal => self.normal.push_back(item),
            TaskPriority::Low => self.low.push_back(item),
        }
    }

    /// Removes and returns the highest-priority item.
    ///
    /// Drains `high` first, then `normal`, then `low`.  Within each level
    /// the order is FIFO.
    pub fn pop(&mut self) -> Option<T> {
        if let Some(item) = self.high.pop_front() {
            return Some(item);
        }
        if let Some(item) = self.normal.pop_front() {
            return Some(item);
        }
        self.low.pop_front()
    }

    /// Returns `true` when all three deques are empty.
    pub fn is_empty(&self) -> bool {
        self.high.is_empty() && self.normal.is_empty() && self.low.is_empty()
    }

    /// Returns the total number of items across all three priority levels.
    pub fn len(&self) -> usize {
        self.high.len() + self.normal.len() + self.low.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_priority_popped_before_normal() {
        let mut q = PriorityTaskQueue::new();
        q.push("normal", TaskPriority::Normal);
        q.push("high", TaskPriority::High);
        q.push("normal-2", TaskPriority::Normal);

        assert_eq!(q.pop(), Some("high"));
        assert_eq!(q.pop(), Some("normal"));
        assert_eq!(q.pop(), Some("normal-2"));
    }

    #[test]
    fn normal_popped_before_low() {
        let mut q = PriorityTaskQueue::new();
        q.push("low", TaskPriority::Low);
        q.push("normal", TaskPriority::Normal);
        q.push("low-2", TaskPriority::Low);

        assert_eq!(q.pop(), Some("normal"));
        assert_eq!(q.pop(), Some("low"));
        assert_eq!(q.pop(), Some("low-2"));
    }

    #[test]
    fn empty_queue_returns_none() {
        let mut q: PriorityTaskQueue<&str> = PriorityTaskQueue::new();
        assert_eq!(q.pop(), None);
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn priority_order_preserved_under_concurrent_push() {
        let mut q = PriorityTaskQueue::new();

        // Simulate interleaved pushes across all three levels.
        q.push(1, TaskPriority::Low);
        q.push(2, TaskPriority::High);
        q.push(3, TaskPriority::Normal);
        q.push(4, TaskPriority::Low);
        q.push(5, TaskPriority::High);
        q.push(6, TaskPriority::Normal);

        // High items come out first (FIFO within level).
        assert_eq!(q.pop(), Some(2));
        assert_eq!(q.pop(), Some(5));

        // Then normal items.
        assert_eq!(q.pop(), Some(3));
        assert_eq!(q.pop(), Some(6));

        // Then low items.
        assert_eq!(q.pop(), Some(1));
        assert_eq!(q.pop(), Some(4));

        assert_eq!(q.pop(), None);
        assert!(q.is_empty());
    }

    #[test]
    fn len_and_is_empty_track_correctly() {
        let mut q = PriorityTaskQueue::new();
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);

        q.push(1, TaskPriority::Low);
        assert!(!q.is_empty());
        assert_eq!(q.len(), 1);

        q.push(2, TaskPriority::High);
        assert_eq!(q.len(), 2);

        q.push(3, TaskPriority::Normal);
        assert_eq!(q.len(), 3);

        q.pop();
        assert_eq!(q.len(), 2);

        q.pop();
        assert_eq!(q.len(), 1);

        q.pop();
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn default_queue_is_empty() {
        let mut q: PriorityTaskQueue<i32> = PriorityTaskQueue::default();
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
        assert_eq!(q.pop(), None);
    }
}
