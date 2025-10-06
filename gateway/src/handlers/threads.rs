use actix_web::{web, HttpResponse, Result};
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::message::MessageService;
use langdb_core::metadata::services::thread::ThreadService;
use langdb_core::types::metadata::project::Project;
use langdb_core::types::threads::{
    MessageThread, MessageThreadWithTitle, PageOptions, PageOrderType,
};
use serde::{Deserialize, Serialize};

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
pub struct ListThreadsResponse {
    pub data: Vec<MessageThreadWithTitle>,
    pub pagination: Pagination,
}

#[derive(Serialize)]
pub struct UpdateThreadResponse {
    pub thread: MessageThread,
}

#[derive(Deserialize)]
pub struct ThreadMessagesQuery {
    #[serde(default, flatten)]
    pub page_options: Option<PageOptions>,
}

#[derive(Serialize)]
pub struct Pagination {
    pub offset: usize,
    pub limit: usize,
    pub total: i64,
}

/// GET /threads - List threads for a single project ordered by last_message_date
pub async fn list_threads(
    db_pool: web::Data<DbPool>,
    query: web::Query<ListThreadsRequest>,
    body: web::Json<ListThreadsRequest>,
    project: web::ReqData<Project>,
) -> Result<HttpResponse> {
    let page_options = body
        .page_options
        .clone()
        .unwrap_or(query.page_options.clone().unwrap_or(PageOptions {
            order_by: vec![("created_at".to_string(), PageOrderType::Desc)],
            limit: Some(50),
            offset: None,
        }));

    // Get project from middleware
    let project = project.into_inner();

    let thread_service = ThreadService::new(db_pool.get_ref().clone());

    match thread_service.list_threads_by_project(&project.slug.to_string(), page_options.clone()) {
        Ok(data) => {
            let total = match thread_service.count_threads_by_project(&project.slug.to_string()) {
                Ok(total) => total,
                Err(e) => {
                    tracing::error!(
                        "Failed to count threads for project {}: {:?}",
                        project.slug,
                        e
                    );
                    return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to count threads",
                        "message": e.to_string()
                    })));
                }
            };

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
        Err(e) => {
            tracing::error!("Failed to list threads for project {}: {:?}", project.id, e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list threads",
                "message": e.to_string()
            })))
        }
    }
}

/// PUT /threads/{id} - Update thread title
pub async fn update_thread(
    path: web::Path<uuid::Uuid>,
    project: web::ReqData<Project>,
    req: web::Json<UpdateThreadRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let thread_id = path.into_inner().to_string();

    let thread_service = ThreadService::new(db_pool.get_ref().clone());

    // First, verify the thread exists and belongs to the project
    match thread_service.get_thread_by_id(&thread_id) {
        Ok(thread) => {
            if thread.project_id != project.slug {
                return Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Thread not found",
                    "message": "Thread does not belong to this project"
                })));
            }

            let update_data = langdb_core::metadata::models::thread::UpdateThreadDTO {
                user_id: None,
                model_name: None,
                is_public: None,
                description: None,
                keywords: None,
                title: req.title.clone(),
            };

            match thread_service.update_thread(&thread_id, update_data) {
                Ok(updated_thread) => {
                    let response = UpdateThreadResponse {
                        thread: updated_thread,
                    };
                    Ok(HttpResponse::Ok().json(response))
                }
                Err(e) => {
                    tracing::error!("Failed to update thread {}: {:?}", thread_id, e);
                    Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to update thread",
                        "message": e.to_string()
                    })))
                }
            }
        }
        Err(_) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Thread not found",
            "message": format!("Thread with ID {} not found", thread_id)
        }))),
    }
}

/// GET /threads/messages - Get messages for a thread
pub async fn get_thread_messages(
    thread_id: web::Path<uuid::Uuid>,
    query: web::Query<ThreadMessagesQuery>,
    db_pool: web::Data<DbPool>,
    project: web::ReqData<Project>,
) -> Result<HttpResponse> {
    let page_options = query.page_options.clone();

    let thread_id = thread_id.into_inner().to_string();

    let thread_service = ThreadService::new(db_pool.get_ref().clone());

    // First, verify the thread exists and belongs to the project
    match thread_service.get_thread_by_id(&thread_id) {
        Ok(thread) => {
            if thread.project_id != project.slug {
                return Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Thread not found",
                    "message": "Thread does not belong to this project"
                })));
            }

            let message_service = MessageService::new(db_pool.get_ref().clone());

            match message_service.get_thread_messages_with_metrics(
                &thread_id,
                page_options.unwrap_or(PageOptions {
                    order_by: vec![("created_at".to_string(), PageOrderType::Asc)],
                    limit: Some(50),
                    offset: None,
                }),
            ) {
                Ok(messages) => Ok(HttpResponse::Ok().json(messages)),
                Err(e) => {
                    tracing::error!("Failed to get messages for thread {}: {:?}", thread_id, e);
                    Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": "Failed to get messages",
                        "message": e.to_string()
                    })))
                }
            }
        }
        Err(_) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Thread not found",
            "message": format!("Thread with ID {} not found", thread_id)
        }))),
    }
}

/// GET /threads/messages/{message_id} - Get message for a thread
pub async fn get_thread_message(
    message_id: web::Path<uuid::Uuid>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let message_service = MessageService::new(db_pool.get_ref().clone());

    let message = message_service.get_message_by_id(&message_id.into_inner().to_string());

    match message {
        Ok(Some(message)) => Ok(HttpResponse::Ok().json(message)),
        Ok(None) => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Message not found",
            "message": "Message not found"
        }))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "Failed to get message",
            "message": e.to_string()
        }))),
    }
}
