use std::sync::{Mutex, MutexGuard};

#[derive(Debug)]
pub struct LatestFrameMailbox<T> {
    value: Mutex<Option<T>>,
}

impl<T> Default for LatestFrameMailbox<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> LatestFrameMailbox<T> {
    pub fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<T>> {
        self.value
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn push(&self, value: T) -> Option<T> {
        self.lock().replace(value)
    }

    pub fn take(&self) -> Option<T> {
        self.lock().take()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_none()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn newest_value_replaces_and_returns_previous() {
        let mailbox = LatestFrameMailbox::new();
        assert_eq!(mailbox.push(1), None);
        assert_eq!(mailbox.push(2), Some(1));
        assert_eq!(mailbox.take(), Some(2));
        assert!(mailbox.is_empty());
    }

    #[test]
    fn concurrent_producer_never_builds_a_queue() {
        let mailbox = Arc::new(LatestFrameMailbox::new());
        let producer = Arc::clone(&mailbox);
        std::thread::spawn(move || {
            for value in 0..1_000 {
                producer.push(value);
            }
        })
        .join()
        .unwrap();
        assert_eq!(mailbox.take(), Some(999));
        assert!(mailbox.take().is_none());
    }
}
