use std::collections::{HashMap, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

use crate::{
    bridge::{Error, Pending, ScriptEvent, Value},
    player::{Inbox, Msg},
    projection::{
        Projection,
        wire::{wire_bool, wire_i64, wire_int},
    },
};
use lucent_protocol::{
    Packet, PacketEncoding,
    item::{ItemContainer, ItemData},
    operation::{Operation, OperationAnswer},
};
use parking_lot::{Condvar, Mutex, RwLock};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(20);
const PACKET_TIMEOUT: Duration = Duration::from_secs(10);

enum Waiting {
    Accept(i64, mpsc::Sender<Result<AcceptResponse, Error>>),
    Equip(i64, mpsc::Sender<Result<(), Error>>),
}

#[derive(Default)]
pub(crate) struct Packets(Mutex<PacketState>);
#[derive(Default)]
struct PacketState {
    stopped: bool,
    claims: super::claims::Claims,
    waiting: Option<Waiting>,
}

struct AcceptResponse {
    success: bool,
    bank: Option<bool>,
    house: Option<bool>,
    quantity: Option<f64>,
    quantity_now: Option<f64>,
}

impl Packets {
    fn subscribe(&self, item: i64) -> Result<mpsc::Receiver<Result<AcceptResponse, Error>>, Error> {
        let mut state = self.0.lock();
        if state.stopped {
            return Err(Error::Stopped);
        }
        let (reply, receiver) = mpsc::channel();
        state.waiting = Some(Waiting::Accept(item, reply));
        Ok(receiver)
    }

    pub(crate) fn packet(&self, packet: &Packet) {
        let mut state = self.0.lock();
        let Some(waiting) = &state.waiting else {
            return;
        };
        let Packet::Extension {
            command,
            data,
            encoding: PacketEncoding::Json,
            ..
        } = packet
        else {
            return;
        };
        if let Waiting::Equip(action, _) = waiting {
            if command == "seia"
                && data.get("iRes").and_then(wire_bool) == Some(true)
                && data.get("o").and_then(|o| o.get("id")).and_then(wire_i64) == Some(*action)
                && let Some(Waiting::Equip(_, reply)) = state.waiting.take()
            {
                let _ = reply.send(Ok(()));
            }
            return;
        }
        let Waiting::Accept(id, _) = waiting else {
            unreachable!()
        };
        if command != "getDrop" {
            return;
        }
        let decode = || {
            let object = data.as_object()?;
            let item = wire_i64(object.get("ItemID")?).filter(|id| *id > 0)?;
            if item != *id {
                return None;
            }
            let optional = |key: &str, decode: fn(&serde_json::Value) -> Option<bool>| {
                object
                    .get(key)
                    .map_or(Some(None), |value| decode(value).map(Some))
            };
            let quantity = |key: &str| {
                object
                    .get(key)
                    .map_or(Some(None), |value| wire_int(value).map(Some))
            };
            Some(AcceptResponse {
                success: wire_bool(object.get("bSuccess")?)?,
                bank: optional("bBank", wire_bool)?,
                house: optional("bHouse", wire_bool)?,
                quantity: quantity("iQty")?,
                quantity_now: quantity("iQtyNow")?,
            })
        };
        if let Some(response) = decode() {
            let Some(Waiting::Accept(_, reply)) = state.waiting.take() else {
                unreachable!()
            };
            let _ = reply.send(Ok(response));
        }
    }

    fn unsubscribe(&self) {
        self.0.lock().waiting.take();
    }

    pub(crate) fn acquire_claim(
        &self,
        owner: super::claims::Owner,
        item: Option<i64>,
    ) -> Result<Option<super::claims::ClaimGrant>, Error> {
        let mut state = self.0.lock();
        if state.stopped {
            return Err(Error::Stopped);
        }
        state.claims.acquire(
            owner,
            super::claims::ClaimResource::ConsumableSlot5 { item },
        )
    }

    pub(crate) fn release_claim(&self, owner: super::claims::Owner, id: u64) {
        self.0.lock().claims.release(owner, id);
    }
    pub(crate) fn release_owner(&self, owner: super::claims::Owner) {
        self.0.lock().claims.release_owner(owner);
    }

    pub(crate) fn binding(&self) -> super::claims::Binding {
        self.0.lock().claims.binding()
    }

    pub(crate) fn acquire_for_job(
        &self,
        owner: super::claims::Owner,
        item: Option<i64>,
        gate: &crate::game::jobs::JobGate,
        detached: &AtomicBool,
    ) -> Result<Option<super::ClaimGrant>, Error> {
        let mut state = self.0.lock();
        if state.stopped || detached.load(Ordering::Acquire) || !gate.open() {
            return Err(Error::Stopped);
        }
        state.claims.acquire(
            owner,
            super::claims::ClaimResource::ConsumableSlot5 { item },
        )
    }

    pub(crate) fn release_attachment(&self, attachment: super::AttachmentId) {
        self.0.lock().claims.release_attachment(attachment);
    }

    pub(crate) fn invalidate(&self) {
        let mut state = self.0.lock();
        state.waiting.take();
        state.claims.clear();
    }

    pub(crate) fn stop(&self) {
        let mut state = self.0.lock();
        state.stopped = true;
        match state.waiting.take() {
            Some(Waiting::Accept(_, reply)) => {
                let _ = reply.send(Err(Error::Stopped));
            }
            Some(Waiting::Equip(_, reply)) => {
                let _ = reply.send(Err(Error::Stopped));
            }
            None => {}
        }
    }
}

#[derive(Clone)]
pub(crate) struct Endpoint {
    pub(crate) inbox: Inbox,
    pub(crate) projection: Arc<RwLock<Projection>>,
    pub(crate) stopped: Arc<AtomicBool>,
    pub(crate) packets: Arc<Packets>,
}

impl Endpoint {
    fn read<R>(
        &self,
        operations: &Shared,
        id: u64,
        read: impl FnOnce(&Projection) -> R,
    ) -> Result<R, Error> {
        let deadline = Instant::now() + CALL_TIMEOUT;
        let projection = loop {
            self.check()?;
            operations.check(id)?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::Timeout);
            }
            if let Some(projection) = self
                .projection
                .try_read_for(remaining.min(STOP_CHECK_INTERVAL))
            {
                break projection;
            }
        };
        self.check()?;
        operations.check(id)?;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(&projection)))
            .map_err(|_| Error::ReadPanicked)
    }
    fn wait<T>(&self, operations: &Shared, id: u64, pending: Pending<T>) -> Result<T, Error> {
        self.wait_until(operations, id, pending, Instant::now() + CALL_TIMEOUT)
    }

    fn wait_until<T>(
        &self,
        operations: &Shared,
        id: u64,
        pending: Pending<T>,
        deadline: Instant,
    ) -> Result<T, Error> {
        loop {
            self.check()?;
            operations.check(id)?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::Timeout);
            }
            match pending.recv_timeout(remaining.min(STOP_CHECK_INTERVAL)) {
                Err(Error::Timeout) => {}
                result => return result,
            }
        }
    }
    fn check(&self) -> Result<(), Error> {
        if self.stopped.load(Ordering::Acquire) {
            Err(Error::Stopped)
        } else {
            Ok(())
        }
    }
    fn accept(
        &self,
        selector: lucent_protocol::query::ItemSelector,
        operations: &Shared,
        id: u64,
    ) -> Result<bool, Error> {
        self.check()?;
        operations.check(id)?;
        let (reply, logged_in) = crate::bridge::Pending::channel();
        self.inbox.post(Msg::Command(
            lucent_protocol::command::Command::RefreshLoggedIn {},
            reply,
        ));
        let logged_in = serde_json::from_str::<bool>(&self.wait(operations, id, logged_in)?)
            .map_err(|error| crate::projection::commands::decode_error("auth.isLoggedIn", error))?;
        if !logged_in {
            return Ok(false);
        }
        let prepared = self.read(operations, id, |projection| {
            let drop = projection
                .items
                .resolve(ItemContainer::Drop, &selector)?
                .data()
                .clone();
            let quantities = [
                ItemContainer::Bank,
                ItemContainer::House,
                ItemContainer::Inventory,
            ]
            .map(|container| {
                projection
                    .items
                    .get(container, drop.item_id)
                    .map_or(0.0, |item| item.data().quantity)
            });
            let response = self.packets.subscribe(drop.item_id);
            Some((drop, quantities, response))
        })?;
        let Some((drop, quantities, response)) = prepared else {
            return Ok(false);
        };
        let response = response?;
        let result = self.accept_subscribed(&drop, quantities, response, operations, id);
        self.packets.unsubscribe();
        result
    }

    fn prepare_consumable_claim(
        &self,
        claim: &str,
        owner: super::claims::Owner,
        operations: &Shared,
        id: u64,
    ) -> Result<bool, Error> {
        let claim = claim.parse().map_err(|_| {
            crate::projection::commands::decode_error(
                "prepareConsumableClaim",
                "invalid claim handle",
            )
        })?;
        let preparation = self.packets.0.lock().claims.preparation(owner, claim);
        let Some((item, binding, preparation)) = preparation else {
            return Ok(false);
        };
        let slot: Option<i64> = match preparation {
            super::claims::Preparation::Ready | super::claims::Preparation::Unchecked => {
                let (reply, slot) = Pending::channel();
                self.inbox.post(Msg::Command(
                    lucent_protocol::command::Command::RefreshEquippedConsumable {},
                    reply,
                ));
                serde_json::from_str(&self.wait(operations, id, slot)?).map_err(|error| {
                    crate::projection::commands::decode_error(
                        "combat.getConsumableSkillItem",
                        error,
                    )
                })?
            }
            super::claims::Preparation::EquipRequired => None,
        };
        if slot != Some(item) {
            let idle = self.read(operations, id, |projection| {
                projection.world.get_me().is_some_and(|player| {
                    player.snapshot_data().data.entity.state
                        == lucent_protocol::world::EntityState::Idle
                })
            })?;
            if !idle {
                return Err(Error::EquipInCombat);
            }
            if !self.packets.0.lock().claims.set_preparation(
                binding,
                item,
                super::claims::Preparation::EquipRequired,
            ) {
                return Ok(false);
            }
            if !self.equip_consumable(
                lucent_protocol::query::ItemSelector::Id(item as f64),
                operations,
                id,
                Some(binding),
            )? {
                return Ok(false);
            }
        }
        Ok(self.packets.0.lock().claims.set_preparation(
            binding,
            item,
            super::claims::Preparation::Ready,
        ))
    }

    fn equip_consumable(
        &self,
        selector: lucent_protocol::query::ItemSelector,
        operations: &Shared,
        id: u64,
        binding: Option<super::claims::Binding>,
    ) -> Result<bool, Error> {
        let prepared = self.read(operations, id, |projection| {
            let item = projection
                .items
                .resolve(ItemContainer::Inventory, &selector)?;
            let data = item.data();
            if data.category != "Item"
                || matches!(
                    crate::projection::wire::trim_js(&data.link)
                        .to_ascii_lowercase()
                        .as_str(),
                    "elixir" | "tonic"
                )
            {
                return None;
            }
            let action =
                wire_i64(&serde_json::Value::String(data.meta.clone())).filter(|id| *id > 0)?;
            let mut state = self.packets.0.lock();
            if state.stopped || binding.is_some_and(|binding| binding != state.claims.binding()) {
                return None;
            }
            let (reply, response) = mpsc::channel();
            state.waiting = Some(Waiting::Equip(action, reply));
            Some((data.item_id, state.claims.binding(), response))
        })?;
        let Some((item, binding, response)) = prepared else {
            return Ok(false);
        };
        let result = (|| {
            let deadline = Instant::now() + CALL_TIMEOUT;
            let Some(Some(call)) = operations.issue(id, || {
                let state = self.packets.0.lock();
                if state.stopped || state.claims.binding() != binding {
                    return None;
                }
                Some(self.inbox.call_loader(
                    "inventory.equip",
                    vec![Value::Object(
                        [("itemId".into(), Value::Number(item as f64))].into(),
                    )],
                ))
            }) else {
                return Err(Error::Stopped);
            };
            if !matches!(
                self.wait_until(operations, id, call, deadline)?,
                Value::Bool(true)
            ) {
                return Ok(false);
            }
            if !matches!(
                response.recv_timeout(deadline.saturating_duration_since(Instant::now())),
                Ok(Ok(()))
            ) {
                self.check()?;
                return Ok(false);
            }
            let state = self.packets.0.lock();
            if state.stopped || state.claims.binding() != binding {
                return Ok(false);
            }
            let (reply, slot) = Pending::channel();
            self.inbox.post(Msg::Command(
                lucent_protocol::command::Command::RefreshEquippedConsumable {},
                reply,
            ));
            drop(state);
            let slot: Option<i64> = serde_json::from_str(
                &self.wait_until(operations, id, slot, deadline)?,
            )
            .map_err(|error| {
                crate::projection::commands::decode_error("combat.getConsumableSkillItem", error)
            })?;
            Ok(slot == Some(item))
        })();
        self.packets.unsubscribe();
        result
    }

    fn accept_subscribed(
        &self,
        drop: &ItemData,
        before: [f64; 3],
        response: mpsc::Receiver<Result<AcceptResponse, Error>>,
        operations: &Shared,
        id: u64,
    ) -> Result<bool, Error> {
        let Some(call) = operations.issue(id, || {
            self.inbox
                .call_loader("drops.accept", vec![Value::Number(drop.item_id as f64)])
        }) else {
            return Err(Error::Stopped);
        };
        let call = self.wait(operations, id, call)?;
        self.check()?;
        match call {
            Value::Bool(true) => {}
            Value::Bool(false) => return Ok(false),
            _ => {
                return Err(crate::projection::commands::decode_error(
                    "drops.accept",
                    "expected a boolean reply",
                ));
            }
        }
        let response = match response.recv_timeout(PACKET_TIMEOUT) {
            Ok(Ok(response)) => response,
            Ok(Err(Error::Stopped)) => return Err(Error::Stopped),
            _ => {
                self.check()?;
                return Ok(false);
            }
        };
        if !response.success {
            return Ok(false);
        }
        let (container, quantity) = if response.bank == Some(true) && drop.category != "Class" {
            (ItemContainer::Bank, before[0])
        } else if response.house == Some(true) || drop.house_item {
            (ItemContainer::House, before[1])
        } else {
            (ItemContainer::Inventory, before[2])
        };
        let expected = response
            .quantity_now
            .unwrap_or(quantity + response.quantity.unwrap_or(drop.quantity));
        self.read(operations, id, |projection| {
            projection
                .items
                .get(container, drop.item_id)
                .is_some_and(|item| item.data().quantity >= expected)
        })
    }
}

