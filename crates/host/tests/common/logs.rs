#![allow(dead_code)]

use std::sync::{Arc, Condvar, Mutex, Once};
use tracing_subscriber::prelude::*;

const WAIT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone, Default)]
pub struct Logs(Arc<Records>);

#[derive(Default)]
struct Records {
    entries: Mutex<Vec<(tracing::Level, String)>>,
    changed: Condvar,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Logs {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        self.0
            .entries
            .lock()
            .unwrap()
            .push((*event.metadata().level(), message.0));
        self.0.changed.notify_all();
    }
}

impl Logs {
    pub fn wait_for(&self, level: tracing::Level, expected: &str) {
        self.wait_for_count(level, expected, 1);
    }

    pub fn wait_for_count(&self, level: tracing::Level, expected: &str, count: usize) {
        let entries = self.0.entries.lock().unwrap();
        let (entries, timeout) = self
            .0
            .changed
            .wait_timeout_while(entries, WAIT, |entries| {
                entries
                    .iter()
                    .filter(|(l, text)| *l == level && text == expected)
                    .count()
                    < count
            })
            .unwrap();
        let messages = entries.clone();
        drop(entries);
        assert!(
            !timeout.timed_out(),
            "did not receive {expected:?} {count} times: {messages:?}"
        );
    }

    pub fn dispatch(&self) -> tracing::Dispatch {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
        });
        tracing::Dispatch::new(tracing_subscriber::registry().with(self.clone()))
    }

    pub fn messages(&self, level: tracing::Level) -> Vec<String> {
        self.0
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|(l, _)| *l == level)
            .map(|(_, text)| text.clone())
            .collect()
    }
}
