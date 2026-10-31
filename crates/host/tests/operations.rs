use lucent_host::bridge::EventInterests;
use lucent_host::{
    ScriptLink,
    bridge::{ScriptEvent, Value},
    testing::{self, ProbeMovie},
};
use lucent_protocol::{
    item::ItemContainer,
    operation::{Operation, OperationAnswer},
    query::ItemSelector,
};
use serde_json::json;
use std::time::{Duration, Instant};
const WAIT: Duration = Duration::from_secs(15);

fn emit(link: &ScriptLink, name: &str, payload: String) {
    link.call_loader(
        "probe.emit",
        vec![Value::String(name.into()), Value::String(payload)],
    )
    .recv_timeout(WAIT)
    .unwrap();
}
fn drop_item(link: &ScriptLink, category: &str, house: bool) {
    emit(
        link,
        "onExtensionResponse",
        json!({"type":"json","dataObj":{"cmd":"dropItem","items":{"70":{
            "ItemID":70,"sName":"Dropped Item","sType":category,"bHouse":house,"iQty":2
        }}}})
        .to_string(),
    );
}
fn next(link: &ScriptLink) -> ScriptEvent {
    next_for(link, WAIT)
}

fn next_for(link: &ScriptLink, timeout: Duration) -> ScriptEvent {
    try_next_for(link, timeout).unwrap()
}

fn try_next_for(
    link: &ScriptLink,
    timeout: Duration,
) -> Result<ScriptEvent, lucent_host::bridge::Error> {
    struct Wake(std::thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    link.set_waker(std::task::Waker::from(std::sync::Arc::new(Wake(
        std::thread::current(),
    ))));
    let deadline = Instant::now() + timeout;
    loop {
        match link.try_event()? {
            Some(event) => return Ok(event),
            None => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(!remaining.is_zero(), "no event within {timeout:?}");
                std::thread::park_timeout(remaining);
            }
        }
    }
}
fn operation_result(link: &ScriptLink, id: u64) -> Result<bool, lucent_host::bridge::Error> {
    let deadline = Instant::now() + WAIT;
    loop {
        if let ScriptEvent::Operation {
            id: current,
            answer,
        } = try_next_for(link, deadline.saturating_duration_since(Instant::now()))?
        {
            assert_eq!(current, id);
            return answer.map(|answer| match answer {
                OperationAnswer::AcceptDrop(value) => value,
                other => panic!("unexpected operation answer: {other:?}"),
            });
        }
    }
}
fn answer(link: &ScriptLink, id: u64) -> bool {
    operation_result(link, id).unwrap()
}
fn accept(link: &ScriptLink) -> u64 {
    link.operate(Operation::AcceptDrop {
        selector: ItemSelector::Name("dropped item".into()),
    })
    .unwrap()
}
fn packet(data: serde_json::Value) -> String {
    json!({"type":"json","dataObj":data}).to_string()
}

#[test]
fn accepts_run_one_at_a_time() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .accepting_drop(
                Duration::from_millis(100),
                packet(json!({"cmd":"getDrop","ItemID":70,"bSuccess":false})),
            ),
    );
    drop_item(&link, "Item", false);
    let first = accept(&link);
    let second = accept(&link);
    let mut order = vec![];
    let mut answers = vec![];
    while answers.len() < 2 {
        match next(&link) {
            ScriptEvent::Loader { event, .. } if event.name == "onProbeAccept" => {
                assert_eq!(event.args, vec![Value::Number(70.0)]);
                order.push("accept");
            }
            ScriptEvent::Loader { event, .. }
                if event.name == "onExtensionResponse"
                    && event.args[0].clone()
                        == Value::String(packet(
                            json!({"cmd":"getDrop","ItemID":70,"bSuccess":false}),
                        )) =>
            {
                order.push("packet")
            }
            ScriptEvent::Operation { id, answer } => {
                assert_eq!(answer.unwrap(), OperationAnswer::AcceptDrop(false));
                answers.push(id);
            }
            _ => {}
        }
    }
    assert_eq!(answers, [first, second]);
    assert_eq!(order, ["accept", "packet", "accept", "packet"]);
}

