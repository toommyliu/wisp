use super::*;
use crate::bridge::Value;
use lucent_protocol::{
    combat_profile::SkillSlot,
    operation::{Operation, OperationAnswer},
};

pub(crate) struct ConsumableInfo {
    pub(crate) current: Option<i64>,
    pub(crate) item: Option<lucent_protocol::item::ItemData>,
    pub(crate) member: bool,
    pub(crate) class_warning: Option<String>,
}

pub(super) struct Preparing {
    action: ActionId,
    claim: Option<u64>,
    state: State,
    item: Option<i64>,
    name: String,
    warnings: Vec<String>,
}
enum State {
    New,
    IdleUntil(Instant),
    Preflight(Pending<Option<ConsumableInfo>>, Instant),
    Available {
        pending: Option<Pending<Value>>,
        next: Instant,
        expires: Instant,
    },
    Equip {
        id: u64,
        owner: AttachmentId,
        operations: Arc<Shared>,
        deadline: Option<Instant>,
        issued: bool,
        answer: Option<Result<OperationAnswer, Error>>,
    },
    Done,
}
fn uses_consumable(profile: &lucent_protocol::combat_profile::CombatProfile) -> bool {
    profile.consumable.is_some()
        || profile
            .steps
            .iter()
            .any(|step| step.skill == SkillSlot::Consumable)
        || profile
            .message_triggers
            .iter()
            .flatten()
            .any(|trigger| trigger.skill == SkillSlot::Consumable)
}
impl Preparing {
    pub(super) fn new(action: ActionId) -> Self {
        Self {
            action,
            claim: None,
            state: State::New,
            item: None,
            name: String::new(),
            warnings: Vec::new(),
        }
    }
    pub(super) fn action(&self) -> ActionId {
        self.action
    }
    pub(super) fn cancel(&mut self) {
        if let State::Equip {
            id,
            owner,
            operations,
            ..
        } = &self.state
        {
            operations.cancel(*owner, *id);
        }
        self.state = State::Done;
    }
    pub(super) fn deadline(&self) -> Option<Instant> {
        match &self.state {
            State::New | State::Done => Some(Instant::now()),
            State::Preflight(_, deadline) | State::IdleUntil(deadline) => Some(*deadline),
            State::Available {
                pending: None,
                next,
                ..
            } => Some(*next),
            State::Available { expires, .. } => Some(*expires),
            State::Equip {
                answer: Some(_), ..
            } => Some(Instant::now()),
            State::Equip { deadline, .. } => *deadline,
        }
    }
    pub(super) fn event(&mut self, event: &ScriptEvent, now: Instant) {
        if let State::Equip {
            id,
            deadline,
            issued,
            answer,
            ..
        } = &mut self.state
        {
            match event {
                ScriptEvent::OperationResponsibility {
                    id: operation,
                    responsible,
                } if operation == id && !*issued => {
                    *deadline = responsible.then_some(now + Duration::from_secs(5))
                }
                ScriptEvent::OperationIssued { id: operation } if operation == id => {
                    *issued = true;
                    *deadline = Some(now + Duration::from_secs(35));
                }
                ScriptEvent::Operation {
                    id: operation,
                    answer: result,
                } if operation == id => *answer = Some(result.clone()),
                _ => {}
            }
        }
    }
    fn finish(&mut self) -> Option<Result<Reply, Error>> {
        self.state = State::Done;
        Some(Ok(Reply::Prepared {
            skill5_item_id: self.item,
            warnings: std::mem::take(&mut self.warnings),
        }))
    }
    fn degrade(&mut self, request: &JobRequest, message: String) -> Option<Result<Reply, Error>> {
        self.warnings.push(format!(
            "{message} Skill 5 will use whichever consumable is equipped."
        ));
        if let Some(claim) = self.claim.take() {
            request.player.packets.release_claim(request.owner, claim);
        }
        self.item = None;
        self.finish()
    }
    pub(super) fn advance(
        &mut self,
        now: Instant,
        request: &JobRequest,
        link: &ScriptLink,
        waker: &Waker,
    ) -> Option<Result<Reply, Error>> {
        match &mut self.state {
            State::New => {
                let profile = request.executor.profile();
                let uses_consumable = uses_consumable(profile);
                let class_names = if matches!(request.kind, JobKind::AutoAttack(_)) {
                    profile.class_names.clone().unwrap_or_default()
                } else {
                    Vec::new()
                };
                if !uses_consumable && class_names.is_empty() {
                    return self.finish();
                }
                let (reply, pending) = Pending::channel();
                pending.set_waker(waker.clone());
                let gate = request.gate.state.lock();
                if gate.closed {
                    return Some(Err(Error::Stopped));
                }
                request.player.inbox.post(Msg::PrepareProfile {
                    consumable: profile.consumable.clone(),
                    class_names,
                    ambient: uses_consumable && profile.consumable.is_none(),
                    reply,
                });
                self.state = State::Preflight(pending, now + Duration::from_secs(5));
            }
            State::Preflight(pending, deadline) => {
                let info = match pending.try_recv() {
                    Some(Ok(Some(info))) => info,
                    Some(Ok(None)) => {
                        self.state = State::IdleUntil(now + Duration::from_millis(250));
                        return None;
                    }
                    Some(Err(error)) => return Some(Err(error)),
                    None if now < *deadline => return None,
                    None => return Some(Err(Error::Timeout)),
                };
                if let Some(warning) = info.class_warning {
                    self.warnings.push(warning);
                }
                if !uses_consumable(request.executor.profile()) {
                    return self.finish();
                }
                let Some(name) = &request.executor.profile().consumable else {
                    let claim = request.player.packets.acquire_for_job(
                        request.owner,
                        info.current,
                        &request.gate,
                        &request.detached,
                    );
                    if let Err(error) = claim {
                        return Some(Err(error));
                    }
                    self.item = info.current;
                    return self.finish();
                };
                self.name.clone_from(name);
                let candidate = info.item.as_ref().map(|item| ConsumableItem {
                    item_id: item.item_id,
                    name: item.name.clone(),
                    category: item.category.clone(),
                    link: item.link.clone(),
                });
                if let Some(warning) = consumable_warning(name, candidate.as_ref()) {
                    return self.degrade(request, warning);
                }
                let item = info.item.unwrap();
                self.name = item.name.clone();
                if item.member_only && !info.member {
                    return self.degrade(
                        request,
                        format!("Could not equip {}. Check its requirements.", self.name),
                    );
                }
                match request.player.packets.acquire_for_job(request.owner, Some(item.item_id), &request.gate, &request.detached) {
                    Ok(Some(claim)) => {
                        self.item = Some(item.item_id);
                        self.state = State::Available { pending: None, next: now, expires: now + Duration::from_secs(2) };
                        self.claim = Some(claim.id);
                    }
                    Ok(None) => return self.degrade(request, format!("Could not equip {} because another combat profile controls preflight equipment.", self.name)),
                    Err(error) => return Some(Err(error)),
                }
            }
            State::Available {
                pending,
                next,
                expires,
            } => {
                if let Some(call) = pending {
                    match call.try_recv() {
                        Some(Ok(Value::Bool(true))) => {
                            let mut gate = request.gate.state.lock();
                            if gate.closed {
                                return Some(Err(Error::Stopped));
                            }
                            let operation = link.submit_as(
                                request.owner,
                                request.detached.clone(),
                                Operation::PrepareConsumableClaim {
                                    claim: self.claim.unwrap().to_string(),
                                },
                            );
                            match operation {
                                Ok(id) => {
                                    gate.operation = Some(id);
                                    self.state = State::Equip {
                                        id,
                                        owner: request.owner.attachment,
                                        operations: link.operations.clone(),
                                        deadline: Some(now + Duration::from_secs(5)),
                                        issued: false,
                                        answer: None,
                                    };
                                }
                                Err(error) => return Some(Err(error)),
                            }
                            return None;
                        }
                        Some(_) => {
                            *pending = None;
                            *next = now + Duration::from_millis(100);
                        }
                        None if now < *expires => return None,
                        None => {}
                    }
                }
                if now >= *expires {
                    return self.degrade(
                        request,
                        format!("Could not equip {}. Check its requirements.", self.name),
                    );
                }
                if pending.is_none() && now >= *next {
                    let gate = request.gate.state.lock();
                    if gate.closed {
                        return Some(Err(Error::Stopped));
                    }
                    let call = request.player.inbox.call_loader(
                        "world.isActionAvailable",
                        vec![Value::String("equipItem".into())],
                    );
                    call.set_waker(waker.clone());
                    *pending = Some(call);
                }
            }
            State::Equip {
                id,
                operations,
                owner,
                answer,
                deadline,
                ..
            } => {
                if let Some(answer) = answer.take() {
                    match answer {
                        Ok(OperationAnswer::EquipConsumable(true)) => return self.finish(),
                        Err(Error::EquipInCombat) => {
                            return self.degrade(
                                request,
                                format!("Could not equip {} while you were in combat.", self.name),
                            );
                        }
                        _ => {
                            return self.degrade(
                                request,
                                format!("Could not equip {}. Check its requirements.", self.name),
                            );
                        }
                    }
                }
                if deadline.is_some_and(|deadline| now >= deadline) {
                    operations.cancel(*owner, *id);
                    return self.degrade(
                        request,
                        format!("Could not equip {}. Check its requirements.", self.name),
                    );
                }
            }
            State::IdleUntil(deadline) if now >= *deadline => self.state = State::New,
            State::IdleUntil(_) | State::Done => {}
        }
        None
    }
}

