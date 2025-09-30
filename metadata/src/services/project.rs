use crate::error::DatabaseError;
use crate::models::project::DbProject;
use crate::pool::DbPool;
use langdb_core::types::metadata::project::Project;
use uuid::Uuid;

pub trait ProjectService {
    fn get_by_id(&self, id: Uuid, owner_id: Uuid) -> Result<Project, DatabaseError>;
    fn create(&self, id: Uuid, owner_id: Uuid) -> Result<Project, DatabaseError>;
    fn count(&self, owner_id: Uuid) -> Result<Project, DatabaseError>;
    fn list(&self, owner_id: Uuid) -> Result<Vec<Project>, DatabaseError>;
}

pub struct ProjectServiceImpl {
    db_pool: DbPool,
}

impl ProjectService for ProjectServiceImpl {
    fn get_by_id(&self, id: Uuid, owner_id: Uuid) -> Result<Project, DatabaseError> {
        Err(DatabaseError::UniqueViolation("not implemented".to_string()))
    }

    fn create(&self, _id: Uuid, _owner_id: Uuid) -> Result<Project, DatabaseError> {
        Err(DatabaseError::UniqueViolation("not implemented".to_string()))
    }

    fn count(&self, _owner_id: Uuid) -> Result<Project, DatabaseError> {
        Err(DatabaseError::UniqueViolation("not implemented".to_string()))
    }

    fn list(&self, _owner_id: Uuid) -> Result<Vec<Project>, DatabaseError> {
        Err(DatabaseError::UniqueViolation("not implemented".to_string()))
    }
}