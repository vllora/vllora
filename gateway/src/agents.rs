use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;
use tracing::{error, info, warn};

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
];

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Failed to read agent file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to register agent: {0}")]
    RegistrationError(String),
    #[error("Distri API URL not configured")]
    MissingApiUrl,
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

            info!("Loaded agent from working directory: {}", filename);
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
        info!(
            "Overriding embedded agent with working directory version: {}",
            name
        );
        merged.insert(name, agent);
    }

    merged
}

/// Get the Distri API URL from environment or use default
fn get_distri_api_url() -> String {
    std::env::var("DISTRI_URL").unwrap_or_else(|_| "http://localhost:8081".to_string())
}

/// Check if Distri server is running by calling GET /v1/agents
async fn check_distri_running(api_url: &str) -> bool {
    let url = format!("{}/v1/agents", api_url);

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
async fn register_agent(api_url: &str, agent: &AgentDefinition) -> Result<(), AgentError> {
    let url = format!("{}/v1/agents", api_url);

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
pub async fn register_agents_with_status() -> Result<RegistrationResult, AgentError> {
    let api_url = get_distri_api_url();
    let distri_running = check_distri_running(&api_url).await;

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

    info!("Distri server is running. Registering agents...");

    // Get working directory
    let work_dir = std::env::current_dir().map_err(|e| {
        AgentError::IoError(std::io::Error::other(format!(
            "Failed to get current directory: {}",
            e
        )))
    })?;

    info!("Working directory: {:?}", work_dir);

    // Load embedded agents
    let embedded_agents = load_embedded_agents();
    info!("Loaded {} embedded agents", embedded_agents.len());

    // Load working directory agents
    let working_dir_agents = load_working_directory_agents(&work_dir)?;
    info!(
        "Loaded {} agents from working directory",
        working_dir_agents.len()
    );

    // Merge agents (working directory overrides embedded)
    let agents = merge_agents(embedded_agents, working_dir_agents);

    if agents.is_empty() {
        warn!("No agents found to register");
        return Ok(result);
    }

    info!("Registering {} agents...", agents.len());

    // Register each agent and track status
    for (name, agent) in &agents {
        match register_agent(&api_url, agent).await {
            Ok(_) => {
                info!("Successfully registered agent: {}", name);
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

/// Register all agents with the Distri server
pub async fn register_agents() -> Result<(), AgentError> {
    let api_url = get_distri_api_url();
    info!("Checking if Distri server is running at: {}", api_url);

    // Check if Distri server is running before attempting registration
    if !check_distri_running(&api_url).await {
        eprintln!(
            "⚠️  Distri server is not running or not reachable at {}",
            api_url
        );
        eprintln!("   Skipping agent registration. Start Distri server to register agents.");
        warn!(
            "Distri server is not running or not reachable at {}",
            api_url
        );
        return Ok(());
    }

    info!("Distri server is running. Registering agents...");
    println!("✅ Distri server is running. Registering agents...");

    // Get working directory
    let work_dir = std::env::current_dir().map_err(|e| {
        AgentError::IoError(std::io::Error::other(format!(
            "Failed to get current directory: {}",
            e
        )))
    })?;

    info!("Working directory: {:?}", work_dir);

    // Load embedded agents
    let embedded_agents = load_embedded_agents();
    info!("Loaded {} embedded agents", embedded_agents.len());

    // Load working directory agents
    let working_dir_agents = load_working_directory_agents(&work_dir)?;
    info!(
        "Loaded {} agents from working directory",
        working_dir_agents.len()
    );

    // Merge agents (working directory overrides embedded)
    let agents = merge_agents(embedded_agents, working_dir_agents);

    if agents.is_empty() {
        warn!("No agents found to register");
        return Ok(());
    }

    info!("Registering {} agents...", agents.len());

    // Register each agent
    let mut success_count = 0;
    let mut error_count = 0;

    for (name, agent) in &agents {
        match register_agent(&api_url, agent).await {
            Ok(_) => {
                info!("Successfully registered agent: {}", name);
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
        info!("Successfully registered all {} agents", success_count);
        println!("✅ Successfully registered all {} agents", success_count);
    }

    Ok(())
}
