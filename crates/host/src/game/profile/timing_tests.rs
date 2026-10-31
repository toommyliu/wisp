use super::tests::*;
use super::*;
use serde_json::json;

#[test]
fn successful_step_waits_before_cycle_delay() {
    let mut h = Harness::new(json!({"delayMs":10,"steps":[{"skill":1,"waitMs":400}]}));
    let effect = h.cycle();
    assert_eq!(effect.outcome, None);
    assert_eq!(effect.next_deadline, Some(400));
    h.now = 399;
    assert_eq!(h.advance(ProfileEvent::Tick).command, None);
    h.now = 400;
    let effect = h.advance(ProfileEvent::Tick);
    assert_eq!(
        effect.outcome,
        Some(Outcome::Cycle(Cycle::Attacked { cast: true }))
    );
    assert_eq!(effect.next_deadline, Some(450));
}

#[test]
fn failed_step_does_not_wait() {
    let mut h = Harness::new(json!({"steps":[{"skill":1,"waitMs":400}]}));
    h.cast_succeeds = false;
    let effect = h.cycle();
    assert_eq!(
        effect.outcome,
        Some(Outcome::Cycle(Cycle::Attacked { cast: false }))
    );
    assert_eq!(effect.next_deadline, Some(250));
}

#[test]
fn waiting_cooldown_has_deadline_and_revalidates_generation() {
    let mut h = Harness::new(json!({"cooldownMode":"wait-for-cooldown","steps":[{"skill":1}]}));
    h.snapshot.skills[1] = SkillReadiness::CooldownUntil(800);
    let effect = h.cycle();
    assert_eq!(effect.next_deadline, Some(850));
    assert!(h.casts.is_empty());
    h.snapshot.monsters[0].key.generation = 1;
    h.snapshot.skills[1] = SkillReadiness::Ready;
    h.now = 850;
    let effect = h.advance(ProfileEvent::Tick);
    assert_eq!(
        effect.outcome,
        Some(Outcome::Cycle(Cycle::Attacked { cast: false }))
    );
    assert_eq!(effect.next_deadline, Some(1100));
    assert!(h.casts.is_empty());
}

#[test]
fn waiting_cooldown_dispatches_once_ready() {
    let mut h = Harness::new(json!({"cooldownMode":"wait-for-cooldown","steps":[{"skill":1}]}));
    h.snapshot.skills[1] = SkillReadiness::CooldownUntil(800);
    h.cycle();
    h.now = 799;
    assert_eq!(h.advance(ProfileEvent::Tick).next_deadline, Some(850));
    h.now = 850;
    h.snapshot.skills[1] = SkillReadiness::Ready;
    let effect = h.advance(ProfileEvent::Tick);
    assert_eq!(effect.next_deadline, Some(1000));
    assert!(h.casts.is_empty());
    h.now = 999;
    assert_eq!(h.advance(ProfileEvent::Tick).command, None);
    h.now = 1000;
    let effect = h.advance(ProfileEvent::Tick);
    let effect = h.settle(effect);
    assert_eq!(h.casts, [1]);
    assert_eq!(effect.next_deadline, Some(1150));
}

#[test]
fn missing_target_returns_within_grace_without_switching() {
    let mut h = Harness::new(rotation_profile());
    h.cycle();
    let first = h.snapshot.monsters.remove(0);
    assert_eq!(h.cycle().outcome, Some(Outcome::Cycle(Cycle::NoTarget)));
    h.snapshot.monsters.push(first);
    h.cycle();
    assert_eq!(h.attacks, [1, 1]);
}

#[test]
fn missing_target_switches_after_full_grace() {
    let mut h = Harness::new(rotation_profile());
    h.cycle();
    h.snapshot.monsters.remove(0);
    assert_eq!(h.cycle().next_deadline, Some(370));
    assert_eq!(h.cycle().next_deadline, Some(620));
    assert_eq!(
        h.cycle().outcome,
        Some(Outcome::Cycle(Cycle::Attacked { cast: true }))
    );
    assert_eq!(h.attacks, [1, 2]);
}

#[test]
fn late_completions_cannot_advance_new_target() {
    let mut h = Harness::new(rotation_profile());
    let prepare = h.advance(ProfileEvent::Tick).command.unwrap();
    let attack = h.complete(prepare).command.unwrap();
    h.die(1);
    h.snapshot.monsters.remove(0);
    let completed = h.complete(attack.clone());
    assert_eq!(completed.command, None);
    assert_eq!(completed.outcome, Some(Outcome::Cycle(Cycle::NoTarget)));
    assert_eq!(h.advance(ProfileEvent::Tick).command, None);
    h.now = completed.next_deadline.unwrap();
    let next = h.advance(ProfileEvent::Tick).command.unwrap();
    assert_ne!(attack.id, next.id);
    let skill = h.complete(next).command.unwrap();
    let completed = h.complete(attack);
    assert_eq!(completed.command, None);
    assert_eq!(completed.outcome, None);
    h.complete(skill);
    assert_eq!(h.casts, [1]);
}

