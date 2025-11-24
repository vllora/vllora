use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use vllora_core::metadata::models::experiment::{NewDbExperiment, UpdateDbExperiment};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::experiment::ExperimentServiceImpl;
use vllora_core::types::metadata::project::Project;

#[derive(Debug, Deserialize)]
#[serde(crate = "serde")]
pub struct CreateExperimentRequest {
    pub name: String,
    pub description: Option<String>,
    pub original_span_id: String,
    pub original_trace_id: String,
    pub original_request: serde_json::Value,
    pub modified_request: serde_json::Value,
    pub headers: Option<serde_json::Value>,
    pub prompt_variables: Option<serde_json::Value>,
    pub model_parameters: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(crate = "serde")]
pub struct UpdateExperimentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub modified_request: Option<serde_json::Value>,
    pub headers: Option<serde_json::Value>,
    pub prompt_variables: Option<serde_json::Value>,
    pub model_parameters: Option<serde_json::Value>,
    pub result_span_id: Option<String>,
    pub result_trace_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(crate = "serde")]
pub struct ExperimentResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub original_span_id: String,
    pub original_trace_id: String,
    pub original_request: serde_json::Value,
    pub modified_request: serde_json::Value,
    pub headers: Option<serde_json::Value>,
    pub prompt_variables: Option<serde_json::Value>,
    pub model_parameters: Option<serde_json::Value>,
    pub result_span_id: Option<String>,
    pub result_trace_id: Option<String>,
    pub status: String,
    pub project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// POST /experiments - Create a new experiment
pub async fn create_experiment(
    project: web::ReqData<Project>,
    req: web::Json<CreateExperimentRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project = project.into_inner();
    let experiment_service = ExperimentServiceImpl::new(db_pool.get_ref().clone());

    let new_experiment = NewDbExperiment {
        name: req.name.clone(),
        description: req.description.clone(),
        original_span_id: req.original_span_id.clone(),
        original_trace_id: req.original_trace_id.clone(),
        original_request: serde_json::to_string(&req.original_request)
            .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid JSON: {}", e)))?,
        modified_request: serde_json::to_string(&req.modified_request)
            .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid JSON: {}", e)))?,
        headers: req
            .headers
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid JSON: {}", e)))?,
        prompt_variables: req
            .prompt_variables
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid JSON: {}", e)))?,
        model_parameters: req
            .model_parameters
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid JSON: {}", e)))?,
        status: "draft".to_string(),
        project_id: Some(project.slug),
    };

    match experiment_service.create(new_experiment) {
        Ok(experiment) => {
            let response = ExperimentResponse {
                id: experiment.id,
                name: experiment.name,
                description: experiment.description,
                original_span_id: experiment.original_span_id,
                original_trace_id: experiment.original_trace_id,
                original_request: serde_json::from_str(&experiment.original_request).unwrap_or_default(),
                modified_request: serde_json::from_str(&experiment.modified_request).unwrap_or_default(),
                headers: experiment.headers.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                prompt_variables: experiment.prompt_variables.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                model_parameters: experiment.model_parameters.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                result_span_id: experiment.result_span_id,
                result_trace_id: experiment.result_trace_id,
                status: experiment.status,
                project_id: experiment.project_id,
                created_at: experiment.created_at,
                updated_at: experiment.updated_at,
            };
            Ok(HttpResponse::Created().json(response))
        }
        Err(e) => {
            tracing::error!("Failed to create experiment: {:?}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to create experiment",
                "message": e.to_string()
            })))
        }
    }
}

/// GET /experiments/{id} - Get experiment by ID
pub async fn get_experiment(
    path: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let experiment_id = path.into_inner();
    let experiment_service = ExperimentServiceImpl::new(db_pool.get_ref().clone());

    match experiment_service.get_by_id(&experiment_id) {
        Ok(experiment) => {
            let response = ExperimentResponse {
                id: experiment.id,
                name: experiment.name,
                description: experiment.description,
                original_span_id: experiment.original_span_id,
                original_trace_id: experiment.original_trace_id,
                original_request: serde_json::from_str(&experiment.original_request).unwrap_or_default(),
                modified_request: serde_json::from_str(&experiment.modified_request).unwrap_or_default(),
                headers: experiment.headers.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                prompt_variables: experiment.prompt_variables.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                model_parameters: experiment.model_parameters.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                result_span_id: experiment.result_span_id,
                result_trace_id: experiment.result_trace_id,
                status: experiment.status,
                project_id: experiment.project_id,
                created_at: experiment.created_at,
                updated_at: experiment.updated_at,
            };
            Ok(HttpResponse::Ok().json(response))
        }
        Err(_) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Experiment not found",
            "message": format!("Experiment with ID {} not found", experiment_id)
        }))),
    }
}

