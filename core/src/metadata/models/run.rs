use diesel::deserialize::{self, FromSql};
#[cfg(feature = "postgres")]
use diesel::pg::sql_types::Uuid;
#[cfg(feature = "postgres")]
use diesel::pg::{Pg, PgValue};
use diesel::prelude::*;
#[cfg(feature = "postgres")]
use diesel::sql_types::Jsonb;
#[cfg(feature = "sqlite")]
use diesel::sql_types::Text;
use diesel::sql_types::{BigInt, Float, Nullable};
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
#[cfg(feature = "sqlite")]
use diesel::sqlite::SqliteValue;
use diesel::{AsExpression, FromSqlRow};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::ops::Deref;
#[cfg(feature = "sqlite")]
use std::str::FromStr;

#[derive(Debug, Clone, Default, Serialize, Deserialize, FromSqlRow, AsExpression)]
#[serde(transparent)]
#[cfg_attr(not(feature = "postgres"), diesel(sql_type = Text))]
#[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
pub struct JsonVec(pub Vec<String>);

impl JsonVec {
    pub fn into_vec(self) -> Vec<String> {
        self.0
    }
}

impl Deref for JsonVec {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "sqlite")]
impl FromSql<Text, Sqlite> for JsonVec {
    fn from_sql(
        bytes: <Sqlite as diesel::backend::Backend>::RawValue<'_>,
    ) -> deserialize::Result<Self> {
        let raw = <String as FromSql<Text, Sqlite>>::from_sql(bytes)?;
        let vec = serde_json::from_str(&raw)?;
        Ok(JsonVec(vec))
    }
}

#[derive(
    Debug, Clone, Copy, FromSqlRow, AsExpression, Hash, Eq, PartialEq, Serialize, Deserialize,
)]
#[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
#[cfg_attr(feature = "postgres", diesel(sql_type = Uuid))]
pub struct UUID(pub uuid::Uuid);

// Small function to easily initialize our uuid
impl UUID {
    pub fn random() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

// Allow easy conversion from UUID to the wanted uuid::Uuid
impl From<UUID> for uuid::Uuid {
    fn from(s: UUID) -> Self {
        s.0
    }
}

impl Display for UUID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(feature = "sqlite")]
// Convert binary data from SQLite to a UUID
impl FromSql<Text, Sqlite> for UUID {
    fn from_sql(bytes: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let bytes = <String as FromSql<Text, Sqlite>>::from_sql(bytes)?;
        let uuid = uuid::Uuid::from_str(&bytes).map_err(|_| "Invalid UUID")?;
        Ok(UUID(uuid))
    }
}

#[cfg(feature = "postgres")]
// Convert binary data from SQLite to a UUID
impl FromSql<Uuid, Pg> for UUID {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        let uuid = <uuid::Uuid as FromSql<Uuid, Pg>>::from_sql(bytes)?;
        Ok(UUID(uuid))
    }
}

#[cfg(feature = "postgres")]
impl FromSql<Jsonb, Pg> for JsonVec {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        let value = <serde_json::Value as FromSql<Jsonb, Pg>>::from_sql(bytes)?;
        let array = match value.as_array() {
            Some(array) => array,
            None => return Err("expected json array when decoding JsonStringVec".into()),
        };
        let mut result = Vec::with_capacity(array.len());
        for item in array {
            match item {
                serde_json::Value::String(s) => result.push(s.clone()),
                other => result.push(other.to_string()),
            }
        }
        Ok(JsonVec(result))
    }
}

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
