use crate::error::DatabaseError;
use crate::models::project::{DbProject, DbNewProject, NewProjectDTO};
use crate::pool::DbPool;
use langdb_core::types::metadata::project::Project;
use uuid::Uuid;
use diesel::dsl::count;
use diesel::ExpressionMethods;
use diesel::{QueryDsl, RunQueryDsl};

pub trait ProjectService {
    fn get_by_id(&self, id: Uuid, owner_id: Uuid) -> Result<Project, DatabaseError>;
    fn create(&self, proj: NewProjectDTO, owner_id: Uuid) -> Result<Project, DatabaseError>;
    fn count(&self, owner_id: Uuid) -> Result<u64, DatabaseError>;
    fn list(&self, owner_id: Uuid) -> Result<Vec<Project>, DatabaseError>;
}

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
            .filter(crate::schema::projects::id.eq(id.to_string()))
            .first(&mut conn)
            .map_err(DatabaseError::QueryError)
            .map(|db_project| db_project.into())
    }

    fn create(&self, proj: NewProjectDTO, _owner_id: Uuid) -> Result<Project, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        
        let settings_json = proj.settings.as_ref()
            .map(|s| serde_json::to_string(s))
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

        diesel::insert_into(crate::schema::projects::table)
            .values(&db_new_project)
            .execute(&mut conn)?;

        // For SQLite, we need to query the inserted record separately
        // We'll use the name as a unique identifier to find the inserted record
        let inserted_project = DbProject::not_archived()
            .filter(crate::schema::projects::name.eq(proj.name))
            .order(crate::schema::projects::created_at.desc())
            .first(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(inserted_project.into())
    }

    fn count(&self, _owner_id: Uuid) -> Result<u64, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let count_result: i64 = DbProject::not_archived()
            .select(count(crate::schema::projects::id))
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
}
