use async_openai::{
    config::OpenAIConfig,
    types::{CreateModerationRequest, ModerationContentPart, ModerationImageUrl, ModerationInput},
    Client,
};
use langdb_core::{
    model::error::AuthorizationError,
    types::{
        credentials::ApiKeyCredentials,
        gateway::{ChatCompletionContent, ChatCompletionMessage, ContentType},
        guardrails::{
            partner::{GuardPartner, GuardPartnerError},
            GuardResult,
        },
    },
};

pub struct OpenaiGuardrailPartner {
    api_key: String,
}

impl OpenaiGuardrailPartner {
    pub fn new(credentials: Option<ApiKeyCredentials>) -> Result<Self, GuardPartnerError> {
        let api_key = if let Some(credentials) = credentials {
            credentials.api_key.clone()
        } else {
            std::env::var("LANGDB_OPENAI_API_KEY")
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
                                ContentType::Text => Ok(ModerationContentPart::Text {
                                    text: content.text.clone().unwrap(),
                                }),
                                ContentType::ImageUrl => Ok(ModerationContentPart::ImageUrl {
                                    image_url: ModerationImageUrl {
                                        url: content
                                            .image_url
                                            .clone()
                                            .ok_or(GuardPartnerError::InputImageIsMissing)?
                                            .url,
                                    },
                                }),
                                ContentType::InputAudio => Err(
                                    GuardPartnerError::InputTypeNotSupported("audio".to_string()),
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
