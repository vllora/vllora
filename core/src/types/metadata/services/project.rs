use crate::metadata::error::DatabaseError;
use crate::metadata::models::project::{NewProjectDTO, UpdateProjectDTO};
use crate::types::metadata::project::Project;
use uuid::Uuid;

pub trait ProjectService {
    fn get_by_id(&self, id: Uuid, owner_id: Uuid) -> Result<Project, DatabaseError>;
    fn create(&self, proj: NewProjectDTO, owner_id: Uuid) -> Result<Project, DatabaseError>;
    fn count(&self, owner_id: Uuid) -> Result<u64, DatabaseError>;
    fn list(&self, owner_id: Uuid) -> Result<Vec<Project>, DatabaseError>;
    fn delete(&self, id: Uuid, owner_id: Uuid) -> Result<(), DatabaseError>;
    fn update(
        &self,
        id: Uuid,
        owner_id: Uuid,
        proj: UpdateProjectDTO,
    ) -> Result<Project, DatabaseError>;
    fn set_default(&self, id: Uuid, owner_id: Uuid) -> Result<Project, DatabaseError>;
    fn get_default(&self, owner_id: Uuid) -> Result<Project, DatabaseError>;
}
