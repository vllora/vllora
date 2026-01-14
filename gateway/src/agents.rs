use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, error, warn};

/// Provider configuration for model settings
/// Matches Distri's [model_settings.provider] section
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderConfig {
    /// Provider type: "openai", "openai_compat", or "vllora"
    pub name: String,
    /// Base URL for the API endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// API key for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Project ID (used by vllora provider)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: "vllora".to_string(),
            base_url: Some("http://localhost:9090/lucy/v1".to_string()),
            api_key: None,
            project_id: None,
        }
    }
}

/// Model settings configuration
/// Matches Distri's [model_settings] section
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelSettingsConfig {
    /// Model name (e.g., "gpt-4o", "gpt-4o-mini")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Temperature for sampling (0.0 - 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Maximum tokens in response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Provider configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderConfig>,
}

impl Default for ModelSettingsConfig {
    fn default() -> Self {
        Self {
            model: None,
            temperature: None,
            max_tokens: None,
            provider: Some(ProviderConfig::default()),
        }
    }
}

/// Write provider TOML fields based on provider type
/// Different provider types have different valid fields:
/// - openai: NO fields (empty struct)
/// - openai_compat: base_url, api_key, project_id
/// - vllora: base_url only
fn write_provider_toml(provider: &ProviderConfig) -> String {
    let mut result = String::new();
    result.push_str("[model_settings.provider]\n");
    result.push_str(&format!("name = \"{}\"\n", provider.name));

    match provider.name.as_str() {
        "openai" => {
            // OpenAI provider has NO additional fields
        }
        "openai_compat" => {
            // OpenAI-compatible has base_url, api_key, project_id
            if let Some(ref base_url) = provider.base_url {
                result.push_str(&format!("base_url = \"{}\"\n", base_url));
            }
            if let Some(ref api_key) = provider.api_key {
                result.push_str(&format!("api_key = \"{}\"\n", api_key));
            }
            if let Some(ref project_id) = provider.project_id {
                result.push_str(&format!("project_id = \"{}\"\n", project_id));
            }
        }
        "vllora" => {
            // Vllora has only base_url
            if let Some(ref base_url) = provider.base_url {
                result.push_str(&format!("base_url = \"{}\"\n", base_url));
            }
        }
        _ => {
            // Unknown provider, include all fields as fallback
            if let Some(ref base_url) = provider.base_url {
                result.push_str(&format!("base_url = \"{}\"\n", base_url));
            }
            if let Some(ref api_key) = provider.api_key {
                result.push_str(&format!("api_key = \"{}\"\n", api_key));
            }
            if let Some(ref project_id) = provider.project_id {
                result.push_str(&format!("project_id = \"{}\"\n", project_id));
            }
        }
    }
    result
}

