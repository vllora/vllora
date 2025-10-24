use diesel::{sql_query, RunQueryDsl};
use langdb_core::metadata::models::session::DbSession;
use langdb_core::{
    metadata::pool::DbPool,
    types::{LANGDB_API_URL, LANGDB_UI_URL},
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub api_key: String,
}

pub fn get_ui_url() -> String {
    std::env::var("LANGDB_UI_URL").unwrap_or_else(|_| LANGDB_UI_URL.to_string())
}

pub fn get_api_url() -> String {
    std::env::var("LANGDB_API_URL").unwrap_or_else(|_| LANGDB_API_URL.to_string())
}

pub async fn login() -> Result<(), crate::CliError> {
    let client = reqwest::Client::new();

    // Start session and get UUID
    let session_response = match client
        .post(format!("{}/session/start", get_api_url()))
        .send()
        .await
    {
        Ok(response) => match response.json::<SessionResponse>().await {
            Ok(session) => session,
            Err(err) => {
                println!("Failed to parse session response: {err:?}");
                return Ok(());
            }
        },
        Err(err) => {
            println!("Failed to start session: {err:?}. Please try again.");
            return Ok(());
        }
    };

    let url = format!(
        "{}/login?session_id={}",
        get_ui_url(),
        session_response.session_id
    );
    println!("Opening {url} in your browser...");
    match open::that(url) {
        Ok(_) => (),
        Err(err) => {
            println!("Failed to open URL: {err:?}. You can manually open it in your browser.")
        }
    }

    let start_time = Instant::now();
    let timeout_duration = Duration::from_secs(120); // 2 minutes

    while start_time.elapsed() < timeout_duration {
        let url = format!(
            "{}/session/fetch_key/{}",
            get_api_url(),
            session_response.session_id
        );
        if let Ok(response) = client.get(&url).send().await {
            if let Ok(json) = response.json::<Credentials>().await {
                let home_dir = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
                let credentials_dir = format!("{home_dir}/.langdb");
                std::fs::create_dir_all(&credentials_dir).unwrap_or_default();

                let credentials_file = format!("{credentials_dir}/credentials.yaml");
                let credentials = serde_yaml::to_string(&json).unwrap_or_default();
                std::fs::write(credentials_file, credentials).unwrap_or_default();

                println!("Successfully logged in and saved credentials!");
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    println!("Login timeout after 2 minutes. Please try again.");
    Ok(())
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

pub fn ping_session(session_id: String) {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        client
            .get(format!("{}/session/ping/{}", get_api_url(), session_id))
            .send()
            .await
    });
}
