use super::*;

pub(super) const MAX_JOBS: usize = 32;

#[derive(Clone, Copy)]
pub(super) struct GateState {
    pub(super) binding: Binding,
    pub(super) recovery: u64,
    pub(super) closed: bool,
    cleanup_claimed: bool,
    pub(super) engaged: bool,
    pub(super) epoch: u64,
    pub(super) operation: Option<u64>,
}

pub(crate) struct JobGate {
    pub(super) state: Mutex<GateState>,
}
impl JobGate {
    pub(super) fn new(binding: Binding) -> Self {
        Self {
            state: Mutex::new(GateState {
                binding,
                recovery: 0,
                closed: false,
                cleanup_claimed: false,
                engaged: false,
                epoch: 0,
                operation: None,
            }),
        }
    }
    pub(crate) fn open(&self) -> bool {
        !self.state.lock().closed
    }
    pub(crate) fn allows(&self, cycle: Option<u64>, recovery: u64) -> bool {
        let state = self.state.lock();
        !state.closed
            && state.recovery == recovery
            && cycle.is_none_or(|epoch| state.engaged && state.epoch == epoch)
    }
    pub(super) fn close(&self) -> bool {
        let mut state = self.state.lock();
        let stop = !state.cleanup_claimed && (state.engaged || state.epoch != 0);
        state.closed = true;
        state.cleanup_claimed = true;
        stop
    }
}

pub(crate) struct ScriptJobs {
    pub(super) attachment: AttachmentId,
    pub(super) detached: Arc<AtomicBool>,
    pub(super) player: Endpoint,
    pub(super) operations: Arc<Shared>,
    pub(super) requests: Arc<JobRequests>,
    pub(super) completions: crate::bridge::ScriptCompletions,
    pub(super) live: Mutex<HashMap<JobId, Arc<JobGate>>>,
}

#[cfg(feature = "testing")]
pub(crate) type DispatchProbe = Arc<dyn Fn(u64, bool) + Send + Sync>;
#[cfg(feature = "testing")]
pub(crate) type ActionProbe = Arc<dyn Fn(bool) + Send + Sync>;
#[cfg(feature = "testing")]
pub(crate) type ObservationProbe = Arc<dyn Fn(Duration) + Send + Sync>;
#[cfg(feature = "testing")]
pub(crate) type TravelObservationProbe =
    Arc<dyn Fn(crate::game::travel::Reads, Vec<String>) + Send + Sync>;

#[derive(Default)]
pub(crate) struct JobRequests {
    #[cfg(feature = "testing")]
    pub(crate) controller_links: Mutex<std::collections::HashSet<AttachmentId>>,
    pub(super) handoff: Mutex<Handoff>,
    #[cfg(feature = "testing")]
    pub(crate) dispatch_probe: Mutex<Option<DispatchProbe>>,
    #[cfg(feature = "testing")]
    pub(crate) action_probe: Mutex<Option<ActionProbe>>,
    #[cfg(feature = "testing")]
    pub(crate) observation_probe: Mutex<Option<ObservationProbe>>,
    #[cfg(feature = "testing")]
    pub(crate) travel_observation_probe: Mutex<Option<TravelObservationProbe>>,
}
#[derive(Default)]
pub(super) struct Handoff {
    pub(super) waker: Option<Waker>,
    pub(super) queue: VecDeque<JobRequest>,
    pub(super) live: usize,
    pub(super) scripts: usize,
    pub(super) travel: VecDeque<travel::Request>,
    pub(super) travel_live: usize,
}
pub(super) enum JobKind {
    Script,
    AutoAttack(Arc<crate::sessions::auto_attack::Shared>),
}
pub(super) struct JobRequest {
    pub(super) kind: JobKind,
    pub(super) id: JobId,
    pub(super) owner: Owner,
    pub(super) detached: Arc<AtomicBool>,
    pub(super) player: Endpoint,
    pub(super) gate: Arc<JobGate>,
    pub(super) executor: ProfileJob,
    pub(super) priorities: Arc<[MonsterSelector]>,
    pub(super) binding: Binding,
}

