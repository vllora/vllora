use serde::de::DeserializeOwned;

use super::{
    error::{HttpTransportError, QueryError},
    DatabaseTransport, QueryResponse,
};

type Result<T> = std::result::Result<T, QueryError>;

pub struct DatabaseExecutor {}

impl DatabaseExecutor {
    pub fn parse_response<T>(response: &[u8]) -> Result<QueryResponse<T>>
    where
        T: DeserializeOwned,
    {
        let json_data: QueryResponse<T> = serde_json::from_slice(response)
            .map_err(|e| QueryError::TransportError(Box::new(HttpTransportError::Serde(e))))?;
        Ok(json_data)
    }

    pub async fn parse_execute<T>(t: &dyn DatabaseTransport, sql: &str) -> Result<QueryResponse<T>>
    where
        T: DeserializeOwned,
    {
        let response = t.execute_binary(sql).await?;
        let response = Self::parse_response(&response)?;
        Ok(response)
    }

    pub async fn fetch_one<T>(t: &dyn DatabaseTransport, sql: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = t.execute_binary(sql).await?;
        let mut response: QueryResponse<T> = Self::parse_response(&response)?;
        if response.data.is_empty() {
            Err(QueryError::RowNotFound)
        } else {
            let item = response.data.remove(0);
            Ok(item)
        }
    }

    pub async fn fetch_all<T>(t: &dyn DatabaseTransport, sql: &str) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let response = t.execute_binary(sql).await?;
        let response: QueryResponse<T> = Self::parse_response(&response)?;
        Ok(response.data)
    }
}
