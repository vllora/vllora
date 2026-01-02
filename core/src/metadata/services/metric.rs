use crate::metadata::error::DatabaseError;
use crate::metadata::models::metric::DbNewMetric;
use crate::metadata::pool::DbPool;
use crate::metadata::schema::metrics;
use crate::metadata::DatabaseServiceTrait;
use diesel::prelude::*;

#[derive(Clone)]
pub struct MetricsServiceImpl {
    db_pool: DbPool,
}

impl DatabaseServiceTrait for MetricsServiceImpl {
    fn init(db_pool: DbPool) -> Self {
        Self { db_pool }
    }
}

impl MetricsServiceImpl {
    pub fn insert_many(&self, metric_list: Vec<DbNewMetric>) -> Result<usize, DatabaseError> {
        if metric_list.is_empty() {
            return Ok(0);
        }

        let mut conn = self.db_pool.get()?;
        let mut inserted_count = 0;

        for metric in &metric_list {
            diesel::insert_into(metrics::table)
                .values(metric)
                .on_conflict((
                    metrics::metric_name,
                    metrics::timestamp_us,
                    metrics::attributes,
                    metrics::trace_id,
                    metrics::span_id,
                ))
                .do_nothing()
                .execute(&mut conn)?;
            inserted_count += 1;
        }

        Ok(inserted_count)
    }
}
