use parking_lot::{Mutex, RwLock};
use std::task::Waker;

#[derive(Default)]
pub(crate) struct Changes(Mutex<Vec<Waker>>);
impl Changes {
    pub fn watch(&self, waker: Waker) {
        let mut watchers = self.0.lock();
        if !watchers.iter().any(|old| old.will_wake(&waker)) {
            watchers.push(waker);
        }
    }
    pub fn notify(&self) {
        let watchers = self.0.lock().clone();
        for watcher in watchers {
            watcher.wake();
        }
    }
    pub fn set<T: PartialEq>(&self, record: &RwLock<T>, next: T) {
        let mut record = record.write();
        if *record == next {
            return;
        }
        *record = next;
        drop(record);
        self.notify();
    }
}
