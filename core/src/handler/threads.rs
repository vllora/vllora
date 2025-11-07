use crate::types::threads::service::ThreadSpan;
use crate::types::threads::ThreadServiceWrapper;
use crate::types::threads::{MessageThread, PageOptions, PageOrderType};
use actix_web::{web, HttpResponse, Result};
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

/// GET /threads - List threads (root spans with thread_id and no parent_span_id)
pub async fn list_threads(
    query: web::Query<ListThreadsRequest>,
    thread_service_wrapper: web::ReqData<Rc<dyn ThreadServiceWrapper>>,
) -> Result<HttpResponse> {
    let page_options: PageOptions = query.page_options.clone().unwrap_or(PageOptions {
        order_by: vec![("created_at".to_string(), PageOrderType::Desc)],
        limit: Some(50),
        offset: None,
    });

    let limit = page_options.limit.unwrap_or(50) as i64;
    let offset = page_options.offset.unwrap_or(0) as i64;

    thread_service_wrapper
        .service()
        .list_threads(limit, offset)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))
        .map(|response| HttpResponse::Ok().json(response))
}
