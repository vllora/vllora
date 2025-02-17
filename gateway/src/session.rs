use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub const LANGDB_API_URL: &str = "https://api.us-east-1.langdb.ai/v1";
pub const LANGDB_UI_URL: &str = "https://app.langdb.ai";

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
                println!("Failed to parse session response: {:?}", err);
                return Ok(());
            }
        },
        Err(err) => {
            println!("Failed to start session: {:?}. Please try again.", err);
            return Ok(());
        }
    };

    let url = format!(
        "{}/login?session_id={}",
        get_ui_url(),
        session_response.session_id
    );
    println!("Opening {} in your browser...", url);
    match open::that(url) {
        Ok(_) => (),
        Err(err) => println!(
            "Failed to open URL: {:?}. You can manually open it in your browser.",
            err
        ),
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
                let credentials_dir = format!("{}/.langdb", home_dir);
                std::fs::create_dir_all(&credentials_dir).unwrap_or_default();

                let credentials_file = format!("{}/credentials.yaml", credentials_dir);
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
