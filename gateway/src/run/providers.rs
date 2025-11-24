use reqwest;
use std::collections::HashSet;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::provider::DbInsertProvider;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::provider::ProvidersServiceImpl;
use vllora_core::types::metadata::services::provider::ProviderService;
use vllora_core::types::LANGDB_API_URL;

#[derive(Debug, thiserror::Error)]
pub enum ProvidersLoadError {
    #[error("Failed to fetch providers: {0}")]
    FetchError(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),
}

/// Provider data structure from LangDB API
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LangDBProvider {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub priority: i32,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub provider_type: String,
}

impl From<LangDBProvider> for DbInsertProvider {
    fn from(provider: LangDBProvider) -> Self {
        DbInsertProvider::new(
            provider.id,
            provider.name,
            provider.description,
            provider.endpoint,
            provider.priority,
            provider.privacy_policy_url,
            provider.terms_of_service_url,
        )
    }
}

/// Returns hardcoded fallback providers when API is unavailable
fn get_fallback_providers() -> Vec<LangDBProvider> {
    vec![
        LangDBProvider {
            id: "cbe839fa-87f5-4644-9f0c-df45a67237a6".to_string(),
            name: "openai".to_string(),
            description: Some("OpenAI develops advanced language models like GPT-4, offering versatile capabilities for NLP tasks such as text generation, summarization, and chatbots via API.".to_string()),
            endpoint: None,
            priority: 100,
            privacy_policy_url: Some("https://openai.com/policies/privacy-policy/".to_string()),
            terms_of_service_url: Some("https://openai.com/policies/row-terms-of-use/".to_string()),
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "e2e9129b-6661-4eeb-80a2-0c86964974c9".to_string(),
            name: "anthropic".to_string(),
            description: Some("Anthropic creates AI models with an emphasis on safety and alignment, aiming to develop ethical and interpretable language models for a variety of tasks.".to_string()),
            endpoint: None,
            priority: 90,
            privacy_policy_url: Some("https://www.anthropic.com/legal/privacy".to_string()),
            terms_of_service_url: Some("https://www.anthropic.com/legal/commercial-terms".to_string()),
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "645f819d-43b7-4188-91c4-18060359982e".to_string(),
            name: "gemini".to_string(),
            description: Some("Gemini, developed by Google DeepMind, is a multimodal model designed to generate text and images, enhancing NLP and image processing capabilities.".to_string()),
            endpoint: None,
            priority: 80,
            privacy_policy_url: Some("https://cloud.google.com/terms/cloud-privacy-notice".to_string()),
            terms_of_service_url: Some("https://cloud.google.com/terms/".to_string()),
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "d5b5d01e-f9b9-4556-b28c-36c2ef3a8ec1".to_string(),
            name: "bedrock".to_string(),
            description: Some("Amazon Bedrock is a fully managed service that offers a choice of high-performing foundation models (FMs).".to_string()),
            endpoint: None,
            priority: 70,
            privacy_policy_url: None,
            terms_of_service_url: None,
            provider_type: "aws".to_string(),
        },
        LangDBProvider {
            id: "3b36bb29-f7e9-426d-bf30-585305c49578".to_string(),
            name: "deepseek".to_string(),
            description: Some("DeepSeek is a provider of advanced AI-driven solutions, specializing in natural language processing, machine learning, and data analytics to deliver intelligent, scalable, and customizable tools for businesses and developers.".to_string()),
            endpoint: Some("https://api.deepseek.com/v1".to_string()),
            priority: 2,
            privacy_policy_url: Some("https://chat.deepseek.com/downloads/DeepSeek%20Privacy%20Policy.html".to_string()),
            terms_of_service_url: Some("https://chat.deepseek.com/downloads/DeepSeek%20Terms%20of%20Use.html".to_string()),
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "1cd15fa5-f569-4223-a25b-deb44705bb16".to_string(),
            name: "xai".to_string(),
            description: Some("x.ai is an artificial intelligence company that specializes in developing AI-powered personal assistants ".to_string()),
            endpoint: Some("https://api.x.ai/v1".to_string()),
            priority: 2,
            privacy_policy_url: Some("https://x.ai/legal/privacy-policy".to_string()),
            terms_of_service_url: Some("https://x.ai/legal/terms-of-service-enterprise".to_string()),
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "b4638cb6-b856-4acf-964b-1692b11fb5b2".to_string(),
            name: "deepinfra".to_string(),
            description: Some("DeepInfra makes it easy to run the latest machine learning models in the cloud.".to_string()),
            endpoint: Some("https://api.deepinfra.com/v1/openai".to_string()),
            priority: 1,
            privacy_policy_url: Some("https://deepinfra.com/privacy".to_string()),
            terms_of_service_url: Some("https://deepinfra.com/terms".to_string()),
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "df1049b2-32e7-42e4-b172-4c08da059d0b".to_string(),
            name: "fireworksai".to_string(),
            description: Some("Inference engine to build production-ready, compound AI systems".to_string()),
            endpoint: Some("https://api.fireworks.ai/inference/v1".to_string()),
            priority: 1,
            privacy_policy_url: Some("https://fireworks.ai/privacy-policy".to_string()),
            terms_of_service_url: Some("https://fireworks.ai/terms-of-service".to_string()),
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "b9912658-599d-437f-be80-2657125fc7de".to_string(),
            name: "togetherai".to_string(),
            description: Some("Together AI makes it easy to run or fine-tune leading open source models".to_string()),
            endpoint: Some("https://api.together.xyz/v1".to_string()),
            priority: 1,
            privacy_policy_url: Some("https://www.together.ai/privacy".to_string()),
            terms_of_service_url: Some("https://www.together.ai/terms-of-service".to_string()),
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "3dd604e3-ecac-4eec-b61a-6b5f72344061".to_string(),
            name: "azure".to_string(),
            description: Some("Microsoft Azure AI is a comprehensive, enterprise-grade AI platform that delivers a broad ecosystem of AI services—from multimodal foundation models and generative APIs to document processing and conversational bots—all backed by scalable infrastructure, model customization, and enterprise-grade trust and governance via Azure AI Foundry.".to_string()),
            endpoint: None,
            priority: 0,
            privacy_policy_url: None,
            terms_of_service_url: Some("https://www.microsoft.com/en-us/legal/terms-of-use?oneroute=true".to_string()),
            provider_type: "azure".to_string(),
        },
        LangDBProvider {
            id: "20519805-a3db-4692-ad7e-cca91a401298".to_string(),
            name: "groq".to_string(),
            description: Some("The LPU™ Inference Engine by Groq is a hardware and software platform that delivers exceptional compute speed, quality, and energy efficiency.".to_string()),
            endpoint: Some("https://api.groq.com/openai/v1".to_string()),
            priority: 0,
            privacy_policy_url: None,
            terms_of_service_url: None,
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "08d0c560-8bdb-43e4-8f94-9a0c68a49229".to_string(),
            name: "mistralai".to_string(),
            description: Some("Mistral AI is a French artificial intelligence startup focused on developing high-performance, efficient, and accessible large language models (LLMs). Founded in April 2023, the company aims to democratize AI by making cutting-edge technology available to businesses, developers, and researchers.".to_string()),
            endpoint: Some("https://api.mistral.ai/v1".to_string()),
            priority: 0,
            privacy_policy_url: None,
            terms_of_service_url: None,
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "5135e6cf-bf7a-461b-b8ed-349b269c88a1".to_string(),
            name: "openrouter".to_string(),
            description: Some("OpenRouter strives to provide access to every potentially useful text-based AI model. ".to_string()),
            endpoint: Some("https://openrouter.ai/api/v1".to_string()),
            priority: 0,
            privacy_policy_url: None,
            terms_of_service_url: None,
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "3f888bd8-7cee-4811-a3a4-ea3eb2453360".to_string(),
            name: "parasail".to_string(),
            description: Some("Parasail is the fastest, most cost-efficient AI deployment network—no quotas, no long-term contracts, and up to 30× cheaper than legacy cloud providers.".to_string()),
            endpoint: Some("https://api.parasail.io/v1".to_string()),
            priority: 0,
            privacy_policy_url: Some("https://www.parasail.io/legal/privacy-policy".to_string()),
            terms_of_service_url: Some("https://www.parasail.io/legal/terms".to_string()),
            provider_type: "api_key".to_string(),
        },
        LangDBProvider {
            id: "9adb8ca1-c184-4aa4-97fb-a54248bb81a3".to_string(),
            name: "vertex-ai".to_string(),
            description: Some("Google Vertex AI is a unified, fully-managed platform on Google Cloud for building, tuning, and deploying ML and generative AI models (including Gemini), custom training, model discovery, and MLOps".to_string()),
            endpoint: None,
            priority: 0,
            privacy_policy_url: None,
            terms_of_service_url: Some("https://cloud.google.com/terms/".to_string()),
            provider_type: "vertex".to_string(),
        },
        LangDBProvider {
            id: "217ec09e-2f71-4c25-8ae1-e569b5d4cb8d".to_string(),
            name: "zai".to_string(),
            description: Some("Z.AI (formerly Zhipu AI) is a leading Chinese artificial intelligence company specializing in foundational LLMs, multimodal models, and agentic capabilities. Founded in 2019 and rebranded as Z.AI in mid-2025, it has rapidly become one of China's top AI providers.".to_string()),
            endpoint: Some("https://api.z.ai/api/paas/v4".to_string()),
            priority: 0,
            privacy_policy_url: None,
            terms_of_service_url: None,
            provider_type: "api_key".to_string(),
        },
    ]
}

