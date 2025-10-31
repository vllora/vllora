pub mod error;
pub mod models;
pub mod pool;
pub mod project_trace;
pub mod schema;
pub mod services;
#[cfg(test)]
pub mod test_utils;
pub mod utils;

#[cfg(feature = "sqlite")]
pub type DB = diesel::sqlite::Sqlite;
#[cfg(feature = "postgres")]
pub type DB = diesel::pg::Pg;
