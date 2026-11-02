use std::sync::{Arc, Mutex, Once};
use tracing_subscriber::prelude::*;

#[derive(Default, Clone)]
pub(crate) struct Logs(Arc<Mutex<Vec<(tracing::Level, String)>>>);

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
            .lock()
            .unwrap()
            .push((*event.metadata().level(), message.0));
    }
}

impl Logs {
    pub(crate) fn dispatch(&self) -> tracing::Dispatch {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
        });
        tracing::Dispatch::new(tracing_subscriber::registry().with(self.clone()))
    }

    pub(crate) fn enter(&self) -> tracing::dispatcher::DefaultGuard {
        tracing::dispatcher::set_default(&self.dispatch())
    }

    pub(crate) fn messages(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|(_, message)| message.clone())
            .collect()
    }

    pub(crate) fn cause(&self, prefix: &str) -> String {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find_map(|(level, message)| {
                (*level == tracing::Level::WARN)
                    .then(|| message.strip_prefix(prefix))
                    .flatten()
                    .map(str::to_owned)
            })
            .unwrap_or_default()
    }

    pub(crate) fn count_at(&self, level: tracing::Level, text: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(actual, message)| *actual == level && message.contains(text))
            .count()
    }

    pub(crate) fn count(&self, text: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, message)| message.contains(text))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_callsites_stay_enabled_on_threads_without_a_local_subscriber() {
        let _dispatch = Logs::default().dispatch();
        assert!(
            std::thread::spawn(|| tracing::enabled!(tracing::Level::WARN))
                .join()
                .unwrap()
        );
    }
}
