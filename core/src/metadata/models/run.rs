#[cfg(feature = "postgres")]
use diesel::pg::sql_types::Uuid;
use diesel::prelude::*;
#[cfg(feature = "postgres")]
use diesel::sql_types::Jsonb;
#[cfg(feature = "sqlite")]
use diesel::sql_types::Text;
use diesel::sql_types::{BigInt, Float, Nullable};
use serde::{Deserialize, Serialize};

use crate::metadata::types::{JsonVec, UUID};

#[derive(Debug, Clone, Serialize, Deserialize, QueryableByName)]
#[cfg_attr(feature = "sqlite", diesel(check_for_backend(diesel::sqlite::Sqlite)))]
#[cfg_attr(feature = "postgres", diesel(check_for_backend(diesel::pg::Pg)))]
pub struct RunUsageInformation {
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Nullable<Text>))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Nullable<Uuid>))]
    pub run_id: Option<UUID>,

    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    thread_ids: JsonVec,

    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    trace_ids: JsonVec,

    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    root_span_ids: JsonVec,

    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    request_models: JsonVec,

    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    used_models: JsonVec,

    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    used_tools: JsonVec,

    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    mcp_template_definition_ids: JsonVec,

    #[diesel(sql_type = BigInt)]
    pub llm_calls: i64,

    #[cfg_attr(feature = "sqlite", diesel(sql_type = Float))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Float))]
    pub cost: f32,

    #[diesel(sql_type = Nullable<BigInt>)]
    pub input_tokens: Option<i64>,

    #[diesel(sql_type = Nullable<BigInt>)]
    pub output_tokens: Option<i64>,

    #[diesel(sql_type = BigInt)]
    pub start_time_us: i64,

    #[diesel(sql_type = BigInt)]
    pub finish_time_us: i64,

    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    errors: JsonVec,
}