#[test]
fn operation_answers_do_not_overflow_another_links_full_queue() {
    let host = testing::headless_host();
    let (mut session, first) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .accepting_drop(
                Duration::ZERO,
                packet(json!({"cmd":"getDrop","ItemID":70,"bSuccess":true,"iQtyNow":2})),
            ),
    );
    drop_item(&first, "Item", false);
    while first.try_event().unwrap().is_some() {}
    let second = session.attach(EventInterests::ALL).unwrap();
    session.read_projection(|_| ()).recv_timeout(WAIT).unwrap();
    assert_eq!(
        second.try_event(),
        Ok(Some(ScriptEvent::AutoZoneChanged {
            config: Default::default()
        }))
    );
    for index in 0..1022 {
        emit(&first, "onDebug", index.to_string());
        assert!(matches!(
            next(&first),
            ScriptEvent::Loader { event, .. } if event.name == "onDebug"
        ));
    }
    let id = accept(&first);
    assert!(answer(&first, id));
    session.read_projection(|_| ()).recv_timeout(WAIT).unwrap();
    assert!(
        !second.overflowed(),
        "a foreign answer overflowed the queue"
    );
    for index in 0..1022 {
        assert!(matches!(
            second.try_event().unwrap(),
            Some(ScriptEvent::Loader { event, .. })
                if event.name == "onDebug" && event.args == [Value::String(index.to_string())]
        ));
    }
    assert!(matches!(
        second.try_event().unwrap(),
        Some(ScriptEvent::Loader { event, .. }) if event.name == "onProbeAccept"
    ));
    assert!(matches!(
        second.try_event().unwrap(),
        Some(ScriptEvent::Loader { event, .. })
            if event.name == "onExtensionResponse" && event.args == [Value::String(packet(
                json!({"cmd":"getDrop","ItemID":70,"bSuccess":true,"iQtyNow":2})
            ))]
    ));
    assert_eq!(second.try_event(), Ok(None));
}

#[test]
fn a_missing_packet_times_out_and_frees_the_resource() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .constant("drops.accept", json!(true)),
    );
    drop_item(&link, "Item", false);
    let at = Instant::now();
    let first = accept(&link);
    let second = link
        .operate(Operation::AcceptDrop {
            selector: ItemSelector::Id(999.0),
        })
        .unwrap();
    assert!(!answer(&link, first));
    assert!(at.elapsed() >= Duration::from_secs(10));
    assert!(at.elapsed() < Duration::from_secs(13));
    assert!(!answer(&link, second));
}

#[test]
fn accepts_choose_the_container_and_verify_the_projected_quantity() {
    for (category, house_item, flags, container) in [
        ("Item", false, json!({"bBank":"1"}), ItemContainer::Bank),
        ("Class", false, json!({"bBank":1}), ItemContainer::Inventory),
        ("Item", false, json!({"bHouse":true}), ItemContainer::House),
        ("Item", true, json!({}), ItemContainer::House),
        ("Item", false, json!({}), ItemContainer::Inventory),
    ] {
        let mut response = json!({"cmd":"getDrop","ItemID":"70.9","bSuccess":"true","iQty":"2"});
        response
            .as_object_mut()
            .unwrap()
            .extend(flags.as_object().unwrap().clone());
        let host = testing::headless_host();
        let (_session, link) = testing::start_probe(
            &host,
            ProbeMovie::new()
                .constant("auth.isLoggedIn", json!(true))
                .accepting_drop(Duration::ZERO, packet(response)),
        );
        drop_item(&link, category, house_item);
        let id = accept(&link);
        assert!(answer(&link, id), "{category}, {container:?}");
        assert_eq!(
            link.read(|state| state
                .items
                .get(container, 70)
                .map(|item| item.data().quantity))
                .unwrap(),
            Some(2.0)
        );
        assert_eq!(
            link.read(|state| state
                .items
                .get(ItemContainer::Drop, 70)
                .map(|item| item.data().quantity))
                .unwrap(),
            None
        );
    }
}

#[test]
fn a_disconnect_and_an_unavailable_loader_free_the_resource() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .accepting_drop(Duration::ZERO, packet(json!({"cmd":"unrelated"}))),
    );
    drop_item(&link, "Item", false);
    let first = accept(&link);
    while !matches!(next(&link), ScriptEvent::Loader { event, .. } if event.name == "onProbeAccept")
    {
    }
    let pending_until = Instant::now() + Duration::from_millis(100);
    while Instant::now() < pending_until {
        assert!(
            !matches!(
                link.try_event().unwrap(),
                Some(ScriptEvent::Operation { .. })
            ),
            "accept ended before the disconnect"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let disconnected = Instant::now();
    emit(&link, "onConnection", "OnConnectionLost".into());
    let second = accept(&link);
    assert!(!answer(&link, first));
    assert!(disconnected.elapsed() < Duration::from_secs(1));
    assert!(!answer(&link, second));
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new().constant("auth.isLoggedIn", json!(true)),
    );
    drop_item(&link, "Item", false);
    let first = accept(&link);
    let second = accept(&link);
    assert_eq!(
        operation_result(&link, first),
        Err(lucent_host::bridge::Error::Unavailable(
            "drops.accept".into()
        ))
    );
    assert_eq!(
        operation_result(&link, second),
        Err(lucent_host::bridge::Error::Unavailable(
            "drops.accept".into()
        ))
    );
}

