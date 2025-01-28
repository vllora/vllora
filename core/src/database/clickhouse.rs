use crate::database::ssh_tunnel::{cleanup_tunnel, create_tunnel};
use crate::database::{DatabaseTransport, MAX_LIMIT};
use crate::types::db_connection::SshSettings;
use clickhouse::rowbinary::deserialize_from;
use clust::futures_core::Stream;
use futures::{StreamExt, TryStreamExt};
use opentelemetry::propagation::{Injector, TextMapPropagator};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use reqwest::header::{HeaderMap, HeaderName};
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fmt::Display;
use std::pin::Pin;
use std::str::FromStr;
use tokio::io::BufReader;
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;
use tracing::log::debug;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::error::{HttpTransportError, QueryError};
use super::user::ClickhouseDataUserEnvVariables;

type Result<T> = std::result::Result<T, QueryError>;

#[derive(Default, Clone)]
pub enum ClickhouseFormat {
    #[default]
    JSON,
    RowBinary,
    CompactJSON,
    CompactJSONWithNames,
}

impl ClickhouseFormat {
    fn as_str(&self) -> &str {
        match self {
            ClickhouseFormat::JSON => "JSON",
            ClickhouseFormat::CompactJSON => "JSONCompactEachRow",
            ClickhouseFormat::RowBinary => "RowBinary",
            ClickhouseFormat::CompactJSONWithNames => "JSONCompactEachRowWithNames",
        }
    }
}

impl Display for ClickhouseFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone)]
pub struct ClickhouseHttp {
    pub url: String,
    pub database: String,
    pub(crate) format: ClickhouseFormat,
    pub ssh: Option<SshSettings>,
}

fn get_url() -> String {
    std::env::var(ClickhouseDataUserEnvVariables::URL).unwrap_or("localhost:8123".to_string())
}

fn get_protocol() -> String {
    std::env::var(ClickhouseDataUserEnvVariables::PROTOCOL).unwrap_or("http".to_string())
}

fn get_default_user() -> String {
    std::env::var(ClickhouseDataUserEnvVariables::USER).unwrap_or("default".to_string())
}

fn get_default_password() -> Option<String> {
    std::env::var(ClickhouseDataUserEnvVariables::PASSWORD).ok()
}

fn get_user_string(user: &str, password: Option<String>) -> String {
    match password {
        Some(password) => format!("{user}:{password}", user = user, password = password),
        None => user.to_string(),
    }
}

impl Default for ClickhouseHttp {
    fn default() -> Self {
        Self::new(
            &get_default_user(),
            get_default_password(),
            "default",
            None,
            None,
            None,
        )
    }
}

impl ClickhouseHttp {
    pub fn new(
        user: &str,
        password: Option<String>,
        database: &str,
        host: Option<String>,
        protocol: Option<String>,
        ssh: Option<SshSettings>,
    ) -> Self {
        Self {
            url: format!(
                "{protocol}://{user_string}@{host}",
                protocol = protocol.unwrap_or(get_protocol()),
                user_string = get_user_string(user, password),
                host = host.unwrap_or(get_url())
            ),
            format: Default::default(),
            database: database.to_string(),
            ssh,
        }
    }

    pub fn with_url(&mut self, url: &str) -> &mut Self {
        self.url = url.to_string();
        self
    }

    pub fn root() -> Self {
        Self::default()
    }

    pub fn set_format(&mut self, format: ClickhouseFormat) {
        self.format = format;
    }

    pub fn parse_row_binary<T>(&self, buf: Vec<u8>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut temp_buf = [0; 1024];
        let buf = bytes::Bytes::from(buf);
        let data: T = deserialize_from(buf, &mut temp_buf).unwrap();
        Ok(data)
    }

    async fn do_execute(&self, sql: &str, body: Option<String>) -> Result<String> {
        let response = self.execute_inner(sql, body, None).await?;
        let status = response.status();
        let text = response.text().await.map_err(HttpTransportError::Reqwest)?;
        if status.is_success() {
            Ok(text)
        } else {
            Err(QueryError::ClickhouseError(format!(
                "Sql: {sql}\nError: {text}"
            )))
        }
    }