#[derive(Clone, Copy)]
enum CallKind {
    Action,
    Confirmation,
}

struct Transaction<'a> {
    player: &'a Endpoint,
    operations: &'a Shared,
    id: u64,
    binding: super::claims::Binding,
    issued: bool,
    last_dispatch: Instant,
}

impl<'a> Transaction<'a> {
    fn new(
        player: &'a Endpoint,
        operations: &'a Shared,
        id: u64,
        binding: super::claims::Binding,
    ) -> Self {
        Self {
            player,
            operations,
            id,
            binding,
            issued: false,
            last_dispatch: Instant::now(),
        }
    }

    fn check(&self) -> Result<(), Error> {
        self.player.check()?;
        self.operations.check(self.id)?;
        let state = self.player.packets.0.lock();
        if state.stopped || state.claims.binding() != self.binding {
            return Err(Error::Stopped);
        }
        Ok(())
    }

    fn read<R>(&self, read: impl FnOnce(&Projection) -> R) -> Result<R, Error> {
        self.check()?;
        let answer = self.player.read(self.operations, self.id, read)?;
        self.check()?;
        Ok(answer)
    }

    fn pause_until(&self, deadline: Instant) -> Result<(), Error> {
        loop {
            self.check()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            let mut queue = self.operations.queue.lock();
            self.operations
                .ready
                .wait_for(&mut queue, remaining.min(STOP_CHECK_INTERVAL));
        }
    }

