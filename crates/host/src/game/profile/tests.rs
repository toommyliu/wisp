use super::*;
use lucent_protocol::combat_profile::{SkillSlot, normalize_combat_profile};
use serde_json::json;

pub(super) fn profile(value: serde_json::Value) -> lucent_protocol::combat_profile::CombatProfile {
    normalize_combat_profile(&value)
}

pub(super) fn snapshot() -> ProfileSnapshot {
    ProfileSnapshot {
        alive: true,
        hp: Stat {
            current: 100.0,
            maximum: 100.0,
        },
        mp: Stat {
            current: 100.0,
            maximum: 100.0,
        },
        skills: [SkillReadiness::Ready; 6],
        ..Default::default()
    }
}

#[test]
fn priority_order_preserves_rotation() {
    let p = profile(
        json!({"steps": [{"skill":1}, {"skill":2,"priority":true}, {"skill":3,"priority":true}]}),
    );
    let mut cursor = Rotation::default();
    let mut s = snapshot();
    let mut casts = Vec::new();
    for attempt in 0..3 {
        if attempt == 2 {
            s.skills[2] = SkillReadiness::Unavailable;
            s.skills[3] = SkillReadiness::Unavailable;
        }
        let cast = cursor.prepare(&p, &s, None, None).unwrap();
        casts.push(u8::from(cast.skill));
        cursor.complete(&cast, true);
    }
    assert_eq!(casts, [2, 2, 1]);
}

#[test]
fn failed_cast_advances_rotation() {
    let p = profile(json!({"steps":[{"skill":1},{"skill":2}]}));
    let mut cursor = Rotation::default();
    let first = cursor.prepare(&p, &snapshot(), None, None).unwrap();
    cursor.complete(&first, false);
    let next = cursor.prepare(&p, &snapshot(), None, None).unwrap();
    assert_eq!(
        (first.skill, next.skill),
        (SkillSlot::Skill1, SkillSlot::Skill2)
    );
}

#[test]
fn skips_ineligible_priorities() {
    let p = profile(json!({"steps":[
        {"skill":1,"priority":true,"conditions":[{"type":"self-hp","value":20}]},
        {"skill":2,"priority":true}, {"skill":3,"priority":true}, {"skill":4}
    ]}));
    let mut s = snapshot();
    s.skills[2] = SkillReadiness::Unavailable;
    assert_eq!(
        Rotation::default()
            .prepare(&p, &s, None, None)
            .unwrap()
            .skill,
        SkillSlot::Skill3
    );
}

#[test]
fn skips_unready_prepared_consumables() {
    let p = profile(json!({"steps":[{"skill":5,"priority":true},{"skill":1}]}));
    let mut cursor = Rotation::default();
    let mut s = snapshot();
    let mut casts = Vec::new();
    for slot in [
        ConsumableSlot {
            item_id: 200,
            ready: true,
        },
        ConsumableSlot {
            item_id: 100,
            ready: false,
        },
        ConsumableSlot {
            item_id: 100,
            ready: true,
        },
    ] {
        s.consumable = Some(slot);
        let cast = cursor.prepare(&p, &s, None, Some(100)).unwrap();
        casts.push(u8::from(cast.skill));
        cursor.complete(&cast, true);
    }
    assert_eq!(casts, [1, 1, 5]);
}

#[test]
fn failed_waiting_priority_preserves_cursor() {
    let p = profile(json!({"steps":[{"skill":4},
        {"skill":1,"priority":true,"cooldownMode":"wait-for-cooldown","conditions":[{"type":"self-hp","value":20}]},
        {"skill":5}, {"skill":2,"priority":true}]}));
    let mut s = snapshot();
    s.hp.current = 10.0;
    let mut cursor = Rotation::default();
    let cast = cursor.prepare(&p, &s, None, None).unwrap();
    assert_eq!(
        (cast.skill, cast.readiness),
        (SkillSlot::Skill1, CastReadiness::Wait)
    );
    cursor.complete(&cast, false);
    s.hp.current = 100.0;
    s.skills[2] = SkillReadiness::Unavailable;
    assert_eq!(
        cursor.prepare(&p, &s, None, None).unwrap().skill,
        SkillSlot::Skill4
    );
}

