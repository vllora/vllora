use crate::metadata::pool::DbPool;
use crate::types::metadata::project::Project;
use actix_web::{web, HttpResponse, Result};
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Nullable, Text};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ListLabelsQueryParams {
    /// Filter to specific thread (optional)
    #[serde(alias = "threadId")]
    pub thread_id: Option<String>,
    /// Maximum number of labels to return (default: 100)
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct LabelInfo {
    pub name: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct ListLabelsResponse {
    pub labels: Vec<LabelInfo>,
}

/// GET /labels - List unique labels with counts
///
/// Query parameters:
/// - threadId (optional): Filter to labels within a specific thread
/// - limit (optional): Maximum number of labels to return (default: 100)
///
/// Returns list of unique labels sorted by count (descending)
pub async fn list_labels(
    query: web::Query<ListLabelsQueryParams>,
    project: web::ReqData<Project>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let mut conn = db_pool
        .get()
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))?;

    let project_slug = project.slug.clone();
    let limit = query.limit.unwrap_or(100);

    #[derive(QueryableByName)]
    struct LabelRow {
        #[diesel(sql_type = Nullable<Text>)]
        label: Option<String>,
        #[diesel(sql_type = BigInt)]
        count: i64,
    }

    // Build SQL query with optional thread_id filter
    let sql = if let Some(thread_id) = &query.thread_id {
        format!(
            "SELECT json_extract(attribute, '$.label') as label, COUNT(*) as count \
             FROM traces \
             WHERE project_id = '{}' \
               AND thread_id = '{}' \
               AND json_extract(attribute, '$.label') IS NOT NULL \
             GROUP BY label \
             ORDER BY count DESC \
             LIMIT {}",
            project_slug.replace('\'', "''"),
            thread_id.replace('\'', "''"),
            limit
        )
    } else {
        format!(
            "SELECT json_extract(attribute, '$.label') as label, COUNT(*) as count \
             FROM traces \
             WHERE project_id = '{}' \
               AND json_extract(attribute, '$.label') IS NOT NULL \
             GROUP BY label \
             ORDER BY count DESC \
             LIMIT {}",
            project_slug.replace('\'', "''"),
            limit
        )
    };

    let results = diesel::sql_query(sql)
        .load::<LabelRow>(&mut conn)
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))?;

    let labels: Vec<LabelInfo> = results
        .into_iter()
        .filter_map(|row| {
            row.label.map(|name| LabelInfo {
                name,
                count: row.count,
            })
        })
        .collect();

    Ok(HttpResponse::Ok().json(ListLabelsResponse { labels }))
}