    fn call(&mut self, name: &str, args: Vec<Value>) -> Result<Value, Error> {
        self.call_until(name, args, Instant::now() + CALL_TIMEOUT, CallKind::Action)
    }

    fn call_until(
        &mut self,
        name: &str,
        args: Vec<Value>,
        deadline: Instant,
        kind: CallKind,
    ) -> Result<Value, Error> {
        self.check()?;
        let dispatch = || {
            let state = self.player.packets.0.lock();
            if state.stopped || state.claims.binding() != self.binding {
                return None;
            }
            Some(self.player.inbox.call_loader(name, args))
        };
        let pending = if self.issued {
            match kind {
                CallKind::Action => self.operations.attached(self.id, dispatch).flatten(),
                CallKind::Confirmation => dispatch(),
            }
        } else {
            let pending = self.operations.issue(self.id, dispatch).flatten();
            self.issued = pending.is_some();
            pending
        }
        .ok_or(Error::Stopped)?;
        self.last_dispatch = Instant::now();
        let answer = self
            .player
            .wait_until(self.operations, self.id, pending, deadline)?;
        self.check()?;
        Ok(answer)
    }

    fn command(&self, command: lucent_protocol::command::Command) -> Result<String, Error> {
        self.check()?;
        let (reply, pending) = Pending::channel();
        {
            let state = self.player.packets.0.lock();
            if state.stopped || state.claims.binding() != self.binding {
                return Err(Error::Stopped);
            }
            self.player.inbox.post(Msg::Command(command, reply));
        }
        let answer = self.player.wait(self.operations, self.id, pending)?;
        self.check()?;
        Ok(answer)
    }

