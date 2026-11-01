use std::ffi::c_void;
use std::sync::{Arc, Once, mpsc};
use std::time::{Duration, Instant};

use crate::ScriptLink;
use crate::bridge::{Error as EventError, ScriptEvent};
use crate::scripting::modules::{self, Scripts};
use lucent_protocol::script::{Script, ScriptInputs};

use crate::scripting::interrupt::Interrupt;
use crate::scripting::{Kill, RuntimeError, host};

const HEAP_LIMIT: usize = 64 << 20;

type Rejections = Vec<(v8::Global<v8::Promise>, v8::Global<v8::Value>)>;

type EvalReply = mpsc::Sender<Result<String, RuntimeError>>;

enum Completion {
    Value(String),
    Promise(v8::Global<v8::Promise>),
}

struct Active {
    promise: v8::Global<v8::Promise>,
    reply: EvalReply,
    run: Option<u64>,
    lost: bool,
}

pub(crate) struct Engine {
    active: Vec<Active>,
    context: v8::Global<v8::Context>,
    isolate: v8::OwnedIsolate,
}

impl Engine {
    pub(crate) fn new(
        link: ScriptLink,
        interrupt: Arc<Interrupt>,
        wake: std::task::Waker,
    ) -> Result<Self, RuntimeError> {
        static PLATFORM: Once = Once::new();
        PLATFORM.call_once(|| {
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform);
            v8::V8::initialize();
        });
        let mut isolate = v8::Isolate::new(v8::CreateParams::default().heap_limits(0, HEAP_LIMIT));
        isolate.add_near_heap_limit_callback(
            near_heap_limit,
            Arc::as_ptr(&interrupt).cast_mut().cast(),
        );
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
        isolate.set_slot(interrupt.clone());
        isolate.set_slot(super::io::Io::new(
            link.records.files.clone(),
            link.records.dialogs.clone(),
            link.records.http.clone(),
            wake,
        ));
        isolate.set_slot(link);
        isolate.set_slot(Instant::now());
        isolate.set_slot(Rejections::new());
        isolate.set_slot(Scripts::default());
        isolate.set_promise_reject_callback(promise_rejected);
        let context = {
            v8::scope!(let scope, &mut isolate);
            let context = v8::Context::new(scope, Default::default());
            let scope = &mut v8::ContextScope::new(scope, context);
            let host = v8::Object::new(scope);
            for (name, function) in [
                ("url", v8::Function::new(scope, host::web::url)),
                ("form", v8::Function::new(scope, host::web::form)),
                ("decode", v8::Function::new(scope, host::web::decode)),
                ("swf", v8::Function::new(scope, host_swf)),
                ("read", v8::Function::new(scope, host_read)),
                ("command", v8::Function::new(scope, host_command)),
                (
                    "prepareOperation",
                    v8::Function::new(scope, host_prepare_operation),
                ),
                ("operate", v8::Function::new(scope, host_operate)),
                (
                    "cancelOperation",
                    v8::Function::new(scope, host_cancel_operation),
                ),
                (
                    "acquireConsumableClaim",
                    v8::Function::new(scope, host_acquire_consumable_claim),
                ),
                ("releaseClaim", v8::Function::new(scope, host_release_claim)),
                ("startTravel", v8::Function::new(scope, host_start_travel)),
                ("cancelTravel", v8::Function::new(scope, host_cancel_job)),
                ("startJob", v8::Function::new(scope, host_start_job)),
                ("engageJob", v8::Function::new(scope, host_engage_job)),
                ("cancelJob", v8::Function::new(scope, host_cancel_job)),
                ("requestIo", v8::Function::new(scope, host_request_io)),
                ("cancelIo", v8::Function::new(scope, host_cancel_io)),
                ("takeIoBody", v8::Function::new(scope, host_take_io_body)),
                ("now", v8::Function::new(scope, host_now)),
                ("log", v8::Function::new(scope, host_log)),
            ] {
                let key = v8::String::new(scope, name).unwrap();
                let function = function.ok_or_else(|| {
                    RuntimeError::JavaScript("couldn't create host function".into())
                })?;
                host.set(scope, key.into(), function.into());
            }
            let key = v8::String::new(scope, "__host").unwrap();
            context.global(scope).set(scope, key.into(), host.into());
            v8::Global::new(scope, context)
        };
        let mut engine = Self {
            active: Vec::new(),
            context,
            isolate,
        };
        engine.evaluate("runtime.js", include_str!("../../../../assets/runtime.js"))?;
        let entries = engine.in_context(EntryPoints::capture)?;
        engine.isolate.set_slot(entries);
        interrupt.attach(engine.isolate.thread_safe_handle());
        Ok(engine)
    }

    pub(crate) fn kill(&mut self, kill: Kill) {
        self.isolate
            .get_slot::<Arc<super::io::Io>>()
            .unwrap()
            .close();
        self.isolate.get_slot::<Arc<Interrupt>>().unwrap().detach();
        for active in self.active.drain(..) {
            let _ = active.reply.send(Err(RuntimeError::Killed(kill)));
        }
    }

    pub(crate) fn take_io(&self) -> Vec<host::Item> {
        self.isolate
            .get_slot::<Arc<super::io::Io>>()
            .unwrap()
            .take()
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.active.is_empty()
    }

    pub(crate) fn try_event(&self) -> Result<Option<ScriptEvent>, EventError> {
        self.isolate.get_slot::<ScriptLink>().unwrap().try_event()
    }

    fn overflowed(&self) -> bool {
        self.isolate.get_slot::<ScriptLink>().unwrap().overflowed()
    }

    pub(crate) fn events_lost(&mut self) {
        tracing::error!("script events overflowed and were lost");
        let mut running = false;
        self.active.retain_mut(|active| {
            if active.run.is_some() {
                active.lost = true;
                running = true;
                true
            } else {
                let _ = active.reply.send(Err(RuntimeError::EventsLost));
                false
            }
        });
        if running {
            self.stop(Some("Script events overflowed and were lost"));
        }
    }

    pub(crate) fn eval(&mut self, name: &str, source: &str, reply: EvalReply) {
        match self.evaluate(name, source) {
            Ok(Completion::Promise(promise)) => self.active.push(Active {
                promise,
                reply,
                run: None,
                lost: false,
            }),
            Ok(Completion::Value(json)) => {
                let _ = reply.send(Ok(json));
            }
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        }
        self.check_pending();
    }

    pub(crate) fn start(&mut self, script: Script, inputs: ScriptInputs, reply: EvalReply) {
        let entry = script.entry();
        let name = entry.to_string();
        let display_name = entry
            .path()
            .rsplit('/')
            .next()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(entry.path());
        self.isolate
            .get_slot::<Arc<super::io::Io>>()
            .unwrap()
            .set_script(display_name.to_owned());
        let id = self
            .isolate
            .get_slot_mut::<Scripts>()
            .unwrap()
            .insert(script);
        let result = self.in_context(|tc| {
            let data = v8::BigInt::new_from_u64(tc, id);
            let load = v8::Function::builder(modules::load)
                .data(data.into())
                .build(tc)
                .ok_or_else(|| exception_text(tc))?;
            let name = v8::String::new(tc, &name).unwrap();
            let inputs = v8::String::new(tc, &serde_json::to_string(&inputs).unwrap()).unwrap();
            let value = call_lucent(
                tc,
                EntryPoint::Start,
                &[load.into(), name.into(), inputs.into()],
            )?;
            let promise =
                v8::Local::<v8::Promise>::try_from(value).map_err(|_| exception_text(tc))?;
            promise.mark_as_handled();
            let promise = v8::Global::new(tc, promise);
            checkpoint(tc)?;
            Ok(promise)
        });
        match result {
            Ok(promise) => self.active.push(Active {
                promise,
                reply,
                run: Some(id),
                lost: false,
            }),
            Err(error) => {
                self.isolate.get_slot_mut::<Scripts>().unwrap().remove(id);
                let _ = reply.send(Err(error));
            }
        }
        self.check_pending();
    }

    pub(crate) fn stop(&mut self, reason: Option<&str>) {
        self.isolate
            .get_slot::<Arc<super::io::Io>>()
            .unwrap()
            .begin_stop();
        let result = self.in_context(|tc| {
            let args: Vec<v8::Local<v8::Value>> = reason
                .into_iter()
                .map(|reason| v8::String::new(tc, reason).unwrap().into())
                .collect();
            let value = call_lucent(tc, EntryPoint::Stop, &args)?;
            if let Ok(promise) = v8::Local::<v8::Promise>::try_from(value) {
                promise.mark_as_handled();
            }
            checkpoint(tc)
        });
        if let Err(error) = result {
            tracing::error!(%error, "couldn't stop the script");
        }
    }

    fn in_context<R>(
        &mut self,
        run: impl FnOnce(
            &mut v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
        ) -> Result<R, RuntimeError>,
    ) -> Result<R, RuntimeError> {
        halted(&self.isolate)?;
        let result = {
            v8::scope!(let scope, &mut self.isolate);
            let context = v8::Local::new(scope, &self.context);
            let scope = &mut v8::ContextScope::new(scope, context);
            v8::tc_scope!(let tc, scope);
            run(tc)
        };
        halted(&self.isolate)?;
        result
    }

    fn evaluate(&mut self, name: &str, source: &str) -> Result<Completion, RuntimeError> {
        self.in_context(|tc| {
            let result = (|| {
                let source = v8::String::new(tc, source)
                    .ok_or_else(|| RuntimeError::JavaScript("script too large".into()))?;
                let name = v8::String::new(tc, name)
                    .ok_or_else(|| RuntimeError::JavaScript("script name too large".into()))?;
                let origin = origin(tc, name, false);
                let script = v8::Script::compile(tc, source, Some(&origin))
                    .ok_or_else(|| exception_text(tc))?;
                script.run(tc).ok_or_else(|| exception_text(tc))
            })();
            let result = result.map(|value| {
                if let Ok(promise) = v8::Local::<v8::Promise>::try_from(value) {
                    promise.mark_as_handled();
                    (value, Some(v8::Global::new(tc, promise)))
                } else {
                    (value, None)
                }
            });
            checkpoint(tc)?;
            let result = result.and_then(|(value, promise)| {
                if tc.has_caught() {
                    return Err(exception_text(tc));
                }
                match promise {
                    Some(promise) => Ok(Completion::Promise(promise)),
                    None => stringify(tc, value).map(Completion::Value),
                }
            });
            settle(tc, result)
        })
    }

    fn check_pending(&mut self) {
        loop {
            let remaining = self.active.len();
            for active in std::mem::take(&mut self.active) {
                if self.overflowed() {
                    self.active.push(active);
                    continue;
                }
                let settled = self.in_context(|tc| {
                    let promise = v8::Local::new(tc, &active.promise);
                    let result = match promise.state() {
                        v8::PromiseState::Pending => return Ok(None),
                        v8::PromiseState::Fulfilled => stringify(tc, promise.result(tc)),
                        v8::PromiseState::Rejected => {
                            let reason = v8::Global::new(tc, promise.result(tc));
                            Err(RuntimeError::JavaScript(rejection_text(tc, &reason)))
                        }
                    };
                    Ok(Some(if active.run.is_some() {
                        result
                    } else {
                        settle(tc, result)
                    }))
                });
                match settled {
                    Ok(Some(result)) if !self.overflowed() => {
                        if let Some(id) = active.run {
                            self.isolate
                                .get_slot::<Arc<super::io::Io>>()
                                .unwrap()
                                .close();
                            self.isolate.get_slot_mut::<Scripts>().unwrap().remove(id);
                        }
                        let _ = active.reply.send(if active.lost {
                            Err(RuntimeError::EventsLost)
                        } else {
                            result
                        });
                    }
                    _ => self.active.push(active),
                }
            }
            if self.active.len() == remaining {
                break;
            }
        }
    }

    pub(crate) fn turn(&mut self, items: &str) -> Result<Option<Instant>, RuntimeError> {
        let (result, rejections) = self.in_context(|tc| {
            let result = (|| {
                let items = v8::String::new(tc, items)
                    .ok_or_else(|| RuntimeError::JavaScript("script events too large".into()))?;
                let errors = call_lucent(tc, EntryPoint::Turn, &[items.into()])?;
                if let Ok(errors) = v8::Local::<v8::Array>::try_from(errors) {
                    for index in 0..errors.length() {
                        if let Some(error) = errors.get_index(tc, index) {
                            tracing::error!(
                                error = error.to_rust_string_lossy(tc),
                                "script callback failed"
                            );
                        }
                    }
                }
                Ok::<_, RuntimeError>(())
            })();
            checkpoint(tc)?;
            Ok((result, take_rejections(tc)))
        })?;
        if let Err(error) = result {
            tracing::error!(%error, "script turn failed");
        }
        for error in rejections {
            tracing::error!(%error, "unhandled script rejection");
        }
        self.check_pending();
        Ok(self.wake())
    }

    fn wake(&mut self) -> Option<Instant> {
        let started = *self.isolate.get_slot::<Instant>().unwrap();
        let result = self.in_context(|tc| {
            call_lucent(tc, EntryPoint::Wake, &[])
                .and_then(|value| value.number_value(tc).ok_or_else(|| exception_text(tc)))
        });
        let deadline = match result {
            Ok(deadline) if deadline.is_finite() => deadline,
            Ok(_) | Err(RuntimeError::Killed(_) | RuntimeError::Stopped) => return None,
            Err(error) => {
                tracing::error!(%error, "script timer deadline failed");
                return None;
            }
        };
        let delay = Duration::try_from_secs_f64(deadline.max(0.0) / 1000.0).ok()?;
        started.checked_add(delay)
    }
}

