use crate::metadata::schema::trace_analyses;
use diesel::{Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// DB row for trace analysis results (trace-informed curriculum).
///
/// Stores the 4 JSON artifacts produced by `trace_analyze.py`:
/// - `priority_json`: per-topic frequency, failure rate, priority score
/// - `topics_json`: discovered topics, coverage gaps vs PDF
/// - `prompts_json`: production system prompt + seed user queries
/// - `grader_hints_json`: failure dimensions + prompt rules + calibration pairs
#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = trace_analyses)]
#[serde(crate = "serde")]
pub struct DbTraceAnalysis {
    pub id: String,
    pub workflow_id: String,
    pub priority_json: Option<String>,
    pub topics_json: Option<String>,
    pub prompts_json: Option<String>,
    pub grader_hints_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = trace_analyses)]
pub struct DbNewTraceAnalysis {
    pub id: String,
    pub workflow_id: String,
    pub priority_json: Option<String>,
    pub topics_json: Option<String>,
    pub prompts_json: Option<String>,
    pub grader_hints_json: Option<String>,
}

/// API-facing representation. JSON fields are parsed into `serde_json::Value`
/// so clients get structured data instead of raw strings.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "serde")]
pub struct TraceAnalysis {
    pub id: String,
    pub workflow_id: String,
    pub priority: serde_json::Value,
    pub topics: serde_json::Value,
    pub prompts: serde_json::Value,
    pub grader_hints: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for `PUT /trace-analysis`. All fields are optional — only non-null
/// fields are updated (upsert semantics).
#[derive(Debug, Deserialize, Clone)]
#[serde(crate = "serde")]
pub struct NewTraceAnalysis {
    pub priority: Option<serde_json::Value>,
    pub topics: Option<serde_json::Value>,
    pub prompts: Option<serde_json::Value>,
    #[serde(rename = "graderHints")]
    pub grader_hints: Option<serde_json::Value>,
}

impl NewTraceAnalysis {
    pub fn into_db_new(self, workflow_id: String) -> DbNewTraceAnalysis {
        DbNewTraceAnalysis {
            id: Uuid::new_v4().to_string(),
            workflow_id,
            priority_json: self.priority.map(|v| v.to_string()),
            topics_json: self.topics.map(|v| v.to_string()),
            prompts_json: self.prompts.map(|v| v.to_string()),
            grader_hints_json: self.grader_hints.map(|v| v.to_string()),
        }
    }
}

impl TraceAnalysis {
    pub fn from_db(row: DbTraceAnalysis) -> Self {
        let parse = |s: Option<String>| -> serde_json::Value {
            s.and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Null)
        };
        Self {
            id: row.id,
            workflow_id: row.workflow_id,
            priority: parse(row.priority_json),
            topics: parse(row.topics_json),
            prompts: parse(row.prompts_json),
            grader_hints: parse(row.grader_hints_json),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
