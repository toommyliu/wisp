mod account_manager;
mod game_window;
mod terminal;
mod wake;

use lucent_host::ScriptRequest;
use std::ffi::OsString;

use gpui::App;
use lucent_host::Host;
use lucent_host::app_settings::LaunchMode;
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
    app.on_reopen(move |cx| open_launch_window(&reopen_host, None, false, cx));
    app.run(move |cx| {
        lucent_ui::text_field::init(cx);
        open_launch_window(&host, request, true, cx);
    });
    Ok(())
}

fn open_launch_window(host: &Host, script: Option<ScriptRequest>, starting: bool, cx: &mut App) {
    let host = host.clone();
    let settings = host.app_settings().get();
    cx.spawn(async move |cx| {
        let mode = if script.is_some() {
            LaunchMode::Game
        } else {
            match settings.await {
                Ok(settings) => settings.preferences.launch_mode,
                Err(error) => {
                    tracing::warn!(%error, "couldn't read the launch mode; opening a game window");
                    LaunchMode::Game
                }
            }
        };
        cx.update(|cx| {
            let opened = match mode {
                LaunchMode::AccountManager => account_manager::show(&host, cx).map(drop),
                LaunchMode::Game => game_window::open(&host, script, None, cx).map(drop),
            };
            match opened {
                Ok(()) => cx.activate(true),
                Err(error) => {
                    tracing::error!(%error, "couldn't open a window");
                    if starting {
                        cx.quit();
                    }
                }
            }
        })
    })
    .detach();
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