    fn poll(&mut self, name: &str, timeout: Duration) -> Result<bool, Error> {
        let deadline = Instant::now() + timeout;
        loop {
            if matches!(
                self.call_until(
                    name,
                    vec![],
                    deadline.min(Instant::now() + CALL_TIMEOUT),
                    CallKind::Confirmation
                ),
                Ok(Value::Bool(true))
            ) {
                return Ok(true);
            }
            self.check()?;
            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }
            self.pause_until((now + Duration::from_millis(100)).min(deadline))?;
        }
    }

    fn load_bank(&mut self, force: bool) -> Result<bool, Error> {
        use lucent_protocol::command::{Command, RefreshContainer};
        let logged_in = self.command(Command::RefreshLoggedIn {})?;
        if logged_in != "true" {
            return Ok(false);
        }
        let hydrated =
            self.read(|projection| projection.items.hydration(ItemContainer::Bank) > 0)?;
        if hydrated && !force {
            return Ok(true);
        }
        self.call("bank.loadItems", vec![Value::Bool(force || !hydrated)])?;
        if !self.poll("bank.isLoaded", Duration::from_secs(10))? {
            return Ok(false);
        }
        self.command(Command::RefreshContainer {
            container: RefreshContainer::Bank,
        })?;
        Ok(true)
    }

    fn bank_is_open(&self) -> Result<bool, Error> {
        self.check()?;
        let pending = self
            .operations
            .attached(self.id, || {
                let state = self.player.packets.0.lock();
                if state.stopped || state.claims.binding() != self.binding {
                    return None;
                }
                Some(self.player.inbox.call_loader("bank.isOpen", vec![]))
            })
            .flatten()
            .ok_or(Error::Stopped)?;
        let answer = self.player.wait(self.operations, self.id, pending);
        self.check()?;
        Ok(matches!(answer, Ok(Value::Bool(true))))
    }

    fn open_bank(&mut self, force: bool) -> Result<bool, Error> {
        if !self.load_bank(force)? {
            return Ok(false);
        }
        if self.bank_is_open()? {
            return Ok(true);
        }
        self.call("bank.open", vec![])?;
        self.poll("bank.isOpen", Duration::from_secs(3))
    }

    fn load_quests(
        &mut self,
        ids: &[f64],
        silent: bool,
        next_load: &mut Instant,
    ) -> Result<Vec<bool>, Error> {
        let mut unique = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for &id in ids {
            if id > 0.0
                && id <= 9_007_199_254_740_991.0
                && id.fract() == 0.0
                && seen.insert(id as i64)
            {
                unique.push(id);
            }
        }
        for batch in unique.chunks(30) {
            for _ in 0..2 {
                self.operations
                    .attached(self.id, || ())
                    .ok_or(Error::Stopped)?;
                let missing = self.read(|projection| {
                    batch
                        .iter()
                        .copied()
                        .filter(|id| projection.quests.get(*id).is_none())
                        .collect::<Vec<_>>()
                })?;
                if missing.is_empty() {
                    break;
                }
                self.pause_until(*next_load)?;
                let name = if silent {
                    "quests.getMultiple"
                } else {
                    "quests.loadMultiple"
                };
                let args = vec![Value::String(
                    missing
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                )];
                let requested = self.call(name, args);
                *next_load = self.last_dispatch + Duration::from_millis(1500);
                self.check()?;
                if requested.is_err() {
                    continue;
                }
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if self.read(|projection| {
                        missing
                            .iter()
                            .all(|id| projection.quests.get(*id).is_some())
                    })? {
                        break;
                    }
                    let now = Instant::now();
                    if now >= deadline {
                        break;
                    }
                    self.pause_until((now + Duration::from_millis(100)).min(deadline))?;
                }
            }
        }
        self.read(|projection| {
            unique
                .iter()
                .map(|id| projection.quests.get(*id).is_some())
                .collect()
        })
    }
}

