mod interests;
pub use interests::{EventInterests, EventKind};

use std::sync::mpsc::{self, RecvTimeoutError, TrySendError};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::task::Waker;
use std::time::Duration;

pub use lucent_protocol::bridge::{Event, Value};
use lucent_protocol::event::ProjectionEvent;
use ruffle_core::Player;
use ruffle_core::context::UpdateContext;
use ruffle_core::external::{CallbackError, ExternalInterfaceProvider, Value as ExternalValue};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("cannot equip while you were in combat")]
    EquipInCombat,
    #[error("the player has stopped")]
    Stopped,
    #[error("the player has not received its first viewport")]
    NotReady,
    #[error("the loader has not registered callback {0}")]
    Unavailable(String),
    #[error("loader callback {callback} cannot run before the AVM1 root is ready")]
    LoaderNotReady { callback: String },
    #[error("loader callback {callback} threw: {message}")]
    LoaderThrew { callback: String, message: String },
    #[error("timed out waiting for the player")]
    Timeout,
    #[error("events were discarded because the event queue overflowed")]
    Overflow,
    #[error("the projection read panicked")]
    ReadPanicked,
    #[error("{operation} failed: {reason}")]
    CommandFailed { operation: String, reason: String },
    #[error("couldn't decode {callback}: {reason}")]
    CommandDecode { callback: String, reason: String },
}

pub struct Pending<T> {
    receiver: mpsc::Receiver<Result<T, Error>>,
    waker: Arc<Mutex<Option<Waker>>>,
}

pub(crate) struct Reply<T> {
    sender: Option<mpsc::Sender<Result<T, Error>>>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl<T> Reply<T> {
    pub(crate) fn send(
        mut self,
        result: Result<T, Error>,
    ) -> Result<(), mpsc::SendError<Result<T, Error>>> {
        let result = self.sender.take().unwrap().send(result);
        self.wake();
        result
    }

    fn wake(&self) {
        let waker = self.waker.lock().unwrap().clone();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Drop for Reply<T> {
    fn drop(&mut self) {
        if self.sender.take().is_some() {
            self.wake();
        }
    }
}

pub type PendingCall = Pending<Value>;
pub type PendingRead<T> = Pending<T>;

impl<T> Pending<T> {
    pub(crate) fn channel() -> (Reply<T>, Self) {
        let (sender, receiver) = mpsc::channel();
        let waker = Arc::new(Mutex::new(None));
        (
            Reply {
                sender: Some(sender),
                waker: waker.clone(),
            },
            Self { receiver, waker },
        )
    }

    pub fn set_waker(&self, waker: Waker) {
        *self.waker.lock().unwrap() = Some(waker);
    }

    pub fn try_recv(&self) -> Option<Result<T, Error>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(Error::Stopped)),
        }
    }

    pub(crate) fn stopped() -> Self {
        Self::channel().1
    }

    /// A timeout does not cancel the request. Keep using this reply to receive its result.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, Error> {
        self.receiver.recv_timeout(timeout).map_err(receive_error)?
    }
}

pub(crate) struct Call {
    name: String,
    args: Vec<Value>,
    reply: Reply<Value>,
}

impl Call {
    pub(crate) fn new(name: String, args: Vec<Value>) -> (Call, PendingCall) {
        let (reply, pending) = Pending::channel();
        (Call { name, args, reply }, pending)
    }

    pub(crate) fn reject(self, error: Error) {
        let _ = self.reply.send(Err(error));
    }

    pub(crate) fn dispatch(&mut self, player: &mut Player) -> Result<Value, Error> {
        invoke(player, &self.name, std::mem::take(&mut self.args))
    }

    pub(crate) fn resolve(self, result: Result<Value, Error>) {
        let _ = self.reply.send(result);
    }
}

