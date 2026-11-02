use std::{
    collections::BTreeSet,
    sync::Arc,
    task::Waker,
    time::{Duration, Instant},
};

use lucent_protocol::command::Command;
use parking_lot::RwLock;

use crate::{
    ScriptLink,
    bridge::{Error, Pending},
    projection::Projection,
};

const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
const READ_RETRY: Duration = Duration::from_millis(10);

pub struct SettingsPolicy {
    inbox: crate::player::Inbox,
    status: crate::PlayerStatus,
    projection: Arc<RwLock<Projection>>,
    settings: Settings,
    waker: Waker,
}

impl SettingsPolicy {
    pub fn new(link: &ScriptLink, waker: Waker) -> Self {
        Self {
            inbox: link.inbox(),
            status: link.player_status(),
            projection: link.projection_handle(),
            settings: Settings::new(Instant::now()),
            waker,
        }
    }

    pub fn advance(&mut self, now: Instant) -> Option<Instant> {
        if self.status.is_stopped() {
            return None;
        }
        let Some(projection) = self.projection.try_read() else {
            return Some(now + READ_RETRY);
        };
        let can_act = projection.readiness.can_act();
        drop(projection);
        self.settings
            .advance(&self.inbox, can_act, &self.waker, now)
    }
}

trait SettingsHost {
    fn command(&self, command: Command) -> Pending<String>;
}

impl SettingsHost for crate::player::Inbox {
    fn command(&self, command: Command) -> Pending<String> {
        let (reply, pending) = Pending::channel();
        self.post(crate::player::Msg::Command(command, reply));
        pending
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Full,
    Actions,
}

impl Kind {
    fn interval(self) -> Duration {
        match self {
            Self::Full => Duration::from_secs(1),
            Self::Actions => Duration::from_millis(500),
        }
    }
}

struct Settings {
    ticks: [Tick; 2],
    failing: BTreeSet<Kind>,
    can_act: bool,
}

impl Settings {
    fn new(now: Instant) -> Self {
        Self {
            ticks: [Tick::new(Kind::Full, now), Tick::new(Kind::Actions, now)],
            failing: BTreeSet::new(),
            can_act: false,
        }
    }

    fn advance(
        &mut self,
        host: &impl SettingsHost,
        can_act: bool,
        waker: &Waker,
        now: Instant,
    ) -> Option<Instant> {
        for tick in &mut self.ticks {
            if can_act && !self.can_act && matches!(tick.state, State::Scheduled(_)) {
                tick.state = State::Scheduled(now);
            }
            if !can_act && matches!(tick.state, State::Scheduled(_)) {
                continue;
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tick.advance(host, waker, now)
            }))
            .unwrap_or_else(|panic| {
                let message = panic
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_owned())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_default();
                Some(Err(format!("settings transition panicked: {message}")))
            });
            if let Some(result) = result {
                tick.state = State::Scheduled(now + tick.kind.interval());
                match result {
                    Ok(()) => {
                        if self.failing.remove(&tick.kind) && self.failing.is_empty() {
                            tracing::info!("settings policy recovered");
                        }
                    }
                    Err(error) => {
                        if self.failing.is_empty() {
                            tracing::warn!("settings policy tick failed: {error}");
                        }
                        self.failing.insert(tick.kind);
                    }
                }
            }
        }
        self.can_act = can_act;
        self.ticks
            .iter()
            .filter(|tick| can_act || matches!(tick.state, State::Applying(_, _)))
            .map(|tick| tick.state.deadline())
            .min()
    }
}

struct Tick {
    kind: Kind,
    state: State,
}

enum State {
    Scheduled(Instant),
    Applying(Pending<String>, Instant),
}

impl State {
    fn deadline(&self) -> Instant {
        match self {
            Self::Scheduled(at) | Self::Applying(_, at) => *at,
        }
    }
}

fn pending<T>(reply: Pending<T>, waker: &Waker) -> Pending<T> {
    reply.set_waker(waker.clone());
    reply
}

fn receive<T>(reply: &Pending<T>, deadline: Instant, now: Instant) -> Option<Result<T, String>> {
    reply
        .try_recv()
        .map(|result| result.map_err(|error| error.to_string()))
        .or_else(|| (now >= deadline).then(|| Err(Error::Timeout.to_string())))
}

impl Tick {
    fn new(kind: Kind, now: Instant) -> Self {
        Self {
            kind,
            state: State::Scheduled(now),
        }
    }

    fn advance(
        &mut self,
        host: &impl SettingsHost,
        waker: &Waker,
        now: Instant,
    ) -> Option<Result<(), String>> {
        if let State::Scheduled(at) = self.state {
            if now < at {
                return None;
            }
            self.state = State::Applying(
                pending(
                    host.command(Command::ReapplySettings {
                        actions_only: self.kind == Kind::Actions,
                    }),
                    waker,
                ),
                now + REPLY_TIMEOUT,
            );
        }
        let State::Applying(reply, deadline) = &self.state else {
            unreachable!()
        };
        receive(reply, *deadline, now).map(|result| {
            if serde_json::from_str::<serde_json::Value>(&result?)
                .map_err(|error| error.to_string())?
                != serde_json::Value::Null
            {
                return Err("Invalid reapplySettings reply".into());
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
