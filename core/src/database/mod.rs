#[cfg(feature = "clickhouse")]
pub mod clickhouse;
pub mod error;
pub mod executor;
#[cfg(feature = "clickhouse")]
pub mod ssh_tunnel;
pub mod user;

use error::QueryError;
use futures::Stream;
use serde::Deserialize;
use serde_json::Value;
use std::fmt::Debug;
use std::pin::Pin;

#[cfg(feature = "clickhouse")]
const MAX_LIMIT: i32 = 10000;

type Result<T> = std::result::Result<T, QueryError>;

#[async_trait::async_trait]
pub trait DatabaseTransport: Send + Sync + DatabaseTransportClone + 'static {
    async fn execute_binary(&self, sql: &str) -> Result<Vec<u8>>;
    async fn execute(&self, sql: &str) -> Result<String>;
    async fn execute_compact_json(&self, sql: &str) -> Result<String>;
    async fn execute_delete(&self, sql: &str) -> Result<String>;
    async fn insert_values(
        &self,
        table_name: &str,
        columns: &[&str],
        body: Vec<Vec<Value>>,
    ) -> Result<String>;
    async fn query_stream(
        &self,
        sql: &str,
    ) -> Result<(
        Vec<String>,
        Pin<Box<dyn Stream<Item = Result<Vec<Value>>> + Send>>,
    )>;
    async fn test_connection(&self) -> Result<()>;
}

#[derive(Deserialize, Debug, Clone)]
pub struct QueryResponse<T>
where
    T: Sized,
{
    pub meta: Vec<QueryMetaItem>,
    pub data: Vec<T>,
    #[serde(default)]
    pub rows: usize,
    pub statistics: Option<QueryStatistics>,
    pub exception: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct QueryMetaItem {
    pub name: String,
    pub r#type: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct QueryStatistics {
    pub elapsed: f64,
    pub rows_read: usize,
    pub bytes_read: usize,
}

pub trait DatabaseTransportClone {
    fn clone_box(&self) -> Box<dyn DatabaseTransport + Send + Sync>;
}

impl<T> DatabaseTransportClone for T
where
    T: 'static + DatabaseTransport + Clone + Send + Sync,
{
    fn clone_box(&self) -> Box<dyn DatabaseTransport + Send + Sync> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn DatabaseTransport + Send + Sync> {
    fn clone(&self) -> Box<dyn DatabaseTransport + Send + Sync> {
        self.clone_box()
    }
}

impl Clone for Box<dyn DatabaseTransport> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
