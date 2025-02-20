use async_openai::types::CreateEmbeddingRequestArgs;

use crate::{
    types::{CommonResponse, Usage},
    EmbeddingConfig, InvokeError,
};

pub async fn embed(
    values: &mut std::slice::Iter<'_, String>,
    config: &EmbeddingConfig,
) -> Result<CommonResponse, InvokeError> {
    let input = values
        .next()
        .cloned()
        .ok_or_else(|| InvokeError::CustomError("No input provided".to_string()))?;
    let mut request = CreateEmbeddingRequestArgs::default();
    tracing::debug!("Calling embeddings with input: {}", input);

    request = request.model(&config.model_settings.model).to_owned();
    request = request.input(input).to_owned();

    let request = request.build()?;
    tracing::debug!("Embedding request: {:?}", request);
    let client = async_openai::Client::with_config(config.config.clone());
    let embedding = client.embeddings().create(request).await?;
    Ok(CommonResponse {
        response: embedding.data[0].embedding.clone().into(),
        usage: Usage {
            total_tokens: embedding.usage.total_tokens as usize,
        },
    })
}
