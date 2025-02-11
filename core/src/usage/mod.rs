use chrono::{Months, Utc};
use parking_lot::RwLock;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Display;
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

impl Display for LimitPeriod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LimitPeriod::Hour => write!(f, "Hour"),
            LimitPeriod::Day => write!(f, "Day"),
            LimitPeriod::Month => write!(f, "Month"),
            LimitPeriod::Total => write!(f, "Total"),
        }
    }
}

impl LimitPeriod {
    pub fn get_seconds_until_refresh(&self) -> Option<i64> {
        match self {
            LimitPeriod::Hour => {
                // Calculate seconds until end of hour
                let now = Utc::now();
                let next_hour = (now + chrono::Duration::minutes(1))
                    // .with_minute(0)
                    // .unwrap()
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

#[derive(Debug, Default, Serialize, Clone)]
pub struct Metrics {
    pub requests: Option<f64>,
    pub input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub total_tokens: Option<f64>,
    pub requests_duration: Option<f64>,
    pub ttft: Option<f64>,
    pub llm_usage: Option<f64>,
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct TimeMetrics {
    pub total: Metrics,
    pub monthly: BTreeMap<String, Metrics>, // Format: "YYYY-MM"
    pub daily: BTreeMap<String, Metrics>,   // Format: "YYYY-MM-DD"
    pub hourly: BTreeMap<String, Metrics>,  // Format: "YYYY-MM-DD-HH"
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct ModelMetrics {
    pub metrics: TimeMetrics,
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct ProviderMetrics {
    pub models: BTreeMap<String, ModelMetrics>,
}

#[derive(Default, Clone)]
pub struct InMemoryStorage {
    counters: Arc<RwLock<BTreeMap<String, AtomicU64>>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            counters: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub async fn increment_and_get_value(
        &self,
        refresh_rate: &LimitPeriod,
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
            let counters: Arc<RwLock<BTreeMap<String, AtomicU64>>> = Arc::clone(&self.counters);
            let key = key.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(expire_seconds as u64)).await;
                counters.write().remove(&key);
            });
        }

        new_value
    }

    pub fn get_value(
        &self,
        refresh_rate: &LimitPeriod,
        identifier: &str,
        key: &str,
    ) -> Option<f64> {
        let key = refresh_rate.get_key(identifier, key);
        let counters = self.counters.read();

        counters
            .get(&key)
            .map(|counter| f64::from_bits(counter.load(Ordering::SeqCst)))
    }

    pub async fn get_all_counters(&self) -> BTreeMap<String, ProviderMetrics> {
        let counters = self.counters.read();
        let mut providers_metrics: BTreeMap<String, ProviderMetrics> = BTreeMap::new();

        for (key, value) in counters.iter() {
            if key.starts_with("default:") {
                continue;
            }

            let parts: Vec<&str> = key.split(':').collect();
            if parts.len() < 2 {
                continue;
            }

            let provider = parts[0].to_string();
            let model = parts[1].to_string();

            if parts.len() <= 2 {
                continue;
            }
            let metric_type = parts[2];

            let provider_metrics = providers_metrics.entry(provider).or_default();
            let model_metrics = provider_metrics.models.entry(model).or_default();

            // Extract time period if present (parts[3] if it exists)
            let period = parts.get(3).map(|&s| s.to_string());

            let metrics = match period {
                Some(p) if p == "total" => &mut model_metrics.metrics.total,
                Some(p) if p.len() == 7 => model_metrics.metrics.monthly.entry(p).or_default(), // YYYY-MM
                Some(p) if p.len() == 10 => model_metrics.metrics.daily.entry(p).or_default(), // YYYY-MM-DD
                Some(p) if p.len() == 13 => model_metrics.metrics.hourly.entry(p).or_default(), // YYYY-MM-DD-HH
                _ => continue,
            };

            let v = Some(f64::from_bits(value.load(Ordering::SeqCst)));

            match metric_type {
                "requests" => metrics.requests = v,
                "input_tokens" => metrics.input_tokens = v,
                "output_tokens" => metrics.output_tokens = v,
                "total_tokens" => metrics.total_tokens = v,
                "requests_duration" => metrics.requests_duration = v,
                "ttft" => metrics.ttft = v,
                "llm_usage" => model_metrics.metrics.total.llm_usage = v,
                _ => {}
            }
        }

        providers_metrics
    }
}
