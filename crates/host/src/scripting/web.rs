use url::{Url, form_urlencoded, quirks};

fn argument(scope: &mut v8::PinScope, args: &v8::FunctionCallbackArguments, index: i32) -> String {
    args.get(index).to_rust_string_lossy(scope)
}

fn reply(scope: &mut v8::PinScope, mut rv: v8::ReturnValue, text: &str) {
    if let Some(value) = v8::String::new(scope, text) {
        rv.set(value.into());
    }
}

pub(crate) fn url(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    let input = argument(scope, &args, 0);
    let base = if args.get(1).is_undefined() {
        None
    } else {
        Url::parse(&argument(scope, &args, 1)).ok()
    };
    if !args.get(1).is_undefined() && base.is_none() {
        return;
    }
    let Ok(mut url) = Url::options().base_url(base.as_ref()).parse(&input) else {
        return;
    };
    let key = argument(scope, &args, 2);
    let value = argument(scope, &args, 3);
    match key.as_str() {
        "href" => {
            if quirks::set_href(&mut url, &value).is_err() {
                return;
            }
        }
        "protocol" => {
            let _ = quirks::set_protocol(&mut url, &value);
        }
        "username" => {
            let _ = quirks::set_username(&mut url, &value);
        }
        "password" => {
            let _ = quirks::set_password(&mut url, &value);
        }
        "host" => {
            let _ = quirks::set_host(&mut url, &value);
        }
        "hostname" => {
            let _ = quirks::set_hostname(&mut url, &value);
        }
        "port" => {
            let _ = quirks::set_port(&mut url, &value);
        }
        "pathname" => quirks::set_pathname(&mut url, &value),
        "search" => quirks::set_search(&mut url, &value),
        "hash" => quirks::set_hash(&mut url, &value),
        _ => {}
    }
    let data = serde_json::json!({
        "href": url.as_str(), "origin": quirks::origin(&url), "protocol": quirks::protocol(&url),
        "username": quirks::username(&url), "password": quirks::password(&url),
        "host": quirks::host(&url), "hostname": quirks::hostname(&url), "port": quirks::port(&url),
        "pathname": quirks::pathname(&url), "search": quirks::search(&url), "hash": quirks::hash(&url)
    });
    reply(scope, rv, &data.to_string());
}

pub(crate) fn form(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    let input = argument(scope, &args, 0);
    let output = if args.get(1).is_true() {
        let Ok(pairs) = serde_json::from_str::<Vec<(String, String)>>(&input) else {
            return;
        };
        form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish()
    } else {
        serde_json::to_string(&form_urlencoded::parse(input.as_bytes()).collect::<Vec<_>>())
            .unwrap()
    };
    reply(scope, rv, &output);
}

pub(crate) fn decode(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    rv: v8::ReturnValue,
) {
    let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(args.get(0)) else {
        return;
    };
    let mut bytes = vec![0; view.byte_length()];
    view.copy_contents(&mut bytes);
    if args.get(1).is_true() && std::str::from_utf8(&bytes).is_err() {
        return;
    }
    let bytes = if !args.get(2).is_true() {
        bytes.strip_prefix(&[239, 187, 191]).unwrap_or(&bytes)
    } else {
        &bytes
    };
    reply(scope, rv, &String::from_utf8_lossy(bytes));
}
