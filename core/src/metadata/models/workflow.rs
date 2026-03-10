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
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
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
    pub updated_at: Option<String>,
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
}