impl ScriptJobs {
    pub(crate) fn new(
        attachment: AttachmentId,
        detached: Arc<AtomicBool>,
        player: Endpoint,
        operations: Arc<Shared>,
        requests: Arc<JobRequests>,
        completions: crate::bridge::ScriptCompletions,
    ) -> Self {
        Self {
            attachment,
            detached,
            player,
            operations,
            requests,
            completions,
            live: Mutex::default(),
        }
    }
    pub(crate) fn start(&self, json: &str) -> Result<JobId, String> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Request {
            profile: serde_json::Value,
            target_priority: Vec<MonsterSelector>,
        }
        let request: Request = serde_json::from_str(json)
            .map_err(|error| format!("invalid combat profile job: {error}"))?;
        let executor = ProfileJob::new(normalize_combat_profile(&request.profile), None)
            .map_err(|limit| format!("combat profile exceeds native {limit:?} limit"))?;
        self.submit(executor, request.target_priority, JobKind::Script)
    }
    pub(super) fn submit(
        &self,
        executor: ProfileJob,
        priorities: Vec<MonsterSelector>,
        kind: JobKind,
    ) -> Result<JobId, String> {
        let script = matches!(kind, JobKind::Script);
        let mut handoff = self.requests.handoff.lock();
        if self.detached.load(Ordering::Acquire) || self.player.stopped.load(Ordering::Acquire) {
            return Err(Error::Stopped.to_string());
        }
        let waker = handoff
            .waker
            .clone()
            .ok_or("combat profile jobs require a session controller")?;
        self.live.lock().retain(|_, gate| gate.open());
        if script && handoff.scripts >= MAX_JOBS {
            return Err("too many combat profile jobs".into());
        }
        let id = JobId::next();
        let binding = self.player.packets.binding();
        let gate = Arc::new(JobGate::new(binding));
        if matches!(kind, JobKind::AutoAttack(_)) {
            let mut state = gate.state.lock();
            state.engaged = true;
            state.epoch = 1;
        }
        self.live.lock().insert(id, gate.clone());
        handoff.queue.push_back(JobRequest {
            kind,
            id,
            owner: Owner {
                attachment: self.attachment,
                job: Some(id),
            },
            detached: self.detached.clone(),
            player: self.player.clone(),
            gate,
            executor,
            priorities: priorities.into(),
            binding,
        });
        handoff.live += 1;
        handoff.scripts += usize::from(script);
        drop(handoff);
        waker.wake();
        Ok(id)
    }
    pub(crate) fn engage(&self, id: JobId, engaged: bool) {
        if let Some(gate) = self.live.lock().get(&id) {
            let mut state = gate.state.lock();
            if !state.closed {
                if state.engaged != engaged {
                    state.epoch += 1;
                }
                state.engaged = engaged;
            }
        }
        self.wake();
    }
    pub(crate) fn cancel(&self, id: JobId) {
        if let Some(gate) = self.live.lock().remove(&id) {
            gate.close();
            if let Some(operation) = gate.state.lock().operation {
                self.operations.cancel(self.attachment, operation);
            }
            self.player.packets.release_owner(Owner {
                attachment: self.attachment,
                job: Some(id),
            });
        }
        self.wake();
    }
    pub(super) fn stop(&self, id: JobId) {
        if let Some(gate) = self.live.lock().get(&id)
            && gate.close()
        {
            self.player
                .inbox
                .post(Msg::StopProfile(gate.state.lock().binding));
        }
        self.cancel(id);
    }
    pub(crate) fn revoke(&self) {
        let binding = self.player.packets.binding();
        let mut stop = false;
        for (_, gate) in self.live.lock().drain() {
            stop |= gate.close() && gate.state.lock().binding == binding;
        }
        if stop {
            self.player.inbox.post(Msg::StopProfile(binding));
        }
        self.operations.detach(self.attachment);
        self.wake();
    }
    fn wake(&self) {
        if let Some(waker) = &self.requests.handoff.lock().waker {
            waker.wake_by_ref();
        }
    }
}
