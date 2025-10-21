use actix_web::{web, HttpResponse, Result};
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::thread::ThreadService;
use langdb_core::types::metadata::project::Project;
use langdb_core::types::threads::{MessageThread, PageOptions, PageOrderType};
use serde::{Deserialize, Serialize};

/// Extract the first N words from a text string
fn extract_first_n_words(text: &str, n: usize) -> String {
    text.split_whitespace()
        .take(n)
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Deserialize)]
pub struct ListThreadsRequest {
    #[serde(default, flatten)]
    pub page_options: Option<PageOptions>,
}

#[derive(Deserialize)]
pub struct UpdateThreadRequest {
    pub title: Option<String>,
}

#[derive(Serialize)]
pub struct ThreadSpan {
    pub thread_id: String,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub run_ids: Vec<String>,
    pub input_models: Vec<String>,
    pub cost: f64,
    pub title: Option<String>,
}

#[derive(Serialize)]
pub struct ListThreadsResponse {
    pub data: Vec<ThreadSpan>,
    pub pagination: Pagination,
}

#[derive(Serialize)]
pub struct UpdateThreadResponse {
    pub thread: MessageThread,
}

#[derive(Serialize)]
pub struct GetThreadResponse {
    pub thread: ThreadSpan,
}


#[derive(Serialize)]
pub struct Pagination {
    pub offset: usize,
    pub limit: usize,
    pub total: i64,
}

/// GET /threads - List threads (root spans with thread_id and no parent_span_id)
pub async fn list_threads(
    db_pool: web::Data<DbPool>,
    query: web::Query<ListThreadsRequest>,
    project: web::ReqData<Project>,
) -> Result<HttpResponse> {
    let page_options: PageOptions = query.page_options.clone().unwrap_or(PageOptions {
        order_by: vec![("created_at".to_string(), PageOrderType::Desc)],
        limit: Some(50),
        offset: None,
    });

    let project = project.into_inner();
    let limit = page_options.limit.unwrap_or(50) as i64;
    let offset = page_options.offset.unwrap_or(0) as i64;

    let thread_service = ThreadService::new(db_pool.get_ref().clone());

    // Query thread spans using the service
    let results = match thread_service.list_thread_spans(&project.slug, limit, offset) {
        Ok(results) => results,
        Err(e) => {
            tracing::error!("Failed to query threads for project {}: {:?}", project.slug, e);
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list threads",
                "message": e.to_string()
            })));
        }
    };

    // Count total threads
    let total = match thread_service.count_thread_spans(&project.slug) {
        Ok(count) => count,
        Err(e) => {
            tracing::error!("Failed to count threads for project {}: {:?}", project.slug, e);
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to count threads",
                "message": e.to_string()
            })));
        }
    };

    // Convert results to ThreadSpan
    let data: Vec<ThreadSpan> = results
        .into_iter()
        .map(|result| {
            let title = result.title.and_then(|t| {
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(extract_first_n_words(trimmed, 10))
                }
            });

            ThreadSpan {
                thread_id: result.thread_id,
                start_time_us: result.start_time_us,
                finish_time_us: result.finish_time_us,
                run_ids: result.run_ids
                    .map(|ids| ids.split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect())
                    .unwrap_or_default(),
                input_models: result.input_models
                    .map(|models| models.split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect())
                    .unwrap_or_default(),
                cost: result.cost,
                title,
            }
        })
        .collect();

    let response = ListThreadsResponse {
        data,
        pagination: Pagination {
            offset: page_options.offset.unwrap_or(0),
            limit: page_options.limit.unwrap_or(50),
            total,
        },
    };

    Ok(HttpResponse::Ok().json(response))
}

/// GET /threads/{id} - Get thread by ID
pub async fn get_thread(
    path: web::Path<uuid::Uuid>,
    project: web::ReqData<Project>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let thread_id = path.into_inner().to_string();
    let project = project.into_inner();

    let thread_service = ThreadService::new(db_pool.get_ref().clone());

    // Query the thread using the service
    let result = match thread_service.get_thread_span(&thread_id, &project.slug) {
        Ok(Some(result)) => result,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(serde_json::json!({
                "error": "Thread not found",
                "message": format!("Thread with ID {} not found", thread_id)
            })));
        }
        Err(e) => {
            tracing::error!("Failed to query thread {} for project {}: {:?}", thread_id, project.slug, e);
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to get thread",
                "message": e.to_string()
            })));
        }
    };

    let title = result.title.and_then(|t| {
        let trimmed = t.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(extract_first_n_words(trimmed, 10))
        }
    });

    let thread_span = ThreadSpan {
        thread_id: result.thread_id,
        start_time_us: result.start_time_us,
        finish_time_us: result.finish_time_us,
        run_ids: result.run_ids
            .map(|ids| ids.split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect())
            .unwrap_or_default(),
        input_models: result.input_models
            .map(|models| models.split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect())
            .unwrap_or_default(),
        cost: result.cost,
        title,
    };

    let response = GetThreadResponse { thread: thread_span };
    Ok(HttpResponse::Ok().json(response))
}

/// PUT /threads/{id} - Update thread title
pub async fn update_thread(
    path: web::Path<uuid::Uuid>,
    project: web::ReqData<Project>,
    req: web::Json<UpdateThreadRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let thread_id = path.into_inner().to_string();
    let project = project.into_inner();

    let thread_service = ThreadService::new(db_pool.get_ref().clone());

    // First, verify the thread exists and belongs to the project
    match thread_service.get_thread_span(&thread_id, &project.slug) {
        Ok(Some(thread_span)) => {
            // Thread exists, update the span's title attribute
            if let Some(ref title) = req.title {
                if let Err(e) = thread_service.update_thread_title(&thread_id, &project.slug, title) {
                    tracing::error!("Failed to update span attribute for thread {}: {:?}", thread_id, e);
                    return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to update thread title",
                        "message": e.to_string()
                    })));
                }
            }

            // Return the updated thread span
            // Parse the first model from the comma-separated string
            let first_model = thread_span
                .input_models
                .as_ref()
                .and_then(|models| {
                    models.split(',')
                        .next()
                        .map(|s| s.trim().to_string())
                })
                .unwrap_or_default();

            let response = UpdateThreadResponse {
                thread: MessageThread {
                    id: thread_span.thread_id.clone(),
                    model_name: first_model,
                    user_id: String::new(),
                    project_id: project.slug.clone(),
                    is_public: false,
                    title: req.title.clone(),
                    description: None,
                    keywords: None,
                },
            };
            Ok(HttpResponse::Ok().json(response))
        }
        Ok(None) => {
            Ok(HttpResponse::NotFound().json(serde_json::json!({
                "error": "Thread not found",
                "message": format!("Thread with ID {} not found", thread_id)
            })))
        }
        Err(e) => {
            tracing::error!("Failed to verify thread {}: {:?}", thread_id, e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to verify thread",
                "message": e.to_string()
            })))
        }
    }
}

