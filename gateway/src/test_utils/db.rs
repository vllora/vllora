use langdb_metadata::pool::DbPool;
use uuid::Uuid;

/// Creates a fresh test database by dropping and recreating it
pub fn setup_test_database() -> DbPool {
    // Use an in-memory database for tests to avoid file I/O issues
    let test_db_path = ":memory:";
    
    // Create new database pool
    let db_pool = langdb_metadata::pool::establish_connection(test_db_path.to_string(), 5);
    
    // Initialize the database schema
    langdb_metadata::utils::init_db(&db_pool);
    
    db_pool
}

/// Seeds the test database with sample data
pub fn seed_test_database(db_pool: &DbPool) {
    use langdb_metadata::services::project::{ProjectService, ProjectServiceImpl};
    use langdb_metadata::models::project::NewProjectDTO;
    use std::sync::Arc;
    
    let project_service = ProjectServiceImpl::new(Arc::new(db_pool.clone()));
    let dummy_owner_id = Uuid::nil();
    
    // Create a default test project
    let default_project = NewProjectDTO {
        name: "Test Project".to_string(),
        description: Some("A test project for unit testing".to_string()),
        settings: Some(serde_json::json!({
            "feature_flags": {
                "chat_tracing": true
            }
        })),
        private_model_prices: None,
        usage_limit: None,
    };
    
    project_service.create(default_project, dummy_owner_id)
        .expect("Failed to create test project");
    
    // Create another test project
    let second_project = NewProjectDTO {
        name: "Second Test Project".to_string(),
        description: Some("Another test project".to_string()),
        settings: None,
        private_model_prices: None,
        usage_limit: None,
    };
    
    project_service.create(second_project, dummy_owner_id)
        .expect("Failed to create second test project");
}

/// Cleans up the test database
pub fn cleanup_test_database() {
    // No cleanup needed for in-memory databases
    // The database is automatically cleaned up when the connection is dropped
}
