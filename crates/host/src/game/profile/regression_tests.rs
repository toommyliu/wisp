use super::tests::*;
use super::*;
use serde_json::json;

#[test]
fn target_death_preserves_pending_cast_completion() {
    for death_event in [false, true] {
        let mut h = Harness::new(rotation_profile());
        let prepare = h.advance(ProfileEvent::Tick).command.unwrap();
        let attack = h.complete(prepare).command.unwrap();
        let cast = h.complete(attack).command.unwrap();
        h.snapshot.monsters[0].hp = 0.0;
        if death_event {
            h.die(1);
        }
        assert_eq!(h.advance(ProfileEvent::Tick).command, None);
        let effect = h.complete(cast);
        assert_eq!(
            effect.outcome,
            Some(Outcome::Cycle(Cycle::Attacked { cast: true }))
        );
        h.now = effect.next_deadline.unwrap();
        h.cycle();
        assert_eq!(h.casts, [1, 2]);
        assert_eq!(h.attacks, [1, 2]);
    }
}

#[test]
fn target_death_during_cooldown_wait_advances_rotation_after_wait() {
    for death_event in [false, true] {
        let mut p = rotation_profile();
        p["cooldownMode"] = json!("wait-for-cooldown");
        let mut h = Harness::new(p);
        h.snapshot.skills[1] = SkillReadiness::CooldownUntil(800);
        assert_eq!(h.cycle().next_deadline, Some(850));
        h.now = 100;
        h.snapshot.monsters[0].hp = 0.0;
        if death_event {
            h.die(1);
        }
        let waiting = h.advance(ProfileEvent::Tick);
        assert_eq!(waiting.command, None);
        assert_eq!(waiting.next_deadline, Some(850));
        h.now = 850;
        h.snapshot.skills[1] = SkillReadiness::Ready;
        let failed = h.advance(ProfileEvent::Tick);
        assert_eq!(
            failed.outcome,
            Some(Outcome::Cycle(Cycle::Attacked { cast: false }))
        );
        assert_eq!(failed.next_deadline, Some(1100));
        h.now = 1100;
        let effects = h.cycle();
        h.now = 1100;
        let effects = h.reconfirm(effects);
        h.settle(effects);
        assert_eq!(h.casts, [2]);
    }
}

#[test]
fn target_death_during_post_cast_wait_preserves_both_delays() {
    let mut h = Harness::new(json!({"delayMs":120,"steps":[{"skill":1,"waitMs":400},{"skill":2}]}));
    assert_eq!(h.cycle().next_deadline, Some(400));
    h.now = 100;
    h.snapshot.monsters.remove(0);
    h.die(1);
    assert_eq!(h.advance(ProfileEvent::Tick).next_deadline, Some(400));
    h.now = 400;
    let effect = h.advance(ProfileEvent::Tick);
    assert_eq!(effect.command, None);
    assert_eq!(effect.next_deadline, Some(520));
    h.now = 519;
    assert_eq!(h.advance(ProfileEvent::Tick).command, None);
    h.now = 520;
    h.cycle();
    assert_eq!(h.attacks, [1, 2]);
    assert_eq!(h.casts, [1, 2]);
}

#[test]
fn target_death_during_trigger_cooldown_holds_action_until_ready_or_timeout() {
    for ready_at in [Some(1000), None] {
        let mut h = Harness::new(json!({"delayMs":120,"steps":[{"skill":1}],
            "messageTriggers":[{"messageIncludes":"enrage","skill":4}]}));
        h.cycle();
        h.now = 100;
        h.snapshot.skills[4] = SkillReadiness::CooldownUntil(800);
        assert_eq!(h.emit(message(Some(1))).next_deadline, Some(850));
        h.now = 300;
        h.snapshot.monsters.remove(0);
        h.die(1);
        let waiting = h.advance(ProfileEvent::Tick);
        assert_eq!(waiting.command, None);
        assert_eq!(waiting.outcome, None);
        assert_eq!(waiting.next_deadline, Some(850));
        assert_eq!(h.emit(message(Some(2))).outcome, Some(Outcome::Deferred));
        let resumes_at = ready_at.unwrap_or(5100);
        for now in (800..resumes_at).step_by(50) {
            h.now = now;
            let waiting = h.advance(ProfileEvent::Tick);
            assert_eq!(waiting.command, None);
            assert_eq!(waiting.next_deadline, Some(now + 50));
        }
        h.now = resumes_at;
        if ready_at.is_some() {
            h.snapshot.skills[4] = SkillReadiness::Ready;
        }
        let failed = h.advance(ProfileEvent::Tick);
        assert_eq!(failed.command, None);
        assert_eq!(failed.next_deadline, Some(resumes_at));
        h.advance(ProfileEvent::Tick);
        h.cycle();
        assert_eq!(h.attacks, [1, 2]);
        assert_eq!(h.casts, [1, 1]);
        assert_eq!(h.now, resumes_at + 120);
    }
}

