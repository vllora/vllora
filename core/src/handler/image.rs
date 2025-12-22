use crate::executor::image_generation::handle_image_generation;
use crate::handler::record_map_err;
use crate::handler::CallbackHandlerFn;
use crate::types::metadata::services::model::ModelService;
use crate::GatewayApiError;
use actix_web::HttpMessage;
use actix_web::{web, HttpRequest, HttpResponse};
use tracing::Span;
use tracing_futures::Instrument;
use vllora_llm::types::credentials::Credentials;
use vllora_llm::types::gateway::CostCalculator;
use vllora_llm::types::gateway::CreateImageRequest;

use super::can_execute_llm_for_request;
use super::extract_tags;
use super::find_model_by_full_name;

pub async fn create_image(
    request: web::Json<CreateImageRequest>,
    models_service: web::Data<Box<dyn ModelService>>,
    req: HttpRequest,
    cost_calculator: web::Data<Box<dyn CostCalculator>>,
    callback_handler: web::Data<CallbackHandlerFn>,
) -> Result<HttpResponse, GatewayApiError> {
    can_execute_llm_for_request(&req).await?;

    let request = request.into_inner();
    let llm_model =
        find_model_by_full_name(&request.model, models_service.as_ref().as_ref(), None)?;

    let span = Span::or_current(tracing::info_span!(
        target: "vllora::user_tracing::api_invoke",
        "api_invoke",
        request = tracing::field::Empty,
        response = tracing::field::Empty,
        error = tracing::field::Empty,
        message_id = tracing::field::Empty,
    ));
    span.record("request", &serde_json::to_string(&request)?);

    let tags = extract_tags(&req)?;

    let key = req.extensions().get::<Credentials>().cloned();
    let result = handle_image_generation(
        request,
        callback_handler.get_ref(),
        &llm_model,
        key.as_ref(),
        cost_calculator.into_inner(),
        tags,
        req,
    )
    .instrument(span.clone())
    .await
    .map_err(|e| record_map_err(e, span.clone()))?;

    Ok(HttpResponse::Ok().json(result))
}
