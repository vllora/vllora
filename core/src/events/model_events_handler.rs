use std::sync::Arc;

use crate::events::callback_handler::GatewayCallbackHandlerFn;
use crate::events::callback_handler::GatewayModelEventWithDetails;
use crate::handler::ModelEventWithDetails;
use tokio::sync::broadcast;
use tracing::Span;
use vllora_llm::types::events::CustomEventType;
use vllora_llm::types::gateway::CostCalculator;
use vllora_llm::types::gateway::Usage;
use vllora_llm::types::CostEvent;
use vllora_llm::types::CustomEvent;
use vllora_llm::types::ModelEvent;
use vllora_llm::types::ModelEventType;

pub struct ModelEventsHandler {
    cloud_callback_handler: GatewayCallbackHandlerFn,
    tenant_name: String,
    project_id: String,
    identifiers: Vec<(String, String)>,
    run_id: Option<String>,
    thread_id: Option<String>,
    cost_calculator: Arc<Box<dyn CostCalculator>>,
}

impl ModelEventsHandler {
    pub fn new(
        cloud_callback_handler: GatewayCallbackHandlerFn,
        tenant_name: String,
        project_id: String,
        identifiers: Vec<(String, String)>,
        run_id: Option<String>,
        thread_id: Option<String>,
        cost_calculator: Arc<Box<dyn CostCalculator>>,
    ) -> Self {
        Self {
            cloud_callback_handler,
            tenant_name,
            project_id,
            identifiers,
            run_id,
            thread_id,
            cost_calculator,
        }
    }

    pub async fn handle_events(&self, mut rx: broadcast::Receiver<ModelEventWithDetails>) {
        let current_span = Span::current();
        let mut content = String::new();
        while let Ok(model_event) = rx.recv().await {
            match &model_event.event.event {
                ModelEventType::LlmContent(e) => {
                    content.push_str(&e.content);
                }
                ModelEventType::LlmStop(e) => {
                    if let Some(output) = e.output.clone() {
                        content.push_str(&output);
                    }

                    if let Some(model) = &model_event.model {
                        let (cost, usage) = match &e.usage {
                            Some(usage) => {
                                let cost = self
                                    .cost_calculator
                                    .as_ref()
                                    .calculate_cost(
                                        &model.price,
                                        &Usage::CompletionModelUsage(usage.clone()),
                                        &model.credentials_ident,
                                    )
                                    .await
                                    .map(|c| (c.cost, Some(usage.clone())))
                                    .unwrap_or((0.0, Some(usage.clone())));

                                if cost.0 == 0.0 {
                                    tracing::error!(
                                        "Cost is 0 for event {e:?}. Event: {event:?}",
                                        event = model_event.event
                                    );
                                }

                                cost
                            }
                            None => {
                                tracing::error!(
                                    "Usage is none for event {e:?}. Event: {event:?}",
                                    event = model_event.event
                                );
                                (0.0, None)
                            }
                        };

                        let cost_event = CostEvent::new(cost, usage.clone());
                        let _ = self
                            .cloud_callback_handler
                            .on_message(
                                GatewayModelEventWithDetails {
                                    event: ModelEventWithDetails::new(
                                        ModelEvent::new(
                                            &current_span,
                                            ModelEventType::Custom(CustomEvent::new(
                                                CustomEventType::Cost {
                                                    value: cost_event.clone(),
                                                },
                                            )),
                                        ),
                                        model_event.model.clone(),
                                    ),
                                    tenant_name: self.tenant_name.clone(),
                                    project_id: self.project_id.clone(),
                                    usage_identifiers: self.identifiers.clone(),
                                    run_id: self.run_id.clone(),
                                    thread_id: self.thread_id.clone(),
                                }
                                .into(),
                            )
                            .await;

                        if let Some(span) = &model_event.event.span {
                            span.record("cost", serde_json::to_string(&cost).unwrap());
                            span.record("usage", serde_json::to_string(&usage).unwrap());
                            span.record("response", content.clone());
                        }

                        current_span.record("cost", serde_json::to_string(&cost).unwrap());
                        current_span.record("usage", serde_json::to_string(&usage).unwrap());
                        current_span.record("response", content.clone());
                    }
                }
                _ => {}
            }

            self.cloud_callback_handler
                .on_message(
                    GatewayModelEventWithDetails {
                        event: model_event,
                        tenant_name: self.tenant_name.clone(),
                        project_id: self.project_id.clone(),
                        usage_identifiers: self.identifiers.clone(),
                        run_id: self.run_id.clone(),
                        thread_id: self.thread_id.clone(),
                    }
                    .into(),
                )
                .await;
        }
    }
}
