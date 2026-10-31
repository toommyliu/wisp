use super::*;

impl ProfileJob {
    pub(super) fn complete_action(
        &mut self,
        id: ActionId,
        reply: Reply,
        now: u64,
    ) -> Option<ProfileEffects> {
        let Phase::Pending {
            id: expected,
            action,
        } = &self.phase
        else {
            return Some(ProfileEffects::default());
        };
        if id != *expected {
            return Some(ProfileEffects::default());
        }
        let pending = action.clone();
        if let Reply::Abandoned { target_lost } = reply {
            let lease = pending.target().filter(|_| target_lost);
            if lease.is_some_and(|lease| lease.version == self.version) {
                self.lose_target(false);
            }
            self.phase = Phase::Ready;
            self.next_step = now.saturating_add(RETRY_MS);
            return Some(ProfileEffects {
                outcome: lease.map(Outcome::TargetLost),
                next_deadline: Some(self.next_step),
                ..Default::default()
            });
        }
        if let Reply::Failed(message) = reply {
            self.phase = if matches!(pending, Pending::Prepare) {
                Phase::New
            } else {
                Phase::Ready
            };
            self.next_step = now.saturating_add(RETRY_MS);
            return Some(ProfileEffects {
                outcome: Some(Outcome::Failure {
                    stage: pending.stage(),
                    message,
                }),
                next_deadline: Some(self.next_step),
                ..Default::default()
            });
        }
        match (pending, reply) {
            (
                Pending::Prepare,
                Reply::Prepared {
                    skill5_item_id,
                    warnings,
                },
            ) => {
                self.skill5_item_id = skill5_item_id;
                self.warnings = warnings;
                self.phase = Phase::Ready;
            }
            (Pending::Attack(lease), Reply::Accepted(accepted)) => {
                if !accepted {
                    return Some(self.cycle(Cycle::AttackRejected, now, 0));
                }
                if lease.version != self.version {
                    return Some(self.cycle(Cycle::NoTarget, now, 0));
                }
                self.target = TargetState::Selected {
                    lease,
                    confirmation: Confirmation::Confirmed {
                        missing_since: None,
                    },
                };
                self.phase = if self.engaged {
                    Phase::Casting(lease)
                } else {
                    Phase::Ready
                };
            }
            (Pending::Cast { cast, .. }, Reply::Accepted(succeeded)) => {
                self.rotation.complete(&cast, succeeded);
                return Some(self.cycle(
                    Cycle::Attacked { cast: succeeded },
                    now,
                    if succeeded { cast.wait_ms } else { 0 },
                ));
            }
            (
                Pending::Trigger {
                    target,
                    index,
                    started,
                    message,
                },
                Reply::Accepted(succeeded),
            ) => {
                if succeeded {
                    self.last_triggers[index] = Some(started);
                }
                self.phase = Phase::Triggering {
                    target,
                    index: index + 1,
                    message,
                };
            }
            _ => return Some(ProfileEffects::default()),
        }
        None
    }
    pub(super) fn start_message(
        &mut self,
        snapshot: &ProfileSnapshot,
        observed_version: u64,
        message: Message,
    ) -> Option<ProfileEffects> {
        if observed_version != self.version || !snapshot.alive {
            return Some(ProfileEffects::default());
        }
        if !matches!(self.phase, Phase::Ready) {
            return Some(ProfileEffects {
                outcome: Some(Outcome::Deferred),
                ..Default::default()
            });
        }
        let owned = match self.target {
            TargetState::Selected {
                lease,
                confirmation: Confirmation::Confirmed { .. },
            } => Some(lease.key),
            _ => None,
        };
        let selected = match snapshot.selected {
            SelectedTarget::Monster(key)
                if snapshot
                    .monsters
                    .iter()
                    .any(|monster| monster.key == key && monster.living()) =>
            {
                Some(key)
            }
            _ => None,
        };
        if message.monster_map_id.is_some_and(|id| {
            owned.is_some_and(|key| key.monster_map_id != id)
                || selected.is_some_and(|key| key.monster_map_id != id)
                || matches!(snapshot.selected, SelectedTarget::Player { alive: true })
        }) {
            return Some(ProfileEffects::default());
        }
        let target_id = message
            .monster_map_id
            .or(selected.or(owned).map(|key| key.monster_map_id));
        let target = if let Some(id) = target_id {
            let Some(monster) = snapshot.monsters.iter().find(|monster| {
                monster.key.monster_map_id == id
                    && monster.living()
                    && monster.attack == AttackEligibility::Allowed
                    && monster.rank.or(self.fallback_rank).is_some()
            }) else {
                return Some(ProfileEffects::default());
            };
            Some(TargetLease {
                key: monster.key,
                version: self.version,
            })
        } else {
            None
        };
        self.phase = Phase::Triggering {
            target,
            index: 0,
            message,
        };
        None
    }
}
