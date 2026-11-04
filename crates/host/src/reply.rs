use parking_lot::Mutex;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use crate::accounts::AccountsError;

pub(crate) trait Stopped {
    const STOPPED: Self;
}

pub struct Reply<T, E = AccountsError>(Arc<Mutex<Slot<T, E>>>);

enum Slot<T, E> {
    Waiting(Option<Waker>),
    Ready(Result<T, E>),
    Taken,
}

pub(crate) struct Answer<T, E: Stopped = AccountsError>(Option<Arc<Mutex<Slot<T, E>>>>);

pub(crate) fn reply<T, E: Stopped>() -> (Answer<T, E>, Reply<T, E>) {
    let slot = Arc::new(Mutex::new(Slot::Waiting(None)));
    (Answer(Some(slot.clone())), Reply(slot))
}

impl<T, E> Future for Reply<T, E> {
    type Output = Result<T, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut slot = self.0.lock();
        match std::mem::replace(&mut *slot, Slot::Taken) {
            Slot::Ready(result) => Poll::Ready(result),
            Slot::Waiting(_) => {
                *slot = Slot::Waiting(Some(cx.waker().clone()));
                Poll::Pending
            }
            Slot::Taken => panic!("a reply was polled after it completed"),
        }
    }
}

impl<T, E: Stopped> Answer<T, E> {
    pub(crate) fn send(mut self, result: Result<T, E>) {
        if let Some(slot) = self.0.take() {
            fill(&slot, result);
        }
    }
}

impl<T, E: Stopped> Drop for Answer<T, E> {
    fn drop(&mut self) {
        if let Some(slot) = self.0.take() {
            fill(&slot, Err(E::STOPPED));
        }
    }
}

fn fill<T, E>(slot: &Mutex<Slot<T, E>>, result: Result<T, E>) {
    let previous = std::mem::replace(&mut *slot.lock(), Slot::Ready(result));
    if let Slot::Waiting(Some(waker)) = previous {
        waker.wake();
    }
}
