use crate::session::device_id;
use actix_web::{web, HttpRequest, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use vllora_core::credentials::construct_key_id;
use vllora_core::credentials::KeyStorage;
use vllora_core::metadata::models::session::DbSession;
use vllora_core::types::metadata::project::Project;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackSessionRequest {
    pub email: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackSessionApiRequest {
    pub session_id: String,
    pub email: String,
}

pub fn get_api_url() -> String {
    std::env::var("LANGDB_API_URL")
        .unwrap_or_else(|_| vllora_core::types::LANGDB_API_URL.to_string())
}

pub async fn track_session(
    session: web::Data<DbSession>,
    request: web::Json<TrackSessionRequest>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    // Extract all headers from the incoming request
    let mut headers = HashMap::new();
    for (key, value) in req.headers().iter() {
        if let Ok(value_str) = value.to_str() {
            headers.insert(key.to_string(), value_str.to_string());
        }
    }

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut reqwest_request = client
            .post(format!("{}/session/track", get_api_url()))
            .json(&TrackSessionApiRequest {
                session_id: session.id.clone(),
                email: request.into_inner().email.clone(),
            });

        // Forward part of the headers to the external API call
        for (key, value) in headers {
            if key.starts_with("x-") || key == "User-Agent" {
                reqwest_request = reqwest_request.header(&key, &value);
            }
        }

        if let Ok(device_id) = device_id() {
            reqwest_request = reqwest_request.header("X-vllora-device-id", &device_id);
        }

        tracing::debug!("Sending request to start session");

        match reqwest_request.send().await {
            Ok(response) => {
                let status = response.status();
                if status != 200 {
                    tracing::error!("Failed to start session: {}", status);
                }
            }
            Err(e) => {
                tracing::error!("Failed to start session: {}", e);
            }
        }
    });

    Ok(HttpResponse::Ok().finish())
}

/// Start a new session
/// Calls the external cloud API to create a session and returns the session_id
pub async fn start_session() -> Result<HttpResponse> {
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/session/start", get_api_url()))
        .send()
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to start session: {}", e))
        })?;

    let session = response.json::<SessionResponse>().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!(
            "Failed to parse session response: {}",
            e
        ))
    })?;

    Ok(HttpResponse::Ok().json(session))
}

/// Fetch API key for a session
/// Calls the external cloud API once to fetch the key (no retry logic)
pub async fn fetch_key(
    session_id: web::Path<String>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    project: web::ReqData<Project>,
) -> Result<HttpResponse> {
    let client = reqwest::Client::new();

    let response = client
        .get(format!(
            "{}/session/fetch_key/{}",
            get_api_url(),
            session_id.into_inner()
        ))
        .send()
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to fetch key: {}", e))
        })?;

    // If the API returns 404, forward it
    if response.status() == 404 {
        return Ok(HttpResponse::NotFound().finish());
    }

    let credentials = response.json::<Credentials>().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!(
            "Failed to parse credentials response: {}",
            e
        ))
    })?;

    key_storage
        .insert_key(
            construct_key_id("default", "langdb", &project.id.to_string()),
            Some(serde_json::to_string(&credentials).unwrap_or_default()),
        )
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to insert key: {}", e))
        })?;

    Ok(HttpResponse::Ok().json(credentials))
}
