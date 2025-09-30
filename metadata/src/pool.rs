use diesel::r2d2::ConnectionManager;
use r2d2::Pool;

#[cfg(feature = "postgres")]
use diesel::pg::PgConnection as Connection;
#[cfg(feature = "sqlite")]
use diesel::sqlite::SqliteConnection as Connection;

pub type DbPool = Pool<ConnectionManager<Connection>>;

pub fn establish_connection(database_url: String, max_size: u32) -> DbPool {
    let manager = ConnectionManager::new(database_url);
    r2d2::Pool::builder()
        .max_size(max_size)
        .build(manager)
        .expect("Failed to create DB pool")
}
