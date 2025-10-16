use actix_web::{web, HttpResponse, Result};
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Nullable, Text};
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::thread::ThreadService;
use langdb_core::types::metadata::project::Project;
use langdb_core::types::threads::{MessageThread, PageOptions, PageOrderType};
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

#[derive(QueryableByName, Debug)]
struct ThreadQueryResult {
    #[diesel(sql_type = Text)]
    thread_id: String,
    #[diesel(sql_type = BigInt)]
    start_time_us: i64,
    #[diesel(sql_type = BigInt)]
    finish_time_us: i64,
    #[diesel(sql_type = Nullable<Text>)]
    run_ids: Option<String>, // comma-separated
    #[diesel(sql_type = Nullable<Text>)]
    input_models: Option<String>, // comma-separated
    #[diesel(sql_type = diesel::sql_types::Double)]
    cost: f64,
}

#[derive(Serialize)]
pub struct ThreadSpan {
    pub thread_id: String,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub run_ids: Vec<String>,
    pub input_models: Vec<String>,
    pub cost: f64,
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

    // Get project from middleware
    let project = project.into_inner();

    let limit = page_options.limit.unwrap_or(50) as i64;
    let offset = page_options.offset.unwrap_or(0) as i64;

    let mut conn = match db_pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            tracing::error!("Failed to get database connection: {:?}", e);
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Database connection error",
                "message": e.to_string()
            })));
        }
    };

    // Optimized SQL query - single scan of traces table
    let query_sql = r#"
        WITH thread_aggregates AS (
            SELECT
                thread_id,
                MIN(CASE WHEN parent_span_id IS NULL THEN start_time_us END) as start_time_us,
                MAX(CASE WHEN parent_span_id IS NULL THEN finish_time_us END) as finish_time_us,
                GROUP_CONCAT(DISTINCT CASE WHEN parent_span_id IS NULL THEN run_id END) as run_ids,
                GROUP_CONCAT(DISTINCT json_extract(attribute, '$.model_name')) as input_models,
                SUM(CAST(json_extract(attribute, '$.cost') AS REAL)) as cost
            FROM traces
            WHERE project_id = ?
                AND thread_id IS NOT NULL
            GROUP BY thread_id
            HAVING start_time_us IS NOT NULL
        )
        SELECT
            thread_id,
            start_time_us,
            finish_time_us,
            run_ids,
            input_models,
            COALESCE(cost, 0.0) as cost
        FROM thread_aggregates
        ORDER BY start_time_us DESC
        LIMIT ? OFFSET ?
    "#;

    let results: Vec<ThreadQueryResult> = match diesel::sql_query(query_sql)
        .bind::<Text, _>(&project.slug) // project_id filter
        .bind::<BigInt, _>(limit)
        .bind::<BigInt, _>(offset)
        .load(&mut conn)
    {
        Ok(results) => results,
        Err(e) => {
            tracing::error!("Failed to query threads for project {}: {:?}", project.slug, e);
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list threads",
                "message": e.to_string()
            })));
        }
    };

    // Count total unique thread_ids (optimized)
    let count_sql = r#"
        SELECT COUNT(DISTINCT thread_id) as count
        FROM traces
        WHERE project_id = ?
            AND thread_id IS NOT NULL
            AND parent_span_id IS NULL
    "#;

    #[derive(QueryableByName)]
    struct CountResult {
        #[diesel(sql_type = BigInt)]
        count: i64,
    }

    let total: i64 = match diesel::sql_query(count_sql)
        .bind::<Text, _>(&project.slug)
        .load::<CountResult>(&mut conn)
    {
        Ok(mut counts) => counts.pop().map(|c| c.count).unwrap_or(0),
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
        .map(|result| ThreadSpan {
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

