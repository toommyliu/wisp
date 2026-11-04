mod account_manager;
mod cli;
mod game_window;
mod terminal;
mod wake;

use lucent_host::ScriptRequest;

use gpui::App;
use lucent_host::Host;
use lucent_host::app_settings::LaunchMode;
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    let args = cli::parse(std::env::args_os().skip(1))?;
    let default_filter = if args.debug {
        "info,lucent=debug,lucent_host=debug,lucent_ui=debug"
    } else {
        "info,lucent_host::scripting::script=debug"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter)),
        )
        .init();

    let cwd = std::env::current_dir()?;
    let script = args
        .script
        .map(|spec| lucent_host::library::load_script(&spec, &cwd))
        .transpose()?;
    let request = script.map(|script| ScriptRequest::new(script).0);
    let host = Host::new()?;
    let app = gpui_platform::application();
    let reopen_host = host.clone();
    app.on_reopen(move |cx| open_launch_window(&reopen_host, None, None, false, cx));
    app.run(move |cx| {
        lucent_ui::text_field::init(cx);
        open_launch_window(&host, args.launch_mode, request, true, cx);
    });
    Ok(())
}

fn open_launch_window(
    host: &Host,
    mode: Option<LaunchMode>,
    script: Option<ScriptRequest>,
    starting: bool,
    cx: &mut App,
) {
    let host = host.clone();
    let settings = host.app_settings().get();
    cx.spawn(async move |cx| {
        let mode = if script.is_some() {
            LaunchMode::Game
        } else if let Some(mode) = mode {
            mode
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
