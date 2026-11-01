use super::{BarAction, BarState, command};
use lucent_host::Session;
use lucent_protocol::auto_relogin::AutoReloginPhase;

impl BarState {
    pub fn read(&mut self, session: &Session) {
        self.settings = session.settings();
        let zone = session.auto_zone();
        self.zone_enabled = zone.enabled;
        self.zone_map = zone.map;
        let relogin = session.auto_relogin();
        self.relogin_enabled = relogin.config.enabled;
        self.delay = relogin.config.delay_ms / 1000.;
        self.server = relogin.config.server;
        self.relogin_status = relogin.status.last_error.unwrap_or_else(|| {
            if !relogin.status.captured {
                "Waiting for login".into()
            } else {
                match relogin.status.phase {
                    AutoReloginPhase::LoggingIn | AutoReloginPhase::SelectingServer => {
                        "Reconnecting".into()
                    }
                    AutoReloginPhase::WaitingDelay
                    | AutoReloginPhase::WaitingKick
                    | AutoReloginPhase::Backoff { .. } => "Waiting to reconnect".into(),
                    _ => "Ready".into(),
                }
            }
        });
        if let Some(attempts) = relogin.status.attempts_remaining {
            self.relogin_status
                .push_str(&format!(" ({attempts} retries left)"));
        }
        let attack = session.auto_attack();
        self.attack_enabled = attack.config.enabled;
        self.attack_profile = attack
            .config
            .profile
            .as_ref()
            .map(|p| p.label.clone())
            .unwrap_or("Generic".into());
        self.attack_issue = attack
            .status
            .last_error
            .unwrap_or_else(|| attack.status.warnings.join("\n"));
        let script = session.script_status();
        self.phase = script.phase;
        self.script = script.script;
        self.last_end = script.last_end;
    }
}

pub fn apply(
    session: &mut Session,
    action: BarAction,
) -> Option<lucent_host::bridge::Pending<String>> {
    match action {
        BarAction::Stop => {
            session.stop_script();
            None
        }
        BarAction::Kill => {
            session.kill_script();
            None
        }
        action => command(action).map(|command| session.command(command)),
    }
}
