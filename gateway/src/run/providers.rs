use langdb_core::metadata::error::DatabaseError;
use langdb_core::metadata::models::providers::DbInsertProvider;
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::providers::{ProviderService, ProviderServiceImpl};
use reqwest;
use std::collections::HashSet;

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
    pub provider_name: String,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub priority: i32,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
}

impl From<LangDBProvider> for DbInsertProvider {
    fn from(provider: LangDBProvider) -> Self {
        DbInsertProvider::new(
            provider.id,
            provider.provider_name,
            provider.description,
            provider.endpoint,
            provider.priority,
            provider.privacy_policy_url,
            provider.terms_of_service_url,
        )
    }
}

pub async fn fetch_and_store_providers(
    db_pool: DbPool,
) -> Result<Vec<LangDBProvider>, ProvidersLoadError> {
    // Fetch providers from LangDB API
    let client = reqwest::Client::new();
    let providers: Vec<LangDBProvider> = client
        .get("https://api.us-east-1.langdb.ai/providers")
        .send()
        .await?
        .json()
        .await?;

    // Convert LangDBProvider to DbInsertProvider
    let db_providers: Vec<DbInsertProvider> = providers
        .iter()
        .map(|p| DbInsertProvider::from(p.clone()))
        .collect();

    // Store in database using ProviderService
    let provider_service = ProviderServiceImpl::new(db_pool.clone());

    // Get existing providers to avoid duplicates
    let existing_providers = provider_service.list_providers()?;
    let existing_provider_names: HashSet<String> = existing_providers
        .iter()
        .map(|p| p.provider_name.clone())
        .collect();

    // Insert only new providers
    let mut inserted_count = 0;
    for db_provider in db_providers {
        if !existing_provider_names.contains(&db_provider.provider_name) {
            provider_service.create_provider(db_provider)?;
            inserted_count += 1;
        }
    }

    tracing::info!(
        "Successfully processed {} providers from LangDB API (inserted {} new ones)",
        providers.len(),
        inserted_count
    );

    // Build set of identifiers from API response
    let synced_provider_names: HashSet<String> =
        providers.iter().map(|p| p.provider_name.clone()).collect();

    // Get all active providers from database
    let db_providers = provider_service.list_providers()?;

    // Find providers in DB but not in API response (these should be deactivated)
    let providers_to_deactivate: Vec<String> = db_providers
        .iter()
        .filter(|db_provider| !synced_provider_names.contains(&db_provider.provider_name))
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

/// Seed providers from hardcoded data (fallback if API is unavailable)
pub async fn seed_providers_from_hardcoded_data(db_pool: DbPool) -> Result<(), ProvidersLoadError> {
    let provider_service = ProviderServiceImpl::new(db_pool.clone());

    // Check if providers already exist
    let existing_providers = provider_service.list_providers()?;
    if !existing_providers.is_empty() {
        tracing::info!("Providers already exist in database, skipping seeding");
        return Ok(());
    }

    // Hardcoded provider data (same as in migration)
    let hardcoded_providers = vec![
        DbInsertProvider::new(
            "cbe839fa-87f5-4644-9f0c-df45a67237a6".to_string(),
            "openai".to_string(),
            Some("OpenAI develops advanced language models like GPT-4, offering versatile capabilities for NLP tasks such as text generation, summarization, and chatbots via API.".to_string()),
            None,
            100,
            Some("https://openai.com/policies/privacy-policy/".to_string()),
            Some("https://openai.com/policies/row-terms-of-use/".to_string()),
        ),
        DbInsertProvider::new(
            "d5b5d01e-f9b9-4556-b28c-36c2ef3a8ec1".to_string(),
            "bedrock".to_string(),
            Some("Amazon Bedrock is a fully managed service that offers a choice of high-performing foundation models (FMs).".to_string()),
            None,
            70,
            None,
            None,
        ),
        DbInsertProvider::new(
            "5135e6cf-bf7a-461b-b8ed-349b269c88a1".to_string(),
            "openrouter".to_string(),
            Some("OpenRouter strives to provide access to every potentially useful text-based AI model.".to_string()),
            Some("https://openrouter.ai/api/v1".to_string()),
            0,
            None,
            None,
        ),
        DbInsertProvider::new(
            "3f888bd8-7cee-4811-a3a4-ea3eb2453360".to_string(),
            "parasail".to_string(),
            Some("Parasail is the fastest, most cost-efficient AI deployment network—no quotas, no long-term contracts, and up to 30× cheaper than legacy cloud providers.".to_string()),
            Some("https://api.parasail.io/v1".to_string()),
            0,
            Some("https://www.parasail.io/legal/privacy-policy".to_string()),
            Some("https://www.parasail.io/legal/terms".to_string()),
        ),
        DbInsertProvider::new(
            "b9912658-599d-437f-be80-2657125fc7de".to_string(),
            "togetherai".to_string(),
            Some("Together AI makes it easy to run or fine-tune leading open source models".to_string()),
            Some("https://api.together.xyz/v1".to_string()),
            1,
            Some("https://www.together.ai/privacy".to_string()),
            Some("https://www.together.ai/terms-of-service".to_string()),
        ),
        DbInsertProvider::new(
            "1cd15fa5-f569-4223-a25b-deb44705bb16".to_string(),
            "xai".to_string(),
            Some("x.ai is an artificial intelligence company that specializes in developing AI-powered personal assistants".to_string()),
            Some("https://api.x.ai/v1".to_string()),
            2,
            Some("https://x.ai/legal/privacy-policy".to_string()),
            Some("https://x.ai/legal/terms-of-service-enterprise".to_string()),
        ),
        DbInsertProvider::new(
            "217ec09e-2f71-4c25-8ae1-e569b5d4cb8d".to_string(),
            "zai".to_string(),
            Some("Z.AI (formerly Zhipu AI) is a leading Chinese artificial intelligence company specializing in foundational LLMs, multimodal models, and agentic capabilities. Founded in 2019 and rebranded as Z.AI in mid-2025, it has rapidly become one of China's top AI providers.".to_string()),
            Some("https://api.z.ai/api/paas/v4".to_string()),
            0,
            None,
            None,
        ),
        DbInsertProvider::new(
            "3dd604e3-ecac-4eec-b61a-6b5f72344061".to_string(),
            "azure".to_string(),
            Some("Microsoft Azure AI is a comprehensive, enterprise-grade AI platform that delivers a broad ecosystem of AI services—from multimodal foundation models and generative APIs to document processing and conversational bots—all backed by scalable infrastructure, model customization, and enterprise-grade trust and governance via Azure AI Foundry.".to_string()),
            None,
            0,
            None,
            Some("https://www.microsoft.com/en-us/legal/terms-of-use?oneroute=true".to_string()),
        ),
        DbInsertProvider::new(
            "9adb8ca1-c184-4aa4-97fb-a54248bb81a3".to_string(),
            "vertex-ai".to_string(),
            Some("Google Vertex AI is a unified, fully-managed platform on Google Cloud for building, tuning, and deploying ML and generative AI models (including Gemini), custom training, model discovery, and MLOps".to_string()),
            None,
            0,
            None,
            Some("https://cloud.google.com/terms/".to_string()),
        ),
        DbInsertProvider::new(
            "08d0c560-8bdb-43e4-8f94-9a0c68a49229".to_string(),
            "mistralai".to_string(),
            Some("Mistral AI is a French artificial intelligence startup focused on developing high-performance, efficient, and accessible large language models (LLMs). Founded in April 2023, the company aims to democratize AI by making cutting-edge technology available to businesses, developers, and researchers.".to_string()),
            Some("https://api.mistral.ai/v1".to_string()),
            0,
            None,
            None,
        ),
        DbInsertProvider::new(
            "20519805-a3db-4692-ad7e-cca91a401298".to_string(),
            "groq".to_string(),
            Some("The LPU™ Inference Engine by Groq is a hardware and software platform that delivers exceptional compute speed, quality, and energy efficiency.".to_string()),
            Some("https://api.groq.com/openai/v1".to_string()),
            0,
            None,
            None,
        ),
        DbInsertProvider::new(
            "e2e9129b-6661-4eeb-80a2-0c86964974c9".to_string(),
            "anthropic".to_string(),
            Some("Anthropic creates AI models with an emphasis on safety and alignment, aiming to develop ethical and interpretable language models for a variety of tasks.".to_string()),
            None,
            90,
            Some("https://www.anthropic.com/legal/privacy".to_string()),
            Some("https://www.anthropic.com/legal/commercial-terms".to_string()),
        ),
        DbInsertProvider::new(
            "b4638cb6-b856-4acf-964b-1692b11fb5b2".to_string(),
            "deepinfra".to_string(),
            Some("DeepInfra makes it easy to run the latest machine learning models in the cloud.".to_string()),
            Some("https://api.deepinfra.com/v1/openai".to_string()),
            1,
            Some("https://deepinfra.com/privacy".to_string()),
            Some("https://deepinfra.com/terms".to_string()),
        ),
        DbInsertProvider::new(
            "3b36bb29-f7e9-426d-bf30-585305c49578".to_string(),
            "deepseek".to_string(),
            Some("DeepSeek is a provider of advanced AI-driven solutions, specializing in natural language processing, machine learning, and data analytics to deliver intelligent, scalable, and customizable tools for businesses and developers.".to_string()),
            Some("https://api.deepseek.com/v1".to_string()),
            2,
            Some("https://chat.deepseek.com/downloads/DeepSeek%20Privacy%20Policy.html".to_string()),
            Some("https://chat.deepseek.com/downloads/DeepSeek%20Terms%20of%20Use.html".to_string()),
        ),
        DbInsertProvider::new(
            "df1049b2-32e7-42e4-b172-4c08da059d0b".to_string(),
            "fireworksai".to_string(),
            Some("Inference engine to build production-ready, compound AI systems".to_string()),
            Some("https://api.fireworks.ai/inference/v1".to_string()),
            1,
            Some("https://fireworks.ai/privacy-policy".to_string()),
            Some("https://fireworks.ai/terms-of-service".to_string()),
        ),
        DbInsertProvider::new(
            "645f819d-43b7-4188-91c4-18060359982e".to_string(),
            "gemini".to_string(),
            Some("Gemini, developed by Google DeepMind, is a multimodal model designed to generate text and images, enhancing NLP and image processing capabilities.".to_string()),
            None,
            80,
            Some("https://cloud.google.com/terms/cloud-privacy-notice".to_string()),
            Some("https://cloud.google.com/terms/".to_string()),
        ),
    ];

    // Insert all hardcoded providers
    for provider in hardcoded_providers {
        provider_service.create_provider(provider)?;
    }

    tracing::info!("Successfully seeded {} providers from hardcoded data", 16);
    Ok(())
}

/// Main function to sync providers from LangDB API with fallback to hardcoded data
pub async fn sync_providers(db_pool: DbPool) -> Result<(), ProvidersLoadError> {
    // Try to fetch from API first
    match fetch_and_store_providers(db_pool.clone()).await {
        Ok(providers) => {
            tracing::info!(
                "Successfully synced {} providers from LangDB API",
                providers.len()
            );
            Ok(())
        }
        Err(e) => {
            tracing::warn!(
                "Failed to sync providers from API: {}. Falling back to hardcoded data.",
                e
            );
            seed_providers_from_hardcoded_data(db_pool).await
        }
    }
}
