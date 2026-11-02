use super::*;
use lucent_protocol::bridge::from_json;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
};

struct Fake {
    snapshot: RefCell<Snapshot>,
    replies: RefCell<HashMap<String, VecDeque<Result<Value, Error>>>>,
    calls: RefCell<Vec<(Instant, String, Vec<Value>)>>,
    now: RefCell<Instant>,
    loads: RefCell<Vec<Vec<f64>>>,
    cancelled: RefCell<Vec<u64>>,
    hold_at: RefCell<Option<(String, usize)>>,
    held: RefCell<Vec<crate::bridge::Reply<Value>>>,
    claims: std::sync::Arc<std::sync::Mutex<HashSet<i64>>>,
}
impl Fake {
    fn new(now: Instant) -> Self {
        Self {
            snapshot: RefCell::new(Snapshot {
                logged_in: true,
                ..Default::default()
            }),
            replies: Default::default(),
            calls: Default::default(),
            now: RefCell::new(now),
            loads: Default::default(),
            cancelled: Default::default(),
            hold_at: Default::default(),
            held: Default::default(),
            claims: Default::default(),
        }
    }
    fn reply(&self, method: &str, values: impl IntoIterator<Item = Result<Value, Error>>) {
        self.replies
            .borrow_mut()
            .insert(method.into(), values.into_iter().collect());
    }
    fn names(&self, name: &str) -> Vec<(Instant, Vec<Value>)> {
        self.calls
            .borrow()
            .iter()
            .filter(|(_, method, _)| method == name)
            .map(|(at, _, args)| (*at, args.clone()))
            .collect()
    }
}
fn resolved<T>(value: T) -> Pending<T> {
    let (reply, pending) = Pending::channel();
    reply
        .send(Ok(value))
        .unwrap_or_else(|_| panic!("receiver disappeared"));
    pending
}
impl Host for Fake {
    fn snapshot(&self, _: bool) -> Pending<Snapshot> {
        resolved(self.snapshot.borrow().clone())
    }
    fn call(&self, method: &str, args: Vec<Value>) -> Pending<Value> {
        self.calls
            .borrow_mut()
            .push((*self.now.borrow(), method.into(), args));
        let count = self.names(method).len();
        if self
            .hold_at
            .borrow()
            .as_ref()
            .is_some_and(|(name, at)| name == method && (*at == 0 || *at == count))
        {
            let (reply, pending) = Pending::channel();
            self.held.borrow_mut().push(reply);
            return pending;
        }
        let result = self
            .replies
            .borrow_mut()
            .get_mut(method)
            .and_then(|r| r.pop_front())
            .unwrap_or({
                Ok(match method {
                    "quests.isInProgress" | "quests.canComplete" => Value::Bool(false),
                    "quests.getMaxTurnIns" => Value::Number(3.),
                    "quests.complete" => Value::Null,
                    _ => Value::Bool(true),
                })
            });
        let (reply, pending) = Pending::channel();
        reply.send(result).unwrap();
        pending
    }
    fn load(&self, ids: Vec<f64>) -> Result<u64, Error> {
        self.loads.borrow_mut().push(ids);
        Ok(self.loads.borrow().len() as u64)
    }
    fn cancel(&self, id: u64) {
        self.cancelled.borrow_mut().push(id);
    }
    fn use_claim(&self, item: i64) -> Result<Option<Lease>, Error> {
        self.claim(-(item as f64))
    }
    fn claim(&self, id: f64) -> Result<Option<Lease>, Error> {
        let id = id as i64;
        if !self.claims.lock().unwrap().insert(id) {
            return Ok(None);
        }
        let claims = self.claims.clone();
        Ok(Some(Lease::new(move || {
            claims.lock().unwrap().remove(&id);
        })))
    }
}
fn tick(state: &mut Environment, host: &Fake, now: Instant) -> Option<Instant> {
    *host.now.borrow_mut() = now;
    let mut next = None;
    for _ in 0..100 {
        next = state.advance(host, Waker::noop(), now);
    }
    next
}
fn quest_config(host: &Fake, ids: Vec<f64>) {
    host.snapshot.borrow_mut().config.quest_ids = ids.clone();
    host.snapshot.borrow_mut().quests = ids;
}

