use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum CredentialsIdent {
    Vllora,
    Own,
}

impl Display for CredentialsIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialsIdent::Vllora => write!(f, "vllora"),
            CredentialsIdent::Own => write!(f, "own"),
        }
    }
}