fn settle<T>(
    tc: &mut v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
    result: Result<T, RuntimeError>,
) -> Result<T, RuntimeError> {
    checkpoint(tc)?;
    let errors = take_rejections(tc);
    let value = result?;
    if tc.has_caught() {
        return Err(exception_text(tc));
    }
    if !errors.is_empty() {
        return Err(RuntimeError::JavaScript(errors.join("\n")));
    }
    Ok(value)
}

fn stringify(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
    value: v8::Local<v8::Value>,
) -> Result<String, RuntimeError> {
    halted(scope)?;
    let json = v8::json::stringify(scope, value).ok_or_else(|| exception_text(scope))?;
    let json = json.to_rust_string_lossy(scope);
    Ok(if json == "undefined" {
        "null".into()
    } else {
        json
    })
}

struct EntryPoints {
    start: v8::Global<v8::Function>,
    stop: v8::Global<v8::Function>,
    turn: v8::Global<v8::Function>,
    wake: v8::Global<v8::Function>,
}

impl EntryPoints {
    fn capture(
        scope: &mut v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
    ) -> Result<Self, RuntimeError> {
        let global = scope.get_current_context().global(scope);
        let key = v8::String::new(scope, "__lucent").unwrap();
        let object = global
            .get(scope, key.into())
            .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
            .ok_or_else(|| exception_text(scope))?;
        let mut get = |name| {
            let key = v8::String::new(scope, name).unwrap();
            let function = object
                .get(scope, key.into())
                .and_then(|value| v8::Local::<v8::Function>::try_from(value).ok())
                .ok_or_else(|| exception_text(scope))?;
            Ok::<_, RuntimeError>(v8::Global::new(scope, function))
        };
        Ok(Self {
            start: get("start")?,
            stop: get("stop")?,
            turn: get("turn")?,
            wake: get("wake")?,
        })
    }
}