    async fn execute_inner(
        &self,
        sql: &str,
        body: Option<String>,
        output_format: Option<ClickhouseFormat>,
    ) -> Result<Response> {
        let http_client_builder = reqwest::Client::builder().danger_accept_invalid_certs(true);
        let client = http_client_builder.build()?;
        let sql = sql.to_string();

        let (url, format, database) = (&self.url, &self.format, &self.database);

        debug!(target: "clickhouse_transport", "[{}] {sql}", url);

        let port = self
            .url
            .split(':')
            .last()
            .map(|p| p.parse::<u16>().unwrap_or(443))
            .unwrap_or(443);

        let ssh_session = match &self.ssh {
            Some(ssh) => Some(
                create_tunnel(ssh.to_owned(), port)
                    .await
                    .map_err(QueryError::SshConnectionError)?,
            ),
            _ => None,
        };

        let mut headers = HeaderMapInjector::default();
        let context = tracing::Span::current().context();
        TraceContextPropagator::new().inject_context(&context, &mut headers);

        let req = client
            .post(url)
            .query(&[
                (
                    "default_format",
                    output_format
                        .map_or(format.to_string(), |f| f.to_string())
                        .as_str(),
                ),
                ("database", database.as_str()),
                (
                    "limit",
                    std::env::var("CLICKHOUSE_MAX_LIMIT")
                        .unwrap_or(MAX_LIMIT.to_string())
                        .as_str(),
                ),
            ])
            .headers(headers.into_inner());

        // Send query as body if body is None
        let req = if let Some(body) = body {
            req.query(&[("query", sql.as_str())]).body(body)
        } else {
            req.body(sql)
        };

        let response = req
            .send()
            .await
            .map_err(|e| QueryError::TransportError(Box::new(HttpTransportError::Reqwest(e))));

        if let Some(session) = ssh_session {
            cleanup_tunnel(session.0, &session.1, session.2, port).await?;
        }

        response
    }
}

#[derive(Default)]
struct HeaderMapInjector(HeaderMap);

impl HeaderMapInjector {
    fn into_inner(self) -> HeaderMap {
        self.0
    }
}

impl Injector for HeaderMapInjector {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(
            HeaderName::from_str(key).unwrap(),
            value.try_into().unwrap(),
        );
    }
}

#[async_trait::async_trait]
impl DatabaseTransport for ClickhouseHttp {
    async fn test_connection(&self) -> Result<()> {
        let sql = "SELECT 1";
        self.execute(sql).await.map(|_| ())
    }

    async fn insert_values(
        &self,
        table_name: &str,
        columns: &[&str],
        body: Vec<Vec<Value>>,
    ) -> Result<String> {
        let columns_str = columns.join(",");

        let body = body
            .iter()
            .map(|b| serde_json::to_string(&b).unwrap())
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "INSERT INTO {}({}) FORMAT JSONCompactEachRow",
            table_name, columns_str
        );

        self.do_execute(&query, Some(body)).await
    }

    async fn execute_binary(&self, sql: &str) -> Result<Vec<u8>> {
        let response = self.execute_inner(sql, None, None).await?;

        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(HttpTransportError::Reqwest)?;

        if status.is_success() {
            Ok(body.to_vec())
        } else {
            Err(QueryError::ClickhouseError(
                std::str::from_utf8(&body).unwrap().to_string(),
            ))
        }
    }

    async fn query_stream(
        &self,
        sql: &str,
    ) -> Result<(
        Vec<String>,
        Pin<Box<dyn Stream<Item = Result<Vec<Value>>> + Send>>,
    )> {
        let client = Client::new();
        let sql = sql.to_string();
        let format = "JSONCompactEachRowWithNames".to_string();

        let res = client
            .post(&self.url)
            .query(&[("default_format", format)])
            .body(sql)
            .send()
            .await
            .map_err(HttpTransportError::Reqwest)?;

        let byte_stream = res
            .bytes_stream()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionReset, e));

        let stream_reader = StreamReader::new(byte_stream);
        let reader = BufReader::new(stream_reader);
        let mut lines_stream = FramedRead::new(reader, LinesCodec::new());

        // Read the first line to get the headers
        let headers = if let Some(first_line) = lines_stream.next().await {
            let first_line = first_line.map_err(HttpTransportError::LinesCodec)?;
            let headers: Vec<String> =
                serde_json::from_str(&first_line).map_err(HttpTransportError::Serde)?;
            headers
        } else {
            return Err(HttpTransportError::NoHeaders.into());
        };

        let stream = lines_stream
            .map(|line_res| {
                line_res
                    .map_err(HttpTransportError::LinesCodec)
                    .and_then(|line| serde_json::from_str(&line).map_err(HttpTransportError::Serde))
            })
            .map_err(QueryError::from);

        Ok((headers, Box::pin(stream)))
    }

    async fn execute(&self, sql: &str) -> Result<String> {
        self.do_execute(sql, None).await
    }

    async fn execute_delete(&self, sql: &str) -> Result<String> {
        self.do_execute(sql, None).await
    }

    async fn execute_compact_json(&self, sql: &str) -> Result<String> {
        let response = self
            .execute_inner(sql, None, Some(ClickhouseFormat::CompactJSONWithNames))
            .await
            .unwrap();
        let status = response.status();
        let text = response.text().await?;
        if status.is_success() {
            Ok(text)
        } else {
            Err(QueryError::ClickhouseError(text))
        }
    }
}