#[test]
fn empty_configuration_has_zero_calls_and_zero_deadlines_for_ten_seconds() {
    let now = Instant::now();
    let host = Fake::new(now);
    let mut state = Environment::new();
    assert_eq!(tick(&mut state, &host, now), None);
    let mut wakes = 0;
    for second in 1..=10 {
        if tick(&mut state, &host, now + Duration::from_secs(second)).is_some() {
            wakes += 1;
        }
    }
    assert_eq!(wakes, 0);
    assert!(host.calls.borrow().is_empty());
    assert!(host.loads.borrow().is_empty());
}

#[test]
fn quest_accepts_then_completes_with_maximum_turn_ins_and_override() {
    let now = Instant::now();
    let host = Fake::new(now);
    quest_config(&host, vec![802.]);
    host.snapshot
        .borrow_mut()
        .config
        .quest_rewards
        .insert("802".into(), 5586.);
    host.reply(
        "quests.isInProgress",
        [false, false, true, true, true]
            .into_iter()
            .map(|v| Ok(Value::Bool(v))),
    );
    host.reply(
        "quests.canComplete",
        [Ok(Value::Bool(true)), Ok(Value::Bool(true))],
    );
    host.reply("quests.getMaxTurnIns", [Ok(Value::String("3".into()))]);
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    assert!(host.calls.borrow().is_empty());
    tick(&mut state, &host, now + QUEST_INTERVAL);
    assert_eq!(host.names("quests.accept").len(), 1);
    tick(&mut state, &host, now + QUEST_INTERVAL * 2);
    assert_eq!(
        host.names("quests.complete")[0].1,
        vec![
            Value::Number(802.),
            Value::Number(3.),
            Value::Number(5586.),
            Value::Bool(false)
        ]
    );
    state.quests.completed(802.);
    tick(&mut state, &host, now + QUEST_INTERVAL * 2);
    assert!(!state.failures.0.iter().any(|k| k.contains("mutation")));
}

#[test]
fn quest_loading_failures_log_once_and_recover_only_after_success() {
    let logs = crate::automation::test_logs::Logs::default();
    let _guard = tracing::dispatcher::set_default(&logs.dispatch());
    let now = Instant::now();
    let host = Fake::new(now);
    host.snapshot.borrow_mut().config.quest_ids = vec![802.];
    host.reply(
        "quests.isAvailable",
        (0..10).map(|_| Ok(Value::Bool(false))),
    );
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    for id in 1..=4 {
        let at = now + Duration::from_secs(id);
        tick(&mut state, &host, at);
        assert_eq!(host.loads.borrow().last().unwrap(), &vec![802.]);
        state
            .quests
            .loaded(id, Ok(OperationAnswer::LoadQuests(vec![id == 4])));
        tick(&mut state, &host, at);
    }
    assert_eq!(logs.count("environment automation failed"), 1);
    assert_eq!(logs.count("environment automation recovered"), 1);
    assert_eq!(logs.count("environment quest is unavailable"), 1);
}

#[test]
fn failed_mutations_back_off_two_five_then_fifteen_seconds() {
    let now = Instant::now();
    let host = Fake::new(now);
    quest_config(&host, vec![1.]);
    host.reply("quests.accept", (0..10).map(|_| Ok(Value::Bool(false))));
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    for second in 1..=40 {
        tick(&mut state, &host, now + Duration::from_secs(second));
    }
    let seconds: Vec<_> = host
        .names("quests.accept")
        .iter()
        .map(|(at, _)| at.duration_since(now).as_secs())
        .collect();
    assert_eq!(seconds, vec![1, 3, 8, 23, 38]);
}

#[test]
fn mutations_are_serialized_and_spaced_seven_hundred_fifty_milliseconds() {
    let now = Instant::now();
    let host = Fake::new(now);
    quest_config(&host, vec![1., 2.]);
    host.reply(
        "quests.isInProgress",
        [false, false, false, true, false, true]
            .into_iter()
            .map(|v| Ok(Value::Bool(v))),
    );
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    tick(&mut state, &host, now + QUEST_INTERVAL);
    assert_eq!(host.names("quests.accept").len(), 1);
    tick(
        &mut state,
        &host,
        now + QUEST_INTERVAL + SPACING_TEST - Duration::from_millis(1),
    );
    assert_eq!(host.names("quests.accept").len(), 1);
    tick(&mut state, &host, now + QUEST_INTERVAL + SPACING_TEST);
    assert_eq!(host.names("quests.accept").len(), 2);
}
const SPACING_TEST: Duration = Duration::from_millis(750);

