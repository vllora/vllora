use crate::executor::embeddings::handle_embeddings;
use crate::metadata::services::model::ModelService;
use crate::types::credentials::Credentials;
use crate::types::embed::EmbeddingResult;
use actix_web::{web, HttpResponse};
use actix_web::{HttpMessage, HttpRequest};
use tracing::Span;
use tracing_futures::Instrument;

use crate::types::gateway::{
    CostCalculator, CreateEmbeddingRequest, CreateEmbeddingResponse, EmbeddingData, EmbeddingUsage,
};

use crate::handler::extract_tags;
use crate::handler::CallbackHandlerFn;
use crate::GatewayApiError;

use super::{can_execute_llm_for_request, find_model_by_full_name};

pub async fn embeddings_handler(
    request: web::Json<CreateEmbeddingRequest>,
    models_service: web::Data<Box<dyn ModelService>>,
    callback_handler: web::Data<CallbackHandlerFn>,
    req: HttpRequest,
    cost_calculator: web::Data<Box<dyn CostCalculator>>,
) -> Result<HttpResponse, GatewayApiError> {
    can_execute_llm_for_request(&req).await?;
    let request = request.into_inner();
    let llm_model = find_model_by_full_name(&request.model, models_service.as_ref().as_ref())?;
    let key_credentials = req.extensions().get::<Credentials>().cloned();

    let span = Span::or_current(tracing::info_span!(
        target: "langdb::user_tracing::api_invoke",
        "api_invoke",
        request = tracing::field::Empty,
        response = tracing::field::Empty,
        error = tracing::field::Empty,
        message_id = tracing::field::Empty,
    ));
    span.record("request", &serde_json::to_string(&request)?);

    let tags = extract_tags(&req)?;

    let result = handle_embeddings(
        request,
        callback_handler.get_ref(),
        &llm_model,
        key_credentials.as_ref(),
        cost_calculator.into_inner(),
        tags,
        req,
    )
    .instrument(span)
    .await?;

    let data = match &result {
        EmbeddingResult::Float(response) => response
            .data
            .iter()
            .map(|v| EmbeddingData {
                object: v.object.clone(),
                embedding: v.embedding.clone().into(),
                index: v.index,
            })
            .collect(),
        EmbeddingResult::Base64(response) => response
            .data
            .iter()
            .map(|v| EmbeddingData {
                object: v.object.clone(),
                embedding: v.embedding.clone().into(),
                index: v.index,
            })
            .collect(),
    };

    Ok(HttpResponse::Ok()
        .append_header(("X-Model-Name", llm_model.model.clone()))
        .append_header((
            "X-Provider-Name",
            llm_model.inference_provider.provider.to_string(),
        ))
        .json(CreateEmbeddingResponse {
            object: "list".into(),
            data,
            model: llm_model.model.clone(),
            usage: EmbeddingUsage {
                prompt_tokens: result.usage().prompt_tokens,
                total_tokens: result.usage().total_tokens,
                cost: 0.0,
            },
        }))
}
