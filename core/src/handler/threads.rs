use crate::types::threads::service::ThreadSpan;
use crate::types::threads::ThreadServiceWrapper;
use crate::types::threads::{MessageThread, PageOptions, PageOrderType};
use actix_web::{web, HttpRequest, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

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
pub struct UpdateThreadResponse {
    pub thread: MessageThread,
}

#[derive(Serialize)]
pub struct GetThreadResponse {
    pub thread: ThreadSpan,
}

fn get_default_page_options() -> PageOptions {
    PageOptions {
        order_by: vec![("created_at".to_string(), PageOrderType::Desc)],
        limit: Some(50),
        offset: None,
    }
}

/// GET /threads - List threads (root spans with thread_id and no parent_span_id)
pub async fn list_threads(
    req: HttpRequest,
    query: web::Query<ListThreadsRequest>,
    body: Option<web::Json<PageOptions>>,
    thread_service_wrapper: web::ReqData<Rc<dyn ThreadServiceWrapper>>,
) -> Result<HttpResponse> {
    // For POST requests, prefer JSON body; for GET requests, use query params
    let page_options: PageOptions = if req.method() == actix_web::http::Method::POST {
        body.map(|b| b.into_inner())
            .unwrap_or_else(get_default_page_options)
    } else {
        query
            .page_options
            .clone()
            .unwrap_or_else(get_default_page_options)
    };

    let limit = page_options.limit.unwrap_or(50) as i64;
    let offset = page_options.offset.unwrap_or(0) as i64;

    thread_service_wrapper
        .service()
        .list_threads(limit, offset)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))
        .map(|response| HttpResponse::Ok().json(response))
}
