use diesel::{sql_query, RunQueryDsl};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vllora_core::metadata::models::session::DbSession;
use vllora_core::{metadata::pool::DbPool, types::LANGDB_API_URL};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub api_key: String,
}

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

pub fn ping_session(session_id: String) {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        client
            .get(format!("{}/session/ping/{}", get_api_url(), session_id))
            .send()
            .await
    });
}
