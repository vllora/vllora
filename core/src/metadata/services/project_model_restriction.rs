use crate::metadata::error::DatabaseError;
use crate::metadata::models::project_model_restriction::{
    CreateProjectModelRestriction, ProjectModelRestriction, UpdateProjectModelRestriction,
};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::project_model_restrictions;
use crate::models::ModelMetadata;
use crate::types::metadata::tag_type::TagType;
use diesel::prelude::*;

#[derive(Clone)]
pub struct ProjectModelRestrictionService {
    db_pool: DbPool,
}

impl ProjectModelRestrictionService {
    pub fn new(db_pool: DbPool) -> Self {
        ProjectModelRestrictionService { db_pool }
    }

    pub fn create(
        &self,
        input: CreateProjectModelRestriction,
    ) -> Result<ProjectModelRestriction, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        diesel::insert_into(project_model_restrictions::table)
            .values(&input)
            .execute(&mut conn)?;

        // Retrieve the inserted record
        project_model_restrictions::table
            .filter(project_model_restrictions::id.eq(&input.id))
            .first::<ProjectModelRestriction>(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<ProjectModelRestriction>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        project_model_restrictions::table
            .filter(project_model_restrictions::id.eq(id))
            .first::<ProjectModelRestriction>(&mut conn)
            .optional()
            .map_err(DatabaseError::QueryError)
    }

    pub fn get_by_project_id(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectModelRestriction>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        project_model_restrictions::table
            .filter(project_model_restrictions::project_id.eq(project_id))
            .get_results::<ProjectModelRestriction>(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn get_by_project_id_and_tag(
        &self,
        project_id: &str,
        tag_type: &TagType,
        tag: &str,
    ) -> Result<Option<ProjectModelRestriction>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        project_model_restrictions::table
            .filter(project_model_restrictions::project_id.eq(project_id))
            .filter(project_model_restrictions::tag_type.eq(tag_type))
            .filter(project_model_restrictions::tag.eq(tag))
            .first::<ProjectModelRestriction>(&mut conn)
            .optional()
            .map_err(DatabaseError::QueryError)
    }

    pub fn get_by_project_id_and_tag_type(
        &self,
        project_id: &str,
        tag_type: &TagType,
    ) -> Result<Vec<ProjectModelRestriction>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        project_model_restrictions::table
            .filter(project_model_restrictions::project_id.eq(project_id))
            .filter(project_model_restrictions::tag_type.eq(tag_type))
            .get_results::<ProjectModelRestriction>(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn update(
        &self,
        id: &str,
        input: UpdateProjectModelRestriction,
    ) -> Result<ProjectModelRestriction, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        diesel::update(project_model_restrictions::table.find(id))
            .set(&input)
            .execute(&mut conn)?;

        // Retrieve the updated record
        project_model_restrictions::table
            .filter(project_model_restrictions::id.eq(id))
            .first::<ProjectModelRestriction>(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn delete(&self, id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        diesel::delete(project_model_restrictions::table.find(id))
            .execute(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn delete_by_project_id(&self, project_id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        diesel::delete(
            project_model_restrictions::table
                .filter(project_model_restrictions::project_id.eq(project_id)),
        )
        .execute(&mut conn)
        .map_err(DatabaseError::QueryError)
    }

    /// Apply restrictions to filter a list of models
    /// 
    /// For each restriction:
    /// - If `allowed_models` is non-empty, only keep models in the allowed list
    /// - If `disallowed_models` is non-empty, remove models in the disallowed list
    /// - If both are empty/null, no filtering is applied by that restriction
    pub fn apply_restrictions(
        models: Vec<ModelMetadata>,
        restrictions: Vec<ProjectModelRestriction>,
    ) -> Vec<ModelMetadata> {
        let mut filtered_models = models;

        for restriction in restrictions {
            let allowed = restriction.allowed_models();
            let disallowed = restriction.disallowed_models();

            // Apply allow-list if present and non-empty
            if !allowed.is_empty() {
                filtered_models.retain(|model| {
                    let model_name = model.model.clone();
                    let qualified_name = model.qualified_model_name();
                    
                    // Match by either simple model name or qualified name
                    allowed.contains(&model_name) || allowed.contains(&qualified_name)
                });
            }

            // Apply deny-list if present and non-empty
            if !disallowed.is_empty() {
                filtered_models.retain(|model| {
                    let model_name = model.model.clone();
                    let qualified_name = model.qualified_model_name();
                    
                    // Keep model only if it's NOT in the disallowed list
                    !disallowed.contains(&model_name) && !disallowed.contains(&qualified_name)
                });
            }
        }

        filtered_models
    }
}

