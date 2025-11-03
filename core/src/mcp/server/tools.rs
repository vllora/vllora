use rmcp::schemars;
use serde::{Deserialize, Serialize};

use crate::types::traces::Operation;

const MAX_LIMIT: i64 = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]

pub struct ListTracesRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "The operation names. Available operations: run, agent, task, tools, openai, anthropic, bedrock, gemini, cloud_api_invoke, api_invoke, model_call"
    )]
    pub operation_names: Option<Vec<Operation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "The parent span IDs.")]
    pub parent_span_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "The minimum start time in microseconds")]
    pub start_time_min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "The maximum start time in microseconds")]
    pub start_time_max: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "The time range filter. Available filters: last_5_minutes, last_15_minutes, last_30_minutes, last_1_hour, last_6_hours, last_1_day, last_7_days, last_30_days, last_90_days, last_180_days, last_365_days"
    )]
    pub range_filter: Option<RangeFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "The limit of the number of traces to return. Default is 100. Maximum is 1000."
    )]
    #[schemars(range(min = 1, max = 1000))]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "The offset of the traces to return. Default is 0.")]
    pub offset: Option<i64>,
}

impl ListTracesRequest {
    pub fn get_limit(&self) -> i64 {
        self.limit.unwrap_or(MAX_LIMIT).clamp(1, MAX_LIMIT)
    }

    pub fn get_offset(&self) -> i64 {
        self.offset.unwrap_or(0)
    }

    pub fn get_range(&self) -> Option<(i64, i64)> {
        let now = chrono::Utc::now().timestamp_micros();
        if let Some(range_filter) = &self.range_filter {
            let multiplier = 1_000_000;
            let duration = match range_filter {
                RangeFilter::Last5Minutes => 5 * 60,
                RangeFilter::Last15Minutes => 15 * 60,
                RangeFilter::Last30Minutes => 30 * 60,
                RangeFilter::Last1Hour => 60 * 60,
                RangeFilter::Last6Hours => 6 * 60 * 60,
                RangeFilter::Last1Day => 24 * 60 * 60,
                RangeFilter::Last7Days => 7 * 24 * 60 * 60,
                RangeFilter::Last30Days => 30 * 24 * 60 * 60,
                RangeFilter::Last90Days => 90 * 24 * 60 * 60,
                RangeFilter::Last180Days => 180 * 24 * 60 * 60,
                RangeFilter::Last365Days => 365 * 24 * 60 * 60,
            };

            return Some((now.saturating_sub(duration * multiplier), now));
        }

        match (self.start_time_min, self.start_time_max) {
            (Some(start_time_min), Some(start_time_max)) => Some((start_time_min, start_time_max)),
            (Some(start_time_min), None) => Some((start_time_min, now)),
            (None, Some(start_time_max)) => Some((now - 60 * 60 * 1_000_000, start_time_max)),
            (None, None) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RangeFilter {
    #[serde(rename = "last_5_minutes")]
    Last5Minutes,
    #[serde(rename = "last_15_minutes")]
    Last15Minutes,
    #[serde(rename = "last_30_minutes")]
    Last30Minutes,
    #[serde(rename = "last_1_hour")]
    Last1Hour,
    #[serde(rename = "last_6_hours")]
    Last6Hours,
    #[serde(rename = "last_1_day")]
    Last1Day,
    #[serde(rename = "last_7_days")]
    Last7Days,
    #[serde(rename = "last_30_days")]
    Last30Days,
    #[serde(rename = "last_90_days")]
    Last90Days,
    #[serde(rename = "last_180_days")]
    Last180Days,
    #[serde(rename = "last_365_days")]
    Last365Days,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_range_serialization() {
        let time_range_filter = ListTracesRequest {
            limit: Some(100),
            offset: Some(0),
            run_ids: None,
            thread_ids: None,
            operation_names: None,
            parent_span_ids: None,
            start_time_min: None,
            start_time_max: None,
            range_filter: Some(RangeFilter::Last5Minutes),
        };

        let v = serde_json::to_string(&time_range_filter).unwrap();

        let expected = r#"{"limit": 100,"offset": 0,"range_filter": "last_5_minutes"}"#;
        assert_eq!(v, expected);
    }
}