#[test]
fn a_player_restart_ends_its_accept_and_the_session_worker_accepts_on_the_new_player() {
    let host = testing::headless_host();
    let (mut session, old) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .accepting_drop(
                Duration::from_millis(100),
                packet(json!({"cmd":"getDrop","ItemID":70,"bSuccess":true,"iQtyNow":2})),
            ),
    );
    drop_item(&old, "Item", false);
    let old_id = accept(&old);
    while !matches!(next(&old), ScriptEvent::Loader { event, .. } if event.name == "onProbeAccept")
    {
    }
    let stopped = Instant::now();
    session.restart(testing::headless_surface()).unwrap();
    assert_eq!(
        operation_result(&old, old_id),
        Err(lucent_host::bridge::Error::Stopped)
    );
    assert!(stopped.elapsed() < Duration::from_secs(1));
    let current = session.attach(EventInterests::ALL).unwrap();
    session.send(lucent_host::ViewEvent::Resized(
        lucent_host::Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    while !matches!(next(&current),ScriptEvent::Loader{event,..} if event.name=="onLoaded") {}
    drop_item(&current, "Item", false);
    let deadline = Instant::now() + Duration::from_secs(2);
    let id = accept(&current);
    loop {
        let event = next_for(&current, deadline.saturating_duration_since(Instant::now()));
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeAccept"
        {
            assert_eq!(event.args, vec![Value::Number(70.0)]);
            assert!(
                Instant::now() < deadline,
                "new player accept waited for the old packet timeout"
            );
            break;
        }
    }
    assert!(answer(&current, id));
}

#[test]
fn malformed_and_unrelated_responses_do_not_end_an_accept() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .accepting_drop(
                Duration::ZERO,
                packet(json!({"cmd":"getDrop","ItemID":70,"bSuccess":"invalid"})),
            ),
    );
    drop_item(&link, "Item", false);
    let id = accept(&link);
    while !matches!(next(&link),ScriptEvent::Loader{event,..} if event.name=="onProbeAccept") {}
    emit(
        &link,
        "onExtensionResponse",
        packet(json!({"cmd":"getDrop","ItemID":99,"bSuccess":true})),
    );
    emit(
        &link,
        "onExtensionResponse",
        packet(json!({"cmd":"getDrop","ItemID":70,"bSuccess":true,"iQtyNow":4})),
    );
    assert!(answer(&link, id));
    assert_eq!(
        link.read(|state| state
            .items
            .get(ItemContainer::Inventory, 70)
            .map(|item| item.data().quantity))
            .unwrap(),
        Some(4.0)
    );
}

#[test]
fn dropping_a_session_does_not_add_an_operation_timeout_to_the_player_stop() {
    let host = testing::headless_host();
    let (session, link) = testing::start_probe(
        &host,
        ProbeMovie::new().constant("auth.isLoggedIn", json!(true)),
    );
    drop_item(&link, "Item", false);
    let (entered, waiting) = std::sync::mpsc::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let player = session.read_projection(move |_| {
        entered.send(()).unwrap();
        blocked.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    waiting.recv_timeout(WAIT).unwrap();
    accept(&link);
    std::thread::sleep(Duration::from_millis(100));
    let at = Instant::now();
    drop(session);
    let elapsed = at.elapsed();
    release.send(()).unwrap();
    player.recv_timeout(WAIT).unwrap();
    assert!(
        elapsed < Duration::from_millis(2500),
        "session drop took {elapsed:?}"
    );
}

#[test]
fn a_drop_accept_propagates_a_throwing_loader_callback() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .throwing("drops.accept", "accept boom"),
    );
    drop_item(&link, "Item", false);
    for _ in 0..2 {
        let id = accept(&link);
        assert_eq!(
            operation_result(&link, id),
            Err(lucent_host::bridge::Error::LoaderThrew {
                callback: "drops.accept".into(),
                message: "accept boom".into(),
            })
        );
    }
}