fn boost(id: i64, name: &str, link: &str) -> ItemData {
    crate::projection::decode_item(
        &serde_json::json!({"ItemID":id,"sName":name,"sType":"ServerUse","sLink":link,"iQty":2}),
        lucent_protocol::item::ItemContainer::Inventory,
    )
    .unwrap()
}

#[test]
fn boosts_use_one_in_normalized_order_and_wait_for_quantity() {
    let now = Instant::now();
    let host = Fake::new(now);
    host.snapshot.borrow_mut().config.boosts = vec!["Gold".into(), "XP".into()];
    host.snapshot.borrow_mut().items =
        vec![boost(1, "XP", "xpboost::1"), boost(2, "Gold", "gboost::1")];
    host.reply("player.hasActiveBoost", [Ok(Value::Bool(false))]);
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    tick(
        &mut state,
        &host,
        now + BOOST_INTERVAL - Duration::from_millis(1),
    );
    assert!(host.names("inventory.use").is_empty());
    tick(&mut state, &host, now + BOOST_INTERVAL);
    assert_eq!(
        host.names("inventory.use")[0].1,
        vec![from_json(serde_json::json!({"itemId":2}))]
    );
    host.snapshot.borrow_mut().items[1].quantity = 1.;
    tick(&mut state, &host, now + BOOST_INTERVAL + POLL);
    assert_eq!(host.names("inventory.use").len(), 1);
}

#[test]
fn reset_cancels_loading_and_restarts_at_the_first_interval() {
    let now = Instant::now();
    let host = Fake::new(now);
    host.snapshot.borrow_mut().config.quest_ids = vec![1.];
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    tick(&mut state, &host, now + QUEST_INTERVAL);
    state.reset(&host);
    state.dirty = true;
    tick(&mut state, &host, now + QUEST_INTERVAL);
    assert_eq!(*host.cancelled.borrow(), vec![1]);
    assert_eq!(host.loads.borrow().len(), 1);
    tick(&mut state, &host, now + QUEST_INTERVAL * 2);
    assert_eq!(host.loads.borrow().len(), 2);
}

#[test]
fn boosts_recover_only_after_selection_use_and_quantity_confirmation() {
    let logs = crate::automation::test_logs::Logs::default();
    let _guard = logs.enter();
    let now = Instant::now();
    let host = Fake::new(now);
    host.snapshot.borrow_mut().config.boosts = vec!["XP".into()];
    host.snapshot.borrow_mut().items = vec![boost(1, "XP", "xpboost::1")];
    host.reply(
        "player.hasActiveBoost",
        [
            Err(Error::Unavailable("active".into())),
            Ok(Value::Bool(false)),
            Ok(Value::Bool(false)),
        ],
    );
    host.reply(
        "inventory.use",
        [Ok(Value::Bool(false)), Ok(Value::Bool(true))],
    );
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    tick(&mut state, &host, now + BOOST_INTERVAL);
    tick(&mut state, &host, now + BOOST_INTERVAL * 2);
    tick(&mut state, &host, now + BOOST_INTERVAL * 3);
    assert_eq!(logs.count("environment automation failed"), 1);
    assert_eq!(logs.count("environment automation recovered"), 0);
    host.snapshot.borrow_mut().items[0].quantity = 1.;
    tick(&mut state, &host, now + BOOST_INTERVAL * 3 + POLL);
    assert_eq!(logs.count("environment automation recovered"), 1);
}

#[test]
fn unavailable_warnings_clear_after_availability_and_removal() {
    let logs = crate::automation::test_logs::Logs::default();
    let _guard = logs.enter();
    let now = Instant::now();
    let host = Fake::new(now);
    quest_config(&host, vec![1.]);
    host.reply(
        "quests.isAvailable",
        [false, false, true, false, false, false]
            .into_iter()
            .map(|v| Ok(Value::Bool(v))),
    );
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    for second in 1..=5 {
        tick(&mut state, &host, now + Duration::from_secs(second));
    }
    assert_eq!(logs.count("environment quest is unavailable"), 2);
    host.snapshot.borrow_mut().config.quest_ids.clear();
    state.dirty = true;
    tick(&mut state, &host, now + Duration::from_secs(6));
    quest_config(&host, vec![1.]);
    state.dirty = true;
    tick(&mut state, &host, now + Duration::from_secs(7));
    host.reply("quests.isAvailable", [Ok(Value::Bool(false))]);
    tick(&mut state, &host, now + Duration::from_secs(8));
    assert_eq!(logs.count("environment quest is unavailable"), 3);
}