/// Apply model settings configuration override to agent content
/// Replaces the [model_settings] and [model_settings.provider] sections in the TOML frontmatter
fn apply_model_settings_override(content: &str, settings: &ModelSettingsConfig) -> String {
    // Agent files can use either +++ or --- as frontmatter delimiters
    // Detect which delimiter is used
    let delimiter = if content.starts_with("+++") {
        "+++"
    } else if content.starts_with("---") {
        "---"
    } else {
        warn!("Agent content does not start with +++ or --- frontmatter delimiter");
        return content.to_string();
    };

    let parts: Vec<&str> = content.splitn(3, delimiter).collect();

    if parts.len() != 3 {
        warn!(
            "Agent content does not have valid {} frontmatter (found {} parts)",
            delimiter,
            parts.len()
        );
        return content.to_string();
    }

    let frontmatter = parts[1];
    let body = parts[2];

    debug!(
        "Applying model settings override with delimiter '{}': {:?}",
        delimiter, settings
    );

    // Process frontmatter line by line
    let mut result = String::new();
    let mut in_model_settings = false;
    let mut in_provider_section = false;
    let mut model_settings_written = false;
    let mut provider_written = false;

    for line in frontmatter.lines() {
        let trimmed = line.trim();

        // Detect section headers
        if trimmed == "[model_settings]" {
            in_model_settings = true;
            in_provider_section = false;
            result.push_str(line);
            result.push('\n');

            // Add overridden model settings right after the header
            if let Some(ref model) = settings.model {
                result.push_str(&format!("model = \"{}\"\n", model));
            }
            if let Some(temperature) = settings.temperature {
                result.push_str(&format!("temperature = {}\n", temperature));
            }
            if let Some(max_tokens) = settings.max_tokens {
                result.push_str(&format!("max_tokens = {}\n", max_tokens));
            }
            model_settings_written = true;
            continue;
        }

        if trimmed == "[model_settings.provider]" {
            in_provider_section = true;
            in_model_settings = false;

            // Write provider section with overrides (using helper for correct field handling)
            if let Some(ref provider) = settings.provider {
                result.push_str(&write_provider_toml(provider));
                provider_written = true;
            }
            continue;
        }

        // Check if we've hit a new section
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // If we were in model_settings and have a provider to write, write it now
            if in_model_settings && !provider_written {
                if let Some(ref provider) = settings.provider {
                    result.push_str(&write_provider_toml(provider));
                    provider_written = true;
                }
            }
            in_model_settings = false;
            in_provider_section = false;
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // Skip lines in model_settings that we're overriding
        if in_model_settings {
            if trimmed.starts_with("model =") && settings.model.is_some() {
                continue;
            }
            if trimmed.starts_with("temperature =") && settings.temperature.is_some() {
                continue;
            }
            if trimmed.starts_with("max_tokens =") && settings.max_tokens.is_some() {
                continue;
            }
        }

        // Skip all lines in provider section if we have overrides
        if in_provider_section && settings.provider.is_some() {
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }

    // If model_settings section doesn't exist and we have settings, add it
    if !model_settings_written
        && (settings.model.is_some()
            || settings.temperature.is_some()
            || settings.max_tokens.is_some()
            || settings.provider.is_some())
    {
        result.push_str("\n[model_settings]\n");
        if let Some(ref model) = settings.model {
            result.push_str(&format!("model = \"{}\"\n", model));
        }
        if let Some(temperature) = settings.temperature {
            result.push_str(&format!("temperature = {}\n", temperature));
        }
        if let Some(max_tokens) = settings.max_tokens {
            result.push_str(&format!("max_tokens = {}\n", max_tokens));
        }

        if let Some(ref provider) = settings.provider {
            result.push_str(&write_provider_toml(provider));
        }
    } else if !provider_written {
        // Add provider section at the end if needed
        if let Some(ref provider) = settings.provider {
            result.push_str(&write_provider_toml(provider));
        }
    }

    format!("{}{}{}{}", delimiter, result, delimiter, body)
}

/// Embedded agent definitions
const EMBEDDED_AGENTS: &[(&str, &str)] = &[
    (
        "vllora-data-agent.md",
        include_str!("../agents/vllora-data-agent.md"),
    ),
    (
        "vllora-experiment-agent.md",
        include_str!("../agents/vllora-experiment-agent.md"),
    ),
    (
        "vllora-orchestrator.md",
        include_str!("../agents/vllora-orchestrator.md"),
    ),
    (
        "vllora-ui-agent.md",
        include_str!("../agents/vllora-ui-agent.md"),
    ),
    (
        "vllora-dataset-orchestrator.md",
        include_str!("../agents/finetune-dataset/vllora-dataset-orchestrator.md"),
    ),
    (
        "vllora-dataset-ui.md",
        include_str!("../agents/finetune-dataset/vllora-dataset-ui.md"),
    ),
    (
        "vllora-dataset-data.md",
        include_str!("../agents/finetune-dataset/vllora-dataset-data.md"),
    ),
    (
        "vllora-dataset-analysis.md",
        include_str!("../agents/finetune-dataset/vllora-dataset-analysis.md"),
    ),
];

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Failed to read agent file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to register agent: {0}")]
    RegistrationError(String),
    #[error("Distri API URL not configured")]
    MissingApiUrl,
    #[error("Distri is not running")]
    DistriNotRunning,
}

/// Agent definition with name and content
#[derive(Debug, Clone)]
pub struct AgentDefinition {
    #[allow(dead_code)]
    pub name: String,
    pub content: String,
}

