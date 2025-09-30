use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use crate::DB;
use crate::pool::DbPool;
use ::tracing::info;
use std::error::Error;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./sqlite_migrations");

pub fn init_db(db_pool: &DbPool) {
    let mut db_connection = db_pool.get().unwrap();
    // run migration
    run_migrations(&mut db_connection).unwrap();
}

fn run_migrations(
    connection: &mut impl MigrationHarness<DB>,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    info!("Running migrations");
    connection.run_pending_migrations(MIGRATIONS)?;
    info!("Migrations complete");
    Ok(())
}

