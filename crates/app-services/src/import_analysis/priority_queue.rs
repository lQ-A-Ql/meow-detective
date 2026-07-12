//! Priority-ordered task queue for import analysis scheduling.

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskPriority {
    High,
    Normal,
    Low,
}

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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: T, priority: TaskPriority) {
        match priority {
            TaskPriority::High => self.high.push_back(item),
            TaskPriority::Normal => self.normal.push_back(item),
            TaskPriority::Low => self.low.push_back(item),
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        if let Some(item) = self.high.pop_front() {
            return Some(item);
        }
        if let Some(item) = self.normal.pop_front() {
            return Some(item);
        }
        self.low.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.high.is_empty() && self.normal.is_empty() && self.low.is_empty()
    }

    pub fn len(&self) -> usize {
        self.high.len() + self.normal.len() + self.low.len()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/import_analysis/priority_queue.rs"]
mod tests;