#[test]
fn drop_accepts_propagate_readiness_failures() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new().throwing("auth.isLoggedIn", "login boom"),
    );
    drop_item(&link, "Item", false);
    let id = accept(&link);
    assert_eq!(
        operation_result(&link, id),
        Err(lucent_host::bridge::Error::LoaderThrew {
            callback: "auth.isLoggedIn".into(),
            message: "login boom".into(),
        })
    );
}

#[test]
fn a_drop_accept_rejects_a_malformed_reply_but_keeps_a_decline_as_false() {
    let host = testing::headless_host();
    for (reply, expected) in [
        (json!(false), Ok(false)),
        (
            json!(null),
            Err(lucent_host::bridge::Error::CommandDecode {
                callback: "drops.accept".into(),
                reason: "expected a boolean reply".into(),
            }),
        ),
    ] {
        let (_session, link) = testing::start_probe(
            &host,
            ProbeMovie::new()
                .constant("auth.isLoggedIn", json!(true))
                .constant("drops.accept", reply),
        );
        drop_item(&link, "Item", false);
        let id = accept(&link);
        assert_eq!(operation_result(&link, id), expected);
    }
}

#[test]
fn a_drop_accept_propagates_a_malformed_logged_in_reply() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new().constant("auth.isLoggedIn", json!("yes")),
    );
    drop_item(&link, "Item", false);
    let id = accept(&link);
    assert_eq!(
        operation_result(&link, id),
        Err(lucent_host::bridge::Error::CommandDecode {
            callback: "auth.isLoggedIn".into(),
            reason: "invalid type: string \"yes\", expected a boolean".into(),
        })
    );
}

#[test]
fn cancelling_queued_handles_immediately_frees_fifo_capacity() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .accepting_drop(Duration::ZERO, packet(json!({"cmd":"unrelated"}))),
    );
    drop_item(&link, "Item", false);
    let issued = accept(&link);
    while !matches!(next(&link), ScriptEvent::Loader { event, .. } if event.name == "onProbeAccept")
    {
    }
    let mut queued = vec![];
    loop {
        match link.operate(Operation::AcceptDrop {
            selector: ItemSelector::Id(999.0),
        }) {
            Ok(id) => queued.push(id),
            Err(lucent_host::bridge::Error::Overflow) => break,
            Err(error) => panic!("{error:?}"),
        }
    }
    assert_eq!(queued.len(), 1024);
    let cancelled = queued.pop().unwrap();
    link.cancel_operation(cancelled);
    link.cancel_operation(cancelled);
    let replacement = link
        .operate(Operation::AcceptDrop {
            selector: ItemSelector::Id(999.0),
        })
        .unwrap();
    assert_ne!(replacement, cancelled);
    assert_eq!(
        link.submit_operation(
            replacement,
            Operation::AcceptDrop {
                selector: ItemSelector::Id(70.0)
            }
        ),
        Err(lucent_host::bridge::Error::Stopped)
    );
    link.cancel_operation(replacement);
    for id in queued {
        link.cancel_operation(id);
    }
    emit(
        &link,
        "onExtensionResponse",
        packet(json!({"cmd":"getDrop","ItemID":70,"bSuccess":false})),
    );
    assert!(!answer(&link, issued));
    let sentinel = link
        .operate(Operation::AcceptDrop {
            selector: ItemSelector::Id(999.0),
        })
        .unwrap();
    assert!(!answer(&link, sentinel));
}

#[test]
fn cancelling_an_issued_handle_suppresses_its_answer_until_the_next_operation() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .accepting_drop(Duration::ZERO, packet(json!({"cmd":"unrelated"}))),
    );
    drop_item(&link, "Item", false);
    let issued = accept(&link);
    while !matches!(next(&link), ScriptEvent::Loader { event, .. } if event.name == "onProbeAccept")
    {
    }
    link.cancel_operation(issued);
    link.cancel_operation(issued);
    let sentinel = link
        .operate(Operation::AcceptDrop {
            selector: ItemSelector::Id(999.0),
        })
        .unwrap();
    emit(
        &link,
        "onExtensionResponse",
        packet(json!({"cmd":"getDrop","ItemID":70,"bSuccess":false})),
    );
    assert!(!answer(&link, sentinel));
    link.cancel_operation(issued);
    let final_id = link
        .operate(Operation::AcceptDrop {
            selector: ItemSelector::Id(999.0),
        })
        .unwrap();
    assert!(!answer(&link, final_id));
}