pub async fn fetch_and_store_providers(
    db_pool: DbPool,
) -> Result<Vec<LangDBProvider>, ProvidersLoadError> {
    tracing::info!("Fetching providers from LangDB API...");
    // Fetch providers from LangDB API
    let langdb_api_url: String = std::env::var("LANGDB_API_URL")
        .ok()
        .unwrap_or(LANGDB_API_URL.to_string())
        .replace("/v1", "");
    let client = reqwest::Client::new();

    // Try to fetch from API, use fallback data on error
    let mut providers: Vec<LangDBProvider> = match client
        .get(format!("{langdb_api_url}/public/providers"))
        .send()
        .await
    {
        Ok(response) => match response.json().await {
            Ok(providers) => providers,
            Err(e) => {
                tracing::warn!("Failed to parse API response: {}. Using fallback data.", e);
                get_fallback_providers()
            }
        },
        Err(e) => {
            tracing::warn!("Failed to fetch from API: {}. Using fallback data.", e);
            get_fallback_providers()
        }
    };

    providers.push(LangDBProvider {
        id: uuid::Uuid::new_v4().to_string(),
        name: "langdb".to_string(),
        description: Some("LangDB".to_string()),
        endpoint: format!("{langdb_api_url}/v1").into(),
        priority: 100,
        privacy_policy_url: None,
        terms_of_service_url: None,
        provider_type: "langdb".to_string(),
    });

    // Convert LangDBProvider to DbInsertProvider
    let db_providers: Vec<DbInsertProvider> = providers
        .iter()
        .map(|p| DbInsertProvider::from(p.clone()))
        .collect();

    // Store in database using ProviderService
    let provider_service = ProvidersServiceImpl::new(db_pool.clone());

    // Get existing providers to avoid duplicates
    let existing_providers = provider_service.list_providers()?;
    let existing_provider_names: HashSet<String> =
        existing_providers.iter().map(|p| p.name.clone()).collect();

    // Insert only new providers
    let mut inserted_count = 0;
    for db_provider in db_providers {
        if !existing_provider_names.contains(&db_provider.provider_name) {
            provider_service.create_provider(db_provider)?;
            inserted_count += 1;
        }
    }

    tracing::info!(
        "Successfully processed {} providers (inserted {} new ones)",
        providers.len(),
        inserted_count
    );

    // Build set of identifiers from API response
    let synced_provider_names: HashSet<String> = providers.iter().map(|p| p.name.clone()).collect();

    // Get all active providers from database
    let db_providers = provider_service.list_providers()?;

    // Find providers in DB but not in API response (these should be deactivated)
    let providers_to_deactivate: Vec<String> = db_providers
        .iter()
        .filter(|db_provider| !synced_provider_names.contains(&db_provider.name))
        .map(|db_provider| db_provider.id.clone())
        .collect();

    // Deactivate obsolete providers
    let deactivate_count = providers_to_deactivate.len();
    if !providers_to_deactivate.is_empty() {
        for provider_id in providers_to_deactivate {
            provider_service.delete_provider(&provider_id)?;
        }
        tracing::info!("Deactivated {} obsolete providers", deactivate_count);
    }

    Ok(providers)
}

/// Main function to sync providers from LangDB API with fallback to hardcoded data
pub async fn sync_providers(db_pool: DbPool) -> Result<(), ProvidersLoadError> {
    // Try to fetch from API first
    match fetch_and_store_providers(db_pool.clone()).await {
        Ok(providers) => {
            tracing::info!("Successfully synced {} providers", providers.len());
        }
        Err(e) => {
            tracing::warn!("Failed to sync providers from API: {}.", e);
        }
    }

    Ok(())
}
