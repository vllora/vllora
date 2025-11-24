use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;
use std::collections::HashMap;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::provider_credential::ProviderCredentialsServiceImpl;
use vllora_core::types::metadata::project::Project;
use vllora_core::types::metadata::services::model::ModelService;
use vllora_core::types::metadata::services::provider_credential::ProviderCredentialsService;
use vllora_core::GatewayApiError;
use vllora_llm::types::models::group_models_by_name_with_endpoints;
use vllora_llm::types::models::ModelMetadata;

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

        let provider_credentials_map =
            get_provider_credentials_map(&db_pool, Some(&project.id.to_string()));
        // Group models by name and create endpoints with availability based on credentials
        group_models_by_name_with_endpoints(models, &provider_credentials_map)
    });

    match result {
        Ok(grouped_models) => Ok(HttpResponse::Ok().json(grouped_models)),
        Err(e) => Err(GatewayApiError::CustomError(format!(
            "Failed to fetch models: {}",
            e
        ))),
    }
}

fn get_provider_credentials_map(
    db_pool: &DbPool,
    project_id: Option<&str>,
) -> HashMap<String, bool> {
    // Create provider credentials service to check availability
    let provider_credentials_service = ProviderCredentialsServiceImpl::new(db_pool.clone());

    // Fetch all provider credentials at once for better performance
    let provider_credentials_map = provider_credentials_service
        .list_providers(project_id)
        .unwrap_or_default();

    provider_credentials_map
        .into_iter()
        .map(|provider_info| (provider_info.name, provider_info.has_credentials))
        .collect::<HashMap<String, bool>>()
}
