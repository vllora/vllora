use std::collections::{HashMap, HashSet};

use crate::routing::ConditionOpType;
use crate::{
    routing::{
        metrics::MetricsRepository, strategy::conditional::evaluator::compare_values,
        MetricsDuration, RouterError,
    },
    telemetry::events::JsonValue,
    usage::{Metrics, ModelMetrics, TimeMetrics},
};
use futures::future;
use rand::seq::IteratorRandom;
use tracing::Span;
use valuable::Valuable;

#[derive(Debug, serde::Serialize, serde::Deserialize, Default, Clone, PartialEq, Eq, Hash)]
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
            MetricSelector::Tps => MetricOptimizationDirection::Maximize,
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

fn create_default_metrics() -> Metrics {
    Metrics {
        requests: Some(0.0),
        input_tokens: Some(0.0),
        output_tokens: Some(0.0),
        total_tokens: Some(0.0),
        latency: Some(0.0),
        ttft: Some(0.0),
        llm_usage: Some(0.0),
        tps: Some(0.0),
        error_rate: Some(0.0),
    }
}

pub async fn route<M: MetricsRepository + Send + Sync>(
    models: &[String],
    metric: &MetricSelector,
    metrics_duration: Option<&MetricsDuration>,
    metrics_repository: &M,
    minimize: Option<bool>,
    filters: Option<&HashMap<MetricSelector, HashMap<ConditionOpType, serde_json::Value>>>,
) -> Result<String, RouterError> {
    let minimize = minimize
        .unwrap_or(metric.get_optimization_direction() == MetricOptimizationDirection::Minimize);

    // Collect all model candidates with their metrics
    let mut candidates = HashMap::new();

    // Prepare parallel fetches
    let mut providers_with_wildcard: HashSet<String> = HashSet::new();
    let mut provider_model_pairs: Vec<(String, String)> = Vec::new();
    let mut models_without_provider: Vec<String> = Vec::new();

    for model in models {
        if let Some((provider, model_name)) = model.split_once('/') {
            if model_name == "*" {
                providers_with_wildcard.insert(provider.to_string());
            } else {
                provider_model_pairs.push((provider.to_string(), model_name.to_string()));
            }
        } else {
            models_without_provider.push(model.clone());
        }
    }

    // Fire provider wildcard fetches in parallel
    let provider_futures = providers_with_wildcard.iter().map(|provider| async {
        let res = metrics_repository.get_provider_metrics(provider).await;
        (provider.clone(), res)
    });

    // Fire specific provider/model fetches in parallel
    let model_futures = provider_model_pairs
        .iter()
        .map(|(provider, model_name)| async {
            let res = metrics_repository
                .get_model_metrics(provider, model_name)
                .await;
            ((provider.clone(), model_name.clone()), res)
        });

    let (provider_results, model_results) = future::join(
        future::join_all(provider_futures),
        future::join_all(model_futures),
    )
    .await;

    // Process provider wildcard results
    for (provider, result) in provider_results {
        if let Ok(Some(provider_metrics)) = result {
            for (model_name, model_metrics) in provider_metrics.models {
                let period_metrics = match metrics_duration {
                    Some(MetricsDuration::Total) | None => &model_metrics.metrics.total,
                    Some(MetricsDuration::LastHour) => &model_metrics.metrics.last_hour,
                    Some(MetricsDuration::Last15Minutes) => &model_metrics.metrics.last_15_minutes,
                };

                candidates.insert(format!("{provider}/{model_name}"), period_metrics.clone());
            }
        }
    }

    // Process specific provider/model results
    for ((provider, model_name), result) in model_results {
        let model_metrics = if let Ok(Some(metrics)) = result {
            metrics
        } else {
            // Use default metrics (0) when no metrics are available for direct model access
            ModelMetrics {
                metrics: TimeMetrics {
                    total: create_default_metrics(),
                    last_15_minutes: create_default_metrics(),
                    last_hour: create_default_metrics(),
                },
            }
        };

        let period_metrics = match metrics_duration {
            Some(MetricsDuration::Total) | None => &model_metrics.metrics.total,
            Some(MetricsDuration::LastHour) => &model_metrics.metrics.last_hour,
            Some(MetricsDuration::Last15Minutes) => &model_metrics.metrics.last_15_minutes,
        };

        candidates.insert(format!("{provider}/{model_name}"), period_metrics.clone());
    }

    // Handle models without provider. Single fetch of all metrics if needed.
    if !models_without_provider.is_empty() {
        if let Ok(all_metrics) = metrics_repository.get_metrics().await {
            for model in models_without_provider {
                let mut found_model = false;
                for (provider, provider_metrics) in &all_metrics {
                    if let Some(metrics) = provider_metrics.models.get(&model) {
                        let period_metrics = match metrics_duration {
                            Some(MetricsDuration::Total) | None => &metrics.metrics.total,
                            Some(MetricsDuration::LastHour) => &metrics.metrics.last_hour,
                            Some(MetricsDuration::Last15Minutes) => {
                                &metrics.metrics.last_15_minutes
                            }
                        };

                        candidates.insert(format!("{provider}/{model}"), period_metrics.clone());
                        found_model = true;
                    }
                }

                // If no provider has this model, add it with default metrics for direct model access
                if !found_model {
                    let default_metrics = create_default_metrics();
                    candidates.insert(model, default_metrics);
                }
            }
        } else {
            // If fetching all metrics failed, fall back to default metrics for each model
            for model in models_without_provider {
                let default_metrics = create_default_metrics();
                candidates.insert(model, default_metrics);
            }
        }
    }

    if let Some(filters) = filters {
        candidates.retain(|_model, metrics| {
            filters.iter().all(|(filter_metric, filter_value)| {
                if let Some(value) = filter_metric.get_value(metrics) {
                    for (op_type, op_value) in filter_value {
                        if !compare_values(op_type, op_value, &serde_json::json!(value)) {
                            return false;
                        }
                    }
                    true
                } else {
                    match filter_metric {
                        // Error rate is always true when no metrics are available
                        MetricSelector::ErrorRate => true,
                        _ => false,
                    }
                }
            })
        });
    }

    let filtered_candidates: Vec<(String, f64)> = candidates
        .into_iter()
        .filter_map(|(model, metrics)| metric.get_value(&metrics).map(|value| (model, value)))
        .collect();

    if filtered_candidates.is_empty() {
        // If no candidates have metrics, select a random model from the available models
        let mut rng = rand::rng();
        if let Some(random_model) = models.iter().choose(&mut rng) {
            let span = Span::current();
            span.record(
                "router.metric_resolution",
                JsonValue(&serde_json::json!({"candidates": [], "best_model": random_model, "metric": metric, "metrics_duration": metrics_duration})).as_value(),
            );
            return Ok(random_model.clone());
        }
    }
    // Find the best candidate
    let best_model = filtered_candidates
        .iter()
        .min_by(|(model_a, value_a), (model_b, value_b)| {
            let metric_comparison = if minimize {
                value_a.partial_cmp(value_b).unwrap()
            } else {
                value_b.partial_cmp(value_a).unwrap()
            };

            // If metrics are equal, sort by model name for deterministic behavior
            if metric_comparison == std::cmp::Ordering::Equal {
                model_a.cmp(model_b)
            } else {
                metric_comparison
            }
        });

    let model = match best_model {
        Some((model, _)) => model.clone(),
        None => models.first().cloned().unwrap_or_default(),
    };

    let span = Span::current();
    span.record(
        "router.metric_resolution",
        JsonValue(&serde_json::json!({"candidates": filtered_candidates, "best_model": model, "metric": metric, "metrics_duration": metrics_duration})).as_value(),
    );

    tracing::info!("Router metric resolution: {:#?}", model);

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
        let new_model = super::route(
            &models,
            &MetricSelector::Ttft,
            None,
            &metrics_repository,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(new_model, "gemini/gemini-1.5-flash-latest".to_string());

        // Test with requests metric (maximize)
        let new_model = super::route(
            &models,
            &MetricSelector::Requests,
            None,
            &metrics_repository,
            None,
            None,
        )
        .await
        .unwrap();

        // All models have same request count, so first one alphabetically should be selected
        assert_eq!(new_model, "gemini/gemini-1.5-flash-latest".to_string());
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
        let new_model = super::route(
            &models,
            &MetricSelector::Ttft,
            None,
            &metrics_repository,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(new_model, "provider_c/model_a".to_string());

        // Test with request duration (minimize)
        let new_model = super::route(
            &models,
            &MetricSelector::Latency,
            None,
            &metrics_repository,
            None,
            None,
        )
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
        let new_model = super::route(
            &models,
            &MetricSelector::Ttft,
            None,
            &metrics_repository,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(new_model, "openai/gpt-4o-mini".to_string());

        // Test with request duration (maximize)
        let new_model = super::route(
            &models,
            &MetricSelector::Latency,
            None,
            &metrics_repository,
            None,
            None,
        )
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
        let selected_model = super::route(
            &models,
            &MetricSelector::Latency,
            None,
            &metrics_repository,
            None,
            None,
        )
        .await
        .unwrap();

        // Should be one of the available models
        assert!(models.contains(&selected_model));
    }

    #[tokio::test]
    async fn test_provider_models_sort_with_wildcard() {
        // Test the wildcard functionality where model name is "openai/*"
        let openai_models = std::collections::BTreeMap::from([
            (
                "gpt-4o-mini".to_string(),
                create_model_metrics(Some(1550.0), Some(1800.0)),
            ),
            (
                "gpt-4o".to_string(),
                create_model_metrics(Some(2550.0), Some(1900.0)),
            ),
            (
                "gpt-3.5-turbo".to_string(),
                create_model_metrics(Some(500.0), Some(1000.0)),
            ),
        ]);
        let openai_metrics = crate::usage::ProviderMetrics {
            models: openai_models,
        };

        let gemini_models = std::collections::BTreeMap::from([
            (
                "gemini-1.5-flash-latest".to_string(),
                create_model_metrics(Some(800.0), Some(1200.0)),
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

        // Test with wildcard model specification
        let models = vec!["openai/*".to_string()];

        let metrics_repository = MockMetricsRepository::new(metrics);

        // Test with TTFT metric (minimize) - should select the model with lowest TTFT
        let selected_model = super::route(
            &models,
            &MetricSelector::Ttft,
            None,
            &metrics_repository,
            None,
            None,
        )
        .await
        .unwrap();

        // Should select gpt-3.5-turbo as it has the lowest TTFT (1000.0)
        assert_eq!(selected_model, "openai/gpt-3.5-turbo".to_string());

        // Test with Latency metric (minimize) - should select the model with lowest latency
        let selected_model = super::route(
            &models,
            &MetricSelector::Latency,
            None,
            &metrics_repository,
            None,
            None,
        )
        .await
        .unwrap();

        // Should select gpt-3.5-turbo as it has the lowest latency (500.0)
        assert_eq!(selected_model, "openai/gpt-3.5-turbo".to_string());

        // Test with Requests metric (maximize) - all models have same request count
        let selected_model = super::route(
            &models,
            &MetricSelector::Requests,
            None,
            &metrics_repository,
            None,
            None,
        )
        .await
        .unwrap();

        // All models have same request count (100.0), so should select the first one alphabetically
        assert_eq!(selected_model, "openai/gpt-3.5-turbo".to_string());
    }

    #[tokio::test]
    async fn test_metric_router_with_default_metrics_for_missing_models() {
        // Test that models without metrics get default metrics (0) for direct model access
        let openai_models = std::collections::BTreeMap::from([(
            "gpt-4o-mini".to_string(),
            create_model_metrics(Some(1550.0), Some(1800.0)),
        )]);
        let openai_metrics = crate::usage::ProviderMetrics {
            models: openai_models,
        };

        let metrics = std::collections::BTreeMap::from([("openai".to_string(), openai_metrics)]);

        let models = vec![
            "openai/gpt-4o-mini".to_string(), // Has metrics
            "openai/gpt-4o".to_string(),      // No metrics - should get defaults
            "nonexistent-model".to_string(),  // No metrics - should get defaults
        ];

        let metrics_repository = MockMetricsRepository::new(metrics);

        // Test with latency metric (minimize) - should select the model with lowest latency
        let selected_model = super::route(
            &models,
            &MetricSelector::Latency,
            None,
            &metrics_repository,
            None,
            None,
        )
        .await
        .unwrap();

        // Should select "nonexistent-model" as it has default latency (0.0) which is lowest
        assert_eq!(selected_model, "nonexistent-model".to_string());

        // Test with requests metric (minimize) - should select the model with highest requests
        let selected_model = super::route(
            &models,
            &MetricSelector::Requests,
            None,
            &metrics_repository,
            None,
            None,
        )
        .await
        .unwrap();

        // Should select "openai/gpt-4o-mini" as it has requests (100.0) vs defaults (0.0)
        assert_eq!(selected_model, "nonexistent-model".to_string());
    }
}
