use actix_http::h1::Payload;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::web::BytesMut;
use actix_web::HttpMessage;
use futures::TryStreamExt;
use langdb_core::telemetry::AdditionalContext;
use langdb_core::types::threads::CompletionsRunId;
use serde::Deserialize;
use std::collections::HashMap;
use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::rc::Rc;
use tokio_stream::StreamExt;
use uuid::Uuid;

#[derive(Deserialize, Debug)]
struct Extra {
    #[serde(alias = "extra_body")]
    extra: SessionPayload,
}

#[derive(Deserialize, Debug)]
struct SessionPayload {
    session_id: Uuid,
}
pub struct RunId;

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl RunId {
    pub fn new() -> Self {
        RunId
    }
}

impl<S, B> Transform<S, ServiceRequest> for RunId
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = RunIdMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RunIdMiddleware {
            service: service.into(),
        }))
    }
}

pub struct RunIdMiddleware<S> {
    service: Rc<S>,
}

type LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

impl<S, B> Service<ServiceRequest> for RunIdMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let service = Rc::clone(&self.service);

        Box::pin(async move {
            if req.path().contains("chat/completion") || req.path().contains("responses") {
                let run_id = match req.headers().get("X-Run-Id") {
                    Some(value) => match value.to_str() {
                        Ok(v) => v.to_string(),
                        Err(_) => Uuid::new_v4().to_string(),
                    },
                    None => {
                        // Try to extract session_id from the request body without consuming the payload
                        let session_id = match extract_session_id_from_request(&mut req).await {
                            Ok(session_id) => session_id.to_string(),
                            Err(_) => Uuid::new_v4().to_string(),
                        };
                        session_id
                    }
                };

                req.extensions_mut()
                    .insert(CompletionsRunId::new(run_id.clone()));
                let mut extensions_mut = req.extensions_mut();
                let additional_context = extensions_mut.get_mut::<AdditionalContext>();
                if let Some(additional_context) = additional_context {
                    additional_context
                        .0
                        .insert("langdb.run_id".to_string(), run_id);
                } else {
                    let mut additional_context = HashMap::new();
                    additional_context.insert("langdb.run_id".to_string(), run_id);
                    extensions_mut.insert(AdditionalContext::new(additional_context));
                }
            }

            let fut = service.call(req);

            let res = fut.await?;
            Ok(res)
        })
    }
}

/// Extract session_id from request body without consuming the payload
async fn extract_session_id_from_request(
    req: &mut ServiceRequest,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    // Clone the payload to avoid consuming the original
    let payload = req.take_payload();
    let mut request_body = BytesMut::new();

    // Collect all chunks
    let mut payload_stream = payload.into_stream();
    while let Some(chunk) = payload_stream.next().await {
        let chunk = chunk?;
        request_body.extend_from_slice(&chunk);
    }

    // Try to parse the JSON
    let json_result = serde_json::from_slice::<Extra>(&request_body);

    // Reconstruct the payload for downstream handlers
    let (_, mut new_payload) = Payload::create(true);
    new_payload.unread_data(request_body.freeze());
    req.set_payload(actix_http::Payload::H1 {
        payload: new_payload,
    });

    match json_result {
        Ok(payload) => Ok(payload.extra.session_id),
        Err(e) => Err(Box::new(e)),
    }
}
