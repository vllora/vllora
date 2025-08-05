use crate::{
    events::JsonValue,
    routing::{metrics::MetricsRepository, MetricsDuration, RouterError},
    usage::Metrics,
};
use rand::seq::IteratorRandom;
use tracing::Span;
use valuable::Valuable;

#[derive(Debug, serde::Serialize, serde::Deserialize, Default, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MetricSelector {
    Requests,
    #[default]
    Latency,
    Ttft,
    Tps,
    ErrorRate,
}

#[derive(PartialEq, Eq)]
pub enum MetricOptimizationDirection {
    Minimize,
    Maximize,
}

impl MetricSelector {
    fn get_optimization_direction(&self) -> MetricOptimizationDirection {
        match self {
            MetricSelector::Requests | MetricSelector::Tps => MetricOptimizationDirection::Maximize,
            _ => MetricOptimizationDirection::Minimize,
        }
    }
}

impl MetricSelector {
    fn get_value(&self, metrics: &Metrics) -> Option<f64> {
        match self {
            MetricSelector::Requests => metrics.requests,
            MetricSelector::Latency => metrics.latency,
            MetricSelector::Ttft => metrics.ttft,
            MetricSelector::Tps => metrics.tps,
            MetricSelector::ErrorRate => metrics.error_rate,
        }
    }
}

