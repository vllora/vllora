use std::fmt::Display;

use chrono::{Months, Utc};
use redis::{aio::ConnectionManager, FromRedisValue, ToRedisArgs};

pub mod connection_manager;

use chrono::Datelike;
use chrono::Timelike;

pub fn get_hour_key(company_id: &str, key: &str) -> String {
    let hour = Utc::now().date_naive().format("%Y-%m-%d%h");
    format!("{company_id}:{key}:{hour}")
}

pub fn get_daily_key(company_id: &str, key: &str) -> String {
    let today = Utc::now().date_naive().format("%Y-%m-%d");
    format!("{company_id}:{key}:{today}")
}

pub fn get_monthly_key(company_id: &str, key: &str) -> String {
    let month = Utc::now().date_naive().format("%Y-%m");
    format!("{company_id}:{key}:{month}")
}

pub fn get_total_key(company_id: &str, key: &str) -> String {
    format!("{company_id}:{key}:total")
}

pub enum LimitPeriod {
    Hour,
    Day,
    Month,
    Total,
}

impl LimitPeriod {
    pub fn get_seconds_until_refresh(&self) -> Option<i64> {
        match self {
            LimitPeriod::Hour => {
                // Calculate seconds until end of hour
                let now = Utc::now();
                let next_hour = (now + chrono::Duration::hours(1))
                    .with_minute(0)
                    .unwrap()
                    .with_second(0)
                    .unwrap()
                    .naive_utc();
                Some((next_hour - now.naive_utc()).num_seconds())
            }
            LimitPeriod::Day => {
                // Calculate seconds until end of day
                let now = Utc::now();
                let end_of_day = (now + chrono::Duration::days(1))
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
                Some((end_of_day - now.naive_utc()).num_seconds())
            }
            LimitPeriod::Month => {
                let now = Utc::now();
                let end_of_day = now
                    .date_naive()
                    .checked_add_months(Months::new(1))
                    .unwrap()
                    .with_day(1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
                Some((end_of_day - now.naive_utc()).num_seconds())
            }
            LimitPeriod::Total => None,
        }
    }

    pub fn get_key(&self, identifier: &str, key: &str) -> String {
        match self {
            LimitPeriod::Hour => get_hour_key(identifier, key),
            LimitPeriod::Day => get_daily_key(identifier, key),
            LimitPeriod::Month => get_monthly_key(identifier, key),
            LimitPeriod::Total => get_total_key(identifier, key),
        }
    }
}

pub async fn increment_and_get_value<T: FromRedisValue + Display + ToRedisArgs>(
    client: &mut ConnectionManager,
    refresh_rate: LimitPeriod,
    identifier: &str,
    key: &str,
    incr_by: T,
) -> Result<T, redis::RedisError> {
    let key = refresh_rate.get_key(identifier, key);

    let mut pipe = redis::pipe();
    let seconds_until_eod = refresh_rate.get_seconds_until_refresh();

    pipe.atomic().incr(&key, incr_by);

    if let Some(expire) = seconds_until_eod {
        pipe.expire(&key, expire).ignore();
    }

    let (current_value,): (T,) = pipe.query_async(client).await?;

    Ok(current_value)
}

pub async fn get_value<T: FromRedisValue + std::fmt::Debug>(
    client: &mut ConnectionManager,
    refresh_rate: LimitPeriod,
    identifier: &str,
    key: &str,
) -> Result<T, redis::RedisError> {
    let key = refresh_rate.get_key(identifier, key);

    let mut pipe = redis::pipe();

    let (current_value,): (T,) = pipe.get(&key).query_async(client).await?;

    Ok(current_value)
}
