use actix_web::{web, HttpResponse, Result};
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::model::ModelService;
use langdb_core::metadata::services::provider_credentials::{
    ProviderCredentialsService, ProviderCredentialsServiceImpl,
};
use langdb_core::models::{Endpoint, EndpointPricing, ModelMetadata, ModelMetadataWithEndpoints};
use langdb_core::types::metadata::project::Project;
use langdb_core::types::provider::ModelPrice;
use langdb_core::GatewayApiError;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct PricingQueryParams {
    #[serde(default)]
    pub include_parameters: bool,
    #[serde(default)]
    pub include_benchmark: bool,
}

/// Handler to list gateway pricing models grouped by name with endpoints
pub async fn list_gateway_pricing(
    query: web::Query<PricingQueryParams>,
    model_service: web::Data<Box<dyn ModelService>>,
    db_pool: web::Data<DbPool>,
    project: web::ReqData<Project>,
) -> Result<HttpResponse, GatewayApiError> {
    // Query all models (project_id = None means global models)
    let project_id = None;

    let result = model_service.list(project_id).map(|db_models| {
        // Convert DbModel to ModelMetadata and filter fields based on query parameters
        let models: Vec<ModelMetadata> = db_models
            .into_iter()
            .map(|m| {
                let mut model_metadata: ModelMetadata = m.into();

                // Filter out parameters if not requested
                if !query.include_parameters {
                    model_metadata.parameters = None;
                }

                // Filter out benchmark_info if not requested
                if !query.include_benchmark {
                    model_metadata.benchmark_info = None;
                }

                model_metadata
            })
            .collect();

        // Group models by name and create endpoints with availability based on credentials
        group_models_by_name_with_endpoints(models, &db_pool, Some(&project.id.to_string()))
    });

    match result {
        Ok(grouped_models) => Ok(HttpResponse::Ok().json(grouped_models)),
        Err(e) => Err(GatewayApiError::CustomError(format!(
            "Failed to fetch models: {}",
            e
        ))),
    }
}

/// Groups models by name and creates endpoints for each model with availability based on credentials
fn group_models_by_name_with_endpoints(
    models: Vec<ModelMetadata>,
    db_pool: &DbPool,
    project_id: Option<&str>,
) -> Vec<ModelMetadataWithEndpoints> {
    let mut grouped: HashMap<String, Vec<ModelMetadata>> = HashMap::new();

    // Group models by their model name
    for model in &models {
        grouped.entry(model.model.clone()).or_default().push(model.clone());
    }

    // Create provider credentials service to check availability
    let provider_credentials_service = ProviderCredentialsServiceImpl::new(db_pool.clone());

    // Fetch all provider credentials at once for better performance
    let provider_credentials_map = provider_credentials_service
        .list_providers(project_id)
        .unwrap_or_default();

    tracing::info!("provider_credentials_map: {:?}", provider_credentials_map);

    let provider_credentials_map = provider_credentials_map
        .into_iter()
        .map(|provider_info| (provider_info.name, provider_info.has_credentials))
        .collect::<HashMap<String, bool>>();

    tracing::info!("provider_credentials_map: {:?}", provider_credentials_map);

    // Convert grouped models to ModelMetadataWithEndpoints
    // Preserve database order by iterating through original models
    let mut result: Vec<ModelMetadataWithEndpoints> = Vec::new();
    let mut processed_models: std::collections::HashSet<String> = std::collections::HashSet::new();
    
    for model in models {
        if processed_models.contains(&model.model) {
            continue; // Skip if we already processed this model name
        }
        
        // Get all instances of this model
        if let Some(model_instances) = grouped.get(&model.model) {
            // Sort model instances by cost (cheapest first)
            let mut model_instances = model_instances.clone();
            model_instances.sort_by(|a, b| {
                let a_cost = a.price.per_input_token();
                let b_cost = b.price.per_input_token();
                a_cost.partial_cmp(&b_cost).unwrap_or(std::cmp::Ordering::Equal)
            });

            // Use the first (cheapest) model instance as the base
            let base_model = model_instances[0].clone();

            // Create endpoints from all instances with availability and pricing
            let endpoints: Vec<Endpoint> = model_instances
                .iter()
                .map(|model| {
                    // Check if provider has credentials configured using pre-fetched data
                    let provider_name =
                        model.inference_provider.provider.to_string().to_lowercase();
                    let has_credentials = provider_credentials_map
                        .get(&provider_name)
                        .copied()
                        .unwrap_or(false);

                    // Extract pricing from model
                    let pricing = match &model.price {
                        ModelPrice::Completion(price) => Some(EndpointPricing {
                            per_input_token: price.per_input_token,
                            per_output_token: price.per_output_token,
                            per_cached_input_token: price.per_cached_input_token,
                            per_cached_input_write_token: price.per_cached_input_write_token,
                        }),
                        ModelPrice::Embedding(price) => Some(EndpointPricing {
                            per_input_token: price.per_input_token,
                            per_output_token: 0.0,
                            per_cached_input_token: None,
                            per_cached_input_write_token: None,
                        }),
                        ModelPrice::ImageGeneration(_) => None,
                    };

                    Endpoint {
                        provider: model.inference_provider.clone(),
                        available: has_credentials,
                        pricing,
                    }
                })
                .collect();

            result.push(ModelMetadataWithEndpoints {
                model: base_model,
                endpoints,
            });
            
            processed_models.insert(model.model.clone());
        }
    }

    result
}
