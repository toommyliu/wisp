use std::sync::Arc;
use std::task::{Wake, Waker};

pub fn channel() -> (Waker, async_channel::Receiver<()>) {
    let (sender, receiver) = async_channel::bounded(1);
    (Waker::from(Arc::new(Notify(sender))), receiver)
}

struct Notify(async_channel::Sender<()>);

impl Wake for Notify {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.try_send(());
    }
}