#[test]
fn reset_survives_cast_completion() {
    let p = profile(json!({"steps":[{"skill":1},{"skill":2}]}));
    let mut cursor = Rotation::default();
    let first = cursor.prepare(&p, &snapshot(), None, None).unwrap();
    cursor.reset();
    cursor.complete(&first, true);
    assert_eq!(
        cursor.prepare(&p, &snapshot(), None, None).unwrap().skill,
        SkillSlot::Skill1
    );
}

pub(super) fn monster(id: i64, rank: Option<usize>) -> Monster {
    Monster {
        key: TargetKey {
            monster_map_id: id,
            generation: 0,
        },
        hp: 100.0,
        state: LifeState::Alive,
        rank,
        attack: AttackEligibility::Allowed,
        auras: Vec::new(),
    }
}

#[test]
fn stat_and_aura_boundaries() {
    let p = profile(json!({"steps":[{"skill":1,"conditions":[
        {"type":"self-hp","op":"<=","value":40,"unit":"percent"},
        {"type":"self-mp","op":">=","value":20,"unit":"value"},
        {"type":"ally-hp","op":"<=","value":20,"unit":"percent"},
        {"type":"self-aura","auraName":"Focus","op":">=","value":3},
        {"type":"target-aura","auraName":"Guard","op":">=","value":1}
    ]}]}));
    let mut s = snapshot();
    s.hp.current = 40.0;
    s.mp.current = 20.0;
    s.self_auras.push(Aura {
        name: "Focus".into(),
        stack: Some(3.0),
        value: None,
    });
    s.allies.push(Ally {
        entity_id: 2,
        username: "Ally".into(),
        hp: Stat {
            current: 20.0,
            maximum: 100.0,
        },
    });
    let mut guarded = monster(7, None);
    guarded.auras.push(Aura {
        name: "Guard".into(),
        stack: Some(1.0),
        value: None,
    });
    s.selected = SelectedTarget::Monster(guarded.key);
    s.monsters = vec![guarded, monster(8, None)];
    assert!(matches_step(&s, &p.steps[0], None));
    assert!(!matches_step(&s, &p.steps[0], Some(s.monsters[1].key)));
    s.hp.current = 41.0;
    assert!(!matches_step(&s, &p.steps[0], None));
}

pub(super) fn message(id: Option<i64>) -> Message {
    Message {
        text: "Boss enrage".into(),
        source: MessageSource::Animation,
        monster_map_id: id,
    }
}

#[test]
fn trigger_matches_normalized_substring_and_source() {
    let p = profile(json!({"messageTriggers":[
        {"messageIncludes":"WARDEN  prepares","skill":5,"source":"animation"},
        {"messageIncludes":"Warden prepares","skill":5,"source":"aura"}
    ]}));
    let event = Message {
        text: "Ultra Warden prepares a strike".into(),
        source: MessageSource::Animation,
        monster_map_id: None,
    };
    let triggers = p.message_triggers.unwrap();
    assert!(matches_message(&triggers[0], &event));
    assert!(!matches_message(&triggers[1], &event));
}

pub(super) struct Harness {
    pub(super) job: ProfileJob,
    pub(super) snapshot: ProfileSnapshot,
    pub(super) now: u64,
    pub(super) attacks: Vec<i64>,
    pub(super) casts: Vec<u8>,
    pub(super) cast_actions: Vec<Action>,
    pub(super) preparations: usize,
    pub(super) accepted: bool,
    pub(super) cast_succeeds: bool,
}

