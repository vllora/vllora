use crate::cost::GatewayCostCalculator;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use vllora_core::credentials::ProviderKeyResolver;
use vllora_core::executor::embeddings::handle_embeddings;
use vllora_core::handler::{find_model_by_full_name, CallbackHandlerFn};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::knowledge_source::KnowledgeSourceService;
use vllora_core::metadata::services::model::ModelServiceImpl;
use vllora_core::types::embed::EmbeddingResult;
use vllora_core::types::metadata::services::model::ModelService;
use vllora_llm::types::gateway::{CostCalculator, CreateEmbeddingRequest, EncodingFormat, Input};

pub fn start_embedding_backfill_job(db_pool: DbPool) -> JoinHandle<()> {
    tokio::spawn(async move {
        let interval_secs = std::env::var("KNOWLEDGE_EMBEDDING_JOB_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);
        let batch_size = std::env::var("KNOWLEDGE_EMBEDDING_JOB_BATCH_SIZE")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(32);

        loop {
            if let Err(e) = process_embedding_batch(db_pool.clone(), batch_size).await {
                tracing::warn!("knowledge embedding backfill failed: {}", e);
            }
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        }
    })
}

pub async fn process_embedding_batch(
    db_pool: DbPool,
    batch_size: i64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = KnowledgeSourceService::new(db_pool.clone());
    let pending = service.list_parts_missing_embeddings(batch_size)?;
    if pending.is_empty() {
        return Ok(());
    }

    for part in pending {
        let project_slug = service
            .get_project_slug_by_workflow_id(&part.workflow_id)?
            .unwrap_or_else(|| "default".to_string());
        match embed_phrase(db_pool.clone(), &part.content, &project_slug).await {
            Ok(embedding) => {
                if let Err(e) = service.update_part_embeddings(&part.id, &embedding) {
                    tracing::warn!("failed to persist embedding for part {}: {}", part.id, e);
                }
            }
            Err(e) => {
                tracing::warn!("failed to generate embedding for part {}: {}", part.id, e);
            }
        }
    }

    Ok(())
}

pub async fn embed_phrase(
    db_pool: DbPool,
    phrase: &str,
    project_slug: &str,
) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
    let model = std::env::var("KNOWLEDGE_EMBEDDING_MODEL")
        .unwrap_or_else(|_| "text-embedding-3-small".to_string());
    let model_service = ModelServiceImpl::new(db_pool.clone());
    let llm_model = find_model_by_full_name(&model, &model_service as &dyn ModelService, None)?;
    let key_storage = ProviderKeyResolver::new(db_pool);
    let request = CreateEmbeddingRequest {
        model,
        input: Input::String(phrase.to_string()),
        user: None,
        dimensions: None,
        encoding_format: EncodingFormat::Float,
    };

    let result = handle_embeddings(
        request,
        &CallbackHandlerFn::default(),
        &llm_model,
        project_slug,
        "default",
        &key_storage,
        Arc::new(Box::new(GatewayCostCalculator::new()) as Box<dyn CostCalculator>),
        HashMap::new(),
    )
    .await?;

    match result {
        EmbeddingResult::Float(response) => response
            .data
            .first()
            .map(|d| d.embedding.clone())
            .ok_or_else(|| std::io::Error::other("empty embedding response").into()),
        EmbeddingResult::Base64(_) => {
            Err(std::io::Error::other("unexpected base64 embedding response").into())
        }
    }
}