#[test]
fn successful_cast_after_target_death_preserves_post_cast_wait() {
    for death_event in [false, true] {
        let mut h = Harness::new(json!({"delayMs":120,
            "steps":[{"skill":1,"waitMs":400},{"skill":2}]}));
        let prepare = h.advance(ProfileEvent::Tick).command.unwrap();
        let attack = h.complete(prepare).command.unwrap();
        let cast = h.complete(attack).command.unwrap();
        h.now = 100;
        h.snapshot.monsters[0].hp = 0.0;
        if death_event {
            h.die(1);
        }
        h.now = 200;
        let completed = h.complete(cast);
        assert_eq!(completed.command, None);
        assert_eq!(completed.outcome, None);
        assert_eq!(completed.next_deadline, Some(600));
        h.now = 599;
        assert_eq!(h.advance(ProfileEvent::Tick).next_deadline, Some(600));
        h.now = 600;
        assert_eq!(h.advance(ProfileEvent::Tick).next_deadline, Some(720));
        h.now = 719;
        assert_eq!(h.advance(ProfileEvent::Tick).command, None);
        h.now = 720;
        h.cycle();
        assert_eq!(h.attacks, [1, 2]);
        assert_eq!(h.casts, [1, 2]);
    }
}

#[test]
fn observation_failure_during_post_cast_wait_keeps_earlier_deadline() {
    for oversized in [false, true] {
        for (fails_at, expected_deadline) in [(100, 350), (300, 400)] {
            let mut h = Harness::new(json!({"delayMs":120,
                "steps":[{"skill":1,"waitMs":400},{"skill":2}]}));
            h.cycle();
            h.now = fails_at;
            let saved = h.snapshot.clone();
            let event = if oversized {
                h.snapshot.monsters = vec![monster(1, None); MAX_MONSTERS + 1];
                ProfileEvent::Tick
            } else {
                ProfileEvent::ObservationFailed("unavailable".into())
            };
            let failed = h.advance(event);
            h.snapshot = saved;
            assert!(matches!(
                failed.outcome,
                Some(Outcome::Failure {
                    stage: FailureStage::TargetSelection,
                    ..
                })
            ));
            assert_eq!(failed.command, None);
            assert_eq!(failed.next_deadline, Some(expected_deadline));
            h.now = expected_deadline;
            let effect = h.advance(ProfileEvent::Tick);
            assert_eq!(effect.command, None);
            if fails_at == 100 {
                assert_eq!(effect.next_deadline, Some(400));
                h.now = 400;
                assert_eq!(h.advance(ProfileEvent::Tick).next_deadline, Some(520));
            } else {
                assert_eq!(effect.next_deadline, Some(520));
            }
            h.now = 520;
            h.cycle();
            assert_eq!(h.attacks, [1, 1]);
            assert_eq!(h.casts, [1, 2]);
        }
    }
}

fn fail_observation(h: &mut Harness, oversized: bool) {
    let effect = if oversized {
        let saved = h.snapshot.clone();
        h.snapshot.monsters = vec![monster(1, None); MAX_MONSTERS + 1];
        let effect = h.advance(ProfileEvent::Tick);
        h.snapshot = saved;
        effect
    } else {
        h.advance(ProfileEvent::ObservationFailed("unavailable".into()))
    };
    assert!(matches!(
        effect.outcome,
        Some(Outcome::Failure {
            stage: FailureStage::TargetSelection,
            ..
        })
    ));
    assert_eq!(effect.command, None);
    h.now += RETRY_MS;
}

#[test]
fn observation_failure_preserves_preparation_and_its_result() {
    for oversized in [false, true] {
        let mut h = Harness::new(json!({"steps":[{"skill":5},{"skill":1}]}));
        fail_observation(&mut h, oversized);
        let prepare = h.advance(ProfileEvent::Tick).command.unwrap();
        assert_eq!(prepare.action, Action::Prepare);
        fail_observation(&mut h, oversized);
        assert_eq!(h.advance(ProfileEvent::Tick).command, None);
        let effect = h.advance(ProfileEvent::Completed {
            id: prepare.id,
            reply: Reply::Prepared {
                skill5_item_id: Some(100),
                warnings: vec!["Class mismatch".into()],
            },
        });
        h.settle(effect);
        assert_eq!(h.job.warnings(), ["Class mismatch"]);
        assert_eq!(h.casts, [1]);
        assert_eq!(h.attacks, [1]);
    }
}