enum EntryPoint {
    Start,
    Stop,
    Turn,
    Wake,
}

fn call_lucent<'s>(
    scope: &mut v8::PinnedRef<'s, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
    method: EntryPoint,
    args: &[v8::Local<v8::Value>],
) -> Result<v8::Local<'s, v8::Value>, RuntimeError> {
    let entries = scope.get_slot::<EntryPoints>().unwrap();
    let function = v8::Local::new(
        scope,
        match method {
            EntryPoint::Start => &entries.start,
            EntryPoint::Stop => &entries.stop,
            EntryPoint::Turn => &entries.turn,
            EntryPoint::Wake => &entries.wake,
        },
    );
    let receiver = v8::undefined(scope).into();
    function
        .call(scope, receiver, args)
        .ok_or_else(|| exception_text(scope))
}

fn take_rejections(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
) -> Vec<String> {
    let rejections: Vec<_> = std::mem::take(scope.get_slot_mut::<Rejections>().unwrap())
        .into_iter()
        .filter(|(promise, _)| !v8::Local::new(scope, promise).has_handler())
        .collect();
    rejections
        .into_iter()
        .map_while(|(_, reason)| {
            halted(scope)
                .is_ok()
                .then(|| rejection_text(scope, &reason))
        })
        .collect()
}

