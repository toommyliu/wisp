use parking_lot::{Mutex, RwLock};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    task::Waker,
};

#[derive(Default)]
pub(crate) struct Changes(Mutex<Vec<Waker>>, AtomicU64);
impl Changes {
    pub fn watch(&self, waker: Waker) {
        let mut watchers = self.0.lock();
        if !watchers.iter().any(|old| old.will_wake(&waker)) {
            watchers.push(waker);
        }
    }
    pub fn version(&self) -> u64 {
        self.1.load(Ordering::Acquire)
    }
    pub fn notify(&self) {
        self.1.fetch_add(1, Ordering::Release);
        let watchers = self.0.lock().clone();
        for watcher in watchers {
            watcher.wake();
        }
    }
    pub fn set<T: PartialEq>(&self, record: &RwLock<T>, next: T) {
        if *record.read() == next {
            return;
        }
        let mut record = record.write();
        if *record == next {
            return;
        }
        *record = next;
        drop(record);
        self.notify();
    }
}