#[test]
fn observation_failure_keeps_one_action_pending() {
    for oversized in [false, true] {
        for pending_attack in [false, true] {
            let mut h = Harness::new(rotation_profile());
            let prepare = h.advance(ProfileEvent::Tick).command.unwrap();
            let attack = h.complete(prepare).command.unwrap();
            let pending = if pending_attack {
                attack
            } else {
                h.complete(attack).command.unwrap()
            };
            fail_observation(&mut h, oversized);
            assert_eq!(h.advance(ProfileEvent::Tick).command, None);
            let effect = h.complete(pending);
            let effect = h.settle(effect);
            assert_eq!(
                effect.outcome,
                Some(Outcome::Cycle(Cycle::Attacked { cast: true }))
            );
            h.now = effect.next_deadline.unwrap();
            h.cycle();
            assert_eq!(h.casts, [1, 2]);
            assert_eq!(h.attacks, [1, 1]);
        }
    }
}

#[test]
fn oversized_snapshot_does_not_drop_action_completions() {
    for stage in 0..3 {
        let mut h = Harness::new(rotation_profile());
        let mut pending = h.advance(ProfileEvent::Tick).command.unwrap();
        for _ in 0..stage {
            pending = h.complete(pending).command.unwrap();
        }
        let saved = h.snapshot.clone();
        h.snapshot.monsters = vec![monster(1, None); MAX_MONSTERS + 1];
        let effect = h.complete(pending);
        h.snapshot = saved;
        h.now = effect.next_deadline.unwrap();
        if stage < 2 {
            assert!(matches!(
                effect.outcome,
                Some(Outcome::Failure {
                    stage: FailureStage::TargetSelection,
                    ..
                })
            ));
            let effect = h.advance(ProfileEvent::Tick);
            let effect = h.settle(effect);
            h.now = effect.next_deadline.unwrap();
        } else {
            assert_eq!(
                effect.outcome,
                Some(Outcome::Cycle(Cycle::Attacked { cast: true }))
            );
        }
        h.cycle();
        assert_eq!(h.casts, [1, 2]);
        assert_eq!(h.preparations, 1);
        assert_eq!(h.attacks, [1, 1]);
    }
}

#[test]
fn successful_trigger_records_cooldown_after_target_death() {
    for death_event in [false, true] {
        let mut h = Harness::new(
            json!({"steps":[{"skill":1}],"messageTriggers":[{"messageIncludes":"enrage","skill":5,"cooldownMs":1000}]}),
        );
        h.cycle();
        h.now = 100;
        let effect = h.advance(ProfileEvent::Message {
            observed_version: h.job.target_version(),
            message: message(Some(1)),
        });
        let cast = h.reconfirm(effect).command.unwrap();
        h.snapshot.monsters[0].hp = 0.0;
        if death_event {
            h.die(1);
        }
        h.now = 300;
        h.complete(cast);
        h.emit(message(Some(2)));
        assert_eq!(h.casts, [1, 5]);
        h.now = 1100;
        h.emit(message(Some(2)));
        assert_eq!(h.casts, [1, 5, 5]);
    }
}

#[test]
fn no_castable_step_preserves_rotation_cursor() {
    let mut h = Harness::new(json!({"steps":[{"skill":1},{"skill":2},{"skill":3}]}));
    h.cycle();
    h.snapshot.skills = [SkillReadiness::Unavailable; 6];
    assert_eq!(
        h.cycle().outcome,
        Some(Outcome::Cycle(Cycle::Attacked { cast: false }))
    );
    h.snapshot.skills = [SkillReadiness::Ready; 6];
    h.cycle();
    assert_eq!(h.casts, [1, 2]);
}

#[test]
fn skipped_rotation_entries_advance_from_the_cast_entry() {
    let mut h = Harness::new(json!({"steps":[{"skill":1},{"skill":2},{"skill":3}]}));
    h.snapshot.skills[1] = SkillReadiness::Unavailable;
    h.cycle();
    h.snapshot.skills[1] = SkillReadiness::Ready;
    h.cycle();
    assert_eq!(h.casts, [2, 3]);
}

#[test]
fn percent_condition_scales_non_hundred_maximum() {
    let mut h = Harness::new(
        json!({"steps":[{"skill":1,"conditions":[{"type":"self-hp","op":"<=","value":40,"unit":"percent"}]},{"skill":2}]}),
    );
    h.snapshot.hp = Stat {
        current: 200.0,
        maximum: 500.0,
    };
    h.cycle();
    h.snapshot.hp.current = 205.0;
    h.cycle();
    assert_eq!(h.casts, [1, 2]);
}

#[test]
fn confirmed_target_death_resets_before_rotation_wraps() {
    let mut h = Harness::new(
        json!({"resetSkillIndexOnTargetDeath":true,"steps":[{"skill":1},{"skill":2},{"skill":3}]}),
    );
    h.cycle();
    h.snapshot.monsters.remove(0);
    h.die(1);
    h.cycle();
    assert_eq!(h.casts, [1, 1]);
}
