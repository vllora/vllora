use diesel::r2d2::ConnectionManager;
use r2d2::Pool;

#[cfg(feature = "sqlite")]
use diesel::connection::SimpleConnection;
#[cfg(feature = "postgres")]
use diesel::pg::PgConnection as Connection;
#[cfg(feature = "sqlite")]
use diesel::sqlite::SqliteConnection as Connection;

pub type DbPool = Pool<ConnectionManager<Connection>>;

pub fn establish_connection(database_url: String, max_size: u32) -> DbPool {
    let manager = ConnectionManager::new(database_url);
    build_pool(manager, max_size)
}

#[cfg(feature = "sqlite")]
fn build_pool(manager: ConnectionManager<Connection>, max_size: u32) -> DbPool {
    r2d2::Pool::builder()
        .max_size(max_size)
        .connection_customizer(Box::new(ConnectionOptions {
            enable_wal: true,
            enable_foreign_keys: true,
            busy_timeout: Some(std::time::Duration::from_secs(30)),
        }))
        .build(manager)
        .expect("Failed to create DB pool")
}

#[cfg(feature = "postgres")]
fn build_pool(manager: ConnectionManager<Connection>, max_size: u32) -> DbPool {
    r2d2::Pool::builder()
        .max_size(max_size)
        .build(manager)
        .expect("Failed to create DB pool")
}

#[cfg(feature = "sqlite")]
#[derive(Debug)]
pub struct ConnectionOptions {
    pub enable_wal: bool,
    pub enable_foreign_keys: bool,
    pub busy_timeout: Option<std::time::Duration>,
}

#[cfg(feature = "sqlite")]
impl diesel::r2d2::CustomizeConnection<Connection, diesel::r2d2::Error> for ConnectionOptions {
    fn on_acquire(&self, conn: &mut Connection) -> Result<(), diesel::r2d2::Error> {
        (|| {
            if self.enable_wal {
                conn.batch_execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
            }
            if self.enable_foreign_keys {
                conn.batch_execute("PRAGMA foreign_keys = ON;")?;
            }
            if let Some(d) = self.busy_timeout {
                conn.batch_execute(&format!("PRAGMA busy_timeout = {};", d.as_millis()))?;
            }
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
}
