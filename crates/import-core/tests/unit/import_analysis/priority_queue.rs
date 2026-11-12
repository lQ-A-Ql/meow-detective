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