fn equip_result(link: &ScriptLink, id: u64) -> Result<bool, lucent_host::bridge::Error> {
    loop {
        if let ScriptEvent::Operation {
            id: current,
            answer,
        } = next(link)
        {
            assert_eq!(current, id);
            return answer.map(|answer| match answer {
                OperationAnswer::EquipConsumable(value) => value,
                other => panic!("unexpected answer: {other:?}"),
            });
        }
    }
}

#[test]
fn malformed_claim_handles_are_invalid_requests() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(&host, ProbeMovie::new());
    let id = link
        .operate(Operation::PrepareConsumableClaim {
            claim: "invalid".into(),
        })
        .unwrap();
    assert!(
        matches!(equip_result(&link, id), Err(lucent_host::bridge::Error::CommandDecode { reason, .. }) if reason.contains("invalid claim handle"))
    );
}

#[test]
fn ready_claims_recheck_the_slot_before_authorizing_a_shared_holder() {
    let host = testing::headless_host();
    let (mut session, link) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("auth.isLoggedIn", json!(true))
            .constant("flash.getGameObjectS", json!("\"player-a\""))
            .constant("flash.callGameFunction0", json!("true"))
            .responding(
                "combat.getConsumableSkillItem",
                [
                    (json!({"itemId":100}), vec![]),
                    (json!({"itemId":101}), vec![]),
                    (json!({"itemId":100}), vec![]),
                ],
            )
            .responding(
                "inventory.equip",
                [(
                    json!(true),
                    vec![(
                        "onExtensionResponse".into(),
                        json!({"type":"json","dataObj":{"cmd":"seia","iRes":1,"o":{"id":400}}})
                            .to_string(),
                    )],
                )],
            ),
    );
    emit(&link, "onExtensionResponse", json!({"type":"json","dataObj":{"cmd":"loadInventoryBig","items":[{"ItemID":100,"sName":"Potion","sType":"Item","sLink":"potion","sMeta":"400","iQty":2}],"hitems":[]}}).to_string());
    emit(&link, "onExtensionResponse", json!({"type":"json","dataObj":{"cmd":"moveToArea","areaId":1,"areaName":"test-1","uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intState":1}]}}).to_string());
    let observer = session.attach(EventInterests::ALL).unwrap();
    let first = link.acquire_consumable_claim(Some(100)).unwrap().unwrap();
    let id = link
        .operate(Operation::PrepareConsumableClaim {
            claim: first.id.to_string(),
        })
        .unwrap();
    assert_eq!(equip_result(&link, id), Ok(true));
    let shared = link.acquire_consumable_claim(Some(100)).unwrap().unwrap();
    let id = link
        .operate(Operation::PrepareConsumableClaim {
            claim: shared.id.to_string(),
        })
        .unwrap();
    assert_eq!(equip_result(&link, id), Ok(true));
    link.call_loader("probe.echo", vec![])
        .recv_timeout(WAIT)
        .unwrap();
    let mut equips = 0;
    while let Some(event) = observer.try_event().unwrap() {
        if let ScriptEvent::Loader { event, .. } = event
            && event.name == "onProbeCall"
            && event.args.first() == Some(&Value::String("inventory.equip".into()))
        {
            equips += 1;
        }
    }
    assert_eq!(equips, 1);
}

#[test]
fn claimed_equipment_reports_combat_as_the_reason_for_rejection() {
    let host = testing::headless_host();
    let (_session, link) = testing::start_probe(
        &host,
        ProbeMovie::new()
            .constant("combat.getConsumableSkillItem", json!({"itemId":101}))
            .constant("flash.getGameObjectS", json!("\"player-a\""))
            .constant("flash.callGameFunction0", json!("true")),
    );
    emit(&link, "onExtensionResponse", json!({"type":"json","dataObj":{"cmd":"moveToArea","areaId":1,"areaName":"test-1","uoBranch":[{"entID":7,"uoName":"player-a","intHP":100,"intState":2}]}}).to_string());
    let claim = link.acquire_consumable_claim(Some(100)).unwrap().unwrap();
    let id = link
        .operate(Operation::PrepareConsumableClaim {
            claim: claim.id.to_string(),
        })
        .unwrap();
    assert!(
        matches!(equip_result(&link, id), Err(error) if error.to_string().contains("while you were in combat"))
    );
}
