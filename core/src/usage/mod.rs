use chrono::{Months, Utc};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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

#[derive(Debug)]
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

#[derive(Default, Clone)]
pub struct InMemoryStorage {
    counters: Arc<RwLock<HashMap<String, AtomicU64>>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn increment_and_get_value(
        &self,
        refresh_rate: LimitPeriod,
        identifier: &str,
        key: &str,
        incr_by: f64,
    ) -> f64 {
        let key = refresh_rate.get_key(identifier, key);
        let mut counters = self.counters.write();

        let counter = counters
            .entry(key.clone())
            .or_insert_with(|| AtomicU64::new(0));

        // Convert f64 to bits for atomic storage
        let current_bits = counter.load(Ordering::SeqCst);
        let current = f64::from_bits(current_bits);
        let new_value = current + incr_by;
        let new_bits = new_value.to_bits();
        counter.store(new_bits, Ordering::SeqCst);

        // If there's an expiry period, spawn a task to remove the counter after that time
        if let Some(expire_seconds) = refresh_rate.get_seconds_until_refresh() {
            let counters: Arc<RwLock<HashMap<String, AtomicU64>>> = Arc::clone(&self.counters);
            let key = key.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(expire_seconds as u64)).await;
                counters.write().remove(&key);
            });
        }

        new_value
    }

    pub async fn get_value(
        &self,
        refresh_rate: LimitPeriod,
        identifier: &str,
        key: &str,
    ) -> Option<f64> {
        let key = refresh_rate.get_key(identifier, key);
        let counters = self.counters.read();

        counters
            .get(&key)
            .map(|counter| f64::from_bits(counter.load(Ordering::SeqCst)))
    }

    pub fn get_counters(&self) -> (f64, f64, f64) {
        let counters = self.counters.read();
        let total = counters
            .get("total:tokens:total")
            .map(|c| f64::from_bits(c.load(Ordering::SeqCst)))
            .unwrap_or(0.0);
        let prompt = counters
            .get("total:prompt_tokens:total")
            .map(|c| f64::from_bits(c.load(Ordering::SeqCst)))
            .unwrap_or(0.0);
        let completion = counters
            .get("total:completion_tokens:total")
            .map(|c| f64::from_bits(c.load(Ordering::SeqCst)))
            .unwrap_or(0.0);
        (total, prompt, completion)
    }
}
