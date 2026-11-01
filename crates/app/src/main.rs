mod game_window;
mod terminal;

use lucent_host::ScriptRequest;
use std::ffi::OsString;

use lucent_host::Host;
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,lucent_host::scripting::script=debug")),
        )
        .init();

    let cwd = std::env::current_dir()?;
    let script = script_spec(std::env::args_os().skip(1))?
        .map(|spec| lucent_host::library::load_script(&spec, &cwd))
        .transpose()?;
    let request = script.map(|script| ScriptRequest::new(script).0);
    let host = Host::new()?;
    let app = gpui_platform::application();
    let reopen_host = host.clone();
    app.on_reopen(move |cx| {
        if let Err(error) = game_window::open(&reopen_host, None, cx) {
            tracing::error!(%error, "couldn't reopen a game window");
        }
    });
    app.run(move |cx| {
        lucent_ui::text_field::init(cx);
        if let Err(error) = game_window::open(&host, request, cx) {
            tracing::error!(%error, "couldn't open a game window");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
    Ok(())
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
enum ArgsError {
    #[error("--script needs a file or package script")]
    MissingSpec,
    #[error("--script was given twice")]
    Repeated,
    #[error("unknown argument {0:?}; the only argument is --script <file or package script>")]
    Unknown(OsString),
    #[error("the script spec must be valid Unicode: {0:?}")]
    NonUnicode(OsString),
}

fn script_spec(args: impl IntoIterator<Item = OsString>) -> Result<Option<String>, ArgsError> {
    let mut args = args.into_iter();
    let mut spec = None;
    while let Some(arg) = args.next() {
        if arg != "--script" {
            return Err(ArgsError::Unknown(arg));
        }
        if spec.is_some() {
            return Err(ArgsError::Repeated);
        }
        let value = args.next().ok_or(ArgsError::MissingSpec)?;
        if value == "--script" {
            return Err(ArgsError::Repeated);
        }
        spec = Some(value.into_string().map_err(ArgsError::NonUnicode)?);
    }
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Option<String>, ArgsError> {
        script_spec(args.iter().map(std::ffi::OsString::from))
    }
    #[test]
    fn script_spec_accepts_one_script() {
        assert_eq!(parse(&["--script", "a.js"]), Ok(Some("a.js".into())));
        assert_eq!(
            parse(&["--script", "@lucent/dailies/scripts/farm-elders-blood.js"]),
            Ok(Some("@lucent/dailies/scripts/farm-elders-blood.js".into()))
        );
        assert_eq!(parse(&[]), Ok(None));
        assert_eq!(parse(&["--script"]), Err(ArgsError::MissingSpec));
        assert_eq!(
            parse(&["--script", "a.js", "--script", "b.js"]),
            Err(ArgsError::Repeated)
        );
        assert_eq!(
            parse(&["--scrpit", "a.js"]),
            Err(ArgsError::Unknown("--scrpit".into()))
        );
    }
}