#[test]
fn unconfirmed_target_death_does_not_reset_rotation() {
    let mut p = rotation_profile();
    p["resetSkillIndexOnTargetDeath"] = json!(true);
    let mut h = Harness::new(p);
    h.cycle();
    h.snapshot.monsters[1].rank = Some(0);
    h.job.fallback_rank = Some(1);
    let attack = h.advance(ProfileEvent::Tick).command.unwrap();
    h.die(2);
    let completed = h.complete(attack);
    assert_eq!(completed.command, None);
    assert_eq!(completed.outcome, Some(Outcome::Cycle(Cycle::NoTarget)));
    h.now = completed.next_deadline.unwrap();
    h.snapshot.monsters.remove(1);
    h.cycle();
    assert_eq!(h.casts, [1, 2]);
}

#[test]
fn failed_trigger_does_not_start_cooldown() {
    let mut h = Harness::new(
        json!({"steps":[],"messageTriggers":[{"messageIncludes":"enrage","skill":5,"cooldownMs":1000}]}),
    );
    h.cycle();
    h.now = 100;
    h.cast_succeeds = false;
    h.emit(message(None));
    h.now = 251;
    h.cast_succeeds = true;
    h.emit(message(None));
    assert_eq!(h.casts, [5, 5]);
}

#[test]
fn self_ally_deduplication_and_zero_maximum() {
    let p = profile(json!({"steps":[{"skill":1,"conditions":[{"type":"ally-hp","value":20}]}]}));
    let mut s = snapshot();
    s.me = Some(Ally {
        entity_id: 1,
        username: "Hero".into(),
        hp: Stat {
            current: 100.0,
            maximum: 100.0,
        },
    });
    s.allies = vec![
        Ally {
            entity_id: 1,
            username: "stale".into(),
            hp: Stat::default(),
        },
        Ally {
            entity_id: 2,
            username: " HERO ".into(),
            hp: Stat::default(),
        },
    ];
    assert!(!matches_step(&s, &p.steps[0], None));
    s.me.as_mut().unwrap().hp.maximum = 0.0;
    assert!(matches_step(&s, &p.steps[0], None));
}

#[test]
fn profile_and_observation_limits_fail_explicitly() {
    let p = profile(json!({"steps":vec![json!({"skill":1}); MAX_STEPS + 1]}));
    assert!(matches!(
        ProfileJob::new(p, Some(0)),
        Err(ProfileLimit::Steps)
    ));
    let mut h = Harness::new(rotation_profile());
    h.snapshot.monsters = vec![monster(1, None); MAX_MONSTERS + 1];
    assert!(matches!(
        h.advance(ProfileEvent::Tick).outcome,
        Some(Outcome::Failure {
            stage: FailureStage::TargetSelection,
            ..
        })
    ));
}

#[test]
fn consumable_validation_rejects_missing_and_unequippable_items() {
    let item = ConsumableItem {
        item_id: 100,
        name: "Potion".into(),
        category: "Item".into(),
        link: "potion".into(),
    };
    assert_eq!(consumable_warning("Potion", Some(&item)), None);
    assert_eq!(
        consumable_warning("Potion", None),
        Some("Potion was not found.".into())
    );
    for link in ["", " none ", "elixir", "TONIC", "\u{feff}none\u{feff}"] {
        let mut invalid = item.clone();
        invalid.link = link.into();
        assert_eq!(
            consumable_warning("Potion", Some(&invalid)),
            Some("Potion cannot be equipped.".into())
        );
    }
}

