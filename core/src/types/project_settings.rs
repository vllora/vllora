use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSettings {
    #[serde(default = "default_enabled_chat_tracing")]
    pub enabled_chat_tracing: bool,
}

fn default_enabled_chat_tracing() -> bool {
    true
}