#[cfg(all(test, feature = "testing"))]
mod tests {
    use super::super::tests::{emit, manual_jobs, start_profile};
    use super::*;
    use serde_json::json;

    #[test]
    fn failed_equipment_releases_its_claim_before_reporting_a_warning() {
        let (_session, observer, jobs, owner) = manual_jobs();
        start_profile(&owner, json!({"consumable":"Potion","steps":[{"skill":5}]}));
        let mut request = jobs.requests.handoff.lock().queue.pop_front().unwrap();
        let claim = request
            .player
            .packets
            .acquire_for_job(request.owner, Some(100), &request.gate, &request.detached)
            .unwrap()
            .unwrap();
        let now = Instant::now();
        let action = request
            .executor
            .advance(&ProfileSnapshot::default(), ProfileEvent::Tick, 0)
            .command
            .unwrap()
            .id;
        let mut preparing = Preparing::new(action);
        preparing.name = "Potion".into();
        preparing.claim = Some(claim.id);
        preparing.item = Some(100);
        preparing.state = State::Available {
            pending: None,
            next: now,
            expires: now,
        };
        assert!(matches!(
            preparing.advance(now, &request, &jobs.link, &jobs.waker),
            Some(Ok(Reply::Prepared {
                skill5_item_id: None,
                ..
            }))
        ));
        let sibling = observer
            .acquire_consumable_claim(Some(101))
            .unwrap()
            .expect("failed equipment retained its claim");
        observer.release_claim(sibling.id);
    }

