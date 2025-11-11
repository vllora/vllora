pub mod error;
pub mod models;
pub mod pool;
pub mod project_trace;
pub mod schema;
pub mod services;
#[cfg(test)]
pub mod test_utils;
pub mod types;
pub mod utils;

use crate::metadata::pool::DbPool;

#[cfg(feature = "sqlite")]
pub type DB = diesel::sqlite::Sqlite;
#[cfg(feature = "postgres")]
pub type DB = diesel::pg::Pg;

#[derive(Clone)]
pub struct DatabaseService {
    db_pool: DbPool,
}

impl DatabaseService {
    pub fn new(db_pool: DbPool) -> DatabaseService {
        DatabaseService { db_pool }
    }

    pub fn init<T: DatabaseServiceTrait>(&self) -> T {
        T::init(self.db_pool.clone())
    }
}

pub trait DatabaseServiceTrait {
    fn init(db_pool: DbPool) -> Self;
}