pub(crate) fn origin<'s>(
    scope: &v8::PinScope<'s, '_>,
    name: v8::Local<'s, v8::String>,
    is_module: bool,
) -> v8::ScriptOrigin<'s> {
    v8::ScriptOrigin::new(
        scope,
        name.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        is_module,
        None,
    )
}

fn exception_text(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
) -> RuntimeError {
    RuntimeError::JavaScript(
        scope
            .stack_trace()
            .or_else(|| scope.exception())
            .map(|value| value.to_rust_string_lossy(scope))
            .unwrap_or_else(|| "JavaScript operation failed".into()),
    )
}

fn halted(isolate: &v8::Isolate) -> Result<(), RuntimeError> {
    match isolate.get_slot::<Arc<Interrupt>>().unwrap().stop() {
        Some(stop) => Err(stop.into()),
        None => Ok(()),
    }
}

fn checkpoint(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
) -> Result<(), RuntimeError> {
    halted(scope)?;
    scope.perform_microtask_checkpoint();
    Ok(())
}

unsafe extern "C" fn near_heap_limit(data: *mut c_void, current: usize, _: usize) -> usize {
    let interrupt = unsafe { &*data.cast::<Interrupt>() };
    interrupt.kill(Kill::HeapLimit);
    current.saturating_add(HEAP_LIMIT)
}