/// Load agents from embedded definitions
fn load_embedded_agents() -> HashMap<String, AgentDefinition> {
    let mut agents = HashMap::new();

    for (filename, content) in EMBEDDED_AGENTS {
        // Extract agent name from filename (remove .md extension)
        let name = filename.strip_suffix(".md").unwrap_or(filename).to_string();
        agents.insert(
            name.clone(),
            AgentDefinition {
                name,
                content: content.to_string(),
            },
        );
    }

    agents
}

/// Load agents from working directory
/// Returns a map of filename (without .md) to agent definition
fn load_working_directory_agents(
    work_dir: &Path,
) -> Result<HashMap<String, AgentDefinition>, AgentError> {
    let agents_dir = work_dir.join("agents");
    let mut agents = HashMap::new();

    // Check if agents directory exists
    if !agents_dir.exists() {
        return Ok(agents);
    }

    if !agents_dir.is_dir() {
        warn!(
            "agents path exists but is not a directory: {:?}",
            agents_dir
        );
        return Ok(agents);
    }

    // Read all .md files from the agents directory
    let entries = std::fs::read_dir(&agents_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Only process .md files
        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            let filename = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                AgentError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid filename",
                ))
            })?;

            let name = filename.strip_suffix(".md").unwrap_or(filename).to_string();
            let content = std::fs::read_to_string(&path)?;

            agents.insert(name.clone(), AgentDefinition { name, content });

            debug!("Loaded agent from working directory: {}", filename);
        }
    }

    Ok(agents)
}

/// Merge embedded and working directory agents
/// Working directory agents override embedded ones
fn merge_agents(
    embedded: HashMap<String, AgentDefinition>,
    working_dir: HashMap<String, AgentDefinition>,
) -> HashMap<String, AgentDefinition> {
    let mut merged = embedded;

    // Override with working directory agents
    for (name, agent) in working_dir {
        debug!(
            "Overriding embedded agent with working directory version: {}",
            name
        );
        merged.insert(name, agent);
    }

    merged
}

/// Lucy configuration stored in HOME/.vllora/lucy.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LucyConfig {
    /// Distri server URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distri_url: Option<String>,
    /// Model settings to override agent defaults
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_settings: Option<ModelSettingsConfig>,
}

impl Default for LucyConfig {
    fn default() -> Self {
        Self {
            distri_url: None,
            model_settings: Some(ModelSettingsConfig::default()),
        }
    }
}

/// Get the path to lucy.json config file
fn get_lucy_config_path() -> Result<PathBuf, AgentError> {
    let home_dir = std::env::var("HOME").map_err(|_| {
        AgentError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "HOME environment variable not set",
        ))
    })?;
    Ok(PathBuf::from(home_dir).join(".vllora").join("lucy.json"))
}

/// Load Lucy configuration from HOME/.vllora/lucy.json
pub fn load_lucy_config() -> Result<Option<LucyConfig>, AgentError> {
    let config_path = get_lucy_config_path()?;

    if !config_path.exists() {
        debug!(
            "Lucy config file not found at {:?}, using defaults",
            config_path
        );
        return Ok(None);
    }

    let content = std::fs::read_to_string(&config_path).map_err(|e| {
        AgentError::IoError(std::io::Error::other(format!(
            "Failed to read lucy.json: {}",
            e
        )))
    })?;

    let config: LucyConfig = serde_json::from_str(&content).map_err(|e| {
        AgentError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse lucy.json: {}", e),
        ))
    })?;

    debug!("Loaded Lucy config from {:?}", config_path);
    Ok(Some(config))
}

/// Save Lucy configuration to HOME/.vllora/lucy.json
pub fn save_lucy_config(config: &LucyConfig) -> Result<(), AgentError> {
    let config_path = get_lucy_config_path()?;

    // Ensure .vllora directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AgentError::IoError(std::io::Error::other(format!(
                "Failed to create .vllora directory: {}",
                e
            )))
        })?;
    }

    let content = serde_json::to_string_pretty(config).map_err(|e| {
        AgentError::IoError(std::io::Error::other(format!(
            "Failed to serialize lucy.json: {}",
            e
        )))
    })?;

    std::fs::write(&config_path, content).map_err(|e| {
        AgentError::IoError(std::io::Error::other(format!(
            "Failed to write lucy.json: {}",
            e
        )))
    })?;

    debug!("Saved Lucy config to {:?}", config_path);
    Ok(())
}

