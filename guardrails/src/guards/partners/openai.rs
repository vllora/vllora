use async_openai::{
    config::OpenAIConfig,
    types::moderations::{CreateModerationRequest, ModerationContentPart, ModerationInput},
    Client,
};
use async_openai::types::moderations::ModerationTextInput;
use async_openai::types::moderations::ModerationImageURLInput;
use tracing::Span;
use vllora_core::types::guardrails::partner::GuardPartner;
use vllora_core::types::guardrails::partner::GuardPartnerError;
use vllora_core::types::guardrails::GuardResult;
use vllora_llm::client::error::AuthorizationError;
use vllora_llm::types::credentials::ApiKeyCredentials;
use vllora_llm::types::gateway::{ChatCompletionContent, ChatCompletionMessage, ContentType};
pub struct OpenaiGuardrailPartner {
    api_key: String,
}

impl OpenaiGuardrailPartner {
    pub fn new(credentials: Option<ApiKeyCredentials>) -> Result<Self, GuardPartnerError> {
        let api_key = if let Some(credentials) = credentials {
            credentials.api_key.clone()
        } else {
            std::env::var("VLLORA_OPENAI_API_KEY")
                .map_err(|_| GuardPartnerError::InvalidApiKey(AuthorizationError::InvalidApiKey))?
        };
        Ok(Self { api_key })
    }
}

#[async_trait::async_trait]
impl GuardPartner for OpenaiGuardrailPartner {
    async fn evaluate(
        &self,
        messages: &[ChatCompletionMessage],
    ) -> Result<GuardResult, GuardPartnerError> {
        let span = Span::current();
        span.record("partner", "openai".to_string());

        match messages.last() {
            Some(last_message) => {
                let input = match &last_message.content {
                    Some(ChatCompletionContent::Text(text)) => {
                        ModerationInput::String(text.clone())
                    }
                    Some(ChatCompletionContent::Content(content)) => ModerationInput::MultiModal(
                        content
                            .iter()
                            .map(|content| match content.r#type {
                                ContentType::Text => Ok(ModerationContentPart::Text(ModerationTextInput {
                                    text: content.text.clone().unwrap(),
                                })),
                                ContentType::ImageUrl => Ok(ModerationContentPart::ImageUrl(ModerationImageURLInput {
                                    image_url: content
                                        .image_url
                                        .clone()
                                        .ok_or(GuardPartnerError::InputImageIsMissing)?
                                        .url,
                                })),
                                ContentType::InputAudio => Err(
                                    GuardPartnerError::InputTypeNotSupported("audio".to_string()),
                                ),
                                ContentType::File => Err(
                                    GuardPartnerError::InputTypeNotSupported("file".to_string()),
                                ),
                            })
                            .collect::<Result<Vec<ModerationContentPart>, GuardPartnerError>>()?,
                    ),
                    None => {
                        return Err(GuardPartnerError::EvaluationFailed(
                            "Last message content is not text or image".to_string(),
                        ));
                    }
                };

                let request = CreateModerationRequest {
                    model: Some("omni-moderation-latest".to_string()),
                    input,
                };

                let mut config = OpenAIConfig::new();
                config = config.with_api_key(self.api_key.clone());
                let client = Client::with_config(config);
                let moderations = client.moderations().create(request).await;

                tracing::info!("Moderations result: {:#?}", moderations);

                match moderations {
                    Ok(moderations) => {
                        span.record(
                            "result_metadata",
                            serde_json::to_string(&moderations)
                                .expect("Failed to serialize moderations"),
                        );
                        let moderation = moderations.results.first().ok_or(
                            GuardPartnerError::EvaluationFailed("No moderations found".to_string()),
                        )?;
                        // Calculate the maximum confidence score by comparing all category scores
                        let max_confidence = [
                            moderation.category_scores.hate,
                            moderation.category_scores.hate_threatening,
                            moderation.category_scores.harassment,
                            moderation.category_scores.harassment_threatening,
                            moderation.category_scores.illicit,
                            moderation.category_scores.illicit_violent,
                            moderation.category_scores.self_harm,
                            moderation.category_scores.self_harm_intent,
                            moderation.category_scores.self_harm_instructions,
                            moderation.category_scores.sexual,
                            moderation.category_scores.sexual_minors,
                            moderation.category_scores.violence,
                            moderation.category_scores.violence_graphic,
                        ]
                        .iter()
                        .fold(0.0_f32, |max, &score| max.max(score));

                        Ok(GuardResult::Boolean {
                            passed: !moderation.flagged,
                            confidence: Some(max_confidence.into()),
                        })
                    }
                    Err(e) => {
                        return Err(GuardPartnerError::BoxedError(e.into()));
                    }
                }
            }
            None => {
                tracing::warn!(target: "guardrails", "No messages to evaluate. Passing by default");
                return Ok(GuardResult::Boolean {
                    passed: true,
                    confidence: Some(1.0),
                });
            }
        }
    }
}
