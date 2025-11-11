use crate::metadata::services::project::ProjectServiceImpl;
use crate::telemetry::TraceTenantResolver;
use crate::types::metadata::services::project::ProjectService;
use std::{collections::HashMap, sync::Arc};
use tonic::metadata::MetadataMap;
use uuid::Uuid;

struct InMemoryCache {
    values: HashMap<String, String>,
}

impl InMemoryCache {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn read(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    pub fn save(&mut self, key: &str, value: String) {
        self.values.insert(key.to_string(), value);
    }
}

pub struct ProjectTraceTenantResolver {
    project_service: ProjectServiceImpl,
    cache: Arc<tokio::sync::Mutex<InMemoryCache>>,
}

impl std::fmt::Debug for ProjectTraceTenantResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraceTenantResolverImpl").finish()
    }
}

impl ProjectTraceTenantResolver {
    pub fn new(project_service: ProjectServiceImpl) -> Self {
        Self {
            project_service,
            cache: Arc::new(tokio::sync::Mutex::new(InMemoryCache::new())),
        }
    }
}

#[async_trait::async_trait]
impl TraceTenantResolver for ProjectTraceTenantResolver {
    async fn get_tenant_id(&self, metadata: &MetadataMap) -> Option<(String, String)> {
        let project_id = metadata
            .get("x-project-id")
            .and_then(|value| value.to_str().ok());

        if let Some(Ok(project_id)) = project_id.map(uuid::Uuid::parse_str) {
            let mut cache = self.cache.lock().await;
            if let Some(t) = cache.read(&project_id.to_string()) {
                return Some(("default".to_string(), t));
            }

            let project = self.project_service.get_by_id(project_id, Uuid::new_v4());
            if let Ok(project) = project {
                cache.save(&project_id.to_string(), project.slug.clone());
                return Some(("default".to_string(), project.slug.clone()));
            }
        } else {
            let projects = self.project_service.list(Uuid::new_v4());
            if let Ok(projects) = projects {
                if projects.len() == 1 {
                    let project = projects.first();
                    let mut cache = self.cache.lock().await;
                    if let Some(project) = project {
                        cache.save(&project.id.to_string(), project.slug.clone());
                        return Some(("default".to_string(), project.slug.clone()));
                    }
                }
            }
        }

        None
    }
}
