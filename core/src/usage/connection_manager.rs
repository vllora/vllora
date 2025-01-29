use redis::aio::ConnectionManager;

pub async fn redis_connection_manager(redis_url: String) -> redis::RedisResult<ConnectionManager> {
    let client = redis::Client::open(redis_url)?;
    let manager = client.get_connection_manager().await?;

    Ok(manager)
}
