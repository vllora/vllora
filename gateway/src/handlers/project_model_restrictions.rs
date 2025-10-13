use actix_web::{web, HttpResponse, Result};
use langdb_core::metadata::models::project_model_restriction::{
    CreateProjectModelRestriction, ProjectModelRestrictionWithModels, UpdateProjectModelRestriction,
};
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::project_model_restriction::ProjectModelRestrictionService;
use langdb_core::types::metadata::tag_type::{ControlType, TagType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, Debug)]
pub struct CreateOrUpdateModelRestrictionRequest {
    #[serde(alias = "entity")]
    pub control_entity: ControlType,
    pub id: String,
    pub allowed_models: Option<Vec<String>>,
    pub disallowed_models: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct ProjectModelRestrictionResponse {
    pub restriction: ProjectModelRestrictionWithModels,
}

#[derive(Serialize)]
pub struct ProjectModelRestrictionsListResponse {
    pub restrictions: Vec<ProjectModelRestrictionWithModels>,
}

pub async fn get_by_project_id(
    project_id: web::Path<Uuid>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project_id = project_id.into_inner();
    let service = ProjectModelRestrictionService::new(db_pool.get_ref().clone());

    let restrictions = service
        .get_by_project_id(&project_id.to_string())
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to fetch model restrictions: {}",
                e
            ))
        })?;

    let restrictions_with_models: Vec<ProjectModelRestrictionWithModels> = restrictions
        .into_iter()
        .map(|r| r.into())
        .collect();

    Ok(HttpResponse::Ok().json(ProjectModelRestrictionsListResponse {
        restrictions: restrictions_with_models,
    }))
}

pub async fn create_or_update(
    project_id: web::Path<Uuid>,
    req: web::Json<CreateOrUpdateModelRestrictionRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let project_id = project_id.into_inner();
    let service = ProjectModelRestrictionService::new(db_pool.get_ref().clone());

    // Validate that only one of allowed_models or disallowed_models is set
    if req.allowed_models.is_some() && req.disallowed_models.is_some() {
        return Err(actix_web::error::ErrorBadRequest(
            "Cannot specify both allowed_models and disallowed_models",
        ));
    }

    // If both are None, we delete the restriction
    if req.allowed_models.is_none() && req.disallowed_models.is_none() {
        let tag_type: TagType = req.control_entity.clone().into();
        
        if let Some(existing) = service
            .get_by_project_id_and_tag(&project_id.to_string(), &tag_type, &req.id)
            .map_err(|e| {
                actix_web::error::ErrorInternalServerError(format!(
                    "Failed to check existing restriction: {}",
                    e
                ))
            })?
        {
            service.delete(&existing.id).map_err(|e| {
                actix_web::error::ErrorInternalServerError(format!(
                    "Failed to delete restriction: {}",
                    e
                ))
            })?;

            return Ok(HttpResponse::Ok().json(serde_json::json!({
                "message": "Restriction deleted successfully"
            })));
        } else {
            return Err(actix_web::error::ErrorNotFound("Restriction not found"));
        }
    }

    let tag_type: TagType = req.control_entity.clone().into();

    // Check if restriction already exists
    let existing_restriction = service
        .get_by_project_id_and_tag(&project_id.to_string(), &tag_type, &req.id)
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to check existing restriction: {}",
                e
            ))
        })?;

    let restriction = if let Some(existing) = existing_restriction {
        // Update existing restriction
        let update = UpdateProjectModelRestriction::new(
            req.allowed_models.clone(),
            req.disallowed_models.clone(),
        );

        service.update(&existing.id, update).map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to update restriction: {}", e))
        })?
    } else {
        // Create new restriction
        let create = CreateProjectModelRestriction::new(
            project_id.to_string(),
            tag_type,
            req.id.clone(),
            req.allowed_models.clone().unwrap_or_default(),
            req.disallowed_models.clone().unwrap_or_default(),
        );

        service.create(create).map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to create restriction: {}", e))
        })?
    };

    Ok(HttpResponse::Ok().json(ProjectModelRestrictionResponse {
        restriction: restriction.into(),
    }))
}

