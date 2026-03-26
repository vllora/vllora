use crate::metadata::error::DatabaseError;
use crate::metadata::models::knowledge_source::{
    DbKnowledgeSource, DbNewKnowledgeSource, KnowledgeSource, NewKnowledgeSource,
};
use crate::metadata::models::knowledge_source_part::{
    DbKnowledgeSourcePart, DbNewKnowledgeSourcePart, KnowledgeSourcePart, NewKnowledgeSourcePart,
};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::knowledge_source_parts::dsl as parts_dsl;
use crate::metadata::schema::knowledge_sources::dsl;
use diesel::BoolExpressionMethods;
use diesel::Connection;
use diesel::ExpressionMethods;
use diesel::JoinOnDsl;
use diesel::OptionalExtension;
use diesel::QueryDsl;
use diesel::RunQueryDsl;
use diesel::SelectableHelper;
use serde::Serialize;

pub struct KnowledgeSourceService {
    db_pool: DbPool,
}

#[derive(Debug, Clone)]
pub struct PendingKnowledgeSourcePartEmbedding {
    pub id: String,
    pub content: String,
    pub workflow_id: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(crate = "serde")]
pub struct KnowledgeSourcePartMatch {
    pub part: KnowledgeSourcePart,
    pub score: f32,
}

impl KnowledgeSourceService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn create(
        &self,
        workflow_id: &str,
        name: &str,
        _source_type: &str,
        description: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<DbKnowledgeSource, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let input = DbNewKnowledgeSource::new(
            workflow_id.to_string(),
            name.to_string(),
            description.map(|s| s.to_string()),
            metadata.map(|s| s.to_string()),
            None,
        );
        let ks_id = input.id.clone();

        diesel::insert_into(dsl::knowledge_sources)
            .values(&input)
            .execute(&mut conn)?;

        Ok(dsl::knowledge_sources
            .filter(dsl::id.eq(ks_id))
            .select(DbKnowledgeSource::as_select())
            .first::<DbKnowledgeSource>(&mut conn)?)
    }

    pub fn create_typed(
        &self,
        input: NewKnowledgeSource,
    ) -> Result<KnowledgeSource, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let (db_source, db_parts): (DbNewKnowledgeSource, Vec<DbNewKnowledgeSourcePart>) =
            input.into_models()?;
        let source_id = db_source.id.clone();

        conn.transaction::<(), diesel::result::Error, _>(|conn| {
            diesel::insert_into(dsl::knowledge_sources)
                .values(&db_source)
                .execute(conn)?;

            if !db_parts.is_empty() {
                diesel::insert_into(parts_dsl::knowledge_source_parts)
                    .values(&db_parts)
                    .execute(conn)?;
            }
            Ok(())
        })?;

        self.get_typed(&source_id)
    }

    pub fn get(&self, ks_id: &str) -> Result<DbKnowledgeSource, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge_sources
            .filter(dsl::id.eq(ks_id))
            .filter(dsl::deleted_at.is_null())
            .select(DbKnowledgeSource::as_select())
            .first::<DbKnowledgeSource>(&mut conn)?)
    }

    pub fn get_typed(&self, ks_id: &str) -> Result<KnowledgeSource, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let source = dsl::knowledge_sources
            .filter(dsl::deleted_at.is_null())
            .filter(dsl::id.eq(ks_id).or(dsl::reference_id.eq(ks_id)))
            .select(DbKnowledgeSource::as_select())
            .first::<DbKnowledgeSource>(&mut conn)?;

        let parts = parts_dsl::knowledge_source_parts
            .filter(parts_dsl::source_id.eq(&source.id))
            .load::<DbKnowledgeSourcePart>(&mut conn)?;

        Ok(KnowledgeSource::from_models(source, parts)?)
    }

    pub fn get_including_deleted(&self, ks_id: &str) -> Result<DbKnowledgeSource, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge_sources
            .filter(dsl::id.eq(ks_id))
            .select(DbKnowledgeSource::as_select())
            .first::<DbKnowledgeSource>(&mut conn)?)
    }

    pub fn list(&self, workflow_id: &str) -> Result<Vec<DbKnowledgeSource>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .order(dsl::created_at.desc())
            .select(DbKnowledgeSource::as_select())
            .load::<DbKnowledgeSource>(&mut conn)?)
    }

    pub fn list_typed(&self, workflow_id: &str) -> Result<Vec<KnowledgeSource>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let sources = dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .order(dsl::created_at.desc())
            .select(DbKnowledgeSource::as_select())
            .load::<DbKnowledgeSource>(&mut conn)?;

        let mut out = Vec::with_capacity(sources.len());
        for source in sources {
            let parts = parts_dsl::knowledge_source_parts
                .filter(parts_dsl::source_id.eq(&source.id))
                .load::<DbKnowledgeSourcePart>(&mut conn)?;
            out.push(KnowledgeSource::from_models(source, parts)?);
        }
        Ok(out)
    }

    pub fn get_by_identifier_and_workflow_id(
        &self,
        workflow_id: &str,
        identifier: &str,
    ) -> Result<DbKnowledgeSource, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .filter(dsl::id.eq(identifier).or(dsl::reference_id.eq(identifier)))
            .select(DbKnowledgeSource::as_select())
            .first::<DbKnowledgeSource>(&mut conn)?)
    }

    pub fn get_typed_by_identifier_and_workflow_id(
        &self,
        workflow_id: &str,
        identifier: &str,
    ) -> Result<KnowledgeSource, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let source = dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .filter(dsl::id.eq(identifier).or(dsl::reference_id.eq(identifier)))
            .select(DbKnowledgeSource::as_select())
            .first::<DbKnowledgeSource>(&mut conn)?;
        let parts = parts_dsl::knowledge_source_parts
            .filter(parts_dsl::source_id.eq(&source.id))
            .load::<DbKnowledgeSourcePart>(&mut conn)?;
        Ok(KnowledgeSource::from_models(source, parts)?)
    }

    pub fn add_parts_by_identifier_and_workflow_id(
        &self,
        workflow_id: &str,
        source_identifier: &str,
        parts: Vec<NewKnowledgeSourcePart>,
    ) -> Result<Vec<KnowledgeSourcePart>, DatabaseError> {
        if parts.is_empty() {
            return Ok(Vec::new());
        }

        let mut conn = self.db_pool.get()?;
        let source = dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .filter(
                dsl::id
                    .eq(source_identifier)
                    .or(dsl::reference_id.eq(source_identifier)),
            )
            .select(DbKnowledgeSource::as_select())
            .first::<DbKnowledgeSource>(&mut conn)?;
        let source_id = source.id;

        let db_parts: Vec<DbNewKnowledgeSourcePart> = parts
            .into_iter()
            .map(|p| p.into_db_new(source_id.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        let inserted_ids: Vec<String> = db_parts.iter().filter_map(|p| p.id.clone()).collect();

        conn.transaction::<(), diesel::result::Error, _>(|conn| {
            diesel::insert_into(parts_dsl::knowledge_source_parts)
                .values(&db_parts)
                .execute(conn)?;
            Ok(())
        })?;

        let inserted = parts_dsl::knowledge_source_parts
            .filter(parts_dsl::source_id.eq(&source_id))
            .filter(parts_dsl::id.eq_any(inserted_ids))
            .load::<DbKnowledgeSourcePart>(&mut conn)?;

        inserted
            .into_iter()
            .map(KnowledgeSourcePart::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::from)
    }

    pub fn list_parts_by_identifier_and_workflow_id(
        &self,
        workflow_id: &str,
        source_identifier: &str,
    ) -> Result<Vec<KnowledgeSourcePart>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let source = dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .filter(
                dsl::id
                    .eq(source_identifier)
                    .or(dsl::reference_id.eq(source_identifier)),
            )
            .select(DbKnowledgeSource::as_select())
            .first::<DbKnowledgeSource>(&mut conn)?;
        let source_id = source.id;

        let parts = parts_dsl::knowledge_source_parts
            .filter(parts_dsl::source_id.eq(source_id))
            .load::<DbKnowledgeSourcePart>(&mut conn)?;

        parts
            .into_iter()
            .map(KnowledgeSourcePart::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::from)
    }

    pub fn delete_part(
        &self,
        workflow_id: &str,
        source_identifier: &str,
        part_identifier: &str,
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let source = dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .filter(
                dsl::id
                    .eq(source_identifier)
                    .or(dsl::reference_id.eq(source_identifier)),
            )
            .select(DbKnowledgeSource::as_select())
            .first::<DbKnowledgeSource>(&mut conn)?;
        let source_id = source.id;

        let affected = diesel::delete(parts_dsl::knowledge_source_parts)
            .filter(parts_dsl::source_id.eq(source_id))
            .filter(
                parts_dsl::id
                    .eq(part_identifier)
                    .or(parts_dsl::reference_id.eq(part_identifier)),
            )
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }
        Ok(())
    }

    pub fn soft_delete_by_identifier_and_workflow_id(
        &self,
        workflow_id: &str,
        identifier: &str,
    ) -> Result<(), DatabaseError> {
        let source = self.get_by_identifier_and_workflow_id(workflow_id, identifier)?;
        self.soft_delete(&source.id)
    }

    pub fn find_by_name_and_workflow_id(
        &self,
        workflow_id: &str,
        name: &str,
    ) -> Result<Option<DbKnowledgeSource>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::name.eq(name))
            .filter(dsl::deleted_at.is_null())
            .select(DbKnowledgeSource::as_select())
            .first::<DbKnowledgeSource>(&mut conn)
            .optional()?)
    }

    pub fn count(&self, workflow_id: &str) -> Result<i64, DatabaseError> {
        use diesel::dsl::count_star;
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .select(count_star())
            .first::<i64>(&mut conn)?)
    }

    pub fn soft_delete(&self, ks_id: &str) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let affected = diesel::update(dsl::knowledge_sources)
            .filter(dsl::id.eq(ks_id))
            .filter(dsl::deleted_at.is_null())
            .set(dsl::deleted_at.eq(Some(now)))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }
        Ok(())
    }

    pub fn soft_delete_all(&self, workflow_id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let affected = diesel::update(dsl::knowledge_sources)
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .set(dsl::deleted_at.eq(Some(now)))
            .execute(&mut conn)?;
        Ok(affected)
    }

    pub fn list_parts_missing_embeddings(
        &self,
        limit: i64,
    ) -> Result<Vec<PendingKnowledgeSourcePartEmbedding>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let rows = parts_dsl::knowledge_source_parts
            .inner_join(dsl::knowledge_sources.on(parts_dsl::source_id.eq(dsl::id)))
            .filter(parts_dsl::embeddings.is_null())
            .select((parts_dsl::id, parts_dsl::content, dsl::workflow_id))
            .limit(limit)
            .load::<(String, String, String)>(&mut conn)?;

        Ok(rows
            .into_iter()
            .map(
                |(id, content, workflow_id)| PendingKnowledgeSourcePartEmbedding {
                    id,
                    content,
                    workflow_id,
                },
            )
            .collect())
    }

    pub fn get_project_slug_by_workflow_id(
        &self,
        workflow_id: &str,
    ) -> Result<Option<String>, DatabaseError> {
        use crate::metadata::schema::finetune_jobs::dsl as jobs_dsl;
        use crate::metadata::schema::projects::dsl as projects_dsl;

        let mut conn = self.db_pool.get()?;
        jobs_dsl::finetune_jobs
            .inner_join(projects_dsl::projects.on(jobs_dsl::project_id.eq(projects_dsl::id)))
            .filter(jobs_dsl::workflow_id.eq(workflow_id))
            .order(jobs_dsl::created_at.desc())
            .select(projects_dsl::slug)
            .first::<String>(&mut conn)
            .optional()
            .map_err(DatabaseError::from)
    }

    pub fn update_part_embeddings(
        &self,
        part_id: &str,
        embeddings: &[f32],
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let serialized = serde_json::to_string(embeddings)?;
        diesel::update(parts_dsl::knowledge_source_parts)
            .filter(parts_dsl::id.eq(part_id))
            .set(parts_dsl::embeddings.eq(Some(serialized)))
            .execute(&mut conn)?;
        Ok(())
    }

    pub fn search_parts_by_similarity(
        &self,
        workflow_id: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<KnowledgeSourcePartMatch>, DatabaseError> {
        if top_k == 0 {
            return Ok(Vec::new());
        }

        let mut conn = self.db_pool.get()?;
        let source_ids = dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .select(dsl::id)
            .load::<String>(&mut conn)?;

        if source_ids.is_empty() {
            return Ok(Vec::new());
        }

        let parts = parts_dsl::knowledge_source_parts
            .filter(parts_dsl::source_id.eq_any(source_ids))
            .filter(parts_dsl::embeddings.is_not_null())
            .load::<DbKnowledgeSourcePart>(&mut conn)?;

        let mut matches = Vec::new();
        for db_part in parts {
            let part = KnowledgeSourcePart::try_from(db_part)?;
            if let Some(embedding) = part.embeddings.as_ref() {
                let score = cosine_similarity(query_embedding, embedding);
                matches.push(KnowledgeSourcePartMatch { part, score });
            }
        }

        matches.sort_by(|a, b| b.score.total_cmp(&a.score));
        matches.truncate(top_k);
        Ok(matches)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_utils::setup_test_database;

    #[test]
    fn test_create_and_list() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        service.create("wf1", "doc.pdf", "pdf", None, None).unwrap();
        service
            .create("wf1", "notes.md", "markdown", None, None)
            .unwrap();

        let sources = service.list("wf1").unwrap();
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn test_soft_delete_hides_from_list() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        let ks = service.create("wf1", "doc.pdf", "pdf", None, None).unwrap();
        service.soft_delete(&ks.id).unwrap();

        let sources = service.list("wf1").unwrap();
        assert_eq!(sources.len(), 0);
    }

    #[test]
    fn test_soft_delete_data_still_exists() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        let ks = service.create("wf1", "doc.pdf", "pdf", None, None).unwrap();
        service.soft_delete(&ks.id).unwrap();

        let fetched = service.get_including_deleted(&ks.id).unwrap();
        assert!(fetched.deleted_at.is_some());
        assert_eq!(fetched.name, "doc.pdf");
    }

    #[test]
    fn test_count() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        service.create("wf1", "a.pdf", "pdf", None, None).unwrap();
        service.create("wf1", "b.pdf", "pdf", None, None).unwrap();
        let ks3 = service.create("wf1", "c.pdf", "pdf", None, None).unwrap();
        service.soft_delete(&ks3.id).unwrap();

        assert_eq!(service.count("wf1").unwrap(), 2);
    }

    #[test]
    fn test_soft_delete_all() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        service.create("wf1", "a.pdf", "pdf", None, None).unwrap();
        service.create("wf1", "b.pdf", "pdf", None, None).unwrap();
        service.create("wf2", "c.pdf", "pdf", None, None).unwrap();

        service.soft_delete_all("wf1").unwrap();

        assert_eq!(service.count("wf1").unwrap(), 0);
        assert_eq!(service.count("wf2").unwrap(), 1);
    }
}