fn rejection_text(
    scope: &mut v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
    value: &v8::Global<v8::Value>,
) -> String {
    let value = v8::Local::new(scope, value);
    v8::Local::<v8::Object>::try_from(value)
        .ok()
        .and_then(|object| {
            let key = v8::String::new(scope, "stack")?;
            object
                .get(scope, key.into())
                .filter(|value| value.is_string())
        })
        .unwrap_or(value)
        .to_rust_string_lossy(scope)
}

extern "C" fn promise_rejected(message: v8::PromiseRejectMessage) {
    v8::callback_scope!(unsafe scope, &message);
    v8::scope!(let scope, scope);
    let promise = v8::Global::new(scope, message.get_promise());
    match message.get_event() {
        v8::PromiseRejectEvent::PromiseRejectWithNoHandler => {
            let Some(value) = message.get_value() else {
                return;
            };
            let value = v8::Global::new(scope, value);
            scope
                .get_slot_mut::<Rejections>()
                .unwrap()
                .push((promise, value));
        }
        v8::PromiseRejectEvent::PromiseHandlerAddedAfterReject => {
            scope
                .get_slot_mut::<Rejections>()
                .unwrap()
                .retain(|(pending, _)| *pending != promise);
        }
        _ => {}
    }
}

fn return_reply(
    scope: &mut v8::PinScope,
    mut reply: v8::ReturnValue<v8::Value>,
    result: Result<Option<String>, String>,
) {
    match result {
        Ok(None) => {}
        Ok(Some(json)) => {
            if let Some(value) = v8::String::new(scope, &json) {
                reply.set(value.into());
            } else {
                throw(scope, "host reply too large");
            }
        }
        Err(message) => throw(scope, &message),
    }
}

