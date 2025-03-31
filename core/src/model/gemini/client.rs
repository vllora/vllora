use crate::{error::GatewayError, GatewayResult};

use super::types::{
    CountTokensRequest, CountTokensResponse, GenerateContentRequest, GenerateContentResponse,
    ModelsResponse,
};
use futures::Stream;
use reqwest::StatusCode;
use reqwest_eventsource::{Error, EventSource};
use serde::Serialize;
use serde_json::Value;
use tokio_stream::StreamExt;

const API_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

// Reference: https://github.com/google/generative-ai-docs/blob/main/site/en/gemini-api/docs/get-started/rest.ipynb
#[derive(Clone)]
pub struct Client {
    /// The API key.
    api_key: String,
    /// Internal HTTP client.
    client: reqwest::Client,
}

enum Method {
    Post,
    Get,
}
impl Client {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    async fn make_request<T: serde::de::DeserializeOwned, P: Serialize>(
        &self,
        path: &str,
        payload: Option<P>,
        method: Method,
    ) -> GatewayResult<T> {
        let url = format!("{API_URL}{path}?key={}", self.api_key);

        let resp = match method {
            Method::Get => self.client.get(url),
            Method::Post => self.client.post(url),
        };
        let resp = if let Some(p) = &payload {
            resp.json(p)
        } else {
            resp
        };

        let resp = resp
            .send()
            // .header("x-api-key", self.api_key.as_str())
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let msg = resp.text().await?;
            let p = if let Some(p) = payload {
                serde_json::to_string(&p).unwrap()
            } else {
                String::new()
            };
            tracing::error!(target: "gemini", "{msg}. Payload: {p}");

            return Err(GatewayError::CustomError(format!(
                "Request failed with status: {}",
                status
            )));
        }

        let text = resp.text().await?;
        let response = serde_json::from_str::<T>(&text).map_err(|e| {
            tracing::error!(target: "gemini", "Response deserialize failed. Response: {text}");
            GatewayError::CustomError(e.to_string())
        })?;
        Ok(response)
    }

    pub fn static_models() -> Vec<&'static str> {
        vec![
            "gemini-1.5-pro-exp-0801",
            "gemini-1.5-flash",
            "gemini-1.5-pro",
            "gemini-pro",
        ]
    }
    pub async fn models(&self) -> GatewayResult<ModelsResponse> {
        let url = "".to_string();
        self.make_request(&url, None::<Value>, Method::Get).await
    }
    pub async fn count_tokens(
        &self,
        model_name: &str,
        payload: CountTokensRequest,
    ) -> GatewayResult<CountTokensResponse> {
        let url = format!("/{model_name}:countTokens");
        self.make_request(&url, Some(&payload), Method::Post).await
    }

    pub async fn invoke(
        &self,
        model_name: &str,
        payload: GenerateContentRequest,
    ) -> GatewayResult<GenerateContentResponse> {
        let invoke_url = format!("/{model_name}:generateContent");
        tracing::debug!(target: "gemini", "Invoking model: {model_name} on {invoke_url} with payload: {:?}", payload);
        self.make_request(&invoke_url, Some(&payload), Method::Post)
            .await
    }

    pub async fn stream(
        &self,
        model_name: &str,
        payload: GenerateContentRequest,
    ) -> GatewayResult<impl Stream<Item = Result<Option<GenerateContentResponse>, GatewayError>>>
    {
        let stream_url = format!(
            "{API_URL}/{model_name}:streamGenerateContent?alt=sse&key={}",
            self.api_key
        );
        tracing::debug!(target: "gemini", "Invoking model: {model_name} on {stream_url} with payload: {}", serde_json::to_string(&payload).unwrap());
        let request = self.client.post(&stream_url).json(&payload);
        // Delegate the request to the EventSource.
        let event_source =
            EventSource::new(request).map_err(|e| GatewayError::CustomError(e.to_string()))?;

        Ok(futures::stream::unfold(
            event_source,
            |mut event_source| async {
                match event_source.next().await {
                    Some(Ok(reqwest_eventsource::Event::Message(msg))) => {
                        let chunk = match serde_json::from_str::<GenerateContentResponse>(&msg.data)
                        {
                            Ok(chunk) => chunk,
                            Err(e) => {
                                tracing::error!(target: "gemini", "{e:?}");
                                return Some((
                                    Err(GatewayError::CustomError(e.to_string())),
                                    event_source,
                                ));
                            }
                        };
                        Some((Ok(Some(chunk)), event_source))
                    }
                    Some(Ok(reqwest_eventsource::Event::Open)) => {
                        tracing::debug!(target: "gemini", "CONNECTION OPENED");
                        Some((Ok(None), event_source))
                    }
                    Some(Err(Error::StreamEnded)) => None,
                    Some(Err(e)) => {
                        let err_str = e.to_string();
                        let err_str = match e {
                            reqwest_eventsource::Error::InvalidStatusCode(_, r) => {
                                let status = r.status();
                                let error = r.text().await.unwrap_or(err_str);

                                tracing::error!(target: "gemini", "Gemini error: {error}");

                                if status == StatusCode::NOT_FOUND {
                                    "Gemini model not found".to_string()
                                } else {
                                    error
                                }
                            }
                            _ => err_str,
                        };

                        Some((Err(GatewayError::CustomError(err_str)), event_source))
                    }
                    _ => None,
                }
            },
        ))
    }
}