pub(crate) fn invoke(player: &mut Player, name: &str, args: Vec<Value>) -> Result<Value, Error> {
    player.mutate_with_update_context(|context| {
        match context.external_interface.get_callback(name) {
            Some(callback) => callback
                .call_result(context, name, args.into_iter().map(into_external))
                .map(from_external)
                .map_err(|error| match error {
                    CallbackError::NotReady => Error::LoaderNotReady {
                        callback: name.into(),
                    },
                    CallbackError::Thrown(message) => Error::LoaderThrew {
                        callback: name.into(),
                        message,
                    },
                    CallbackError::ReplyConversion(reason) => Error::CommandDecode {
                        callback: name.into(),
                        reason,
                    },
                }),
            None => Err(Error::Unavailable(name.to_owned())),
        }
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScriptEvent {
    JobPrepared {
        id: u64,
        label: String,
        class_names: Vec<String>,
        warnings: Vec<String>,
    },
    JobWarning {
        id: u64,
        warning: String,
    },
    JobFailed {
        id: u64,
        error: String,
    },
    ProfileDeath {
        area: u64,
        key: crate::game::profile::TargetKey,
    },
    OperationResponsibility {
        id: u64,
        responsible: bool,
    },
    OperationStarted {
        id: u64,
    },
    OperationIssued {
        id: u64,
    },
    AutoZoneChanged {
        config: lucent_protocol::auto_zone::AutoZoneConfig,
    },
    Loader {
        event: Event,
        projected: Vec<ProjectionEvent>,
    },
    Invalidated,
    Operation {
        id: u64,
        answer: Result<lucent_protocol::operation::OperationAnswer, Error>,
    },
}

pub(crate) struct ScriptEventSender {
    interests: EventInterests,
    events: Option<EventSender<ScriptEvent>>,
    waker: Arc<Mutex<Option<Waker>>>,
}

pub(crate) struct ScriptEvents {
    events: Events<ScriptEvent>,
    waker: Arc<Mutex<Option<Waker>>>,
}

pub(crate) fn script_events(
    capacity: usize,
    interests: EventInterests,
) -> (ScriptEventSender, ScriptEvents) {
    let (sender, events) = event_queue(capacity);
    let waker = Arc::new(Mutex::new(None));
    (
        ScriptEventSender {
            interests,
            events: Some(sender),
            waker: waker.clone(),
        },
        ScriptEvents { events, waker },
    )
}

impl ScriptEventSender {
    pub(crate) fn accepts(&self, event: &ScriptEvent) -> bool {
        self.interests.accepts(event)
    }

    pub(crate) fn send(&self, event: ScriptEvent) {
        if !self.accepts(&event) {
            return;
        }
        self.events.as_ref().unwrap().send(event);
        self.wake();
    }

    fn wake(&self) {
        let waker = self.waker.lock().unwrap().clone();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Drop for ScriptEventSender {
    fn drop(&mut self) {
        self.events.take();
        self.wake();
    }
}

impl ScriptEvents {
    pub(crate) fn try_recv(&self) -> Result<Option<ScriptEvent>, Error> {
        self.events.try_recv()
    }

    pub(crate) fn overflowed(&self) -> bool {
        self.events.overflow.load(Ordering::Acquire)
    }

    pub(crate) fn set_waker(&self, waker: Waker) {
        *self.waker.lock().unwrap() = Some(waker);
    }
}

pub(crate) const EVENT_CAPACITY: usize = 1024;

enum QueueEntry<T> {
    Event(T),
    Overflow,
}

pub(crate) struct EventSender<T> {
    sender: mpsc::SyncSender<QueueEntry<T>>,
    overflow: Arc<AtomicBool>,
}

pub(crate) struct Events<T = Event> {
    receiver: mpsc::Receiver<QueueEntry<T>>,
    overflow: Arc<AtomicBool>,
}

pub(crate) fn event_queue<T>(capacity: usize) -> (EventSender<T>, Events<T>) {
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let overflow = Arc::new(AtomicBool::new(false));
    (
        EventSender {
            sender,
            overflow: overflow.clone(),
        },
        Events { receiver, overflow },
    )
}

impl<T> EventSender<T> {
    pub(crate) fn mark_overflow(&self) {
        self.overflow.store(true, Ordering::Release);
        let _ = self.sender.try_send(QueueEntry::Overflow);
    }
    pub(crate) fn send(&self, event: T) {
        if let Err(TrySendError::Full(_)) = self.sender.try_send(QueueEntry::Event(event)) {
            self.mark_overflow();
        }
    }
}

impl<T> Events<T> {
    fn take_overflow(&self) -> bool {
        if !self.overflow.swap(false, Ordering::AcqRel) {
            return false;
        }
        while self.receiver.try_recv().is_ok() {}
        true
    }

    pub(crate) fn try_recv(&self) -> Result<Option<T>, Error> {
        if self.take_overflow() {
            return Err(Error::Overflow);
        }
        match self.receiver.try_recv() {
            Ok(QueueEntry::Event(event)) => Ok(Some(event)),
            Ok(QueueEntry::Overflow) => Err(Error::Overflow),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(Error::Stopped),
        }
    }
}

fn receive_error(error: RecvTimeoutError) -> Error {
    match error {
        RecvTimeoutError::Timeout => Error::Timeout,
        RecvTimeoutError::Disconnected => Error::Stopped,
    }
}

pub(crate) struct Bridge {
    events: EventSender<Event>,
}

impl Bridge {
    pub(crate) fn new() -> (Bridge, Events) {
        Self::with_capacity(EVENT_CAPACITY)
    }

    pub(crate) fn with_capacity(capacity: usize) -> (Bridge, Events) {
        let (events, receiver) = event_queue(capacity);
        (Bridge { events }, receiver)
    }
}

impl ExternalInterfaceProvider for Bridge {
    fn call_method(
        &self,
        _context: &mut UpdateContext<'_>,
        name: &str,
        args: &[ExternalValue],
    ) -> ExternalValue {
        let event = Event {
            name: name.to_owned(),
            args: args.iter().cloned().map(from_external).collect(),
        };
        self.events.send(event);
        ExternalValue::Undefined
    }

    fn on_callback_available(&self, name: &str) {
        tracing::debug!(callback = name, "loader callback registered");
    }

    fn get_id(&self) -> Option<String> {
        None
    }
}

fn from_external(value: ExternalValue) -> Value {
    match value {
        ExternalValue::Undefined => Value::Undefined,
        ExternalValue::Null => Value::Null,
        ExternalValue::Bool(value) => Value::Bool(value),
        ExternalValue::Number(value) => Value::Number(value),
        ExternalValue::String(value) => Value::String(value),
        ExternalValue::Object(value) => Value::Object(
            value
                .into_iter()
                .map(|(key, value)| (key, from_external(value)))
                .collect(),
        ),
        ExternalValue::List(value) => Value::List(value.into_iter().map(from_external).collect()),
    }
}

fn into_external(value: Value) -> ExternalValue {
    match value {
        Value::Undefined => ExternalValue::Undefined,
        Value::Null => ExternalValue::Null,
        Value::Bool(value) => ExternalValue::Bool(value),
        Value::Number(value) => ExternalValue::Number(value),
        Value::String(value) => ExternalValue::String(value),
        Value::Object(value) => ExternalValue::Object(
            value
                .into_iter()
                .map(|(key, value)| (key, into_external(value)))
                .collect(),
        ),
        Value::List(value) => ExternalValue::List(value.into_iter().map(into_external).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Weak;
    use std::task::Wake;

    struct Observe {
        events: Weak<Mutex<ScriptEvents>>,
        seen: mpsc::Sender<Result<Option<ScriptEvent>, Error>>,
    }

    impl Wake for Observe {
        fn wake(self: Arc<Self>) {
            let events = self.events.upgrade().unwrap();
            let events = events.lock().unwrap();
            let event = events.try_recv();
            events.set_waker(Waker::from(self.clone()));
            self.seen.send(event).unwrap();
        }
    }

    struct ReplyWake {
        pending: Weak<Mutex<Pending<u32>>>,
        seen: mpsc::Sender<Option<Result<u32, Error>>>,
    }
    impl Wake for ReplyWake {
        fn wake(self: Arc<Self>) {
            let pending = self.pending.upgrade().unwrap();
            let pending = pending.lock().unwrap();
            self.seen.send(pending.try_recv()).unwrap();
            pending.set_waker(Waker::from(self.clone()));
        }
    }

    #[test]
    fn interested_streams_wake_only_for_selected_events_and_close() {
        struct Count(std::sync::atomic::AtomicUsize);
        impl std::task::Wake for Count {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let (producer, events) = script_events(2, crate::automation::AutoZonePolicy::INTERESTS);
        let count = Arc::new(Count(std::sync::atomic::AtomicUsize::new(0)));
        events.set_waker(Waker::from(count.clone()));
        for _ in 0..20_000 {
            producer.send(ScriptEvent::Loader {
                event: Event {
                    name: "onDebug".into(),
                    args: vec![],
                },
                projected: vec![],
            });
        }
        assert_eq!(count.0.load(Ordering::Relaxed), 0);
        assert_eq!(events.try_recv(), Ok(None));
        producer.send(ScriptEvent::Loader {
            event: Event {
                name: "onExtensionResponse".into(),
                args: vec![],
            },
            projected: vec![ProjectionEvent::Zone {
                map: "ledgermayne".into(),
                zone: "A".into(),
            }],
        });
        assert_eq!(count.0.load(Ordering::Relaxed), 1);
        assert!(matches!(
            events.try_recv(),
            Ok(Some(ScriptEvent::Loader { .. }))
        ));
        producer.send(ScriptEvent::AutoZoneChanged {
            config: Default::default(),
        });
        assert_eq!(count.0.load(Ordering::Relaxed), 2);
        assert!(matches!(
            events.try_recv(),
            Ok(Some(ScriptEvent::AutoZoneChanged { .. }))
        ));
        producer.send(ScriptEvent::Invalidated);
        assert_eq!(count.0.load(Ordering::Relaxed), 3);
        assert_eq!(events.try_recv(), Ok(Some(ScriptEvent::Invalidated)));
        drop(producer);
        assert_eq!(count.0.load(Ordering::Relaxed), 4);
        assert_eq!(events.try_recv(), Err(Error::Stopped));
    }

    #[test]
    fn consumers_declare_independent_interests_and_mixed_batches_arrive_once() {
        let (connection, connections) =
            script_events(4, EventInterests::new(&[EventKind::Connection]));
        let (combat, combats) = script_events(
            4,
            EventInterests::new(&[EventKind::MonsterDeath, EventKind::PlayerDeath]),
        );
        let (script, scripts) = script_events(4, EventInterests::ALL);
        let events = [
            ScriptEvent::Loader {
                event: Event {
                    name: "onDebug".into(),
                    args: vec![],
                },
                projected: vec![],
            },
            ScriptEvent::Loader {
                event: Event {
                    name: "onConnection".into(),
                    args: vec![],
                },
                projected: vec![],
            },
            ScriptEvent::Loader {
                event: Event {
                    name: "onExtensionResponse".into(),
                    args: vec![],
                },
                projected: vec![
                    ProjectionEvent::MonsterDeath { monster_map_id: 3 },
                    ProjectionEvent::PlayerDeath {
                        entity_id: 7,
                        username: "player".into(),
                    },
                ],
            },
        ];
        for event in &events {
            for sender in [&connection, &combat, &script] {
                sender.send(event.clone());
            }
        }
        assert_eq!(connections.try_recv(), Ok(Some(events[1].clone())));
        assert_eq!(connections.try_recv(), Ok(None));
        assert_eq!(combats.try_recv(), Ok(Some(events[2].clone())));
        assert_eq!(combats.try_recv(), Ok(None));
        for event in events {
            assert_eq!(scripts.try_recv(), Ok(Some(event)));
        }
        assert_eq!(scripts.try_recv(), Ok(None));
    }

    #[test]
    fn replies_and_disconnections_wake_after_their_result_is_visible() {
        for disconnected in [false, true] {
            let (reply, pending) = Pending::channel();
            let pending = Arc::new(Mutex::new(pending));
            let (sender, seen) = mpsc::channel();
            pending
                .lock()
                .unwrap()
                .set_waker(Waker::from(Arc::new(ReplyWake {
                    pending: Arc::downgrade(&pending),
                    seen: sender,
                })));
            if disconnected {
                drop(reply);
                assert_eq!(seen.try_recv().unwrap(), Some(Err(Error::Stopped)));
            } else {
                reply.send(Ok(7)).unwrap();
                assert_eq!(seen.try_recv().unwrap(), Some(Ok(7)));
            }
        }
    }

    #[test]
    fn a_reply_received_before_waker_registration_is_still_available() {
        let (reply, pending) = Pending::channel();
        reply.send(Ok(7)).unwrap();
        pending.set_waker(Waker::noop().clone());
        assert_eq!(pending.try_recv(), Some(Ok(7)));
    }

    #[test]
    fn the_latest_waker_observes_events_and_disconnection_and_can_register_again() {
        let (producer, events) = script_events(1, EventInterests::ALL);
        let events = Arc::new(Mutex::new(events));
        let (old_sender, old) = mpsc::channel();
        let (sender, current) = mpsc::channel();
        for seen in [old_sender, sender] {
            events
                .lock()
                .unwrap()
                .set_waker(Waker::from(Arc::new(Observe {
                    events: Arc::downgrade(&events),
                    seen,
                })));
        }
        producer.send(ScriptEvent::Invalidated);
        assert_eq!(
            current.try_recv().unwrap(),
            Ok(Some(ScriptEvent::Invalidated))
        );
        assert_eq!(old.try_recv(), Err(mpsc::TryRecvError::Disconnected));
        drop(producer);
        assert_eq!(current.try_recv().unwrap(), Err(Error::Stopped));
    }
}