pub(crate) fn throw(scope: &mut v8::PinScope, message: &str) {
    if let Some(message) = v8::String::new(scope, message) {
        let error = v8::Exception::error(scope, message);
        scope.throw_exception(error);
    }
}

fn host_swf(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    reply: v8::ReturnValue<v8::Value>,
) {
    let Some(method) = args.get(0).to_string(scope) else {
        return;
    };
    let method = method.to_rust_string_lossy(scope);
    let Some(json) = args.get(1).to_string(scope) else {
        return;
    };
    let json = json.to_rust_string_lossy(scope);
    let result = host::swf(
        scope.get_slot::<ScriptLink>().unwrap(),
        scope.get_slot::<Arc<Interrupt>>().unwrap(),
        method,
        &json,
    );
    return_reply(scope, reply, result);
}

fn host_read(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    reply: v8::ReturnValue<v8::Value>,
) {
    let Some(json) = args.get(0).to_string(scope) else {
        return;
    };
    let json = json.to_rust_string_lossy(scope);
    let result = host::read(
        scope.get_slot::<ScriptLink>().unwrap(),
        scope.get_slot::<Arc<Interrupt>>().unwrap(),
        &json,
    )
    .map(Some);
    return_reply(scope, reply, result);
}

fn host_prepare_operation(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    reply: v8::ReturnValue<v8::Value>,
) {
    let result = scope
        .get_slot::<ScriptLink>()
        .unwrap()
        .prepare_operation()
        .map(|id| Some(id.to_string()))
        .map_err(|error| error.to_string());
    return_reply(scope, reply, result);
}

fn host_cancel_operation(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _reply: v8::ReturnValue<v8::Value>,
) {
    if let Some(id) = args.get(0).to_string(scope)
        && let Ok(id) = id.to_rust_string_lossy(scope).parse()
    {
        scope.get_slot::<ScriptLink>().unwrap().cancel_operation(id);
    }
}

fn host_acquire_consumable_claim(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    reply: v8::ReturnValue<v8::Value>,
) {
    let Some(json) = args.get(0).to_string(scope) else {
        return;
    };
    let json = json.to_rust_string_lossy(scope);
    let result =
        host::acquire_consumable_claim(scope.get_slot::<ScriptLink>().unwrap(), &json).map(Some);
    return_reply(scope, reply, result);
}

fn host_release_claim(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _reply: v8::ReturnValue<v8::Value>,
) {
    if let Some(id) = args.get(0).to_string(scope)
        && let Ok(id) = id.to_rust_string_lossy(scope).parse()
    {
        scope.get_slot::<ScriptLink>().unwrap().release_claim(id);
    }
}

fn host_operate(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    reply: v8::ReturnValue<v8::Value>,
) {
    let Some(id) = args.get(0).to_string(scope) else {
        return;
    };
    let id = id.to_rust_string_lossy(scope);
    let Some(json) = args.get(1).to_string(scope) else {
        return;
    };
    let json = json.to_rust_string_lossy(scope);
    let result = host::operate(scope.get_slot::<ScriptLink>().unwrap(), &id, &json).map(|()| None);
    return_reply(scope, reply, result);
}

fn host_command(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    reply: v8::ReturnValue<v8::Value>,
) {
    let Some(json) = args.get(0).to_string(scope) else {
        return;
    };
    let json = json.to_rust_string_lossy(scope);
    let result = host::command(
        scope.get_slot::<ScriptLink>().unwrap(),
        scope.get_slot::<Arc<Interrupt>>().unwrap(),
        &json,
    )
    .map(Some);
    return_reply(scope, reply, result);
}

fn host_now(
    scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    mut reply: v8::ReturnValue<v8::Value>,
) {
    reply.set_double(scope.get_slot::<Instant>().unwrap().elapsed().as_secs_f64() * 1000.0);
}

