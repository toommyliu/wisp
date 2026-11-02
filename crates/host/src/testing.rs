use std::sync::Arc;
use std::task::{Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

use ruffle_core::backend::audio::NullAudioBackend;
use swf::avm1::types::{Action, DefineFunction, Push, Value};
use swf::{Compression, Header, Rectangle, SwfStr, Tag, Twips};

use crate::bridge::ScriptEvent;
use crate::player::{Clipboard, Movie, surface::noop_gpu};
use crate::{Host, ScriptLink, Session, Surface, ViewEvent, Viewport};

pub use crate::data::FileStage;

pub fn headless_host() -> Host {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    headless_host_with_data(std::env::temp_dir().join(format!(
        "lucent-headless-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )))
}

pub fn headless_host_with_data(root: impl Into<std::path::PathBuf>) -> Host {
    Host::with_services_and_data_root(
        Clipboard::memory().expect("couldn't start the in-memory clipboard"),
        || Box::new(NullAudioBackend::new()),
        root.into(),
    )
    .expect("couldn't start the headless host")
}

pub type ProbeReply = (serde_json::Value, Vec<(String, String)>);

#[derive(Default)]
pub struct ProbeMovie {
    frame_rate: Option<f32>,
    constants: Vec<(String, serde_json::Value)>,
    game_objects: Vec<(String, serde_json::Value)>,
    raises: Vec<(String, String, String, u16)>,
    sequences: Vec<(String, Vec<(String, String)>)>,
    throws: Vec<(String, String)>,
    records: Vec<String>,
    guarded_records: Vec<String>,
    drop_accept: Option<(Duration, String)>,
    replies: Vec<(String, Vec<ProbeReply>)>,
}

impl ProbeMovie {
    pub fn frame_rate(mut self, rate: f32) -> Self {
        self.frame_rate = Some(rate);
        self
    }

    pub fn game_object(mut self, path: &str, value: serde_json::Value) -> Self {
        self.game_objects.retain(|(key, _)| key != path);
        self.game_objects.push((path.into(), value));
        self
    }

    pub fn without_game_object(mut self, path: &str) -> Self {
        self.game_objects.retain(|(key, _)| key != path);
        self
    }

    pub fn combat_facts(self) -> Self {
        self.game_object("world.actions.active", serde_json::json!([]))
            .game_object("world.myAvatar.dataLeaf.sta.$tha", serde_json::json!(0))
            .game_object("world.GCD", serde_json::json!(0))
            .game_object("world.GCDTS", serde_json::json!(0))
            .game_object("world.lockdownPots", serde_json::json!(false))
            .constant("combat.getTarget", serde_json::json!(null))
            .constant("combat.getSkillCooldownRemaining", serde_json::json!(0))
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn responding(
        mut self,
        name: impl Into<String>,
        replies: impl IntoIterator<Item = ProbeReply>,
    ) -> Self {
        let name = name.into();
        let replies: Vec<_> = replies.into_iter().collect();
        assert!(!replies.is_empty());
        self.constants.retain(|(method, _)| method != &name);
        self.raises.retain(|(method, ..)| method != &name);
        self.sequences.retain(|(method, _)| method != &name);
        self.throws.retain(|(method, _)| method != &name);
        self.records.retain(|method| method != &name);
        self.guarded_records.retain(|method| method != &name);
        self.replies.retain(|(method, _)| method != &name);
        if name == "drops.accept" {
            self.drop_accept = None;
        }
        self.replies.push((name, replies));
        self
    }

    pub fn recording(mut self, name: impl Into<String>) -> Self {
        self.records.push(name.into());
        self
    }

    pub fn recording_when_ready(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        self.guarded_records.push(name.clone());
        self.records.push(name);
        self
    }

    pub fn accepting_drop(mut self, delay: Duration, packet: impl Into<String>) -> Self {
        self.drop_accept = Some((delay, packet.into()));
        self
    }

    pub fn constant(mut self, name: impl Into<String>, value: serde_json::Value) -> Self {
        let name = name.into();
        self.replies.retain(|(method, _)| method != &name);
        self.constants.push((name, value));
        self
    }

    pub fn throwing(mut self, name: impl Into<String>, message: impl Into<String>) -> Self {
        let name = name.into();
        self.replies.retain(|(method, _)| method != &name);
        self.throws.push((name, message.into()));
        self
    }

    pub fn raising(
        self,
        name: impl Into<String>,
        event: impl Into<String>,
        payload: impl Into<String>,
    ) -> Self {
        self.raising_many(name, event, payload, 1)
    }

    pub fn raising_many(
        mut self,
        name: impl Into<String>,
        event: impl Into<String>,
        payload: impl Into<String>,
        count: u16,
    ) -> Self {
        let name = name.into();
        self.raises.retain(|(method, ..)| method != &name);
        self.sequences.retain(|(method, _)| method != &name);
        self.replies.retain(|(method, _)| method != &name);
        self.raises
            .push((name, event.into(), payload.into(), count));
        self
    }

    pub fn raising_sequence(
        mut self,
        name: impl Into<String>,
        events: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        let name = name.into();
        self.raises.retain(|(method, ..)| method != &name);
        self.sequences.retain(|(method, _)| method != &name);
        self.replies.retain(|(method, _)| method != &name);
        self.sequences.push((name, events.into_iter().collect()));
        self
    }
}

pub fn headless_surface() -> Surface {
    Surface::textures(noop_gpu(), || {})
}

pub fn start_probe(host: &Host, movie: ProbeMovie) -> (Session, ScriptLink) {
    let (session, link) = start_probe_without_viewport(host, movie);
    session.send(ViewEvent::Resized(
        Viewport::new(100.0, 100.0, 1.0).unwrap(),
    ));
    wait_loaded(&link);
    (session, link)
}

pub fn start_probe_without_viewport(host: &Host, movie: ProbeMovie) -> (Session, ScriptLink) {
    let mut session = host
        .start_movie(headless_surface(), Movie(Arc::from(fixture_swf(&movie))))
        .expect("couldn't start the probe player");
    let link = session.attach(crate::bridge::EventInterests::ALL).unwrap();
    (session, link)
}

struct Unpark(thread::Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

pub fn wait_loaded(link: &ScriptLink) {
    link.set_waker(Waker::from(Arc::new(Unpark(thread::current()))));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "probe did not emit onLoaded within 10 seconds"
        );
        match link.try_event() {
            Ok(Some(ScriptEvent::Loader { event, .. })) if event.name == "onLoaded" => return,
            Ok(Some(_)) => {}
            Ok(None) => thread::park_timeout(remaining),
            Err(error) => {
                panic!("probe failed while waiting up to 10 seconds for onLoaded: {error}")
            }
        }
    }
}

fn string(value: &str) -> &SwfStr {
    SwfStr::from_utf8_str(value)
}

fn actions(list: &[Action<'_>]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut writer = swf::avm1::write::Writer::new(&mut output, 10);
    for action in list {
        writer.write_action(action).unwrap();
    }
    output
}

fn push(values: Vec<Value<'_>>) -> Action<'_> {
    Action::Push(Push { values })
}

fn external_interface() -> Vec<Action<'static>> {
    vec![
        push(vec![Value::Str(string("flash"))]),
        Action::GetVariable,
        push(vec![Value::Str(string("external"))]),
        Action::GetMember,
        push(vec![Value::Str(string("ExternalInterface"))]),
        Action::GetMember,
    ]
}

fn push_json<'a>(output: &mut Vec<Action<'a>>, value: &'a serde_json::Value) {
    use serde_json::Value as Json;
    match value {
        Json::Null => output.push(push(vec![Value::Null])),
        Json::Bool(value) => output.push(push(vec![Value::Bool(*value)])),
        Json::Number(value) => output.push(push(vec![match value
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
        {
            Some(value) => Value::Int(value),
            None => Value::Double(value.as_f64().unwrap()),
        }])),
        Json::String(value) => output.push(push(vec![Value::Str(string(value))])),
        Json::Array(values) => {
            for value in values.iter().rev() {
                push_json(output, value);
            }
            output.push(push(vec![Value::Int(values.len() as i32)]));
            output.push(Action::InitArray);
        }
        Json::Object(fields) => {
            for (key, value) in fields {
                output.push(push(vec![Value::Str(string(key))]));
                push_json(output, value);
            }
            output.push(push(vec![Value::Int(fields.len() as i32)]));
            output.push(Action::InitObject);
        }
    }
}

fn fixture_swf(movie: &ProbeMovie) -> Vec<u8> {
    let constants = &movie.constants;
    let echo_body = actions(&[
        push(vec![Value::Str(string("x"))]),
        Action::GetVariable,
        Action::Return,
    ]);
    let mut emit = vec![
        push(vec![Value::Str(string("payload"))]),
        Action::GetVariable,
        push(vec![Value::Str(string("name"))]),
        Action::GetVariable,
        push(vec![Value::Int(2)]),
    ];
    emit.extend(external_interface());
    emit.extend([
        push(vec![Value::Str(string("call"))]),
        Action::CallMethod,
        Action::Pop,
        push(vec![Value::Bool(true)]),
        Action::Return,
    ]);
    let emit_body = actions(&emit);
    let ready_body = actions(&[
        push(vec![
            Value::Str(string("_root.probeReady")),
            Value::Bool(true),
        ]),
        Action::SetVariable,
        push(vec![Value::Bool(true)]),
        Action::Return,
    ]);
    let user_id_body = actions(&[push(vec![Value::Int(7)]), Action::Return]);
    let constant_bodies: Vec<(&str, Vec<u8>)> = constants
        .iter()
        .map(|(name, value)| {
            let mut body = Vec::new();
            push_json(&mut body, value);
            body.push(Action::Return);
            (name.as_str(), actions(&body))
        })
        .collect();
    let throw_bodies: Vec<(&str, Vec<u8>)> = movie
        .throws
        .iter()
        .map(|(name, message)| {
            (
                name.as_str(),
                actions(&[push(vec![Value::Str(string(message))]), Action::Throw]),
            )
        })
        .collect();
    let raise_bodies: Vec<(&str, Vec<u8>)> = movie
        .raises
        .iter()
        .map(|(name, event, payload, count)| {
            let mut body = vec![
                push(vec![Value::Str(string(payload))]),
                push(vec![Value::Str(string(event))]),
                push(vec![Value::Int(2)]),
            ];
            body.extend(external_interface());
            body.extend([
                push(vec![Value::Str(string("call"))]),
                Action::CallMethod,
                Action::Pop,
            ]);
            body.extend([
                push(vec![
                    Value::Str(string("remaining")),
                    Value::Str(string("remaining")),
                ]),
                Action::GetVariable,
                push(vec![Value::Int(1)]),
                Action::Subtract,
                Action::SetVariable,
                push(vec![Value::Str(string("remaining"))]),
                Action::GetVariable,
            ]);
            let offset = -i16::try_from(actions(&body).len() + 5).unwrap();
            body.push(Action::If(swf::avm1::types::If { offset }));
            let mut repeated = vec![
                push(vec![
                    Value::Str(string("remaining")),
                    Value::Int(i32::from(*count)),
                ]),
                Action::DefineLocal,
            ];
            if *count != 0 {
                repeated.extend(body);
            }
            (name.as_str(), actions(&repeated))
        })
        .collect();
    let sequence_bodies: Vec<(&str, Vec<u8>)> = movie
        .sequences
        .iter()
        .map(|(name, events)| {
            let mut body = Vec::new();
            for (event, payload) in events {
                body.push(push(vec![
                    Value::Str(string(payload)),
                    Value::Str(string(event)),
                    Value::Int(2),
                ]));
                body.extend(external_interface());
                body.extend([
                    push(vec![Value::Str(string("call"))]),
                    Action::CallMethod,
                    Action::Pop,
                ]);
            }
            (name.as_str(), actions(&body))
        })
        .collect();
    let record_bodies: Vec<(&str, Vec<u8>)> = movie
        .records
        .iter()
        .filter(|name| name.as_str() != "flash.getGameObject" || movie.game_objects.is_empty())
        .map(|name| {
            let mut body = vec![
                push(vec![Value::Str(string(
                    if movie.throws.iter().any(|(method, _)| method == name) {
                        "arguments"
                    } else {
                        "value"
                    },
                ))]),
                Action::GetVariable,
                push(vec![
                    Value::Str(string(name)),
                    Value::Str(string("onProbeCall")),
                    Value::Int(3),
                ]),
            ];
            body.extend(external_interface());
            body.extend([
                push(vec![Value::Str(string("call"))]),
                Action::CallMethod,
                Action::Pop,
            ]);
            if movie.guarded_records.contains(name) {
                let mut rejected = vec![push(vec![
                    Value::Str(string(name)),
                    Value::Str(string("onProbeNotReady")),
                    Value::Int(2),
                ])];
                rejected.extend(external_interface());
                rejected.extend([
                    push(vec![Value::Str(string("call"))]),
                    Action::CallMethod,
                    Action::Pop,
                    push(vec![Value::Str(string("settings called before readiness"))]),
                    Action::Throw,
                ]);
                let offset = actions(&rejected).len() as i16;
                let mut guarded = vec![
                    push(vec![Value::Str(string("_root.probeReady"))]),
                    Action::GetVariable,
                    Action::If(swf::avm1::types::If { offset }),
                ];
                guarded.extend(rejected);
                guarded.extend(body);
                body = guarded;
            }
            if let Some((_, message)) = movie.throws.iter().find(|(method, _)| method == name) {
                body.extend([push(vec![Value::Str(string(message))]), Action::Throw]);
            } else if let Some((_, value)) = constants.iter().find(|(method, _)| method == name) {
                push_json(&mut body, value);
                body.push(Action::Return);
            }
            (name.as_str(), actions(&body))
        })
        .collect();
    let reply_counters: Vec<_> = (0..movie.replies.len())
        .map(|index| format!("_root.probeReply{index}"))
        .collect();
    let reply_bodies: Vec<_> = movie
        .replies
        .iter()
        .zip(&reply_counters)
        .map(|((name, replies), counter)| {
            let mut body = vec![
                push(vec![Value::Str(string("arguments"))]),
                Action::GetVariable,
                push(vec![
                    Value::Str(string(name)),
                    Value::Str(string("onProbeCall")),
                    Value::Int(3),
                ]),
            ];
            body.extend(external_interface());
            body.extend([
                push(vec![Value::Str(string("call"))]),
                Action::CallMethod,
                Action::Pop,
                push(vec![
                    Value::Str(string("index")),
                    Value::Str(string(counter)),
                ]),
                Action::GetVariable,
                Action::DefineLocal,
                push(vec![
                    Value::Str(string(counter)),
                    Value::Str(string("index")),
                ]),
                Action::GetVariable,
                push(vec![Value::Int(1)]),
                Action::Add2,
                Action::SetVariable,
            ]);
            for (index, (value, events)) in replies.iter().enumerate() {
                let mut reply = Vec::new();
                for (event, payload) in events {
                    reply.push(push(vec![
                        Value::Str(string(payload)),
                        Value::Str(string(event)),
                        Value::Int(2),
                    ]));
                    reply.extend(external_interface());
                    reply.extend([
                        push(vec![Value::Str(string("call"))]),
                        Action::CallMethod,
                        Action::Pop,
                    ]);
                }
                push_json(&mut reply, value);
                reply.push(Action::Return);
                if index + 1 < replies.len() {
                    body.extend([
                        push(vec![Value::Str(string("index"))]),
                        Action::GetVariable,
                        push(vec![Value::Int(index as i32)]),
                        Action::Equals2,
                        Action::Not,
                        Action::If(swf::avm1::types::If {
                            offset: i16::try_from(actions(&reply).len()).unwrap(),
                        }),
                    ]);
                }
                body.extend(reply);
            }
            (name.as_str(), actions(&body))
        })
        .collect();
    let accept_body = movie.drop_accept.as_ref().map(|(delay, packet)| {
        let mut notify = vec![
            push(vec![Value::Str(string("item"))]),
            Action::GetVariable,
            push(vec![Value::Str(string("onProbeAccept")), Value::Int(2)]),
        ];
        notify.extend(external_interface());
        notify.extend([
            push(vec![Value::Str(string("call"))]),
            Action::CallMethod,
            Action::Pop,
        ]);
        let mut emit = vec![push(vec![
            Value::Str(string(packet)),
            Value::Str(string("onExtensionResponse")),
            Value::Int(2),
        ])];
        emit.extend(external_interface());
        emit.extend([
            push(vec![Value::Str(string("call"))]),
            Action::CallMethod,
            Action::Pop,
        ]);
        let emit_bytes = actions(&emit);
        if delay.is_zero() {
            notify.extend(emit);
        } else {
            notify.extend([
                push(vec![Value::Double(delay.as_secs_f64() * 1000.0)]),
                Action::DefineFunction(DefineFunction {
                    name: string(""),
                    params: vec![],
                    actions: &emit_bytes,
                }),
                push(vec![Value::Int(2), Value::Str(string("setTimeout"))]),
                Action::CallFunction,
                Action::Pop,
            ]);
        }
        notify.extend([push(vec![Value::Bool(true)]), Action::Return]);
        actions(&notify)
    });
    let object_strings: Vec<_> = movie
        .game_objects
        .iter()
        .map(|(_, value)| value.to_string())
        .collect();
    let mut object_actions = Vec::new();
    if movie
        .records
        .iter()
        .any(|name| name == "flash.getGameObject")
    {
        object_actions.extend([
            push(vec![Value::Str(string("path"))]),
            Action::GetVariable,
            push(vec![
                Value::Str(string("flash.getGameObject")),
                Value::Str(string("onProbeCall")),
                Value::Int(3),
            ]),
        ]);
        object_actions.extend(external_interface());
        object_actions.extend([
            push(vec![Value::Str(string("call"))]),
            Action::CallMethod,
            Action::Pop,
        ]);
    }

    for ((path, _), value) in movie.game_objects.iter().zip(&object_strings) {
        let answer = vec![push(vec![Value::Str(string(value))]), Action::Return];
        object_actions.extend([
            push(vec![Value::Str(string("path"))]),
            Action::GetVariable,
            push(vec![Value::Str(string(path))]),
            Action::Equals2,
            Action::Not,
            Action::If(swf::avm1::types::If {
                offset: i16::try_from(actions(&answer).len()).unwrap(),
            }),
        ]);
        object_actions.extend(answer);
    }
    object_actions.extend([push(vec![Value::Null]), Action::Return]);
    let object_body = actions(&object_actions);
    let mut callbacks = vec![
        ("probe.ready", vec![], ready_body.as_slice()),
        ("probe.echo", vec![string("x")], echo_body.as_slice()),
        (
            "probe.emit",
            vec![string("name"), string("payload")],
            emit_body.as_slice(),
        ),
    ];
    if !movie.game_objects.is_empty() {
        callbacks.push((
            "flash.getGameObject",
            vec![string("path")],
            object_body.as_slice(),
        ));
    }
    if let Some(body) = &accept_body {
        callbacks.push(("drops.accept", vec![string("item")], body.as_slice()));
    }
    if !constants.iter().any(|(name, _)| name == "player.getUserId") {
        callbacks.push(("player.getUserId", vec![], user_id_body.as_slice()));
    }
    callbacks.extend(
        constant_bodies
            .iter()
            .chain(&raise_bodies)
            .chain(&sequence_bodies)
            .chain(&throw_bodies)
            .map(|(name, body)| (*name, vec![], body.as_slice())),
    );
    callbacks.extend(
        record_bodies
            .iter()
            .map(|(name, body)| (*name, vec![string("value")], body.as_slice())),
    );
    callbacks.extend(
        reply_bodies
            .iter()
            .map(|(name, body)| (*name, vec![], body.as_slice())),
    );
    let mut main = Vec::new();
    for counter in &reply_counters {
        main.extend([
            push(vec![Value::Str(string(counter)), Value::Int(0)]),
            Action::SetVariable,
        ]);
    }
    for (name, params, body) in callbacks {
        main.push(Action::DefineFunction(DefineFunction {
            name: string(""),
            params,
            actions: body,
        }));
        main.push(push(vec![
            Value::Null,
            Value::Str(string(name)),
            Value::Int(3),
        ]));
        main.extend(external_interface());
        main.extend([
            push(vec![Value::Str(string("addCallback"))]),
            Action::CallMethod,
            Action::Pop,
        ]);
    }
    main.push(push(vec![Value::Str(string("onLoaded")), Value::Int(1)]));
    main.extend(external_interface());
    main.extend([
        push(vec![Value::Str(string("call"))]),
        Action::CallMethod,
        Action::Pop,
        Action::Stop,
    ]);
    let main_bytes = actions(&main);
    let header = Header {
        compression: Compression::None,
        version: 10,
        stage_size: Rectangle {
            x_min: Twips::ZERO,
            x_max: Twips::from_pixels(100.0),
            y_min: Twips::ZERO,
            y_max: Twips::from_pixels(100.0),
        },
        frame_rate: swf::Fixed8::from_f32(movie.frame_rate.unwrap_or(24.0)),
        num_frames: 1,
    };
    let mut output = Vec::new();
    swf::write_swf(
        &header,
        &[Tag::DoAction(&main_bytes), Tag::ShowFrame],
        &mut output,
    )
    .unwrap();
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{Error, Value};

    #[test]
    fn successive_probe_replies_record_all_arguments_and_raise_events_before_returning() {
        let host = headless_host();
        let (_session, link) = start_probe(
            &host,
            ProbeMovie::new()
                .constant("probe.reply", serde_json::json!("old constant"))
                .raising("probe.reply", "onDebug", "old event")
                .throwing("probe.reply", "old throw")
                .recording("probe.reply")
                .responding("probe.reply", [(serde_json::json!("old reply"), vec![])])
                .responding(
                    "probe.reply",
                    [
                        (
                            serde_json::json!(false),
                            vec![("onDebug".into(), "first".into())],
                        ),
                        (
                            serde_json::json!({"accepted":true}),
                            vec![("onDebug".into(), "last".into())],
                        ),
                    ],
                ),
        );
        for (reply, event) in [
            (serde_json::json!(false), "first"),
            (serde_json::json!({"accepted":true}), "last"),
            (serde_json::json!({"accepted":true}), "last"),
        ] {
            let arguments = vec![
                Value::String("quest".into()),
                Value::Number(3075.0),
                Value::Bool(true),
            ];
            assert_eq!(
                link.call_loader("probe.reply", arguments.clone())
                    .recv_timeout(Duration::from_secs(5)),
                Ok(lucent_protocol::bridge::from_json(reply)),
            );
            let mut received = Vec::new();
            while let Some(ScriptEvent::Loader { event, .. }) = link.try_event().unwrap() {
                received.push((event.name, event.args));
            }
            assert_eq!(
                received,
                vec![
                    (
                        "onProbeCall".into(),
                        vec![Value::String("probe.reply".into()), Value::List(arguments)]
                    ),
                    ("onDebug".into(), vec![Value::String(event.into())]),
                ]
            );
        }
    }

    #[test]
    fn a_synchronous_burst_overflows_the_script_event_queue() {
        let host = headless_host();
        let (_session, link) = start_probe(
            &host,
            ProbeMovie::new().raising_many("probe.burst", "onConnection", "OnConnection", 600),
        );
        link.call_loader("probe.burst", vec![])
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(link.try_event(), Err(Error::Overflow));
    }

    #[test]
    fn a_loader_call_raises_the_configured_number_of_events() {
        let host = headless_host();
        let (_session, link) = start_probe(
            &host,
            ProbeMovie::new()
                .raising("probe.burst", "onDebug", "old")
                .raising_many("probe.burst", "onDebug", "burst", 3),
        );
        link.call_loader("probe.burst", vec![])
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let mut received = Vec::new();
        while let Some(ScriptEvent::Loader { event, .. }) = link.try_event().unwrap() {
            received.push((event.name, event.args));
        }
        assert_eq!(
            received,
            vec![("onDebug".into(), vec![Value::String("burst".into())]); 3]
        );
    }

    #[test]
    fn a_constant_answers_its_loader_call_with_nested_values() {
        let host = headless_host();
        let (_session, link) = start_probe(
            &host,
            ProbeMovie::new()
                .constant(
                    "bank.getItems",
                    serde_json::json!([
                        {"ItemID": 1, "sName": "Sword", "iQty": 2.5, "bTemp": false},
                        {"ItemID": -3, "sName": "", "tags": [null, "a"]}
                    ]),
                )
                .constant("player.getUserId", serde_json::json!(9)),
        );
        assert_eq!(
            link.call_loader("bank.getItems", vec![])
                .recv_timeout(Duration::from_secs(5))
                .map(lucent_protocol::bridge::to_json),
            Ok(Some(serde_json::json!([
                {"ItemID": 1.0, "bTemp": false, "iQty": 2.5, "sName": "Sword"},
                {"ItemID": -3.0, "sName": "", "tags": [null, "a"]}
            ])))
        );
        assert_eq!(
            link.call_loader("player.getUserId", vec![])
                .recv_timeout(Duration::from_secs(5)),
            Ok(Value::Number(9.0))
        );
    }

    #[test]
    fn restart_reloads_the_movie_and_ends_the_previous_link() {
        let host = headless_host();
        let (mut session, old) = start_probe(&host, ProbeMovie::new());
        let other = session.attach(crate::bridge::EventInterests::ALL).unwrap();
        session.restart(headless_surface()).unwrap();
        for stream in [&old, &other] {
            if let Ok(Some(event)) = stream.try_event() {
                assert_eq!(
                    event,
                    crate::bridge::ScriptEvent::AutoZoneChanged {
                        config: Default::default()
                    }
                );
            }
        }
        assert_eq!(old.try_event(), Err(Error::Stopped));
        assert_eq!(other.try_event(), Err(Error::Stopped));
        assert_eq!(other.read(|_| 42), Err(Error::Stopped));
        assert_eq!(
            old.call_loader("probe.echo", vec![])
                .recv_timeout(Duration::from_secs(1)),
            Err(Error::Stopped)
        );
        assert_eq!(old.read(|_| 42), Err(Error::Stopped));
        let current = session.attach(crate::bridge::EventInterests::ALL).unwrap();
        assert_eq!(current.id(), old.id());
        drop(old);
        drop(other);
        session.send(ViewEvent::Resized(
            Viewport::new(100.0, 100.0, 1.0).unwrap(),
        ));
        wait_loaded(&current);
        assert_eq!(
            session
                .attached_link_count()
                .recv_timeout(Duration::from_secs(5)),
            Ok(1)
        );
        assert_eq!(
            current
                .call_loader("probe.echo", vec![Value::Number(42.0)])
                .recv_timeout(Duration::from_secs(5)),
            Ok(Value::Number(42.0))
        );
    }
}
