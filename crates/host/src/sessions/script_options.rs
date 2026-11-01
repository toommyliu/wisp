use super::Records;
use lucent_protocol::script_options::{RoomPolicy, ScriptOptions, ScriptOptionsPatch};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("script.options.update requires valid runtime options.")]
pub struct OptionsError;

impl Records {
    pub(crate) fn script_options(&self) -> ScriptOptions {
        self.script_options.read().clone()
    }

    pub(crate) fn update_script_options(
        &self,
        patch: ScriptOptionsPatch,
    ) -> Result<ScriptOptions, OptionsError> {
        if let Some(RoomPolicy::Specific { room_number }) = &patch.room_policy
            && !(1..=99_999).contains(room_number)
        {
            return Err(OptionsError);
        }
        let mut state = self.script_options.write();
        let mut next = state.clone();
        if let Some(value) = patch.restart_after_reconnect {
            next.restart_after_reconnect = value;
        }
        if let Some(value) = patch.room_policy {
            next.room_policy = value;
        }
        if let Some(value) = patch.safe_start_stop {
            next.safe_start_stop = value;
        }
        let changed = *state != next;
        *state = next.clone();
        drop(state);
        if changed {
            self.changes.notify();
        }
        Ok(next)
    }
}
