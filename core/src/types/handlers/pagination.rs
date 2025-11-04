use rmcp::schemars;
use serde::Serialize;

#[derive(Serialize, schemars::JsonSchema)]
pub struct PaginatedResult<T> {
    pub pagination: Pagination,
    pub data: Vec<T>,
}

impl<T> PaginatedResult<T> {
    pub fn new(data: Vec<T>, pagination: Pagination) -> Self {
        Self { pagination, data }
    }
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct Pagination {
    pub offset: i64,
    pub limit: i64,
    pub total: i64,
}
