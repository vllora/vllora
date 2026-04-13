use crate::metadata::schema::trace_bundles;
use diesel::{Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
use uuid::Uuid;

/// Raw DB row for a stored OTel GenAI trace bundle.
///
/// `raw_blob` holds the full semconv spans JSON as-stored bytes. Rollup columns
/// (`span_count`, `tool_names`, `model_names`) are computed at insert-time from
/// the blob so list-views don't have to re-parse it.
#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = trace_bundles)]
#[serde(crate = "serde")]
pub struct DbTraceBundle {
    pub id: String,
    pub workflow_id: String,
    pub name: String,
    pub span_count: i32,
    /// JSON-encoded `Vec<String>` of distinct tool names observed in the blob.
    pub tool_names: String,
    /// JSON-encoded `Vec<String>` of distinct model names observed in the blob.
    pub model_names: String,
    pub raw_blob: Vec<u8>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = trace_bundles)]
pub struct DbNewTraceBundle {
    pub id: String,
    pub workflow_id: String,
    pub name: String,
    pub span_count: i32,
    pub tool_names: String,
    pub model_names: String,
    pub raw_blob: Vec<u8>,
}

/// API-facing representation. `raw_blob` is parsed back into JSON so clients
/// don't have to base64-decode it. `tool_names` / `model_names` are parsed back
/// into `Vec<String>` for convenience.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "serde")]
pub struct TraceBundle {
    pub id: String,
    pub workflow_id: String,
    pub name: String,
    pub span_count: i32,
    pub tool_names: Vec<String>,
    pub model_names: Vec<String>,
    /// Full semconv spans blob, parsed as JSON.
    pub semconv_spans: JsonValue,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for `POST /trace-bundles`. Rollup metadata is computed from
/// `semconv_spans` — callers do not supply it.
#[derive(Debug, Deserialize, Clone)]
#[serde(crate = "serde")]
pub struct NewTraceBundle {
    pub workflow_id: String,
    pub name: String,
    pub semconv_spans: JsonValue,
}

/// Extract distinct tool names and model names from a semconv spans JSON array.
///
/// Per OTel GenAI semantic conventions (≥ v1.38.0), model names appear under
/// `gen_ai.request.model` / `gen_ai.response.model`, and the tool name for
/// `execute_tool` spans appears under `gen_ai.tool.name`. Missing attributes
/// degrade gracefully — we simply don't contribute that span to the rollup.
fn compute_rollups(spans: &JsonValue) -> (i32, Vec<String>, Vec<String>) {
    let spans_array = spans.as_array();
    let span_count = spans_array.map(|a| a.len() as i32).unwrap_or(0);

    let mut tools: BTreeSet<String> = BTreeSet::new();
    let mut models: BTreeSet<String> = BTreeSet::new();

    if let Some(arr) = spans_array {
        for span in arr {
            let attrs = span.get("attributes").unwrap_or(span);
            if let Some(name) = attrs.get("gen_ai.tool.name").and_then(|v| v.as_str()) {
                tools.insert(name.to_string());
            }
            for key in ["gen_ai.request.model", "gen_ai.response.model"] {
                if let Some(name) = attrs.get(key).and_then(|v| v.as_str()) {
                    models.insert(name.to_string());
                }
            }
        }
    }

    (
        span_count,
        tools.into_iter().collect(),
        models.into_iter().collect(),
    )
}

impl NewTraceBundle {
    pub fn into_db_new(self) -> Result<DbNewTraceBundle, serde_json::Error> {
        let (span_count, tool_names, model_names) = compute_rollups(&self.semconv_spans);
        let raw_blob = serde_json::to_vec(&self.semconv_spans)?;
        Ok(DbNewTraceBundle {
            id: Uuid::new_v4().to_string(),
            workflow_id: self.workflow_id,
            name: self.name,
            span_count,
            tool_names: serde_json::to_string(&tool_names)?,
            model_names: serde_json::to_string(&model_names)?,
            raw_blob,
        })
    }
}

impl TraceBundle {
    pub fn from_db(row: DbTraceBundle) -> Result<Self, serde_json::Error> {
        let tool_names: Vec<String> = serde_json::from_str(&row.tool_names)?;
        let model_names: Vec<String> = serde_json::from_str(&row.model_names)?;
        let semconv_spans: JsonValue = if row.raw_blob.is_empty() {
            JsonValue::Null
        } else {
            serde_json::from_slice(&row.raw_blob)?
        };
        Ok(Self {
            id: row.id,
            workflow_id: row.workflow_id,
            name: row.name,
            span_count: row.span_count,
            tool_names,
            model_names,
            semconv_spans,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn computes_rollups_from_flat_spans() {
        let spans = json!([
            {"gen_ai.request.model": "gpt-4o", "gen_ai.tool.name": "search"},
            {"gen_ai.response.model": "gpt-4o", "gen_ai.tool.name": "fetch"},
            {"gen_ai.request.model": "claude-3-5", "gen_ai.tool.name": "search"},
        ]);
        let (count, tools, models) = compute_rollups(&spans);
        assert_eq!(count, 3);
        assert_eq!(tools, vec!["fetch".to_string(), "search".to_string()]);
        assert_eq!(models, vec!["claude-3-5".to_string(), "gpt-4o".to_string()]);
    }

    #[test]
    fn missing_attributes_do_not_crash() {
        let spans = json!([{}, {"something": "else"}]);
        let (count, tools, models) = compute_rollups(&spans);
        assert_eq!(count, 2);
        assert!(tools.is_empty());
        assert!(models.is_empty());
    }
}
