use lucent_host::app_settings::LaunchMode;
use std::ffi::OsString;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    pub script: Option<String>,
    pub launch_mode: Option<LaunchMode>,
    pub debug: bool,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    #[error("--script needs a file or package script")]
    MissingScript,
    #[error("--launch-mode needs game, account-manager or manager")]
    MissingLaunchMode,
    #[error("--launch-mode must be game, account-manager or manager, not {0:?}")]
    InvalidLaunchMode(String),
    #[error("{0} was given twice")]
    Repeated(&'static str),
    #[error("--script opens a game window, so it can't be used with --launch-mode")]
    ScriptWithLaunchMode,
    #[error("--debug takes no value")]
    DebugValue,
    #[error(
        "unknown argument {0:?}; the arguments are --script <file or package script>, --launch-mode <game|account-manager|manager> and --debug"
    )]
    Unknown(OsString),
    #[error("{0:?} must be valid Unicode")]
    NonUnicode(OsString),
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Args, ArgsError> {
    let mut args = args.into_iter().peekable();
    let mut parsed = Args::default();
    while let Some(arg) = args.next() {
        let Some(text) = arg.to_str() else {
            return Err(ArgsError::NonUnicode(arg));
        };
        let (name, inline) = match text.split_once('=') {
            Some((name, value)) => (name, Some(value.to_owned())),
            None => (text, None),
        };
        let mut value = || match inline.clone() {
            Some(value) => Ok(Some(value)),
            None => args
                .next_if(|next| !next.to_string_lossy().starts_with("--"))
                .map(|next| next.into_string().map_err(ArgsError::NonUnicode))
                .transpose(),
        };
        match name {
            "--script" => {
                if parsed.script.is_some() {
                    return Err(ArgsError::Repeated("--script"));
                }
                let spec = value()?.filter(|spec| !spec.is_empty());
                parsed.script = Some(spec.ok_or(ArgsError::MissingScript)?);
            }
            "--launch-mode" | "--launchMode" => {
                if parsed.launch_mode.is_some() {
                    return Err(ArgsError::Repeated("--launch-mode"));
                }
                let mode = value()?.ok_or(ArgsError::MissingLaunchMode)?;
                parsed.launch_mode = Some(match mode.trim().to_lowercase().as_str() {
                    "game" => LaunchMode::Game,
                    "account-manager" | "manager" => LaunchMode::AccountManager,
                    _ => return Err(ArgsError::InvalidLaunchMode(mode)),
                });
            }
            "--debug" if inline.is_some() => return Err(ArgsError::DebugValue),
            "--debug" if parsed.debug => return Err(ArgsError::Repeated("--debug")),
            "--debug" => parsed.debug = true,
            _ => return Err(ArgsError::Unknown(arg)),
        }
    }
    if parsed.script.is_some() && parsed.launch_mode.is_some() {
        return Err(ArgsError::ScriptWithLaunchMode);
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, ArgsError> {
        super::parse(args.iter().map(OsString::from))
    }

    fn mode(launch_mode: LaunchMode) -> Result<Args, ArgsError> {
        Ok(Args {
            launch_mode: Some(launch_mode),
            ..Args::default()
        })
    }

    #[test]
    fn no_arguments_leave_every_choice_to_settings() {
        assert_eq!(parse(&[]), Ok(Args::default()));
    }

    #[test]
    fn script_accepts_one_file_or_package_script() {
        assert_eq!(
            parse(&["--script", "a.js"]),
            Ok(Args {
                script: Some("a.js".into()),
                ..Args::default()
            })
        );
        assert_eq!(
            parse(&["--script=@lucent/dailies/scripts/farm-elders-blood.js"]),
            Ok(Args {
                script: Some("@lucent/dailies/scripts/farm-elders-blood.js".into()),
                ..Args::default()
            })
        );
        assert_eq!(parse(&["--script"]), Err(ArgsError::MissingScript));
        assert_eq!(
            parse(&["--script", "--debug"]),
            Err(ArgsError::MissingScript)
        );
        assert_eq!(
            parse(&["--script", "a.js", "--script", "b.js"]),
            Err(ArgsError::Repeated("--script"))
        );
    }

    #[test]
    fn launch_mode_takes_either_spelling_and_either_value_form() {
        for args in [
            &["--launch-mode", "game"][..],
            &["--launch-mode=game"],
            &["--launchMode", "game"],
            &["--launchMode=game"],
        ] {
            assert_eq!(parse(args), mode(LaunchMode::Game), "{args:?}");
        }
        assert_eq!(
            parse(&["--launch-mode=account-manager"]),
            mode(LaunchMode::AccountManager)
        );
    }

    #[test]
    fn manager_is_an_alias_for_the_account_manager() {
        assert_eq!(
            parse(&["--launch-mode", "manager"]),
            mode(LaunchMode::AccountManager)
        );
        assert_eq!(
            parse(&["--launch-mode", " MANAGER "]),
            mode(LaunchMode::AccountManager)
        );
    }

    #[test]
    fn launch_mode_refuses_missing_unknown_and_repeated_values() {
        assert_eq!(parse(&["--launch-mode"]), Err(ArgsError::MissingLaunchMode));
        assert_eq!(
            parse(&["--launch-mode", "--debug"]),
            Err(ArgsError::MissingLaunchMode)
        );
        assert_eq!(
            parse(&["--launch-mode="]),
            Err(ArgsError::InvalidLaunchMode("".into()))
        );
        assert_eq!(
            parse(&["--launch-mode", "grid"]),
            Err(ArgsError::InvalidLaunchMode("grid".into()))
        );
        assert_eq!(
            parse(&["--launch-mode", "game", "--launchMode=manager"]),
            Err(ArgsError::Repeated("--launch-mode"))
        );
    }

    #[test]
    fn debug_is_a_flag_without_a_value() {
        assert_eq!(
            parse(&["--debug"]),
            Ok(Args {
                debug: true,
                ..Args::default()
            })
        );
        assert_eq!(parse(&["--debug=false"]), Err(ArgsError::DebugValue));
        assert_eq!(
            parse(&["--debug", "--debug"]),
            Err(ArgsError::Repeated("--debug"))
        );
    }

    #[test]
    fn flags_combine_in_any_order() {
        let expected = Ok(Args {
            script: None,
            launch_mode: Some(LaunchMode::AccountManager),
            debug: true,
        });
        assert_eq!(parse(&["--debug", "--launch-mode", "manager"]), expected);
        assert_eq!(parse(&["--launch-mode=manager", "--debug"]), expected);
        assert_eq!(
            parse(&["--script", "a.js", "--debug"]),
            Ok(Args {
                script: Some("a.js".into()),
                launch_mode: None,
                debug: true,
            })
        );
    }

    #[test]
    fn a_script_cannot_pick_the_launch_window() {
        assert_eq!(
            parse(&["--script", "a.js", "--launch-mode", "game"]),
            Err(ArgsError::ScriptWithLaunchMode)
        );
        assert_eq!(
            parse(&["--launch-mode=manager", "--script", "a.js"]),
            Err(ArgsError::ScriptWithLaunchMode)
        );
    }

    #[test]
    fn unknown_arguments_are_refused() {
        assert_eq!(
            parse(&["--scrpit", "a.js"]),
            Err(ArgsError::Unknown("--scrpit".into()))
        );
        assert_eq!(parse(&["a.js"]), Err(ArgsError::Unknown("a.js".into())));
    }
}
