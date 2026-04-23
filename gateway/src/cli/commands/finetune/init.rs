//! Scaffold finetune-project/ + create gateway workflow.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §5.1

use clap::Parser;
use vllora_core::metadata::pool::DbPool;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    // TODO [B]: flags per parent design §5.X
}

pub async fn handle(_db_pool: DbPool, _args: Args) -> Result<(), crate::CliError> {
    // TODO [B]: compose Layer B ops via vllora_finetune::LangdbCloudFinetuneClient
    //          + spawn workers via workers::claude_client (claude -p subprocess)
    //          + write local state via vllora_finetune::state::{Journal, Analysis, ChangeLog}
    //          + mirror state to gateway via workflows.pipeline_journal + .iteration_state
    unimplemented!("TODO Track B — 003-cli-pipeline-verbs — init");
}
