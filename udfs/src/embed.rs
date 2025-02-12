use async_openai::types::CreateEmbeddingRequestArgs;

use crate::{
    types::{CommonResponse, Usage},
    EmbeddingConfig, InvokeError,
};

pub async fn embed(input: String, config: &EmbeddingConfig) -> Result<CommonResponse, InvokeError> {
    tracing::debug!("Embedding input: {}", input);

    let input = serde_json::from_str::<Vec<String>>(&input)?;
    let input = input[0].to_string();
    let mut request = CreateEmbeddingRequestArgs::default();

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
