use super::*;

impl ProfileJob {
    fn cast_or_wait(
        &mut self,
        snapshot: &ProfileSnapshot,
        lease: TargetLease,
        cast: PreparedCast,
        expires_at: u64,
        now: u64,
        confirmed: bool,
    ) -> ProfileEffects {
        if cast.readiness == CastReadiness::Wait
            && now < expires_at
            && snapshot.skills[usize::from(u8::from(cast.skill))] != SkillReadiness::Ready
        {
            let deadline = match snapshot.skills[usize::from(u8::from(cast.skill))] {
                SkillReadiness::CooldownUntil(until) if until > now => {
                    until.saturating_add(50).min(expires_at)
                }
                _ => now.saturating_add(50).min(expires_at),
            };
            self.phase = Phase::Cooldown {
                lease,
                cast,
                expires_at,
            };
            return ProfileEffects {
                next_deadline: Some(deadline),
                ..Default::default()
            };
        }
        let allowed = self.lease_current(snapshot, lease)
            && snapshot.alive
            && snapshot.monsters.iter().any(|monster| {
                monster.key == lease.key
                    && monster.attack == AttackEligibility::Allowed
                    && monster.living()
            })
            && skill_available(snapshot, cast.skill, self.skill5_item_id);
        if !allowed || now >= expires_at {
            self.rotation.complete(&cast, false);
            return self.cycle(Cycle::Attacked { cast: false }, now, 0);
        }
        if cast.readiness == CastReadiness::Wait && !confirmed {
            let until = now.saturating_add(150).min(expires_at);
            self.phase = Phase::Reconfirm {
                pending: Pending::Cast { lease, cast },
                expires_at,
                until,
            };
            return ProfileEffects {
                next_deadline: Some(until),
                ..Default::default()
            };
        }
        self.issue(
            Action::UseSkill {
                skill: cast.skill,
                target: Some(lease),
                readiness: cast.readiness,
                force: CastForce::Normal,
            },
            Pending::Cast { lease, cast },
        )
    }

    fn trigger_or_wait(
        &mut self,
        snapshot: &ProfileSnapshot,
        pending: Pending,
        expires_at: u64,
        now: u64,
        confirmed: bool,
    ) -> ProfileEffects {
        let Pending::Trigger {
            target,
            index,
            started,
            message,
        } = pending
        else {
            unreachable!()
        };
        let skill = self
            .profile
            .message_triggers
            .as_ref()
            .expect("trigger state belongs to profile")[index]
            .skill;
        if now < expires_at
            && snapshot.skills[usize::from(u8::from(skill))] != SkillReadiness::Ready
        {
            let deadline = match snapshot.skills[usize::from(u8::from(skill))] {
                SkillReadiness::CooldownUntil(until) if until > now => {
                    until.saturating_add(50).min(expires_at)
                }
                _ => now.saturating_add(50).min(expires_at),
            };
            self.phase = Phase::TriggerCooldown {
                target,
                index,
                started,
                message,
                expires_at,
            };
            return ProfileEffects {
                next_deadline: Some(deadline),
                ..Default::default()
            };
        }
        let allowed = snapshot.alive
            && skill_available(snapshot, skill, self.skill5_item_id)
            && target.is_none_or(|lease| {
                self.lease_current(snapshot, lease)
                    && snapshot.monsters.iter().any(|monster| {
                        monster.key == lease.key
                            && monster.living()
                            && monster.attack == AttackEligibility::Allowed
                    })
            });
        if !allowed || now >= expires_at {
            self.phase = Phase::Triggering {
                target,
                index: index + 1,
                message,
            };
            return ProfileEffects {
                next_deadline: Some(now),
                ..Default::default()
            };
        }
        if !confirmed {
            let until = now.saturating_add(150).min(expires_at);
            self.phase = Phase::Reconfirm {
                pending: Pending::Trigger {
                    target,
                    index,
                    started,
                    message,
                },
                expires_at,
                until,
            };
            return ProfileEffects {
                next_deadline: Some(until),
                ..Default::default()
            };
        }
        self.issue(
            Action::UseSkill {
                skill,
                target,
                readiness: CastReadiness::Wait,
                force: CastForce::Forced,
            },
            Pending::Trigger {
                target,
                index,
                started,
                message,
            },
        )
    }