#[test]
fn resetting_at_every_quest_call_boundary_prevents_further_calls() {
    for (complete, method, occurrence) in [
        (false, "quests.isInProgress", 1),
        (false, "quests.isAvailable", 1),
        (false, "quests.isAvailable", 2),
        (false, "quests.isInProgress", 2),
        (false, "world.isActionAvailable", 1),
        (false, "quests.accept", 1),
        (false, "quests.isInProgress", 3),
        (true, "quests.canComplete", 1),
        (true, "quests.canComplete", 2),
        (true, "quests.getMaxTurnIns", 1),
        (true, "quests.complete", 1),
    ] {
        let now = Instant::now();
        let host = Fake::new(now);
        quest_config(&host, vec![1.]);
        if complete {
            host.reply(
                "quests.isInProgress",
                (0..10).map(|_| Ok(Value::Bool(true))),
            );
            host.reply("quests.canComplete", (0..10).map(|_| Ok(Value::Bool(true))));
        }
        *host.hold_at.borrow_mut() = Some((method.into(), occurrence));
        let mut state = Environment::new();
        tick(&mut state, &host, now);
        tick(&mut state, &host, now + QUEST_INTERVAL);
        assert_eq!(host.held.borrow().len(), 1, "{method} {occurrence}");
        let count = host.calls.borrow().len();
        state.reset(&host);
        for reply in host.held.borrow_mut().drain(..) {
            let _ = reply.send(Ok(Value::Bool(true)));
        }
        assert_eq!(tick(&mut state, &host, now + Duration::from_secs(20)), None);
        assert_eq!(host.calls.borrow().len(), count, "{method} {occurrence}");
    }
}

#[test]
fn disabling_an_issued_turn_in_holds_its_quest_until_confirmation_or_timeout() {
    for confirmed in [true, false] {
        let now = Instant::now();
        let host = Fake::new(now);
        quest_config(&host, vec![1.]);
        host.reply(
            "quests.isInProgress",
            (0..10).map(|_| Ok(Value::Bool(true))),
        );
        host.reply("quests.canComplete", (0..10).map(|_| Ok(Value::Bool(true))));
        let mut state = Environment::new();
        tick(&mut state, &host, now);
        tick(&mut state, &host, now + QUEST_INTERVAL);
        assert_eq!(host.names("quests.complete").len(), 1);
        host.snapshot.borrow_mut().config.automation.quests = false;
        state.dirty = true;
        tick(&mut state, &host, now + QUEST_INTERVAL);
        assert!(host.claim(1.).unwrap().is_none());
        let count = host.calls.borrow().len();
        if confirmed {
            state.quests.completed(1.);
        } else {
            tick(&mut state, &host, now + QUEST_INTERVAL + TIMEOUT);
        }
        assert!(host.claim(1.).unwrap().is_some());
        assert_eq!(host.calls.borrow().len(), count);
    }
}

#[test]
fn availability_recheck_warns_without_claiming_mutation_recovery() {
    let logs = crate::automation::test_logs::Logs::default();
    let _guard = logs.enter();
    let now = Instant::now();
    let host = Fake::new(now);
    quest_config(&host, vec![1.]);
    host.reply("quests.accept", [Ok(Value::Bool(false))]);
    host.reply(
        "quests.isAvailable",
        [true, true, true, true, false]
            .into_iter()
            .map(|v| Ok(Value::Bool(v))),
    );
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    for second in 1..=3 {
        tick(&mut state, &host, now + Duration::from_secs(second));
    }
    assert_eq!(host.names("quests.accept").len(), 1);
    assert_eq!(logs.count("environment quest is unavailable"), 1);
    assert_eq!(logs.count("environment automation failed"), 1);
    assert_eq!(logs.count("environment automation recovered"), 0);
}