/// Delete Lucy configuration file from HOME/.vllora/lucy.json
pub fn delete_lucy_config() -> Result<(), AgentError> {
    let config_path = get_lucy_config_path()?;

    if !config_path.exists() {
        debug!(
            "Lucy config file does not exist at {:?}, nothing to delete",
            config_path
        );
        return Ok(());
    }

    std::fs::remove_file(&config_path).map_err(|e| {
        AgentError::IoError(std::io::Error::other(format!(
            "Failed to delete lucy.json: {}",
            e
        )))
    })?;

    debug!("Deleted Lucy config from {:?}", config_path);
    Ok(())
}

/// Client for interacting with the Distri server
#[derive(Debug, Clone)]
pub struct DistriClient {
    api_url: String,
}

impl DistriClient {
    /// Create a DistriClient from a full URL string
    pub fn from_url(url: impl Into<String>) -> Self {
        Self {
            api_url: url.into(),
        }
    }

    /// Check if Distri server is running by calling GET /v1/agents
    pub async fn check_distri_running(&self) -> bool {
        let url = format!("{}/v1/agents", self.api_url);

        let client = reqwest::Client::new();
        match client
            .get(&url)
            .header("Content-Type", "application/json")
            .send()
            .await
        {
            Ok(response) => {
                // Any 2xx or 4xx response means the server is running
                // (404 is OK - it just means no agents registered yet)
                response.status().is_client_error() || response.status().is_success()
            }
            Err(_) => false,
        }
    }