pub(crate) struct Request {
    pub(crate) binding: super::claims::Binding,
    pub(crate) id: u64,
    pub(crate) owner: super::claims::Owner,
    pub(crate) recipient: super::AttachmentId,
    pub(crate) detached: Arc<AtomicBool>,
    pub(crate) operation: Operation,
    pub(crate) player: Endpoint,
}

enum Lifecycle {
    Queued {
        owner: super::AttachmentId,
        request: Option<Arc<Request>>,
    },
    IssuedAttached(super::AttachmentId),
    IssuedDetached,
    Settled,
}

impl Lifecycle {
    fn issue(&mut self) -> bool {
        match self {
            Self::Queued { owner, .. } => {
                *self = Self::IssuedAttached(*owner);
                true
            }
            Self::IssuedAttached(_) | Self::IssuedDetached | Self::Settled => false,
        }
    }

    fn cancel(&mut self, caller: super::AttachmentId) {
        match *self {
            Self::Queued { owner, .. } if owner == caller => *self = Self::Settled,
            Self::IssuedAttached(owner) if owner == caller => *self = Self::IssuedDetached,
            Self::Queued { .. }
            | Self::IssuedAttached(_)
            | Self::IssuedDetached
            | Self::Settled => {}
        }
    }

    fn settle(&mut self) -> Option<super::AttachmentId> {
        match std::mem::replace(self, Self::Settled) {
            Self::Queued { owner, .. } | Self::IssuedAttached(owner) => Some(owner),
            Self::IssuedDetached | Self::Settled => None,
        }
    }
}

