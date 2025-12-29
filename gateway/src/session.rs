use diesel::{sql_query, RunQueryDsl};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Deserialize;
use uuid::Uuid;
use vllora_core::metadata::models::session::DbSession;
use vllora_core::{metadata::pool::DbPool, types::LANGDB_API_URL};
use mid;

pub fn get_api_url() -> String {
    std::env::var("LANGDB_API_URL").unwrap_or_else(|_| LANGDB_API_URL.to_string())
}

pub async fn fetch_session_id(pool: DbPool) -> DbSession {
    let connection = pool.get();

    match connection {
        Ok(mut connection) => {
            let result = sql_query("SELECT id FROM sessions LIMIT 1")
                .get_result::<DbSession>(&mut connection);

            result.unwrap_or(DbSession {
                id: Uuid::new_v4().to_string(),
            })
        }
        Err(_e) => DbSession {
            id: Uuid::new_v4().to_string(),
        },
    }
}

pub fn device_id() -> Result<String, String> {
    mid::get("vllora-device-id")
        .map_err(|e| e.to_string())
}

pub fn check_version(session_id: String) {
    tokio::spawn(async move {
        let version = format!("v{}", env!("CARGO_PKG_VERSION"));
        let mut headers = HeaderMap::new();
        headers.insert("X-vllora-version", HeaderValue::from_str(&version).unwrap());

        match device_id() {
            Ok(device_id) => {
                headers.insert(
                    "X-vllora-device-id",
                    HeaderValue::from_str(&device_id).unwrap(),
                );
            }
            Err(e) => {
                tracing::error!("Failed to get device id: {}", e);
            }
        }

        if let Some(latest) = fetch_latest_release_version().await {
            if let Ok(v) = HeaderValue::from_str(&latest) {
                headers.insert("X-vllora-latest", v);
            }

            if version != latest {
                println!("New version available: {latest}. Please update to the latest version");
                println!("Do upgrade with \x1b[32mbrew upgrade vllora\x1b[0m");
            }
        }

        let client = reqwest::Client::new();
        let _ = client
            .get(format!("{}/session/ping/{}", get_api_url(), session_id))
            .headers(headers)
            .send()
            .await;
    });
}

async fn fetch_latest_release_version() -> Option<String> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
    }

    let client = reqwest::Client::builder()
        .user_agent(format!("vllora/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()?;

    let response = client
        .get("https://api.github.com/repos/vllora/vllora/releases/latest")
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let release: Release = response.json().await.ok()?;
    Some(release.tag_name)
}
