use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

use lucent_protocol::script::{ModulePath, Script};

use crate::scripting::engine::{origin, throw};
use crate::scripting::interrupt::Interrupt;

#[derive(Default)]
pub(crate) struct Scripts {
    next: u64,
    scripts: HashMap<u64, Rc<Script>>,
}

impl Scripts {
    pub(crate) fn insert(&mut self, script: Script) -> u64 {
        self.next += 1;
        self.scripts.insert(self.next, Rc::new(script));
        self.next
    }

    pub(crate) fn remove(&mut self, id: u64) {
        self.scripts.remove(&id);
    }
}

type Linking = HashMap<(i32, String), v8::Global<v8::Module>>;

pub(crate) fn load(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    mut reply: v8::ReturnValue<v8::Value>,
) {
    let Some(script) = v8::Local::<v8::BigInt>::try_from(args.data())
        .ok()
        .and_then(|id| {
            scope
                .get_slot::<Scripts>()?
                .scripts
                .get(&id.u64_value().0)
                .cloned()
        })
    else {
        return throw(scope, "This script run has ended.");
    };
    let Ok(builtins) = v8::Local::<v8::Object>::try_from(args.get(0)) else {
        return throw(scope, "Loading a script needs its built-in modules.");
    };
    scope.get_slot::<Arc<Interrupt>>().unwrap().start_script();
    v8::tc_scope!(let tc, scope);
    let result = graph(tc, &script, builtins);
    match result {
        Ok(entry) => {
            let Some(value) = entry.evaluate(tc) else {
                if tc.has_caught() {
                    tc.rethrow();
                }
                return;
            };
            let promise = v8::Local::<v8::Promise>::try_from(value).unwrap();
            promise.mark_as_handled();
            if promise.state() == v8::PromiseState::Rejected {
                let reason = promise.result(tc);
                tc.throw_exception(reason);
                tc.rethrow();
            } else {
                reply.set(entry.get_module_namespace());
            }
        }
        Err(message) if !tc.is_execution_terminating() => {
            tc.reset();
            throw(tc, &message);
            tc.rethrow();
        }
        Err(_) => {}
    }
}

fn resource(path: &ModulePath) -> String {
    fn encode(text: &str) -> String {
        let mut result = String::new();
        for byte in text.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_.!~*'()/".contains(&byte) {
                result.push(char::from(byte));
            } else {
                use std::fmt::Write;
                write!(result, "%{byte:02X}").unwrap();
            }
        }
        result
    }
    match path.package() {
        Some(package) => format!(
            "lucent-script://package/{}/{}",
            encode(package),
            encode(path.path())
        ),
        None => format!("lucent-script://loose/{}", encode(path.path())),
    }
}

fn compile<'s>(
    tc: &mut v8::PinnedRef<'s, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
    script: &Script,
    path: &ModulePath,
    total: &mut usize,
) -> Result<v8::Local<'s, v8::Module>, String> {
    let source = script.source(path).unwrap();
    if source.len() > crate::library::SNAPSHOT_MAX_BYTES - *total {
        return Err("Script sources exceed the 16 MiB execution limit.".into());
    }
    *total += source.len();
    let source = v8::String::new(tc, source).ok_or("Script source is too large")?;
    let name = v8::String::new(tc, &resource(path)).unwrap();
    let origin = origin(tc, name, true);
    let mut source = v8::script_compiler::Source::new(source, Some(&origin));
    let module = v8::script_compiler::compile_module(tc, &mut source).ok_or_else(|| located(tc))?;
    if module.has_top_level_await() {
        return Err(format!(
            "{}: Scripts can't use top-level await.",
            resource(path)
        ));
    }
    Ok(module)
}

