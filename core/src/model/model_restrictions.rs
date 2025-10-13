use crate::metadata::models::project_model_restriction::ProjectModelRestriction;
use crate::metadata::services::project_model_restriction::ProjectModelRestrictionService;
use crate::GatewayApiError;
use async_trait::async_trait;
use uuid::Uuid;

// Define a trait for ProjectModelRestrictions to make it mockable
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait ProjectModelRestrictionsTrait: Send + Sync {
    async fn fetch(&self) -> Result<Vec<ProjectModelRestriction>, GatewayApiError>;
}

#[derive(Clone)]
pub struct ProjectModelRestrictionsManager {
    project_model_restriction_service: ProjectModelRestrictionService,
    project_id: Uuid,
}

impl ProjectModelRestrictionsManager {
    pub fn new(
        project_model_restriction_service: ProjectModelRestrictionService,
        project_id: Uuid,
    ) -> Self {
        Self {
            project_model_restriction_service,
            project_id,
        }
    }
}

#[async_trait::async_trait]
impl ProjectModelRestrictionsTrait for ProjectModelRestrictionsManager {
    async fn fetch(&self) -> Result<Vec<ProjectModelRestriction>, GatewayApiError> {
        self.project_model_restriction_service
            .get_by_project_id(&self.project_id.to_string())
            .map_err(|e| GatewayApiError::CustomError(format!("Failed to fetch restrictions: {}", e)))
    }
}