    #[test]
    fn preparation_preserves_each_consumable_warning() {
        let (_session, observer, jobs, owner) = manual_jobs();
        emit(
            &observer,
            json!({"cmd":"loadInventoryBig","items":[{"ItemID":100,"sName":"Potion","sType":"Item","sLink":"potion","sMeta":"400","iQty":2}]}),
        );
        let item = observer
            .read(|state| {
                state
                    .items
                    .get(lucent_protocol::item::ItemContainer::Inventory, 100)
                    .unwrap()
                    .data()
                    .clone()
            })
            .unwrap();
        let id = start_profile(&owner, json!({"consumable":"Potion","steps":[{"skill":5}]}));
        let mut request = jobs.requests.handoff.lock().queue.pop_front().unwrap();
        let action = request
            .executor
            .advance(&ProfileSnapshot::default(), ProfileEvent::Tick, 0)
            .command
            .unwrap()
            .id;
        let now = Instant::now();
        let expected_failure = "Could not equip Potion. Check its requirements. Skill 5 will use whichever consumable is equipped.";
        for (case, expected) in [
            (
                "missing",
                "Potion was not found. Skill 5 will use whichever consumable is equipped.",
            ),
            (
                "invalid",
                "Potion cannot be equipped. Skill 5 will use whichever consumable is equipped.",
            ),
            ("member", expected_failure),
            ("available", expected_failure),
            ("rejected", expected_failure),
            ("error", expected_failure),
            ("timeout", expected_failure),
            (
                "combat",
                "Could not equip Potion while you were in combat. Skill 5 will use whichever consumable is equipped.",
            ),
            (
                "conflict",
                "Could not equip Potion because another combat profile controls preflight equipment. Skill 5 will use whichever consumable is equipped.",
            ),
        ] {
            let mut preparing = Preparing::new(action);
            preparing.name = "Potion".into();
            preparing.item = Some(100);
            preparing.state = match case {
                "missing" | "invalid" | "member" | "conflict" => {
                    let mut item = item.clone();
                    item.member_only = case == "member";
                    if case == "invalid" {
                        item.link = "none".into();
                    }
                    let (reply, pending) = Pending::channel();
                    reply
                        .send(Ok(Some(ConsumableInfo {
                            current: None,
                            item: (case != "missing").then_some(item),
                            member: false,
                            class_warning: None,
                        })))
                        .unwrap();
                    State::Preflight(pending, now + Duration::from_secs(5))
                }
                "available" => State::Available {
                    pending: None,
                    next: now,
                    expires: now,
                },
                _ => State::Equip {
                    id: u64::MAX,
                    owner: request.owner.attachment,
                    operations: jobs.link.operations.clone(),
                    deadline: Some(now),
                    issued: false,
                    answer: match case {
                        "rejected" => Some(Ok(OperationAnswer::EquipConsumable(false))),
                        "error" => Some(Err(Error::Timeout)),
                        "combat" => Some(Err(Error::EquipInCombat)),
                        "timeout" => None,
                        _ => unreachable!(),
                    },
                },
            };
            if case == "conflict" {
                observer
                    .acquire_consumable_claim(Some(101))
                    .unwrap()
                    .unwrap();
            }
            let reply = preparing
                .advance(now, &request, &jobs.link, Waker::noop())
                .unwrap()
                .unwrap();
            assert_eq!(
                reply,
                Reply::Prepared {
                    skill5_item_id: None,
                    warnings: vec![expected.into()]
                },
                "{case}"
            );
        }
        owner.jobs.cancel(id);
    }
}