fn graph<'s>(
    tc: &mut v8::PinnedRef<'s, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
    script: &Script,
    builtins: v8::Local<v8::Object>,
) -> Result<v8::Local<'s, v8::Module>, String> {
    let mut total = 0;
    let entry = compile(tc, script, script.entry(), &mut total)?;
    let mut modules = BTreeMap::from([(script.entry().clone(), entry)]);
    let mut queue = VecDeque::from([script.entry().clone()]);
    let mut synthetic = HashMap::new();
    let mut linking = Linking::new();
    while let Some(path) = queue.pop_front() {
        let module = modules[&path];
        let requests = module.get_module_requests();
        for index in 0..requests.length() {
            let request =
                v8::Local::<v8::ModuleRequest>::try_from(requests.get(tc, index).unwrap()).unwrap();
            let specifier = request.get_specifier().to_rust_string_lossy(tc);
            let key = request.get_specifier();
            let target = if builtins
                .has_own_property(tc, key.into())
                .ok_or_else(|| located(tc))?
            {
                if !synthetic.contains_key(&specifier) {
                    let object = builtins
                        .get(tc, key.into())
                        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
                        .ok_or_else(|| located(tc))?;
                    let keys = object
                        .get_own_property_names(
                            tc,
                            v8::GetPropertyNamesArgs {
                                mode: v8::KeyCollectionMode::OwnOnly,
                                property_filter: v8::PropertyFilter::ONLY_ENUMERABLE
                                    | v8::PropertyFilter::SKIP_SYMBOLS,
                                index_filter: v8::IndexFilter::IncludeIndices,
                                key_conversion: v8::KeyConversionMode::ConvertToString,
                            },
                        )
                        .ok_or_else(|| located(tc))?;
                    let mut names = vec![v8::String::new(tc, "default").unwrap()];
                    for index in 0..keys.length() {
                        let name = keys
                            .get_index(tc, index)
                            .and_then(|value| value.to_string(tc))
                            .ok_or_else(|| located(tc))?;
                        if name.to_rust_string_lossy(tc) != "default" {
                            names.push(name);
                        }
                    }
                    let module =
                        v8::Module::create_synthetic_module(tc, key, &names, synthetic_steps);
                    synthetic.insert(specifier.clone(), (module, object, names));
                }
                synthetic[&specifier].0
            } else {
                let target = script.resolve(&path, &specifier).ok_or_else(|| {
                    let location = module.source_offset_to_location(request.get_source_offset());
                    format!(
                        "{}:{}:{}: Cannot find module {specifier:?}",
                        resource(&path),
                        location.get_line_number() + 1,
                        location.get_column_number() + 1
                    )
                })?;
                if !modules.contains_key(target) {
                    let module = compile(tc, script, target, &mut total)?;
                    modules.insert(target.clone(), module);
                    queue.push_back(target.clone());
                }
                modules[target]
            };
            linking.insert(
                (module.script_id().unwrap(), specifier),
                v8::Global::new(tc, target),
            );
        }
    }
    tc.set_slot(linking);
    let instantiated = entry.instantiate_module(tc, resolve);
    tc.remove_slot::<Linking>();
    if instantiated != Some(true) {
        return Err(located(tc));
    }
    for (module, object, names) in synthetic.values() {
        for (index, name) in names.iter().enumerate() {
            let value = if index == 0 {
                (*object).into()
            } else {
                object.get(tc, (*name).into()).ok_or_else(|| located(tc))?
            };
            module
                .set_synthetic_module_export(tc, *name, value)
                .ok_or_else(|| located(tc))?;
        }
    }
    Ok(entry)
}

fn resolve<'s>(
    context: v8::Local<'s, v8::Context>,
    specifier: v8::Local<'s, v8::String>,
    _: v8::Local<'s, v8::FixedArray>,
    referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    v8::callback_scope!(unsafe scope, context);
    let key = (
        referrer.script_id().unwrap(),
        specifier.to_rust_string_lossy(scope),
    );
    let module = scope.get_slot::<Linking>().unwrap().get(&key).cloned();
    if let Some(module) = module {
        return Some(v8::Local::new(scope, &module));
    }
    throw(scope, "Cannot resolve module");
    None
}

fn synthetic_steps<'s>(
    context: v8::Local<'s, v8::Context>,
    _: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Value>> {
    v8::callback_scope!(unsafe scope, context);
    Some(v8::undefined(scope).into())
}

fn located(tc: &mut v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>) -> String {
    let Some(exception) = tc.exception() else {
        return "JavaScript operation failed".into();
    };
    let message = tc
        .message()
        .unwrap_or_else(|| v8::Exception::create_message(tc, exception));
    message_text(tc, message, exception)
}

fn message_text(
    scope: &mut v8::PinScope,
    message: v8::Local<v8::Message>,
    exception: v8::Local<v8::Value>,
) -> String {
    let resource = message
        .get_script_resource_name(scope)
        .map(|name| name.to_rust_string_lossy(scope))
        .unwrap_or_default();
    format!(
        "{resource}:{}:{}: {}",
        message.get_line_number(scope).unwrap_or(0),
        message.get_start_column() + 1,
        exception.to_rust_string_lossy(scope)
    )
}
