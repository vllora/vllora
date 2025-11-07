use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::thread::ThreadService;
use vllora_core::types::metadata::project::Project;
use vllora_core::types::threads::service::ThreadSpan;
use vllora_core::types::threads::MessageThread;

#[derive(Deserialize)]
pub struct UpdateThreadRequest {
    pub title: Option<String>,
}

#[derive(Serialize)]
pub struct UpdateThreadResponse {
    pub thread: MessageThread,
}

#[derive(Serialize)]
pub struct GetThreadResponse {
    pub thread: ThreadSpan,
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
            tracing::error!(
                "Failed to query thread {} for project {}: {:?}",
                thread_id,
                project.slug,
                e
            );
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to get thread",
                "message": e.to_string()
            })));
        }
    };

    let response = GetThreadResponse {
        thread: ThreadSpan::from(result),
    };
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
                if let Err(e) = thread_service.update_thread_title(&thread_id, &project.slug, title)
                {
                    tracing::error!(
                        "Failed to update span attribute for thread {}: {:?}",
                        thread_id,
                        e
                    );
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
                .and_then(|models| models.split(',').next().map(|s| s.trim().to_string()))
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
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Thread not found",
            "message": format!("Thread with ID {} not found", thread_id)
        }))),
        Err(e) => {
            tracing::error!("Failed to verify thread {}: {:?}", thread_id, e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to verify thread",
                "message": e.to_string()
            })))
        }
    }
}
