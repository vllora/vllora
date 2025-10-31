use crate::metadata::pool::DbPool;

/// Creates a fresh test database by dropping and recreating it
pub fn setup_test_database() -> DbPool {
    // Use an in-memory database for tests to avoid file I/O issues
    let test_db_path = format!(":memory:");

    // Create new database pool
    let db_pool = crate::metadata::pool::establish_connection(test_db_path, 5);

    // Initialize the database schema
    crate::metadata::utils::init_db(&db_pool);

    db_pool
}

/// Seeds the test database with sample data
pub fn seed_test_database(db_pool: &DbPool) {
    use crate::metadata::models::project::NewProjectDTO;
    use crate::metadata::services::project::{ProjectService, ProjectServiceImpl};

    let project_service = ProjectServiceImpl::new(db_pool.clone());
    let dummy_owner_id = uuid::Uuid::nil();

    // Create a default test project
    let default_project = NewProjectDTO {
        name: "Test Project".to_string(),
        description: Some("A test project for unit testing".to_string()),
        settings: Some(serde_json::json!({
            "enabled_chat_tracing": true
        })),
        private_model_prices: None,
        usage_limit: None,
    };

    project_service
        .create(default_project, dummy_owner_id)
        .expect("Failed to create test project");

    // Create another test project
    let second_project = NewProjectDTO {
        name: "Second Test Project".to_string(),
        description: Some("Another test project".to_string()),
        settings: None,
        private_model_prices: None,
        usage_limit: None,
    };

    project_service
        .create(second_project, dummy_owner_id)
        .expect("Failed to create second test project");
}

/// Cleans up the test database
pub fn cleanup_test_database() {
    // Note: With unique database names in temp directory, cleanup is less critical
    // The OS will eventually clean up temp files
    // We can still try to clean up any remaining test files in temp directory
    let temp_dir = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(temp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                if filename.starts_with("test_metadata_") && filename.ends_with(".sqlite") {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }
}