    pub(super) fn advance_phase(&mut self, snapshot: &ProfileSnapshot, now: u64) -> ProfileEffects {
        match self.phase.clone() {
            Phase::New => {
                if self.profile.steps.is_empty() && self.last_triggers.is_empty() {
                    return self.cycle(Cycle::Idle, now, 0);
                }
                if now < self.next_step {
                    return ProfileEffects {
                        next_deadline: Some(self.next_step),
                        ..Default::default()
                    };
                }
                self.issue(Action::Prepare, Pending::Prepare)
            }
            Phase::Waiting { until } => {
                if now < until {
                    return ProfileEffects {
                        next_deadline: Some(until),
                        ..Default::default()
                    };
                }
                self.cycle(Cycle::Attacked { cast: true }, now, 0)
            }
            Phase::Ready => {
                if !self.engaged {
                    return ProfileEffects::default();
                }
                if now < self.next_step {
                    return ProfileEffects {
                        next_deadline: Some(self.next_step),
                        ..Default::default()
                    };
                }
                if self.profile.steps.is_empty() {
                    return self.cycle(Cycle::Idle, now, 0);
                }
                if !snapshot.alive {
                    return self.cycle(Cycle::PlayerDead, now, 0);
                }
                let current = match self.target {
                    TargetState::Selected {
                        lease,
                        confirmation: Confirmation::Confirmed { missing_since },
                    } => {
                        if !snapshot
                            .monsters
                            .iter()
                            .any(|monster| monster.key == lease.key)
                        {
                            let since = missing_since.unwrap_or(now);
                            self.target = TargetState::Selected {
                                lease,
                                confirmation: Confirmation::Confirmed {
                                    missing_since: Some(since),
                                },
                            };
                            if now.saturating_sub(since) < TARGET_ABSENCE_GRACE_MS {
                                return self.cycle(Cycle::NoTarget, now, 0);
                            }
                        }
                        Some(lease.key)
                    }
                    _ => None,
                };
                let selected =
                    select_target(snapshot, current, self.fallback_rank).map(|monster| monster.key);
                let Some(key) = selected else {
                    if let TargetState::Selected { lease, .. } = self.target
                        && !snapshot.monsters.iter().any(|monster| {
                            monster.key == lease.key
                                && monster.living()
                                && monster.rank.or(self.fallback_rank).is_some()
                        })
                    {
                        self.lose_target(false);
                    }
                    return self.cycle(Cycle::NoTarget, now, 0);
                };
                let lease = match self.target {
                    TargetState::Selected {
                        lease,
                        confirmation,
                    } if lease.key == key => {
                        self.target = TargetState::Selected {
                            lease,
                            confirmation: match confirmation {
                                Confirmation::Confirmed { .. } => Confirmation::Confirmed {
                                    missing_since: None,
                                },
                                Confirmation::Unconfirmed => Confirmation::Unconfirmed,
                            },
                        };
                        lease
                    }
                    _ => {
                        self.lose_target(false);
                        let lease = TargetLease {
                            key,
                            version: self.version,
                        };
                        self.target = TargetState::Selected {
                            lease,
                            confirmation: Confirmation::Unconfirmed,
                        };
                        lease
                    }
                };
                self.issue(Action::Attack(lease), Pending::Attack(lease))
            }
            Phase::Casting(lease) => {
                if !self.lease_current(snapshot, lease) {
                    self.lose_target(false);
                    return self.cycle(Cycle::NoTarget, now, 0);
                }
                let Some(cast) = self.rotation.prepare(
                    &self.profile,
                    snapshot,
                    Some(lease.key),
                    self.skill5_item_id,
                ) else {
                    return self.cycle(Cycle::Attacked { cast: false }, now, 0);
                };
                self.cast_or_wait(snapshot, lease, cast, now.saturating_add(5_000), now, false)
            }
            Phase::Cooldown {
                lease,
                cast,
                expires_at,
            } => self.cast_or_wait(snapshot, lease, cast, expires_at, now, false),
            Phase::TriggerCooldown {
                target,
                index,
                started,
                message,
                expires_at,
            } => self.trigger_or_wait(
                snapshot,
                Pending::Trigger {
                    target,
                    index,
                    started,
                    message,
                },
                expires_at,
                now,
                false,
            ),
            Phase::Triggering {
                target,
                index,
                message,
            } => {
                for index in index..self.last_triggers.len() {
                    let trigger = &self
                        .profile
                        .message_triggers
                        .as_ref()
                        .expect("trigger state belongs to profile")[index];
                    if !matches_message(trigger, &message)
                        || self.last_triggers[index].is_some_and(|last| {
                            now.saturating_sub(last) < u64::from(trigger.cooldown_ms.unwrap_or(0))
                        })
                        || !skill_available(snapshot, trigger.skill, self.skill5_item_id)
                    {
                        continue;
                    }
                    return self.trigger_or_wait(
                        snapshot,
                        Pending::Trigger {
                            target,
                            index,
                            started: now,
                            message,
                        },
                        now.saturating_add(5_000),
                        now,
                        false,
                    );
                }
                self.phase = Phase::Ready;
                ProfileEffects {
                    next_deadline: Some(self.next_step.max(now)),
                    ..Default::default()
                }
            }
            Phase::Reconfirm {
                pending,
                expires_at,
                until,
            } => {
                if now < until {
                    return ProfileEffects {
                        next_deadline: Some(until),
                        ..Default::default()
                    };
                }
                match pending {
                    Pending::Cast { lease, cast } => {
                        let ready = snapshot.skills[usize::from(u8::from(cast.skill))]
                            == SkillReadiness::Ready;
                        self.cast_or_wait(snapshot, lease, cast, expires_at, now, ready)
                    }
                    pending @ Pending::Trigger { index, .. } => {
                        let skill = self.profile.message_triggers.as_ref().unwrap()[index].skill;
                        let ready =
                            snapshot.skills[usize::from(u8::from(skill))] == SkillReadiness::Ready;
                        self.trigger_or_wait(snapshot, pending, expires_at, now, ready)
                    }
                    _ => unreachable!(),
                }
            }
            Phase::Pending { .. } => ProfileEffects::default(),
            Phase::Cancelled => ProfileEffects::default(),
        }
    }
}