/// GET /experiments - List all experiments
pub async fn list_experiments(
    project: web::ReqData<Project>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project = project.into_inner();
    let experiment_service = ExperimentServiceImpl::new(db_pool.get_ref().clone());

    match experiment_service.list(Some(&project.slug)) {
        Ok(experiments) => {
            let response: Vec<ExperimentResponse> = experiments
                .into_iter()
                .map(|experiment| ExperimentResponse {
                    id: experiment.id,
                    name: experiment.name,
                    description: experiment.description,
                    original_span_id: experiment.original_span_id,
                    original_trace_id: experiment.original_trace_id,
                    original_request: serde_json::from_str(&experiment.original_request).unwrap_or_default(),
                    modified_request: serde_json::from_str(&experiment.modified_request).unwrap_or_default(),
                    headers: experiment.headers.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                    prompt_variables: experiment.prompt_variables.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                    model_parameters: experiment.model_parameters.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                    result_span_id: experiment.result_span_id,
                    result_trace_id: experiment.result_trace_id,
                    status: experiment.status,
                    project_id: experiment.project_id,
                    created_at: experiment.created_at,
                    updated_at: experiment.updated_at,
                })
                .collect();
            Ok(HttpResponse::Ok().json(response))
        }
        Err(e) => {
            tracing::error!("Failed to list experiments: {:?}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list experiments",
                "message": e.to_string()
            })))
        }
    }
}

/// GET /experiments/by-span/{span_id} - Get experiments by span ID
pub async fn get_experiments_by_span(
    path: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let span_id = path.into_inner();
    let experiment_service = ExperimentServiceImpl::new(db_pool.get_ref().clone());

    match experiment_service.get_by_span_id(&span_id) {
        Ok(experiments) => {
            let response: Vec<ExperimentResponse> = experiments
                .into_iter()
                .map(|experiment| ExperimentResponse {
                    id: experiment.id,
                    name: experiment.name,
                    description: experiment.description,
                    original_span_id: experiment.original_span_id,
                    original_trace_id: experiment.original_trace_id,
                    original_request: serde_json::from_str(&experiment.original_request).unwrap_or_default(),
                    modified_request: serde_json::from_str(&experiment.modified_request).unwrap_or_default(),
                    headers: experiment.headers.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                    prompt_variables: experiment.prompt_variables.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                    model_parameters: experiment.model_parameters.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                    result_span_id: experiment.result_span_id,
                    result_trace_id: experiment.result_trace_id,
                    status: experiment.status,
                    project_id: experiment.project_id,
                    created_at: experiment.created_at,
                    updated_at: experiment.updated_at,
                })
                .collect();
            Ok(HttpResponse::Ok().json(response))
        }
        Err(e) => {
            tracing::error!("Failed to get experiments by span: {:?}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to get experiments",
                "message": e.to_string()
            })))
        }
    }
}

/// PUT /experiments/{id} - Update an experiment
pub async fn update_experiment(
    path: web::Path<String>,
    req: web::Json<UpdateExperimentRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let experiment_id = path.into_inner();
    let experiment_service = ExperimentServiceImpl::new(db_pool.get_ref().clone());

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let update_data = UpdateDbExperiment {
        name: req.name.clone(),
        description: req.description.clone(),
        modified_request: req
            .modified_request
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid JSON: {}", e)))?,
        headers: req
            .headers
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid JSON: {}", e)))?,
        prompt_variables: req
            .prompt_variables
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid JSON: {}", e)))?,
        model_parameters: req
            .model_parameters
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()
            .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid JSON: {}", e)))?,
        result_span_id: req.result_span_id.clone(),
        result_trace_id: req.result_trace_id.clone(),
        status: req.status.clone(),
        updated_at: now,
    };

    match experiment_service.update(&experiment_id, update_data) {
        Ok(experiment) => {
            let response = ExperimentResponse {
                id: experiment.id,
                name: experiment.name,
                description: experiment.description,
                original_span_id: experiment.original_span_id,
                original_trace_id: experiment.original_trace_id,
                original_request: serde_json::from_str(&experiment.original_request).unwrap_or_default(),
                modified_request: serde_json::from_str(&experiment.modified_request).unwrap_or_default(),
                headers: experiment.headers.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                prompt_variables: experiment.prompt_variables.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                model_parameters: experiment.model_parameters.as_ref().and_then(|s| serde_json::from_str(s).ok()),
                result_span_id: experiment.result_span_id,
                result_trace_id: experiment.result_trace_id,
                status: experiment.status,
                project_id: experiment.project_id,
                created_at: experiment.created_at,
                updated_at: experiment.updated_at,
            };
            Ok(HttpResponse::Ok().json(response))
        }
        Err(_) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Experiment not found",
            "message": format!("Experiment with ID {} not found", experiment_id)
        }))),
    }
}

/// DELETE /experiments/{id} - Delete an experiment
pub async fn delete_experiment(
    path: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let experiment_id = path.into_inner();
    let experiment_service = ExperimentServiceImpl::new(db_pool.get_ref().clone());

    match experiment_service.delete(&experiment_id) {
        Ok(_) => Ok(HttpResponse::NoContent().finish()),
        Err(_) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Experiment not found",
            "message": format!("Experiment with ID {} not found", experiment_id)
        }))),
    }
}