struct ClaimPreparation {
    request: Arc<Request>,
    binding: super::claims::Binding,
    item: i64,
    waiting: bool,
}

#[derive(Clone, Copy)]
enum Lane {
    Game,
    Load,
}

impl Lane {
    fn for_operation(operation: &Operation) -> Self {
        match operation {
            Operation::LoadQuests { .. }
            | Operation::LoadBank { .. }
            | Operation::OpenBank { .. } => Self::Load,
            Operation::AcceptDrop { .. }
            | Operation::EquipConsumable { .. }
            | Operation::PrepareConsumableClaim { .. } => Self::Game,
        }
    }
}

#[derive(Default)]
struct Queue {
    entries: HashMap<u64, Lifecycle>,
    lanes: [VecDeque<u64>; 2],
    preparations: Vec<ClaimPreparation>,
    stopped: bool,
}

impl Queue {
    fn update_preparations(&mut self) {
        self.preparations
            .retain(|entry| self.entries.contains_key(&entry.request.id));
        for index in 0..self.preparations.len() {
            let entry = &self.preparations[index];
            let waiting = self.preparations[..index]
                .iter()
                .any(|earlier| earlier.binding == entry.binding && earlier.item == entry.item);
            if entry.waiting && !waiting {
                entry.request.player.inbox.post(Msg::OperationAnswer(
                    entry.request.recipient,
                    ScriptEvent::OperationResponsibility {
                        id: entry.request.id,
                        responsible: true,
                    },
                ));
                self.preparations[index].waiting = false;
            }
        }
    }
}

#[cfg(feature = "testing")]
type IssuanceProbe = Arc<dyn Fn(u64, bool) + Send + Sync>;

#[cfg(feature = "testing")]
pub(crate) type LockProbe = Arc<dyn Fn() + Send + Sync>;

#[cfg(feature = "testing")]
fn probe_lock(probe: &Mutex<Option<LockProbe>>) {
    let probe = probe.lock().clone();
    if let Some(probe) = probe {
        probe();
    }
}

#[derive(Default)]
pub(crate) struct Shared {
    queue: Mutex<Queue>,
    ready: Condvar,
    #[cfg(feature = "testing")]
    probe: Mutex<Option<IssuanceProbe>>,
    #[cfg(feature = "testing")]
    pub(crate) cancel_probe: Mutex<Option<LockProbe>>,
    #[cfg(feature = "testing")]
    pub(crate) dispatch_probe: Mutex<Option<LockProbe>>,
}