#[test]
fn a_script_using_a_boost_prevents_native_reuse_and_disable_keeps_issued_use_claimed() {
    let now = Instant::now();
    let host = Fake::new(now);
    host.snapshot.borrow_mut().config.boosts = vec!["XP".into()];
    host.snapshot.borrow_mut().items = vec![boost(1, "XP", "xpboost::1")];
    host.reply("player.hasActiveBoost", [Ok(Value::Bool(false))]);
    let script = host.use_claim(1).unwrap().unwrap();
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    tick(&mut state, &host, now + BOOST_INTERVAL);
    assert!(host.names("inventory.use").is_empty());
    drop(script);
    tick(&mut state, &host, now + BOOST_INTERVAL * 2);
    assert_eq!(host.names("inventory.use").len(), 1);
    host.snapshot.borrow_mut().config.automation.boosts = false;
    state.dirty = true;
    tick(&mut state, &host, now + BOOST_INTERVAL * 2);
    assert!(host.use_claim(1).unwrap().is_none());
    tick(&mut state, &host, now + BOOST_INTERVAL * 2 + TIMEOUT);
    assert!(host.use_claim(1).unwrap().is_some());
    assert_eq!(host.names("inventory.use").len(), 1);
}

#[test]
fn quest_intent_reads_never_exceed_four_in_flight() {
    let now = Instant::now();
    let host = Fake::new(now);
    quest_config(&host, (1..=10).map(f64::from).collect());
    *host.hold_at.borrow_mut() = Some(("quests.isInProgress".into(), 0));
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    tick(&mut state, &host, now + QUEST_INTERVAL);
    assert_eq!(host.names("quests.isInProgress").len(), 4);
    host.held
        .borrow_mut()
        .pop()
        .unwrap()
        .send(Ok(Value::Bool(false)))
        .unwrap();
    tick(&mut state, &host, now + QUEST_INTERVAL);
    assert_eq!(host.names("quests.isInProgress").len(), 5);
    assert_eq!(host.held.borrow().len(), 4);
}

#[test]
fn mutation_timeout_is_five_seconds_and_then_uses_failure_backoff() {
    let now = Instant::now();
    let host = Fake::new(now);
    quest_config(&host, vec![1.]);
    *host.hold_at.borrow_mut() = Some(("quests.accept".into(), 1));
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    tick(&mut state, &host, now + QUEST_INTERVAL);
    tick(
        &mut state,
        &host,
        now + QUEST_INTERVAL + TIMEOUT - Duration::from_millis(1),
    );
    assert!(!state.failures.0.iter().any(|key| key.contains("mutation")));
    tick(&mut state, &host, now + QUEST_INTERVAL + TIMEOUT);
    assert!(state.failures.0.iter().any(|key| key.contains("mutation")));
    tick(&mut state, &host, now + Duration::from_secs(7));
    assert_eq!(host.names("quests.accept").len(), 1);
    tick(&mut state, &host, now + Duration::from_secs(8));
    assert_eq!(host.names("quests.accept").len(), 2);
}

#[test]
fn boost_use_rechecks_quantity_after_selection_before_confirming_consumption() {
    let logs = crate::automation::test_logs::Logs::default();
    let _guard = logs.enter();
    let now = Instant::now();
    let host = Fake::new(now);
    host.snapshot.borrow_mut().config.boosts = vec!["XP".into()];
    host.snapshot.borrow_mut().items = vec![boost(1, "XP", "xpboost::1")];
    host.reply(
        "player.hasActiveBoost",
        [Err(Error::Unavailable("active".into()))],
    );
    let mut state = Environment::new();
    tick(&mut state, &host, now);
    tick(&mut state, &host, now + BOOST_INTERVAL);
    *host.hold_at.borrow_mut() = Some(("player.hasActiveBoost".into(), 2));
    tick(&mut state, &host, now + BOOST_INTERVAL * 2);
    host.snapshot.borrow_mut().items[0].quantity = 1.;
    host.held
        .borrow_mut()
        .pop()
        .unwrap()
        .send(Ok(Value::Bool(false)))
        .unwrap();
    tick(&mut state, &host, now + BOOST_INTERVAL * 2);
    assert_eq!(host.names("inventory.use").len(), 1);
    assert_eq!(logs.count("environment automation recovered"), 0);
    host.snapshot.borrow_mut().items[0].quantity = 0.;
    tick(&mut state, &host, now + BOOST_INTERVAL * 2 + POLL);
    assert_eq!(logs.count("environment automation recovered"), 1);
}