pub async fn route<M: MetricsRepository + Send + Sync>(
    models: &[String],
    metric: &MetricSelector,
    metrics_duration: Option<&MetricsDuration>,
    metrics_repository: &M,
) -> Result<String, RouterError> {
    let minimize = metric.get_optimization_direction() == MetricOptimizationDirection::Minimize;

    // Collect all model candidates with their metrics
    let mut candidates = Vec::new();

    for model in models {
        if let Some((provider, model_name)) = model.split_once('/') {
            // Provider specified, fetch metrics for this specific provider/model
            if let Ok(Some(model_metrics)) = metrics_repository
                .get_model_metrics(provider, model_name)
                .await
            {
                let period_metrics = match metrics_duration {
                    Some(MetricsDuration::Total) | None => &model_metrics.metrics.total,
                    Some(MetricsDuration::LastHour) => &model_metrics.metrics.last_hour,
                    Some(MetricsDuration::Last15Minutes) => &model_metrics.metrics.last_15_minutes,
                };

                if let Some(value) = metric.get_value(period_metrics) {
                    candidates.push((model.clone(), value));
                }
            }
        } else {
            // No provider specified, look in all providers for this model
            // We need to fetch all provider metrics to find this model
            if let Ok(all_metrics) = metrics_repository.get_metrics().await {
                let mut all_matches: Vec<_> = all_metrics
                    .iter()
                    .filter_map(|(provider, provider_metrics)| {
                        provider_metrics.models.get(model).and_then(|metrics| {
                            let period_metrics = match metrics_duration {
                                Some(MetricsDuration::Total) | None => &metrics.metrics.total,
                                Some(MetricsDuration::LastHour) => &metrics.metrics.last_hour,
                                Some(MetricsDuration::Last15Minutes) => {
                                    &metrics.metrics.last_15_minutes
                                }
                            };

                            metric
                                .get_value(period_metrics)
                                .map(|value| (format!("{provider}/{model}"), value))
                        })
                    })
                    .collect();

                // Sort by metric value and take the best one
                if minimize {
                    all_matches.sort_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap());
                } else {
                    all_matches.sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap());
                }

                if let Some((model_with_provider, value)) = all_matches.into_iter().next() {
                    candidates.push((model_with_provider, value));
                }
            }
        }
    }

    if candidates.is_empty() {
        // If no candidates have metrics, select a random model from the available models
        let mut rng = rand::rng();
        if let Some(random_model) = models.iter().choose(&mut rng) {
            let span = Span::current();
            span.record(
                "router_resolution",
                JsonValue(&serde_json::json!({"candidates": [], "best_model": random_model, "metric": metric, "metrics_duration": metrics_duration})).as_value(),
            );
            return Ok(random_model.clone());
        }
    }
    // Find the best candidate
    let best_model = candidates.iter().min_by(|(_, value_a), (_, value_b)| {
        if minimize {
            value_a.partial_cmp(value_b).unwrap()
        } else {
            value_b.partial_cmp(value_a).unwrap()
        }
    });

    let model = match best_model {
        Some((model, _)) => model.clone(),
        None => models.first().cloned().unwrap_or_default(),
    };

    let span = Span::current();
    span.record(
        "router_resolution",
        JsonValue(&serde_json::json!({"candidates": candidates, "best_model": model, "metric": metric, "metrics_duration": metrics_duration})).as_value(),
    );

    tracing::info!("Router resolution: {:#?}", model);

    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::metrics::MetricsRepository;
    use crate::usage::{ModelMetrics, TimeMetrics};
    use async_trait::async_trait;

    fn create_model_metrics(latency: Option<f64>, ttft: Option<f64>) -> ModelMetrics {
        let metrics = Metrics {
            requests: Some(100.0),
            input_tokens: Some(5000.0),
            output_tokens: Some(2000.0),
            total_tokens: Some(7000.0),
            latency,
            ttft,
            llm_usage: Some(0.05),
            tps: Some(0.1),
            error_rate: Some(0.01),
        };

        ModelMetrics {
            metrics: TimeMetrics {
                total: metrics.clone(),
                last_15_minutes: metrics.clone(),
                last_hour: metrics,
            },
        }
    }

    // Mock metrics repository for testing
    struct MockMetricsRepository {
        metrics: std::collections::BTreeMap<String, crate::usage::ProviderMetrics>,
    }

    impl MockMetricsRepository {
        fn new(metrics: std::collections::BTreeMap<String, crate::usage::ProviderMetrics>) -> Self {
            Self { metrics }
        }
    }

    #[async_trait]
    impl MetricsRepository for MockMetricsRepository {
        async fn get_metrics(
            &self,
        ) -> Result<std::collections::BTreeMap<String, crate::usage::ProviderMetrics>, RouterError>
        {
            Ok(self.metrics.clone())
        }

        async fn get_provider_metrics(
            &self,
            provider: &str,
        ) -> Result<Option<crate::usage::ProviderMetrics>, RouterError> {
            Ok(self.metrics.get(provider).cloned())
        }

        async fn get_model_metrics(
            &self,
            provider: &str,
            model: &str,
        ) -> Result<Option<ModelMetrics>, RouterError> {
            Ok(self
                .metrics
                .get(provider)
                .and_then(|provider_metrics| provider_metrics.models.get(model))
                .cloned())
        }
    }

    #[tokio::test]
    async fn test_metric_router() {
        let openai_models = std::collections::BTreeMap::from([
            (
                "gpt-4o-mini".to_string(),
                create_model_metrics(Some(1550.0), Some(1800.0)),
            ),
            (
                "gpt-4o".to_string(),
                create_model_metrics(Some(2550.0), Some(1900.0)),
            ),
        ]);
        let openai_metrics = crate::usage::ProviderMetrics {
            models: openai_models,
        };

        let gemini_models = std::collections::BTreeMap::from([
            (
                "gemini-1.5-flash-latest".to_string(),
                create_model_metrics(Some(500.0), Some(1000.0)),
            ),
            (
                "gemini-1.5-pro-latest".to_string(),
                create_model_metrics(Some(4500.0), Some(1100.0)),
            ),
        ]);
        let gemini_metrics = crate::usage::ProviderMetrics {
            models: gemini_models,
        };

        let metrics = std::collections::BTreeMap::from([
            ("openai".to_string(), openai_metrics),
            ("gemini".to_string(), gemini_metrics),
        ]);

        let models = vec![
            "openai/gpt-4o-mini".to_string(),
            "gemini/gemini-1.5-flash-latest".to_string(),
            "openai/gpt-4o".to_string(),
            "gemini/gemini-1.5-pro-latest".to_string(),
        ];

        let metrics_repository = MockMetricsRepository::new(metrics);

        // Test with TTFT metric (minimize)
        let new_model = super::route(&models, &MetricSelector::Ttft, None, &metrics_repository)
            .await
            .unwrap();

        assert_eq!(new_model, "gemini/gemini-1.5-flash-latest".to_string());

        // Test with requests metric (maximize)
        let new_model = super::route(
            &models,
            &MetricSelector::Requests,
            None,
            &metrics_repository,
        )
        .await
        .unwrap();

        // All models have same request count, so first one should be selected
        assert_eq!(new_model, "openai/gpt-4o-mini".to_string());
    }

    #[tokio::test]
    async fn test_metric_router_for_all_providers() {
        let provider_a_models = std::collections::BTreeMap::from([
            (
                "model_a".to_string(),
                create_model_metrics(Some(4550.0), Some(3800.0)),
            ),
            (
                "model_b".to_string(),
                create_model_metrics(Some(3550.0), Some(2900.0)),
            ),
        ]);
        let provider_a_metrics = crate::usage::ProviderMetrics {
            models: provider_a_models,
        };
        let provider_b_models = std::collections::BTreeMap::from([
            (
                "model_a".to_string(),
                create_model_metrics(Some(1550.0), Some(1800.0)),
            ),
            (
                "model_c".to_string(),
                create_model_metrics(Some(2550.0), Some(1900.0)),
            ),
        ]);
        let provider_b_metrics = crate::usage::ProviderMetrics {
            models: provider_b_models,
        };
        let provider_c_models = std::collections::BTreeMap::from([
            (
                "model_a".to_string(),
                create_model_metrics(Some(1950.0), Some(1200.0)),
            ),
            (
                "model_d".to_string(),
                create_model_metrics(Some(2950.0), Some(1700.0)),
            ),
        ]);
        let provider_c_metrics = crate::usage::ProviderMetrics {
            models: provider_c_models,
        };

        let metrics = std::collections::BTreeMap::from([
            ("provider_a".to_string(), provider_a_metrics),
            ("provider_b".to_string(), provider_b_metrics),
            ("provider_c".to_string(), provider_c_metrics),
        ]);

        let models = vec!["model_a".to_string(), "provider_c/model_d".to_string()];

        let metrics_repository = MockMetricsRepository::new(metrics);

        // Test with TTFT metric (minimize)
        let new_model = super::route(&models, &MetricSelector::Ttft, None, &metrics_repository)
            .await
            .unwrap();

        assert_eq!(new_model, "provider_c/model_a".to_string());

        // Test with request duration (minimize)
        let new_model = super::route(&models, &MetricSelector::Latency, None, &metrics_repository)
            .await
            .unwrap();

        assert_eq!(new_model, "provider_b/model_a".to_string());
    }

    #[tokio::test]
    async fn test_metric_router_when_one_model_does_not_have_metrics() {
        let openai_models = std::collections::BTreeMap::from([
            (
                "gpt-4o-mini".to_string(),
                create_model_metrics(Some(1550.0), Some(1800.0)),
            ),
            ("gpt-4o".to_string(), create_model_metrics(None, None)),
        ]);
        let openai_metrics = crate::usage::ProviderMetrics {
            models: openai_models,
        };

        let metrics = std::collections::BTreeMap::from([("openai".to_string(), openai_metrics)]);

        let models = vec![
            "openai/gpt-4o".to_string(),
            "openai/gpt-4o-mini".to_string(),
        ];

        let metrics_repository = MockMetricsRepository::new(metrics);

        // Test with TTFT metric (minimize)
        let new_model = super::route(&models, &MetricSelector::Ttft, None, &metrics_repository)
            .await
            .unwrap();

        assert_eq!(new_model, "openai/gpt-4o-mini".to_string());

        // Test with request duration (maximize)
        let new_model = super::route(&models, &MetricSelector::Latency, None, &metrics_repository)
            .await
            .unwrap();

        // All models have same request count, so first one should be selected
        assert_eq!(new_model, "openai/gpt-4o-mini".to_string());
    }

    #[tokio::test]
    async fn test_metric_router_when_no_candidates_have_metrics() {
        // Create empty metrics - no models have any metrics
        let metrics = std::collections::BTreeMap::new();
        let metrics_repository = MockMetricsRepository::new(metrics);

        let models = vec![
            "openai/gpt-4o-mini".to_string(),
            "gemini/gemini-1.5-flash-latest".to_string(),
            "anthropic/claude-3-haiku".to_string(),
        ];

        // Test that we get one of the models randomly when no metrics are available
        let selected_model =
            super::route(&models, &MetricSelector::Latency, None, &metrics_repository)
                .await
                .unwrap();

        // Should be one of the available models
        assert!(models.contains(&selected_model));
    }
}