impl Harness {
    pub(super) fn new(value: serde_json::Value) -> Self {
        let mut s = snapshot();
        s.monsters = vec![monster(1, None), monster(2, None)];
        Self {
            job: ProfileJob::new(profile(value), Some(0)).unwrap(),
            snapshot: s,
            now: 0,
            attacks: Vec::new(),
            casts: Vec::new(),
            cast_actions: Vec::new(),
            preparations: 0,
            accepted: true,
            cast_succeeds: true,
        }
    }
    pub(super) fn advance(&mut self, event: ProfileEvent) -> ProfileEffects {
        self.job.advance(&self.snapshot, event, self.now)
    }
    pub(super) fn complete(&mut self, command: Command) -> ProfileEffects {
        let reply = match command.action {
            Action::Prepare => {
                self.preparations += 1;
                Reply::Prepared {
                    skill5_item_id: None,
                    warnings: Vec::new(),
                }
            }
            Action::Attack(lease) => {
                self.attacks.push(lease.key.monster_map_id);
                Reply::Accepted(self.accepted)
            }
            action @ Action::UseSkill { skill, .. } => {
                self.casts.push(u8::from(skill));
                self.cast_actions.push(action);
                Reply::Accepted(self.cast_succeeds)
            }
            Action::ReleasePreparation => panic!("cleanup is not a cycle action"),
        };
        self.advance(ProfileEvent::Completed {
            id: command.id,
            reply,
        })
    }
    pub(super) fn settle(&mut self, mut effects: ProfileEffects) -> ProfileEffects {
        for _ in 0..256 {
            match effects.command.take() {
                Some(command) => effects = self.complete(command),
                None => return effects,
            }
        }
        panic!("unbounded profile transition");
    }
    pub(super) fn cycle(&mut self) -> ProfileEffects {
        let effect = self.advance(ProfileEvent::Tick);
        let result = self.settle(effect);
        if let Some(deadline) = result.next_deadline {
            self.now = deadline;
        }
        result
    }
    pub(super) fn emit(&mut self, event: Message) -> ProfileEffects {
        let event = ProfileEvent::Message {
            observed_version: self.job.target_version(),
            message: event,
        };
        let effects = self.advance(event);
        let effects = self.reconfirm(effects);
        self.settle(effects)
    }
    pub(super) fn reconfirm(&mut self, effects: ProfileEffects) -> ProfileEffects {
        if let Phase::Reconfirm { until, .. } = self.job.phase {
            assert_eq!(effects.command, None);
            assert_eq!(effects.next_deadline, Some(self.now + 150));
            self.now = until - 1;
            assert_eq!(self.advance(ProfileEvent::Tick).command, None);
            self.now = until;
            return self.advance(ProfileEvent::Tick);
        }
        effects
    }
    pub(super) fn die(&mut self, id: i64) {
        self.advance(ProfileEvent::MonsterDeath(TargetKey {
            monster_map_id: id,
            generation: 0,
        }));
    }
}

pub(super) fn rotation_profile() -> serde_json::Value {
    json!({"delayMs":120,"steps":[{"skill":1},{"skill":2}]})
}
pub(super) fn trigger_profile() -> serde_json::Value {
    json!({"steps":[],"messageTriggers":[{"messageIncludes":"enrage","skill":5}]})
}

#[test]
fn trigger_targets_and_per_trigger_cooldowns() {
    let mut h = Harness::new(json!({"steps":[],"messageTriggers":[
        {"messageIncludes":"enrage","skill":5,"cooldownMs":1000},
        {"messageIncludes":"other","skill":5,"cooldownMs":1000}
    ]}));
    h.snapshot.monsters = vec![monster(7, None)];
    h.cycle();
    for (now, text) in [
        (100, "Boss enrage"),
        (500, "Boss enrage"),
        (500, "other"),
        (1200, "Boss enrage"),
    ] {
        h.now = now;
        let mut event = message(Some(7));
        event.text = text.into();
        event.source = MessageSource::Aura;
        h.emit(event);
    }
    assert_eq!(h.casts, [5, 5, 5]);
    for action in h.cast_actions {
        assert!(matches!(
            action,
            Action::UseSkill {
                target: Some(TargetLease {
                    key: TargetKey {
                        monster_map_id: 7,
                        ..
                    },
                    ..
                }),
                readiness: CastReadiness::Wait,
                force: CastForce::Forced,
                ..
            }
        ));
    }
}