    /// Register a single agent with the Distri server
    pub async fn register_agent(&self, agent: &AgentDefinition) -> Result<(), AgentError> {
        let url = format!("{}/v1/agents", self.api_url);

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("Content-Type", "text/markdown")
            .body(agent.content.clone())
            .send()
            .await
            .map_err(|e| AgentError::RegistrationError(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::RegistrationError(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        Ok(())
    }
}

/// Get the Distri API URL from environment or use default
fn get_distri_api_url() -> String {
    std::env::var("DISTRI_URL").unwrap_or_else(|_| "http://localhost:8081".to_string())
}

/// Agent registration status
#[derive(Debug, Clone, Serialize)]
pub struct AgentRegistrationStatus {
    pub name: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Detailed registration result
#[derive(Debug, Clone, Serialize)]
pub struct RegistrationResult {
    pub distri_running: bool,
    pub distri_endpoint: String,
    pub agents: Vec<AgentRegistrationStatus>,
}

/// Register all agents with the Distri server and return detailed status
/// If distri_url is provided, use it; otherwise fall back to DISTRI_URL env var or default
/// If model_settings is provided, override the [model_settings] section in all agents
pub async fn register_agents_with_status(
    distri_url: Option<&str>,
    model_settings: Option<&ModelSettingsConfig>,
) -> Result<RegistrationResult, AgentError> {
    let api_url = distri_url
        .map(|s| s.to_string())
        .unwrap_or_else(get_distri_api_url);
    let client = DistriClient::from_url(api_url.clone());
    let distri_running = client.check_distri_running().await;

    let mut result = RegistrationResult {
        distri_running,
        distri_endpoint: api_url.clone(),
        agents: Vec::new(),
    };

    if !distri_running {
        warn!(
            "Distri server is not running or not reachable at {}",
            api_url
        );
        return Ok(result);
    }

    debug!("Distri server is running. Registering agents...");

    // Get working directory
    let work_dir = std::env::current_dir().map_err(|e| {
        AgentError::IoError(std::io::Error::other(format!(
            "Failed to get current directory: {}",
            e
        )))
    })?;

    debug!("Working directory: {:?}", work_dir);

    // Load embedded agents
    let embedded_agents = load_embedded_agents();
    debug!("Loaded {} embedded agents", embedded_agents.len());

    // Load working directory agents
    let working_dir_agents = load_working_directory_agents(&work_dir)?;
    debug!(
        "Loaded {} agents from working directory",
        working_dir_agents.len()
    );

    // Merge agents (working directory overrides embedded)
    let mut agents = merge_agents(embedded_agents, working_dir_agents);

    // Apply model settings override if provided
    if let Some(settings) = model_settings {
        debug!("Applying model settings override: {:?}", settings);
        for agent in agents.values_mut() {
            agent.content = apply_model_settings_override(&agent.content, settings);
        }
    }

    if agents.is_empty() {
        warn!("No agents found to register");
        return Ok(result);
    }

    debug!("Registering {} agents...", agents.len());

    // Register each agent and track status
    for (name, agent) in &agents {
        match client.register_agent(agent).await {
            Ok(_) => {
                debug!("Successfully registered agent: {}", name);
                result.agents.push(AgentRegistrationStatus {
                    name: name.clone(),
                    success: true,
                    error: None,
                });
            }
            Err(e) => {
                error!("Failed to register agent {}: {}", name, e);
                result.agents.push(AgentRegistrationStatus {
                    name: name.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    Ok(result)
}

fn set_backend_url(config: &mut LucyConfig, backend_url: Option<String>) {
    if let Some(backend_url) = backend_url {
        if let Some(model_settings) = config.model_settings.as_mut() {
            if let Some(provider) = model_settings.provider.as_mut() {
                provider.base_url = Some(format!("{}/lucy/v1", backend_url));
            }
        }
    }
}

/// Register all agents with the Distri server
/// Loads config from lucy.json if available
pub async fn register_agents(
    distri_url: Option<String>,
    backend_url: Option<String>,
) -> Result<(), AgentError> {
    // Try to load config from lucy.json
    let lucy_config = match load_lucy_config() {
        Ok(Some(config)) => config,
        _ => {
            let mut config = LucyConfig::default();
            set_backend_url(&mut config, backend_url);
            config
        }
    };

    // Use config values if available, otherwise fall back to env/defaults
    let api_url = distri_url.unwrap_or(lucy_config.distri_url.unwrap_or_else(get_distri_api_url));
    let client = DistriClient::from_url(api_url.clone());

    debug!("Checking if Distri server is running at: {}", api_url);

    // Check if Distri server is running before attempting registration
    if !client.check_distri_running().await {
        return Err(AgentError::DistriNotRunning);
    }

    debug!("Distri server is running. Registering agents...");
    println!("✅ Distri server is running. Registering agents...");

    // Get working directory
    let work_dir = std::env::current_dir().map_err(|e| {
        AgentError::IoError(std::io::Error::other(format!(
            "Failed to get current directory: {}",
            e
        )))
    })?;

    debug!("Working directory: {:?}", work_dir);

    // Load embedded agents
    let embedded_agents = load_embedded_agents();
    debug!("Loaded {} embedded agents", embedded_agents.len());

    // Load working directory agents
    let working_dir_agents = load_working_directory_agents(&work_dir)?;
    debug!(
        "Loaded {} agents from working directory",
        working_dir_agents.len()
    );

    // Merge agents (working directory overrides embedded)
    let mut agents = merge_agents(embedded_agents, working_dir_agents);

    if agents.is_empty() {
        warn!("No agents found to register");
        return Ok(());
    }

    // Apply model settings override if provided in config
    if let Some(ref settings) = lucy_config.model_settings {
        debug!("Applying model settings override from lucy.json");
        for agent in agents.values_mut() {
            agent.content = apply_model_settings_override(&agent.content, settings);
        }
    }

    debug!("Registering {} agents...", agents.len());

    // Register each agent
    let mut success_count = 0;
    let mut error_count = 0;

    for (name, agent) in &agents {
        match client.register_agent(agent).await {
            Ok(_) => {
                debug!("Successfully registered agent: {}", name);
                success_count += 1;
            }
            Err(e) => {
                error!("Failed to register agent {}: {}", name, e);
                error_count += 1;
            }
        }
    }

    if error_count > 0 {
        warn!(
            "Registered {}/{} agents successfully. {} failed.",
            success_count,
            agents.len(),
            error_count
        );
        eprintln!(
            "⚠️  Registered {}/{} agents successfully. {} failed.",
            success_count,
            agents.len(),
            error_count
        );
    } else {
        debug!("Successfully registered all {} agents", success_count);
        println!("✅ Successfully registered all {} agents", success_count);
    }

    Ok(())
}