fn host_log(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _reply: v8::ReturnValue<v8::Value>,
) {
    const TARGET: &str = "lucent_host::scripting::script";
    let text = args.get(1);
    match args.get(0).to_rust_string_lossy(scope).as_str() {
        "debug" => tracing::debug!(target: TARGET, "{}", text.to_rust_string_lossy(scope)),
        "warn" => tracing::warn!(target: TARGET, "{}", text.to_rust_string_lossy(scope)),
        "error" => tracing::error!(target: TARGET, "{}", text.to_rust_string_lossy(scope)),
        _ => tracing::info!(target: TARGET, "{}", text.to_rust_string_lossy(scope)),
    }
}

#[cfg(test)]
mod tests;

fn host_start_job(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    reply: v8::ReturnValue<v8::Value>,
) {
    let Some(json) = args.get(0).to_string(scope) else {
        return;
    };
    let json = json.to_rust_string_lossy(scope);
    let result = scope
        .get_slot::<ScriptLink>()
        .unwrap()
        .jobs
        .start(&json)
        .map(|id| Some(id.to_string()));
    return_reply(scope, reply, result);
}
fn host_engage_job(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _reply: v8::ReturnValue<v8::Value>,
) {
    if let Some(id) = args.get(0).to_string(scope)
        && let Ok(id) = id.to_rust_string_lossy(scope).parse()
    {
        scope
            .get_slot::<ScriptLink>()
            .unwrap()
            .jobs
            .engage(id, args.get(1).boolean_value(scope));
    }
}
fn host_cancel_job(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _reply: v8::ReturnValue<v8::Value>,
) {
    if let Some(id) = args.get(0).to_string(scope)
        && let Ok(id) = id.to_rust_string_lossy(scope).parse()
    {
        scope.get_slot::<ScriptLink>().unwrap().jobs.cancel(id);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.isolate
            .get_slot::<Arc<super::io::Io>>()
            .unwrap()
            .close();
    }
}

fn host_request_io(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    reply: v8::ReturnValue<v8::Value>,
) {
    let Some(json) = args.get(0).to_string(scope) else {
        return;
    };
    let json = json.to_rust_string_lossy(scope);
    if scope
        .get_slot::<ScriptLink>()
        .unwrap()
        .player_status()
        .is_stopped()
    {
        scope.get_slot::<Arc<super::io::Io>>().unwrap().close();
    }
    let body = v8::Local::<v8::ArrayBufferView>::try_from(args.get(1))
        .ok()
        .map(|view| {
            let mut bytes = vec![0; view.byte_length()];
            view.copy_contents(&mut bytes);
            bytes
        });
    let result = scope
        .get_slot::<Arc<super::io::Io>>()
        .unwrap()
        .submit_bytes(&json, body)
        .map(Some);
    return_reply(scope, reply, result);
}

fn host_cancel_io(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    _reply: v8::ReturnValue<v8::Value>,
) {
    if let Some(id) = args.get(0).to_string(scope)
        && let Ok(id) = id.to_rust_string_lossy(scope).parse()
    {
        scope.get_slot::<Arc<super::io::Io>>().unwrap().cancel(id);
    }
}

fn host_start_travel(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    reply: v8::ReturnValue<v8::Value>,
) {
    let Some(json) = args.get(0).to_string(scope) else {
        return;
    };
    let json = json.to_rust_string_lossy(scope);
    let result = scope
        .get_slot::<ScriptLink>()
        .unwrap()
        .jobs
        .start_travel(&json)
        .map(|id| id.map(|id| id.to_string()));
    return_reply(scope, reply, result);
}

fn host_take_io_body(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut reply: v8::ReturnValue,
) {
    let id = args.get(0).to_rust_string_lossy(scope).parse().ok();
    let body = id.and_then(|id| {
        scope
            .get_slot::<Arc<super::io::Io>>()
            .unwrap()
            .take_body(id)
    });
    if let Some(body) = body {
        let store = v8::ArrayBuffer::new_backing_store_from_vec(body).make_shared();
        reply.set(v8::ArrayBuffer::with_backing_store(scope, &store).into());
    } else {
        let text = v8::String::new(scope, "HTTP response body is no longer available.").unwrap();
        let error = v8::Exception::error(scope, text);
        scope.throw_exception(error);
    }
}