#[test]
fn serializes_trigger_cooldown_checks() {
    let mut h = Harness::new(
        json!({"steps":[],"messageTriggers":[{"messageIncludes":"enrage","skill":5,"cooldownMs":1000}]}),
    );
    h.cycle();
    h.now = 100;
    let event = ProfileEvent::Message {
        observed_version: h.job.target_version(),
        message: message(None),
    };
    let effects = h.advance(event.clone());
    let first = h.reconfirm(effects).command.unwrap();
    assert_eq!(h.advance(event.clone()).command, None);
    h.complete(first);
    assert_eq!(h.advance(event).command, None);
    assert_eq!(h.casts, [5]);
}

#[test]
fn empty_rotations_remain_idle_with_or_without_triggers() {
    for has_triggers in [false, true] {
        let mut p = trigger_profile();
        p["consumable"] = json!("Potion");
        if !has_triggers {
            p["messageTriggers"] = json!([]);
        }
        let mut h = Harness::new(p);
        assert_eq!(h.cycle().outcome, Some(Outcome::Cycle(Cycle::Idle)));
        h.emit(message(None));
        h.emit(message(Some(1)));
        assert!(h.attacks.is_empty());
        assert_eq!(h.preparations, usize::from(has_triggers));
        assert_eq!(h.casts, if has_triggers { vec![5, 5] } else { vec![] });
        if has_triggers {
            assert!(matches!(
                h.cast_actions[0],
                Action::UseSkill {
                    target: None,
                    readiness: CastReadiness::Wait,
                    force: CastForce::Forced,
                    ..
                }
            ));
            assert!(matches!(
                h.cast_actions[1],
                Action::UseSkill {
                    target: Some(TargetLease {
                        key: TargetKey {
                            monster_map_id: 1,
                            ..
                        },
                        ..
                    }),
                    ..
                }
            ));
        }
    }
}

#[test]
fn discards_trigger_after_owned_target_dies_during_validation() {
    let mut p = trigger_profile();
    p["steps"] = rotation_profile()["steps"].clone();
    let mut h = Harness::new(p);
    h.cycle();
    let event = ProfileEvent::Message {
        observed_version: h.job.target_version(),
        message: message(Some(1)),
    };
    h.die(1);
    assert_eq!(h.advance(event).command, None);
    assert_eq!(h.casts, [1]);
}

#[test]
fn target_priority_retains_equal_rank() {
    let mut s = snapshot();
    let mut dead = monster(3, None);
    dead.hp = 0.0;
    dead.state = LifeState::Dead;
    s.monsters = vec![dead, monster(1, None), monster(2, Some(0))];
    assert_eq!(
        select_target(&s, None, Some(1)).unwrap().key.monster_map_id,
        2
    );
    s.monsters[2].rank = None;
    assert_eq!(
        select_target(&s, None, Some(0)).unwrap().key.monster_map_id,
        1
    );
    assert_eq!(
        select_target(&s, Some(s.monsters[2].key), Some(0))
            .unwrap()
            .key
            .monster_map_id,
        2
    );
    s.monsters[2].rank = Some(0);
    assert_eq!(
        select_target(&s, Some(s.monsters[1].key), Some(1))
            .unwrap()
            .key
            .monster_map_id,
        2
    );
}

#[test]
fn equal_rank_respawn_keeps_current_target() {
    let mut h = Harness::new(rotation_profile());
    h.cycle();
    h.die(1);
    h.snapshot.monsters.remove(0);
    h.cycle();
    h.snapshot.monsters.insert(0, monster(1, None));
    h.cycle();
    assert_eq!(h.attacks, [1, 2, 2]);
}

