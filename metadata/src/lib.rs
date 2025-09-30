pub mod pool;
pub mod schema;
pub mod models;
pub mod utils;
pub mod services;
pub mod error;

#[cfg(feature = "sqlite")]
pub type DB = diesel::sqlite::Sqlite;
#[cfg(feature = "postgres")]
pub type DB = diesel::pg::Pg;