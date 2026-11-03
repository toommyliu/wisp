use lucent_protocol::script_options::ScriptOptions;

use super::Loaded;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("Invalid account username.")]
pub(super) struct InvalidUsername;

pub(super) fn file_name(username: &str) -> Result<String, InvalidUsername> {
    todo!()
}

pub(super) fn decode(file: Option<&[u8]>) -> Loaded<ScriptOptions> {
    todo!()
}

pub(super) fn encode(options: &ScriptOptions) -> Vec<u8> {
    todo!()
}