#[test]
fn anti_counter_switches_without_switching_back_at_equal_rank() {
    let mut h = Harness::new(rotation_profile());
    h.cycle();
    h.snapshot.monsters[0].attack = AttackEligibility::Blocked;
    h.cycle();
    h.snapshot.monsters[0].attack = AttackEligibility::Allowed;
    h.cycle();
    for monster in &mut h.snapshot.monsters {
        monster.attack = AttackEligibility::Blocked;
    }
    assert_eq!(h.cycle().outcome, Some(Outcome::Cycle(Cycle::NoTarget)));
    assert_eq!(h.attacks, [1, 2, 2]);
}

#[test]
fn player_life_attack_rejection_and_cycle_delays() {
    let mut h = Harness::new(rotation_profile());
    h.snapshot.alive = false;
    let result = h.cycle();
    assert_eq!(result.outcome, Some(Outcome::Cycle(Cycle::PlayerDead)));
    assert_eq!(result.next_deadline, Some(250));
    assert!(h.attacks.is_empty());
    let mut h = Harness::new(rotation_profile());
    h.accepted = false;
    h.snapshot.monsters[1].rank = Some(0);
    h.job.fallback_rank = Some(1);
    let result = h.cycle();
    assert_eq!(result.outcome, Some(Outcome::Cycle(Cycle::AttackRejected)));
    assert_eq!(result.next_deadline, Some(250));
    assert_eq!(h.attacks, [2]);
    let mut h = Harness::new(rotation_profile());
    let result = h.cycle();
    assert_eq!(
        result.outcome,
        Some(Outcome::Cycle(Cycle::Attacked { cast: true }))
    );
    assert_eq!(result.next_deadline, Some(120));
}

#[test]
fn failures_retain_target_attack_and_profile_stages() {
    let mut h = Harness::new(rotation_profile());
    assert_eq!(
        h.advance(ProfileEvent::ObservationFailed("target failed".into()))
            .outcome,
        Some(Outcome::Failure {
            stage: FailureStage::TargetSelection,
            message: "target failed".into()
        })
    );
    for stage in [FailureStage::Attack, FailureStage::ProfileCast] {
        let mut h = Harness::new(rotation_profile());
        let prepare = h.advance(ProfileEvent::Tick).command.unwrap();
        let mut command = h.complete(prepare).command.unwrap();
        if stage == FailureStage::ProfileCast {
            command = h.complete(command).command.unwrap();
        }
        let failure = h.advance(ProfileEvent::Completed {
            id: command.id,
            reply: Reply::Failed("failed".into()),
        });
        assert_eq!(
            failure.outcome,
            Some(Outcome::Failure {
                stage,
                message: "failed".into()
            })
        );
    }
}

#[test]
fn preflight_warnings_do_not_guard_skill_five() {
    let mut h = Harness::new(json!({"delayMs":120,"steps":[{"skill":5}]}));
    let command = h.advance(ProfileEvent::Tick).command.unwrap();
    let warnings = vec![
        "This profile is for Lord of Order, but Legion Revenant is equipped.".into(),
        "Skill 5 will use whichever consumable is available.".into(),
    ];
    let effect = h.advance(ProfileEvent::Completed {
        id: command.id,
        reply: Reply::Prepared {
            skill5_item_id: None,
            warnings: warnings.clone(),
        },
    });
    let result = h.settle(effect);
    assert_eq!(h.job.warnings(), warnings);
    assert_eq!(
        result.outcome,
        Some(Outcome::Cycle(Cycle::Attacked { cast: true }))
    );
    assert_eq!(result.next_deadline, Some(120));
    assert_eq!(h.attacks, [1]);
    assert_eq!(h.casts, [5]);
}