impl Shared {
    pub(crate) fn prepare(&self, owner: super::AttachmentId) -> Result<u64, Error> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let mut queue = self.queue.lock();
        if queue.stopped {
            return Err(Error::Stopped);
        }
        if queue.entries.len() > crate::bridge::EVENT_CAPACITY {
            return Err(Error::Overflow);
        }
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        queue.entries.insert(
            id,
            Lifecycle::Queued {
                owner,
                request: None,
            },
        );
        Ok(id)
    }

    pub(crate) fn submit(&self, request: Request) -> Result<(), Error> {
        let mut queue = self.queue.lock();
        if queue.stopped || request.detached.load(Ordering::Acquire) {
            return Err(Error::Stopped);
        }
        let entry = queue.entries.get_mut(&request.id).ok_or(Error::Stopped)?;
        let Lifecycle::Queued {
            owner,
            request: pending,
        } = entry
        else {
            return Err(Error::Stopped);
        };
        if *owner != request.owner.attachment || pending.is_some() {
            return Err(Error::Stopped);
        }
        let id = request.id;
        let lane = Lane::for_operation(&request.operation);
        let request = Arc::new(request);
        *pending = Some(request.clone());
        if let Operation::PrepareConsumableClaim { claim } = &request.operation {
            let preparation = claim.parse().ok().and_then(|claim| {
                request
                    .player
                    .packets
                    .0
                    .lock()
                    .claims
                    .preparation(request.owner, claim)
            });
            if let Some((item, binding, _)) = preparation {
                let waiting = queue
                    .preparations
                    .iter()
                    .any(|entry| entry.binding == binding && entry.item == item);
                if waiting {
                    request.player.inbox.post(Msg::OperationAnswer(
                        request.recipient,
                        ScriptEvent::OperationResponsibility {
                            id,
                            responsible: false,
                        },
                    ));
                }
                queue.preparations.push(ClaimPreparation {
                    request,
                    binding,
                    item,
                    waiting,
                });
            }
        }
        queue.lanes[lane as usize].push_back(id);
        self.ready.notify_all();
        Ok(())
    }

    pub(crate) fn cancel(&self, owner: super::AttachmentId, id: u64) {
        #[cfg(feature = "testing")]
        probe_lock(&self.cancel_probe);
        let mut queue = self.queue.lock();
        if let Some(entry) = queue.entries.get_mut(&id) {
            entry.cancel(owner);
            if matches!(entry, Lifecycle::Settled) {
                queue.entries.remove(&id);
                for lane in &mut queue.lanes {
                    lane.retain(|queued| *queued != id);
                }
                queue.update_preparations();
            }
        }
    }

    pub(crate) fn detach(&self, owner: super::AttachmentId) {
        let mut queue = self.queue.lock();
        queue.entries.retain(|_, entry| {
            entry.cancel(owner);
            !matches!(entry, Lifecycle::Settled)
        });
        let Queue { entries, lanes, .. } = &mut *queue;
        for lane in lanes {
            lane.retain(|id| entries.contains_key(id));
        }
        queue.update_preparations();
    }

    fn next(&self, lane: Lane) -> Option<Arc<Request>> {
        let mut queue = self.queue.lock();
        loop {
            if queue.stopped {
                return None;
            }
            if let Some(id) = queue.lanes[lane as usize].pop_front() {
                if let Some(Lifecycle::Queued {
                    request: Some(request),
                    ..
                }) = queue.entries.get(&id)
                {
                    return Some(request.clone());
                }
                continue;
            }
            self.ready.wait(&mut queue);
        }
    }

    fn check(&self, id: u64) -> Result<(), Error> {
        if self.queue.lock().entries.contains_key(&id) {
            Ok(())
        } else {
            Err(Error::Stopped)
        }
    }

    fn attached<T>(&self, id: u64, dispatch: impl FnOnce() -> T) -> Option<T> {
        let queue = self.queue.lock();
        match queue.entries.get(&id)? {
            Lifecycle::Queued { .. } | Lifecycle::IssuedAttached(_) => Some(dispatch()),
            Lifecycle::IssuedDetached | Lifecycle::Settled => None,
        }
    }

    fn issue<T>(&self, id: u64, dispatch: impl FnOnce() -> T) -> Option<T> {
        #[cfg(feature = "testing")]
        self.probe(id, false);
        let result = {
            let mut queue = self.queue.lock();
            #[cfg(feature = "testing")]
            probe_lock(&self.dispatch_probe);
            let entry = queue.entries.get_mut(&id)?;
            let request = match entry {
                Lifecycle::Queued { request, .. } => request.clone(),
                _ => None,
            };
            if !entry.issue() {
                return None;
            }
            if let Some(request) = request
                && matches!(
                    request.operation,
                    Operation::EquipConsumable { .. } | Operation::PrepareConsumableClaim { .. }
                )
            {
                request.player.inbox.post(Msg::OperationAnswer(
                    request.recipient,
                    ScriptEvent::OperationIssued { id },
                ));
            }
            dispatch()
        };
        #[cfg(feature = "testing")]
        self.probe(id, true);
        Some(result)
    }

    fn settle(&self, request: &Request, answer: Result<OperationAnswer, Error>) {
        let mut queue = self.queue.lock();
        if let Some(mut entry) = queue.entries.remove(&request.id)
            && entry.settle().is_some()
        {
            request.player.inbox.post(Msg::OperationAnswer(
                request.recipient,
                ScriptEvent::Operation {
                    id: request.id,
                    answer,
                },
            ));
        }
        queue.update_preparations();
    }

    #[cfg(feature = "testing")]
    pub(crate) fn set_probe(&self, probe: impl Fn(u64, bool) + Send + Sync + 'static) {
        *self.probe.lock() = Some(Arc::new(probe));
    }

    #[cfg(feature = "testing")]
    fn probe(&self, id: u64, issued: bool) {
        let probe = self.probe.lock().clone();
        if let Some(probe) = probe {
            probe(id, issued);
        }
    }

    #[cfg(test)]
    pub(crate) fn send(&self, request: Option<Request>) -> Result<(), Error> {
        let request = request.ok_or(Error::Stopped)?;
        self.queue.lock().entries.insert(
            request.id,
            Lifecycle::Queued {
                owner: request.owner.attachment,
                request: None,
            },
        );
        self.submit(request)
    }
}

