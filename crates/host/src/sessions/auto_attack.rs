use crate::bridge::{Error, Pending, Reply};
use parking_lot::Mutex;
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::Waker,
    time::{Duration, Instant},
};

pub(crate) enum Edit {
    Enable {
        profile: serde_json::Value,
        target_priority: Vec<String>,
    },
    Disable,
}

#[derive(Default)]
pub(crate) struct Shared {
    pub records: Arc<parking_lot::RwLock<lucent_protocol::auto_attack::AutoAttackRecords>>,
    queue: Mutex<VecDeque<Request>>,
    waker: Mutex<Option<Waker>>,
    count: Arc<AtomicUsize>,
}

pub(crate) struct Request {
    pub edit: Edit,
    pub reply: Option<Reply<String>>,
    pub detached: Arc<AtomicBool>,
    pub expires: Instant,
    count: Arc<AtomicUsize>,
}

impl Request {
    pub fn alive(&self, now: Instant) -> bool {
        !self.detached.load(Ordering::Acquire) && now < self.expires
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Shared {
    pub fn submit(&self, edit: Edit, detached: Arc<AtomicBool>, now: Instant) -> Pending<String> {
        let (reply, pending) = Pending::channel();
        let mut queue = self.queue.lock();
        if self.count.load(Ordering::Acquire) >= 1024 {
            let _ = reply.send(Err(Error::Overflow));
            return pending;
        }
        self.count.fetch_add(1, Ordering::AcqRel);
        queue.push_back(Request {
            edit,
            reply: Some(reply),
            detached,
            expires: now + Duration::from_secs(5),
            count: self.count.clone(),
        });
        drop(queue);
        if let Some(waker) = self.waker.lock().clone() {
            waker.wake();
        }
        pending
    }
    pub fn take(&self) -> Option<Request> {
        self.queue.lock().pop_front()
    }
    pub fn set_waker(&self, waker: Waker) {
        *self.waker.lock() = Some(waker);
    }
}
