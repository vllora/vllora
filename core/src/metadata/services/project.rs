use crate::metadata::error::DatabaseError;
use crate::metadata::models::project::{DbNewProject, DbProject, NewProjectDTO, UpdateProjectDTO};
use crate::metadata::pool::DbPool;
use crate::types::metadata::project::Project;
use crate::types::metadata::services::project::ProjectService;
use diesel::dsl::count;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::{QueryDsl, RunQueryDsl};
use uuid::Uuid;

pub struct ProjectServiceImpl {
    db_pool: DbPool,
}

impl ProjectServiceImpl {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    fn slugify(name: &str) -> String {
        name.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .chars()
            .fold(String::new(), |mut acc, c| {
                if !acc.ends_with('-') || c != '-' {
                    acc.push(c);
                }
                acc
            })
            .trim_matches('-')
            .to_string()
    }
}

impl ProjectService for ProjectServiceImpl {
    fn get_by_id(&self, id: Uuid, _owner_id: Uuid) -> Result<Project, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        DbProject::not_archived()
            .filter(crate::metadata::schema::projects::id.eq(id.to_string()))
            .first(&mut conn)
            .map_err(DatabaseError::QueryError)
            .map(|db_project| db_project.into())
    }

    fn create(&self, proj: NewProjectDTO, _owner_id: Uuid) -> Result<Project, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let settings_json = proj
            .settings
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        let slug = Self::slugify(&proj.name);
        let project_id = Uuid::new_v4().to_string();

        let db_new_project = DbNewProject {
            id: Some(project_id),
            name: proj.name.clone(),
            description: proj.description.clone(),
            slug,
            settings: settings_json,
            is_default: Some(0),
        };

        diesel::insert_into(crate::metadata::schema::projects::table)
            .values(&db_new_project)
            .execute(&mut conn)?;

        // For SQLite, we need to query the inserted record separately
        // We'll use the name as a unique identifier to find the inserted record
        let inserted_project = DbProject::not_archived()
            .filter(crate::metadata::schema::projects::name.eq(proj.name))
            .order(crate::metadata::schema::projects::created_at.desc())
            .first(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(inserted_project.into())
    }

    fn count(&self, _owner_id: Uuid) -> Result<u64, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let count_result: i64 = DbProject::not_archived()
            .select(count(crate::metadata::schema::projects::id))
            .first(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(count_result as u64)
    }

    fn list(&self, _owner_id: Uuid) -> Result<Vec<Project>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let db_projects = DbProject::not_archived()
            .load::<DbProject>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        let projects = db_projects
            .into_iter()
            .map(|db_project| db_project.into())
            .collect();

        Ok(projects)
    }

    fn delete(&self, id: Uuid, _owner_id: Uuid) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Check if the project exists and is not archived
        let project_exists = DbProject::not_archived()
            .filter(crate::metadata::schema::projects::id.eq(id.to_string()))
            .first::<DbProject>(&mut conn)
            .optional()
            .map_err(DatabaseError::QueryError)?;

        if project_exists.is_none() {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }

        // Soft delete by setting archived_at timestamp
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        diesel::update(crate::metadata::schema::projects::table)
            .filter(crate::metadata::schema::projects::id.eq(id.to_string()))
            .set(crate::metadata::schema::projects::archived_at.eq(Some(now)))
            .execute(&mut conn)?;

        Ok(())
    }

    fn update(
        &self,
        id: Uuid,
        _owner_id: Uuid,
        update_data: UpdateProjectDTO,
    ) -> Result<Project, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Check if the project exists and is not archived
        let existing_project = DbProject::not_archived()
            .filter(crate::metadata::schema::projects::id.eq(id.to_string()))
            .first::<DbProject>(&mut conn)
            .optional()
            .map_err(DatabaseError::QueryError)?;

        let existing_project = match existing_project {
            Some(project) => project,
            None => return Err(DatabaseError::QueryError(diesel::result::Error::NotFound)),
        };

        // Check if there are any fields to update
        let has_updates = update_data.name.is_some()
            || update_data.description.is_some()
            || update_data.settings.is_some()
            || update_data.is_default.is_some();

        if !has_updates {
            return Ok(existing_project.into());
        }

        // Prepare the update data
        let mut name = existing_project.name.clone();
        let mut description = existing_project.description.clone();
        let mut settings = existing_project.settings.clone();
        let mut is_default = existing_project.is_default;
        let mut slug = existing_project.slug.clone();

        // Update name and slug if provided
        if let Some(new_name) = update_data.name {
            name = new_name.clone();
            if new_name != existing_project.name {
                slug = Self::slugify(&new_name);
            }
        }

        // Update description if provided
        if let Some(new_description) = update_data.description {
            description = Some(new_description);
        }

        // Update settings if provided
        if let Some(new_settings) = update_data.settings {
            let settings_json = serde_json::to_string(&new_settings).map_err(|e| {
                DatabaseError::QueryError(diesel::result::Error::DeserializationError(Box::new(e)))
            })?;
            settings = Some(settings_json);
        }

        // Update is_default if provided
        if let Some(new_is_default) = update_data.is_default {
            if new_is_default {
                // If setting this project as default, first set all other projects to non-default
                diesel::update(crate::metadata::schema::projects::table)
                    .filter(crate::metadata::schema::projects::id.ne(id.to_string()))
                    .filter(crate::metadata::schema::projects::archived_at.is_null())
                    .set(crate::metadata::schema::projects::is_default.eq(0))
                    .execute(&mut conn)?;
            }
            is_default = if new_is_default { 1 } else { 0 };
        }

        // Update updated_at timestamp
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        // Perform the update using individual field updates
        diesel::update(crate::metadata::schema::projects::table)
            .filter(crate::metadata::schema::projects::id.eq(id.to_string()))
            .set((
                crate::metadata::schema::projects::name.eq(name),
                crate::metadata::schema::projects::slug.eq(slug),
                crate::metadata::schema::projects::description.eq(description),
                crate::metadata::schema::projects::settings.eq(settings),
                crate::metadata::schema::projects::is_default.eq(is_default),
                crate::metadata::schema::projects::updated_at.eq(now),
            ))
            .execute(&mut conn)?;

        // Retrieve the updated project
        let updated_project = DbProject::not_archived()
            .filter(crate::metadata::schema::projects::id.eq(id.to_string()))
            .first::<DbProject>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(updated_project.into())
    }

    fn set_default(&self, id: Uuid, _owner_id: Uuid) -> Result<Project, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Check if the project exists and is not archived
        let existing_project = DbProject::not_archived()
            .filter(crate::metadata::schema::projects::id.eq(id.to_string()))
            .first::<DbProject>(&mut conn)
            .optional()
            .map_err(DatabaseError::QueryError)?;

        match existing_project {
            Some(_project) => {
                // Project exists, we can proceed
            }
            None => return Err(DatabaseError::QueryError(diesel::result::Error::NotFound)),
        };

        // First set all other projects to non-default
        diesel::update(crate::metadata::schema::projects::table)
            .filter(crate::metadata::schema::projects::id.ne(id.to_string()))
            .filter(crate::metadata::schema::projects::archived_at.is_null())
            .set(crate::metadata::schema::projects::is_default.eq(0))
            .execute(&mut conn)?;

        // Then set the specified project as default
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        diesel::update(crate::metadata::schema::projects::table)
            .filter(crate::metadata::schema::projects::id.eq(id.to_string()))
            .set((
                crate::metadata::schema::projects::is_default.eq(1),
                crate::metadata::schema::projects::updated_at.eq(now),
            ))
            .execute(&mut conn)?;

        // Retrieve the updated project
        let updated_project = DbProject::not_archived()
            .filter(crate::metadata::schema::projects::id.eq(id.to_string()))
            .first::<DbProject>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(updated_project.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::project_settings::ProjectSettings;

    #[test]
    fn test_slugify_function() {
        assert_eq!(ProjectServiceImpl::slugify("Hello World"), "hello-world");
        assert_eq!(
            ProjectServiceImpl::slugify("Test Project 123"),
            "test-project-123"
        );
        assert_eq!(
            ProjectServiceImpl::slugify("Special@Chars!#"),
            "special-chars"
        );
        assert_eq!(
            ProjectServiceImpl::slugify("Multiple---Dashes"),
            "multiple-dashes"
        );
        assert_eq!(
            ProjectServiceImpl::slugify("  Leading and Trailing  "),
            "leading-and-trailing"
        );
        assert_eq!(ProjectServiceImpl::slugify("UPPERCASE"), "uppercase");
        assert_eq!(ProjectServiceImpl::slugify("mixedCASE123"), "mixedcase123");
    }

    #[test]
    fn test_create_project() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        let new_project = NewProjectDTO {
            name: "Test Project".to_string(),
            description: Some("A test project".to_string()),
            settings: Some(serde_json::json!({
                "feature_flags": {
                    "chat_tracing": true
                }
            })),
            private_model_prices: None,
            usage_limit: None,
        };

        let result = service.create(new_project, owner_id);
        assert!(result.is_ok());

        let project = result.unwrap();
        assert_eq!(project.name, "Test Project");
        assert_eq!(project.description, Some("A test project".to_string()));
        assert_eq!(project.slug, "test-project");
        assert!(project.id != Uuid::nil());
        assert!(!project.is_default); // Should be false since we set is_default to Some(0)

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_create_project_minimal() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        let new_project = NewProjectDTO {
            name: "Minimal Project".to_string(),
            description: None,
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let result = service.create(new_project, owner_id);
        assert!(result.is_ok());

        let project = result.unwrap();
        assert_eq!(project.name, "Minimal Project");
        assert_eq!(project.description, None);
        // The database has a default value for settings, so we expect it to be Some
        assert!(project.settings.is_some());
        assert_eq!(project.slug, "minimal-project");

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_get_by_id() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // First create a project
        let new_project = NewProjectDTO {
            name: "Get Test Project".to_string(),
            description: Some("Project for get test".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let created_project = service.create(new_project, owner_id).unwrap();
        let project_id = created_project.id;

        // Now try to get it by ID
        let result = service.get_by_id(project_id, owner_id);
        assert!(result.is_ok());

        let retrieved_project = result.unwrap();
        assert_eq!(retrieved_project.id, project_id);
        assert_eq!(retrieved_project.name, "Get Test Project");
        assert_eq!(
            retrieved_project.description,
            Some("Project for get test".to_string())
        );

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_get_by_id_not_found() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Try to get a project that doesn't exist
        let fake_uuid = Uuid::new_v4();
        let result = service.get_by_id(fake_uuid, owner_id);
        assert!(result.is_err());

        // Check that it's a QueryError (which includes "not found" cases)
        match result.unwrap_err() {
            DatabaseError::QueryError(_) => {
                // This is expected for not found cases
            }
            _ => panic!("Expected QueryError for not found case"),
        }

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_list_projects_empty() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        let result = service.list(owner_id);
        assert!(result.is_ok());

        let projects = result.unwrap();
        assert_eq!(projects.len(), 0);

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_list_projects_with_data() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Create multiple projects
        let project1 = NewProjectDTO {
            name: "Project 1".to_string(),
            description: Some("First project".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let project2 = NewProjectDTO {
            name: "Project 2".to_string(),
            description: Some("Second project".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        service.create(project1, owner_id).unwrap();
        service.create(project2, owner_id).unwrap();

        let result = service.list(owner_id);
        assert!(result.is_ok());

        let projects = result.unwrap();
        assert_eq!(projects.len(), 2);

        // Check that both projects are in the list
        let project_names: Vec<&String> = projects.iter().map(|p| &p.name).collect();
        assert!(project_names.contains(&&"Project 1".to_string()));
        assert!(project_names.contains(&&"Project 2".to_string()));

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_count_projects() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Initially should be 0
        let count = service.count(owner_id).unwrap();
        assert_eq!(count, 0);

        // Create a project
        let new_project = NewProjectDTO {
            name: "Count Test Project".to_string(),
            description: None,
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        service.create(new_project, owner_id).unwrap();

        // Now should be 1
        let count = service.count(owner_id).unwrap();
        assert_eq!(count, 1);

        // Create another project
        let new_project2 = NewProjectDTO {
            name: "Count Test Project 2".to_string(),
            description: None,
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        service.create(new_project2, owner_id).unwrap();

        // Now should be 2
        let count = service.count(owner_id).unwrap();
        assert_eq!(count, 2);

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_project_slug_generation() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        let test_cases = vec![
            ("My Project", "my-project"),
            ("Project with Spaces", "project-with-spaces"),
            ("Project@With#Special$Chars", "project-with-special-chars"),
            ("Project123", "project123"),
            ("UPPERCASE PROJECT", "uppercase-project"),
        ];

        for (i, (name, expected_slug)) in test_cases.iter().enumerate() {
            let unique_name = format!("{} {}", name, i); // Make names unique
            let new_project = NewProjectDTO {
                name: unique_name.clone(),
                description: None,
                settings: None,
                private_model_prices: None,
                usage_limit: None,
            };

            let project = service.create(new_project, owner_id).unwrap();
            // For unique names, we expect the slug to be based on the original name
            let expected_unique_slug =
                format!("{}-{}", expected_slug.replace("-", " "), i).replace(" ", "-");
            assert_eq!(
                project.slug, expected_unique_slug,
                "Failed for name: {}",
                name
            );
        }

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_project_settings_json() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        let settings = ProjectSettings {
            enabled_chat_tracing: true,
        };

        let new_project = NewProjectDTO {
            name: "Settings Test Project".to_string(),
            description: None,
            settings: Some(serde_json::to_value(&settings).unwrap()),
            private_model_prices: None,
            usage_limit: None,
        };

        let project = service.create(new_project, owner_id).unwrap();
        assert_eq!(project.settings, Some(settings));

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_project_uuid_generation() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        let new_project1 = NewProjectDTO {
            name: "UUID Test Project 1".to_string(),
            description: None,
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let new_project2 = NewProjectDTO {
            name: "UUID Test Project 2".to_string(),
            description: None,
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let project1 = service.create(new_project1, owner_id).unwrap();
        let project2 = service.create(new_project2, owner_id).unwrap();

        // Both projects should have different UUIDs
        assert_ne!(project1.id, project2.id);
        assert_ne!(project1.id, Uuid::nil());
        assert_ne!(project2.id, Uuid::nil());

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_delete_project() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // First create a project
        let new_project = NewProjectDTO {
            name: "Project to Delete".to_string(),
            description: Some("This project will be deleted".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let created_project = service.create(new_project, owner_id).unwrap();
        let project_id = created_project.id;

        // Verify the project exists
        let retrieved_project = service.get_by_id(project_id, owner_id).unwrap();
        assert_eq!(retrieved_project.name, "Project to Delete");

        // Delete the project
        let delete_result = service.delete(project_id, owner_id);
        assert!(delete_result.is_ok());

        // Verify the project is no longer retrievable (soft deleted)
        let get_result = service.get_by_id(project_id, owner_id);
        assert!(get_result.is_err());

        // Verify the project doesn't appear in the list
        let projects = service.list(owner_id).unwrap();
        assert_eq!(projects.len(), 0);

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_delete_project_not_found() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Try to delete a project that doesn't exist
        let fake_uuid = Uuid::new_v4();
        let result = service.delete(fake_uuid, owner_id);
        assert!(result.is_err());

        // Check that it's a QueryError with NotFound
        match result.unwrap_err() {
            DatabaseError::QueryError(diesel::result::Error::NotFound) => {
                // This is expected for not found cases
            }
            _ => panic!("Expected QueryError with NotFound for non-existent project"),
        }

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_delete_already_archived_project() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Create a project
        let new_project = NewProjectDTO {
            name: "Project to Archive".to_string(),
            description: Some("This project will be archived twice".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let created_project = service.create(new_project, owner_id).unwrap();
        let project_id = created_project.id;

        // Delete the project (soft delete)
        let delete_result = service.delete(project_id, owner_id);
        assert!(delete_result.is_ok());

        // Try to delete the already archived project
        let second_delete_result = service.delete(project_id, owner_id);
        assert!(second_delete_result.is_err());

        // Should get NotFound error since the project is already archived
        match second_delete_result.unwrap_err() {
            DatabaseError::QueryError(diesel::result::Error::NotFound) => {
                // This is expected for already archived projects
            }
            _ => panic!("Expected QueryError with NotFound for already archived project"),
        }

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_update_project() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // First create a project
        let new_project = NewProjectDTO {
            name: "Original Project".to_string(),
            description: Some("Original description".to_string()),
            settings: Some(serde_json::json!({"feature": "original"})),
            private_model_prices: None,
            usage_limit: None,
        };

        let created_project = service.create(new_project, owner_id).unwrap();
        let project_id = created_project.id;

        // Update the project
        let update_data = UpdateProjectDTO {
            name: Some("Updated Project".to_string()),
            description: Some("Updated description".to_string()),
            settings: Some(serde_json::json!({"feature": "updated"})),
            is_default: Some(true),
        };

        let updated_project = service.update(project_id, owner_id, update_data).unwrap();

        // Verify the updates
        assert_eq!(updated_project.name, "Updated Project");
        assert_eq!(
            updated_project.description,
            Some("Updated description".to_string())
        );
        assert_eq!(
            updated_project.settings,
            Some(ProjectSettings {
                enabled_chat_tracing: true,
            })
        );
        assert_eq!(updated_project.is_default, true);
        assert_eq!(updated_project.slug, "updated-project"); // Should be updated based on new name

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_update_project_partial() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Create a project
        let new_project = NewProjectDTO {
            name: "Test Project".to_string(),
            description: Some("Original description".to_string()),
            settings: Some(serde_json::json!({"feature": "original"})),
            private_model_prices: None,
            usage_limit: None,
        };

        let created_project = service.create(new_project, owner_id).unwrap();
        let project_id = created_project.id;

        // Update only the description
        let update_data = UpdateProjectDTO {
            name: None,
            description: Some("Updated description only".to_string()),
            settings: None,
            is_default: None,
        };

        let updated_project = service.update(project_id, owner_id, update_data).unwrap();

        // Verify only description was updated
        assert_eq!(updated_project.name, "Test Project"); // Should remain unchanged
        assert_eq!(
            updated_project.description,
            Some("Updated description only".to_string())
        );
        assert_eq!(
            updated_project.settings,
            Some(ProjectSettings {
                enabled_chat_tracing: true,
            })
        ); // Should remain unchanged
        assert_eq!(updated_project.is_default, false); // Should remain unchanged
        assert_eq!(updated_project.slug, "test-project"); // Should remain unchanged

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_update_project_not_found() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Try to update a project that doesn't exist
        let fake_uuid = Uuid::new_v4();
        let update_data = UpdateProjectDTO {
            name: Some("Updated Name".to_string()),
            description: None,
            settings: None,
            is_default: None,
        };

        let result = service.update(fake_uuid, owner_id, update_data);
        assert!(result.is_err());

        // Check that it's a QueryError with NotFound
        match result.unwrap_err() {
            DatabaseError::QueryError(diesel::result::Error::NotFound) => {
                // This is expected for not found cases
            }
            _ => panic!("Expected QueryError with NotFound for non-existent project"),
        }

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_update_project_empty_update() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Create a project
        let new_project = NewProjectDTO {
            name: "Test Project".to_string(),
            description: Some("Test description".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let created_project = service.create(new_project, owner_id).unwrap();
        let project_id = created_project.id;

        // Update with no fields (empty update)
        let update_data = UpdateProjectDTO {
            name: None,
            description: None,
            settings: None,
            is_default: None,
        };

        let updated_project = service.update(project_id, owner_id, update_data).unwrap();

        // Verify the project remains unchanged
        assert_eq!(updated_project.name, "Test Project");
        assert_eq!(
            updated_project.description,
            Some("Test description".to_string())
        );
        assert_eq!(updated_project.slug, "test-project");

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_update_project_name_slug_generation() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Create a project
        let new_project = NewProjectDTO {
            name: "Original Project Name".to_string(),
            description: None,
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let created_project = service.create(new_project, owner_id).unwrap();
        let project_id = created_project.id;
        assert_eq!(created_project.slug, "original-project-name");

        // Update the name
        let update_data = UpdateProjectDTO {
            name: Some("New Project Name!".to_string()),
            description: None,
            settings: None,
            is_default: None,
        };

        let updated_project = service.update(project_id, owner_id, update_data).unwrap();

        // Verify the slug was updated based on the new name
        assert_eq!(updated_project.name, "New Project Name!");
        assert_eq!(updated_project.slug, "new-project-name");

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_set_default_project() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Create two projects
        let project1 = NewProjectDTO {
            name: "Project 1".to_string(),
            description: Some("First project".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let project2 = NewProjectDTO {
            name: "Project 2".to_string(),
            description: Some("Second project".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let created_project1 = service.create(project1, owner_id).unwrap();
        let created_project2 = service.create(project2, owner_id).unwrap();

        // Initially, both projects should be non-default
        assert!(!created_project1.is_default);
        assert!(!created_project2.is_default);

        // Set project 1 as default
        let updated_project1 = service.set_default(created_project1.id, owner_id).unwrap();
        assert!(updated_project1.is_default);

        // Verify project 2 is still non-default
        let retrieved_project2 = service.get_by_id(created_project2.id, owner_id).unwrap();
        assert!(!retrieved_project2.is_default);

        // Now set project 2 as default
        let updated_project2 = service.set_default(created_project2.id, owner_id).unwrap();
        assert!(updated_project2.is_default);

        // Verify project 1 is now non-default
        let retrieved_project1 = service.get_by_id(created_project1.id, owner_id).unwrap();
        assert!(!retrieved_project1.is_default);

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_update_project_set_default() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Create two projects
        let project1 = NewProjectDTO {
            name: "Project 1".to_string(),
            description: Some("First project".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let project2 = NewProjectDTO {
            name: "Project 2".to_string(),
            description: Some("Second project".to_string()),
            settings: None,
            private_model_prices: None,
            usage_limit: None,
        };

        let created_project1 = service.create(project1, owner_id).unwrap();
        let created_project2 = service.create(project2, owner_id).unwrap();

        // Initially, both projects should be non-default
        assert!(!created_project1.is_default);
        assert!(!created_project2.is_default);

        // Update project 1 to set it as default
        let update_data = UpdateProjectDTO {
            name: None,
            description: None,
            settings: None,
            is_default: Some(true),
        };

        let updated_project1 = service
            .update(created_project1.id, owner_id, update_data)
            .unwrap();
        assert!(updated_project1.is_default);

        // Verify project 2 is still non-default
        let retrieved_project2 = service.get_by_id(created_project2.id, owner_id).unwrap();
        assert!(!retrieved_project2.is_default);

        // Now update project 2 to set it as default
        let update_data2 = UpdateProjectDTO {
            name: None,
            description: None,
            settings: None,
            is_default: Some(true),
        };

        let updated_project2 = service
            .update(created_project2.id, owner_id, update_data2)
            .unwrap();
        assert!(updated_project2.is_default);

        // Verify project 1 is now non-default
        let retrieved_project1 = service.get_by_id(created_project1.id, owner_id).unwrap();
        assert!(!retrieved_project1.is_default);

        crate::metadata::test_utils::cleanup_test_database();
    }

    #[test]
    fn test_set_default_project_not_found() {
        let db_pool = crate::metadata::test_utils::setup_test_database();
        let service = ProjectServiceImpl::new(db_pool);
        let owner_id = Uuid::nil();

        // Try to set a non-existent project as default
        let fake_uuid = Uuid::new_v4();
        let result = service.set_default(fake_uuid, owner_id);
        assert!(result.is_err());

        // Check that it's a QueryError with NotFound
        match result.unwrap_err() {
            DatabaseError::QueryError(diesel::result::Error::NotFound) => {
                // This is expected for not found cases
            }
            _ => panic!("Expected QueryError with NotFound for non-existent project"),
        }

        crate::metadata::test_utils::cleanup_test_database();
    }
}
