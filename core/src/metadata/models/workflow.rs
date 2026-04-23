use crate::metadata::schema::workflows;
use diesel::{AsChangeset, Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = workflows)]
#[serde(crate = "serde")]
pub struct DbWorkflow {
    pub id: String,
    pub name: String,
    pub objective: String,
    pub eval_script: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub state: Option<String>,
    pub iteration_state: Option<String>,
    pub pipeline_journal: Option<String>,
    /// Feature 002 mirror: raw Markdown of `finetune-project/change-log.md`.
    /// UI reads this when the user has no local checkout. Nullable for
    /// workflows that predate the mirror.
    pub change_log_md: Option<String>,
    /// Feature 002 mirror: raw Markdown of `finetune-project/execution-log.md`.
    pub execution_log_md: Option<String>,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = workflows)]
pub struct DbNewWorkflow {
    pub id: Option<String>,
    pub name: String,
    pub objective: String,
}

impl DbNewWorkflow {
    pub fn new(name: String, objective: String) -> Self {
        Self {
            id: Some(Uuid::new_v4().to_string()),
            name,
            objective,
        }
    }
}

#[derive(Debug, AsChangeset, Clone, Default)]
#[diesel(table_name = workflows)]
pub struct DbUpdateWorkflow {
    pub name: Option<String>,
    pub objective: Option<String>,
    pub eval_script: Option<String>,
    pub updated_at: Option<String>,
    pub state: Option<Option<String>>,
    pub iteration_state: Option<Option<String>>,
    pub pipeline_journal: Option<Option<String>>,
    pub change_log_md: Option<Option<String>>,
    pub execution_log_md: Option<Option<String>>,
}

impl DbUpdateWorkflow {
    pub fn new() -> Self {
        Self {
            updated_at: Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            ..Default::default()
        }
    }

    pub fn with_name(mut self, name: Option<String>) -> Self {
        self.name = name;
        self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        self
    }

    pub fn with_objective(mut self, objective: Option<String>) -> Self {
        self.objective = objective;
        self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        self
    }

    pub fn with_eval_script(mut self, eval_script: Option<String>) -> Self {
        self.eval_script = eval_script;
        self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        self
    }

    pub fn with_state(mut self, state: Option<String>) -> Self {
        self.state = Some(state);
        self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        self
    }

    pub fn with_iteration_state(mut self, iteration_state: Option<String>) -> Self {
        self.iteration_state = Some(iteration_state);
        self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        self
    }

    pub fn with_pipeline_journal(mut self, pipeline_journal: Option<String>) -> Self {
        self.pipeline_journal = Some(pipeline_journal);
        self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        self
    }

    pub fn with_change_log_md(mut self, change_log_md: Option<String>) -> Self {
        self.change_log_md = Some(change_log_md);
        self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        self
    }

    pub fn with_execution_log_md(mut self, execution_log_md: Option<String>) -> Self {
        self.execution_log_md = Some(execution_log_md);
        self.updated_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        self
    }
}
