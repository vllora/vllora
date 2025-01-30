use redis::aio::{ConnectionManager, ConnectionManagerConfig};

pub async fn redis_connection_manager(redis_url: String) -> redis::RedisResult<ConnectionManager> {
    let client = redis::Client::open(redis_url)?;

    let mut config = ConnectionManagerConfig::new();
    config = config.set_number_of_retries(1);
    let manager = client.get_connection_manager_with_config(config).await?;

    Ok(manager)
}