pub(crate) struct Operations {
    pub(crate) sender: Arc<Shared>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl Operations {
    pub(crate) fn start(id: u64) -> std::io::Result<Self> {
        let sender = Arc::new(Shared::default());
        let mut operations = Self {
            sender,
            threads: Vec::new(),
        };
        for (lane, name) in [(Lane::Game, "operations"), (Lane::Load, "loads")] {
            let requests = operations.sender.clone();
            let thread = thread::Builder::new()
                .name(format!("{name}-{id}"))
                .spawn(move || {
                    let mut next_quest_load = Instant::now();
                    while let Some(request) = requests.next(lane) {
                        request.player.inbox.post(Msg::OperationAnswer(
                            request.recipient,
                            ScriptEvent::OperationStarted { id: request.id },
                        ));
                        let mut transaction = Transaction::new(
                            &request.player,
                            &requests,
                            request.id,
                            request.binding,
                        );
                        let answer = match &request.operation {
                            Operation::LoadQuests { ids, silent } => transaction
                                .load_quests(ids, *silent, &mut next_quest_load)
                                .map(OperationAnswer::LoadQuests),
                            Operation::LoadBank { force } => {
                                transaction.load_bank(*force).map(OperationAnswer::LoadBank)
                            }
                            Operation::OpenBank { force } => {
                                transaction.open_bank(*force).map(OperationAnswer::OpenBank)
                            }
                            Operation::PrepareConsumableClaim { claim } => request
                                .player
                                .prepare_consumable_claim(
                                    claim,
                                    request.owner,
                                    &requests,
                                    request.id,
                                )
                                .map(OperationAnswer::EquipConsumable),
                            Operation::EquipConsumable { selector } => request
                                .player
                                .equip_consumable(selector.clone(), &requests, request.id, None)
                                .map(OperationAnswer::EquipConsumable),
                            Operation::AcceptDrop { selector } => request
                                .player
                                .accept(selector.clone(), &requests, request.id)
                                .map(OperationAnswer::AcceptDrop),
                        };
                        requests.settle(&request, answer);
                    }
                })?;
            operations.threads.push(thread);
        }
        Ok(operations)
    }
}

impl Drop for Operations {
    fn drop(&mut self) {
        {
            let mut queue = self.sender.queue.lock();
            queue.stopped = true;
            for lane in &mut queue.lanes {
                lane.clear();
            }
            queue.entries.clear();
            queue.preparations.clear();
            self.sender.ready.notify_all();
        }
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::AttachmentId;
    use super::*;

    #[test]
    fn cancelling_queued_work_prevents_issuance_and_settlement_delivery() {
        let owner = AttachmentId(1);
        let mut state = Lifecycle::Queued {
            owner,
            request: None,
        };
        state.cancel(owner);
        assert!(matches!(state, Lifecycle::Settled));
        assert!(!state.issue());
        assert_eq!(state.settle(), None);
        state.cancel(owner);
        assert!(matches!(state, Lifecycle::Settled));
    }

    #[test]
    fn cancelling_issued_work_detaches_without_settling() {
        let owner = AttachmentId(1);
        let mut state = Lifecycle::Queued {
            owner,
            request: None,
        };
        assert!(state.issue());
        assert!(!state.issue());
        state.cancel(owner);
        assert!(matches!(state, Lifecycle::IssuedDetached));
        state.cancel(owner);
        assert!(matches!(state, Lifecycle::IssuedDetached));
        assert_eq!(state.settle(), None);
        assert!(matches!(state, Lifecycle::Settled));
    }

    #[test]
    fn attached_work_delivers_once_and_foreign_cancellation_is_ignored() {
        for issued in [false, true] {
            let owner = AttachmentId(1);
            let mut state = Lifecycle::Queued {
                owner,
                request: None,
            };
            if issued {
                assert!(state.issue());
            }
            state.cancel(AttachmentId(2));
            assert_eq!(state.settle(), Some(owner));
            assert_eq!(state.settle(), None);
            assert!(!state.issue());
            state.cancel(owner);
            assert!(matches!(state, Lifecycle::Settled));
        }
    }
}