#[test]
fn trigger_waits_for_cooldown_and_respects_timeout() {
    let mut h = Harness::new(trigger_profile());
    h.cycle();
    h.now = 100;
    h.snapshot.skills[5] = SkillReadiness::CooldownUntil(800);
    let effect = h.emit(message(None));
    assert!(h.casts.is_empty());
    assert_eq!(effect.next_deadline, Some(850));
    h.now = 850;
    h.snapshot.skills[5] = SkillReadiness::Ready;
    let effect = h.advance(ProfileEvent::Tick);
    assert_eq!(effect.next_deadline, Some(1000));
    assert!(h.casts.is_empty());
    h.now = 999;
    assert_eq!(h.advance(ProfileEvent::Tick).command, None);
    h.now = 1000;
    let effect = h.advance(ProfileEvent::Tick);
    h.settle(effect);
    assert_eq!(h.casts, [5]);
    h.snapshot.skills[5] = SkillReadiness::Unavailable;
    let effect = h.emit(message(None));
    assert_eq!(effect.next_deadline, Some(1050));
    h.now = 6000;
    let effect = h.advance(ProfileEvent::Tick);
    assert_eq!(effect.command, None);
    assert_eq!(h.casts, [5]);
}

#[test]
fn stale_trigger_cooldown_finishes_before_target_reselection() {
    let mut p = trigger_profile();
    p["steps"] = rotation_profile()["steps"].clone();
    let mut h = Harness::new(p);
    h.cycle();
    let observed_version = h.job.target_version();
    h.snapshot.skills[5] = SkillReadiness::CooldownUntil(800);
    h.emit(message(Some(1)));
    h.snapshot.monsters[0].key.generation = 1;
    h.now = 300;
    let effect = h.advance(ProfileEvent::Tick);
    assert_eq!(effect.command, None);
    assert_eq!(effect.outcome, None);
    assert_eq!(effect.next_deadline, Some(850));
    h.now = 850;
    h.snapshot.skills[5] = SkillReadiness::Ready;
    let effect = h.advance(ProfileEvent::Tick);
    assert_eq!(effect.command, None);
    assert_eq!(effect.outcome, None);
    assert_eq!(effect.next_deadline, Some(850));
    h.advance(ProfileEvent::Tick);
    assert_eq!(
        h.advance(ProfileEvent::Tick).outcome,
        Some(Outcome::Cycle(Cycle::NoTarget))
    );
    h.now = 1350;
    let attack = h.advance(ProfileEvent::Tick).command.unwrap();
    assert!(matches!(attack.action, Action::Attack(_)));
    assert!(h.job.target_version() > observed_version);
    assert_eq!(
        h.advance(ProfileEvent::Message {
            observed_version,
            message: message(Some(1))
        })
        .command,
        None
    );
    assert_eq!(h.casts, [1]);
}

#[test]
fn ready_wait_cast_reconfirms_after_150_milliseconds() {
    let mut h = Harness::new(json!({"cooldownMode":"wait-for-cooldown","steps":[{"skill":1}]}));
    let effect = h.cycle();
    assert!(h.casts.is_empty());
    assert_eq!(effect.next_deadline, Some(150));
    h.now = 149;
    assert_eq!(h.advance(ProfileEvent::Tick).command, None);
    h.now = 150;
    let effect = h.advance(ProfileEvent::Tick);
    h.settle(effect);
    assert_eq!(h.casts, [1]);
}

#[test]
fn cooldown_reconfirmation_retries_if_readiness_changes() {
    let mut h = Harness::new(json!({"cooldownMode":"wait-for-cooldown","steps":[{"skill":1}]}));
    assert_eq!(h.cycle().next_deadline, Some(150));
    h.now = 150;
    h.snapshot.skills[1] = SkillReadiness::CooldownUntil(200);
    assert_eq!(h.advance(ProfileEvent::Tick).next_deadline, Some(250));
    h.now = 250;
    h.snapshot.skills[1] = SkillReadiness::Ready;
    assert_eq!(h.advance(ProfileEvent::Tick).next_deadline, Some(400));
    h.now = 399;
    assert_eq!(h.advance(ProfileEvent::Tick).command, None);
    h.now = 400;
    let effects = h.advance(ProfileEvent::Tick);
    h.settle(effects);
    assert_eq!(h.casts, [1]);
}

#[test]
fn cooldown_reconfirmation_does_not_extend_the_five_second_bound() {
    let mut h = Harness::new(json!({"cooldownMode":"wait-for-cooldown","steps":[{"skill":1}]}));
    h.snapshot.skills[1] = SkillReadiness::CooldownUntil(4990);
    assert_eq!(h.cycle().next_deadline, Some(5000));
    h.now = 4990;
    h.snapshot.skills[1] = SkillReadiness::Ready;
    assert_eq!(h.advance(ProfileEvent::Tick).next_deadline, Some(5000));
    h.now = 5000;
    let effect = h.advance(ProfileEvent::Tick);
    assert_eq!(
        effect.outcome,
        Some(Outcome::Cycle(Cycle::Attacked { cast: false }))
    );
    assert!(h.casts.is_empty());
}