#[test]
fn triggers_and_death_resets_follow_active_target() {
    let mut p = trigger_profile();
    p["steps"] = rotation_profile()["steps"].clone();
    p["resetSkillIndexOnTargetDeath"] = json!(true);
    let mut h = Harness::new(p);
    h.cycle();
    h.die(2);
    h.cycle();
    h.snapshot.selected = SelectedTarget::Monster(h.snapshot.monsters[1].key);
    h.emit(message(Some(1)));
    h.snapshot.selected = SelectedTarget::None;
    h.emit(message(Some(2)));
    h.snapshot.monsters[0].attack = AttackEligibility::Blocked;
    h.emit(message(None));
    h.snapshot.monsters[0].attack = AttackEligibility::Allowed;
    h.die(1);
    h.cycle();
    h.emit(message(None));
    assert_eq!(h.casts, [1, 2, 1, 5]);
    let result = h.advance(ProfileEvent::Cancel);
    assert!(matches!(
        result.command.unwrap().action,
        Action::ReleasePreparation
    ));
    assert_eq!(h.emit(message(None)).command, None);
    assert_eq!(h.cycle().command, None);
}

#[test]
fn asynchronous_trigger_failures_keep_message_stage() {
    let mut h = Harness::new(trigger_profile());
    h.cycle();
    let effect = h.advance(ProfileEvent::Message {
        observed_version: h.job.target_version(),
        message: message(None),
    });
    let effect = h.reconfirm(effect);
    let result = h.advance(ProfileEvent::Completed {
        id: effect.command.unwrap().id,
        reply: Reply::Failed("trigger failed".into()),
    });
    assert_eq!(
        result.outcome,
        Some(Outcome::Failure {
            stage: FailureStage::MessageTrigger,
            message: "trigger failed".into()
        })
    );
}

#[test]
fn runner_cursors_are_independent() {
    let mut left = Harness::new(rotation_profile());
    let mut right = Harness::new(rotation_profile());
    left.cycle();
    right.cycle();
    left.cycle();
    assert_eq!([left.casts[0], right.casts[0], left.casts[1]], [1, 1, 2]);
}

#[test]
fn attacks_without_drop_responses() {
    let mut h = Harness::new(rotation_profile());
    h.snapshot.monsters = vec![monster(2, None)];
    let result = h.cycle();
    assert_eq!(h.attacks, [2]);
    assert_eq!(h.casts, [1]);
    assert_eq!(
        result.outcome,
        Some(Outcome::Cycle(Cycle::Attacked { cast: true }))
    );
    assert_eq!(result.next_deadline, Some(120));
}

#[test]
fn class_warning_for_different_equipped_class() {
    let p = profile(json!({"classNames":["Void Highlord","Lord of Order"]}));
    assert_eq!(
        class_warning(&p, Some("Legion Revenant"), "LEGION REVENANT").as_deref(),
        Some(
            "This profile is for Void Highlord or Lord of Order, but Legion Revenant is equipped."
        )
    );
}

#[test]
fn class_name_matches_without_case() {
    let p = profile(json!({"classNames":["void highlord"]}));
    assert_eq!(
        class_warning(&p, Some("Void Highlord (IoDA)"), "VOID HIGHLORD"),
        None
    );
}

#[test]
fn equipped_class_item_name_matches() {
    let p = profile(json!({"classNames":["Void Highlord (IoDA)"]}));
    assert_eq!(
        class_warning(&p, Some("Void Highlord (IoDA)"), "VOID HIGHLORD"),
        None
    );
}

#[test]
fn absent_class_list_allows_any_class() {
    assert_eq!(
        class_warning(
            &profile(json!({})),
            Some("Legion Revenant"),
            "LEGION REVENANT"
        ),
        None
    );
}

#[test]
fn unknown_equipped_class_does_not_warn() {
    assert_eq!(
        class_warning(&profile(json!({"classNames":["Void Highlord"]})), None, ""),
        None
    );
}
