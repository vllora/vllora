use crate::error::GatewayError;
use actix_web::HttpResponse;
use async_openai::types::responses::CreateResponse;

pub async fn create(request: &CreateResponse) -> Result<HttpResponse, GatewayError> {
    tracing::warn!("Creating response");
    tracing::warn!("Request {:?}", request);

    Ok(HttpResponse::Ok().finish())
}
