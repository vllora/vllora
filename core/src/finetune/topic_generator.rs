use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::Span;
use vllora_llm::{
    client::message_mapper::MessageMapper,
    types::{
        gateway::{
            ChatCompletionContent, ChatCompletionMessage, ChatCompletionRequest,
            ChatCompletionRequestWithTools,
        },
        tools::ModelTools,
    },
};
use vllora_llm::types::ModelEvent;
use vllora_llm::types::gateway::OpenaiResponseFormat;

use crate::{
    credentials::GatewayCredentials,
    executor::{
        chat_completion::{basic_executor, basic_executor::BasicCacheContext, resolve_model_instance},
        context::ExecutorContext,
    },
};

/// Represents a node in the topic hierarchy tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicHierarchyNode {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TopicHierarchyNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
}

/// Properties for generating topic hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateTopicHierarchyProperties {
    /// User's description of dataset goals (used as context for LLM)
    pub goals: String,
    /// Hierarchy depth (1-5 levels)
    pub depth: u32,
    /// Sample records from the dataset for context
    pub records: Vec<DatasetRecord>,
    /// Maximum number of root topics (default: 5)
    #[serde(default = "default_max_topics")]
    pub max_topics: u32,
    /// Number of subtopics per topic (default: 3)
    #[serde(default = "default_degree")]
    pub degree: u32,
    /// Model to use (default: "gpt-4o-mini")
    #[serde(default = "default_model")]
    pub model: String,
    /// Temperature (default: 0.7)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_max_topics() -> u32 {
    5
}

fn default_degree() -> u32 {
    3
}

fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

fn default_temperature() -> f32 {
    0.7
}

/// Represents a dataset record with input/output data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRecord {
    /// The record data (typically JSON object)
    pub data: serde_json::Value,
}

/// Result of topic hierarchy generation
#[derive(Debug, Serialize)]
pub struct GenerateHierarchyResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<Vec<TopicHierarchyNode>>,
}

/// Response schema for topic list extraction
#[derive(Debug, Deserialize)]
struct TopicListResponse {
    topics: Vec<String>,
}

/// Response schema for subtopic list extraction
#[derive(Debug, Deserialize)]
struct SubtopicListResponse {
    subtopics: Vec<String>,
}

/// System prompt for topic extraction
const TOPIC_EXTRACTION_SYSTEM_PROMPT: &str = r#"You are a hierarchical topic extraction engine.

Your job is to identify DIMENSIONS OF VARIATION in user interactions for organizing synthetic training data.

You must output variation dimensions that help generate NEW, diverse records later.

Two kinds of variation are allowed:
A) OBSERVED variations: directly evidenced in the trace
B) COVERAGE variations: not necessarily in the trace, but plausible and useful given the objective + available tools + IO contract

CRITICAL PRINCIPLE:
- Topics represent how future USER INTERACTIONS can vary.
- Do NOT restate static system instructions or formatting rules as topics unless they are frequently violated in practice.

Rules:
- Prefer user intent/action variations, input-parameter variations, and tool-usage variations.
- Prefer variations that are actionable for data generation (you can write a new user message/tool scenario from it).
- Avoid meta topics like "temperature", "analysis_depth", "misc", "general", "other".
- Use lowercase_with_underscores only.
- Do not include literal concrete values from the trace as a topic (e.g., a specific ID, a specific parameter value). Use a general dimension name instead.

You do NOT explain your reasoning.
You ONLY return structured output matching the schema."#;

/// System prompt for subtopic expansion
const SUBTOPIC_EXPANSION_SYSTEM_PROMPT: &str = r#"You are a hierarchical topic expansion engine.

Expand a VARIATION DIMENSION into more specific sub-dimensions for synthetic data generation.

Two kinds of sub-variation are allowed:
A) OBSERVED sub-variations: evidenced in the trace
B) COVERAGE sub-variations: plausible variants consistent with the objective + tools + IO contract

Rules:
- Children must be strictly more specific than the parent.
- Siblings must be mutually distinct (minimal overlap).
- Focus on variations that can directly drive generation of new user inputs or tool-call scenarios.
- Avoid meta topics like "temperature", "analysis_depth", "general".
- Use lowercase_with_underscores only.
- If no meaningful expansion exists, return an empty list.

You do NOT explain your reasoning.
You ONLY return structured output matching the schema."#;

/// Generate a topic hierarchy tree based on the provided properties
pub async fn generate_topic_hierarchy(
    properties: GenerateTopicHierarchyProperties,
    executor_context: &ExecutorContext,
    project_slug: &str,
) -> GenerateHierarchyResult {
    // Validate depth
    if properties.depth < 1 || properties.depth > 5 {
        return GenerateHierarchyResult {
            success: false,
            error: Some("Depth must be between 1 and 5".to_string()),
            hierarchy: None,
        };
    }

    // Format trace content from records
    let trace_content = format_trace_for_prompt(&properties.records);

    // Extract root topics
    let extraction_prompt = build_topic_extraction_prompt(
        &trace_content,
        &properties.goals,
        properties.max_topics,
    );

    let root_topics = match call_llm_for_topics(
        &extraction_prompt,
        executor_context,
        project_slug,
        &properties.model,
        properties.temperature,
    )
    .await
    {
        Ok(topics) => topics,
        Err(e) => {
            tracing::warn!("Failed to extract topics: {}", e);
            vec!["general".to_string()]
        }
    };

    if root_topics.is_empty() {
        return GenerateHierarchyResult {
            success: false,
            error: Some("No root topics extracted".to_string()),
            hierarchy: None,
        };
    }

    // Build hierarchy iteratively using a queue and node map
    use std::collections::HashMap;
    
    // Map to store all nodes by ID: node_id -> (node, parent_id, depth)
    let mut node_map: HashMap<String, (TopicHierarchyNode, Option<String>, u32)> = HashMap::new();
    
    // Queue of nodes to expand: (node_id, depth, parent_path)
    let mut expansion_queue: Vec<(String, u32, Vec<String>)> = Vec::new();
    
    // Initialize queue with root topics
    let mut root_node_ids = Vec::new();
    for topic in root_topics.into_iter().take(properties.max_topics as usize) {
        let node_id = build_node_id(None, &topic);
        let node = TopicHierarchyNode {
            id: node_id.clone(),
            name: topic.clone(),
            children: None,
            selected: None,
        };
        
        node_map.insert(node_id.clone(), (node, None, 1));
        root_node_ids.push(node_id.clone());
        
        if properties.depth > 1 {
            expansion_queue.push((node_id, 1, vec![topic.clone()]));
        }
    }
    
    // Process queue iteratively
    while !expansion_queue.is_empty() {
        let mut batch = Vec::new();
        std::mem::swap(&mut expansion_queue, &mut batch);
        
        // Process all items in the batch
        for (node_id, current_depth, parent_path) in batch {
            if current_depth >= properties.depth {
                continue;
            }
            
            let prompt = build_subtopic_expansion_prompt(
                &parent_path,
                &trace_content,
                &properties.goals,
                properties.degree,
            );
            
            match call_llm_for_subtopics(
                &prompt,
                executor_context,
                project_slug,
                &properties.model,
                properties.temperature,
            )
            .await
            {
                Ok(subtopics) => {
                    let subtopics = subtopics.into_iter().take(properties.degree as usize).collect::<Vec<_>>();
                    
                    if !subtopics.is_empty() {
                        let mut child_ids = Vec::new();
                        for subtopic in subtopics {
                            let child_id = build_node_id(Some(&node_id), &subtopic);
                            let mut new_path = parent_path.clone();
                            new_path.push(subtopic.clone());
                            
                            let child_node = TopicHierarchyNode {
                                id: child_id.clone(),
                                name: subtopic.clone(),
                                children: None,
                                selected: None,
                            };
                            
                            child_ids.push(child_id.clone());
                            node_map.insert(child_id.clone(), (child_node, Some(node_id.clone()), current_depth + 1));
                            
                            // Add to queue if we haven't reached max depth
                            if current_depth + 1 < properties.depth {
                                expansion_queue.push((child_id, current_depth + 1, new_path));
                            }
                        }
                        
                        // Update parent node with children IDs (we'll build the tree structure later)
                        // The tree structure will be built at the end from the node map
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to expand topic {:?}: {}", parent_path, e);
                }
            }
        }
    }
    
    // Build the tree structure from the node map
    // We need to build from leaves to root
    let mut depth_levels: Vec<Vec<String>> = vec![Vec::new(); (properties.depth + 1) as usize];
    for (node_id, (_, _, depth)) in node_map.iter() {
        let depth_usize = *depth as usize;
        if depth_usize <= properties.depth as usize && depth_usize < depth_levels.len() {
            depth_levels[depth_usize].push(node_id.clone());
        }
    }
    
    // Build tree from bottom up
    for depth in (1..=properties.depth).rev() {
        let depth_usize = depth as usize;
        if depth_usize >= depth_levels.len() {
            continue;
        }
        for node_id in &depth_levels[depth_usize] {
            // Find all children of this node (collect first to avoid borrow issues)
            let children: Vec<TopicHierarchyNode> = node_map
                .iter()
                .filter(|(_, (_, p, d))| p.as_ref() == Some(node_id) && *d == depth + 1)
                .map(|(_, (n, _, _))| n.clone())
                .collect();
            
            // Now update the node with children
            if let Some((node, _, _)) = node_map.get_mut(node_id) {
                if !children.is_empty() {
                    node.children = Some(children);
                }
            }
        }
    }
    
    // Build final hierarchy from root nodes
    let hierarchy: Vec<TopicHierarchyNode> = root_node_ids
        .into_iter()
        .filter_map(|id| node_map.remove(&id).map(|(node, _, _)| node))
        .collect();

    GenerateHierarchyResult {
        success: true,
        error: None,
        hierarchy: Some(hierarchy),
    }
}

/// Format trace content for prompts (emphasizes VARIABLE content over STATIC content)
fn format_trace_for_prompt(records: &[DatasetRecord]) -> String {
    let mut lines = Vec::new();

    for record in records.iter().take(20) {
        let data = &record.data;

        // -------- INPUT --------
        if let Some(input) = data.get("input") {
            // Messages
            if let Some(messages) = input.get("messages") {
                if let Some(arr) = messages.as_array() {
                    for msg in arr {
                        if let Some(msg_obj) = msg.as_object() {
                            let role = msg_obj
                                .get("role")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");

                            // SYSTEM messages are STATIC - summarize briefly
                            if role == "system" {
                                if let Some(content) = msg_obj.get("content").and_then(|v| v.as_str()) {
                                    let first_line = content.lines().next().unwrap_or("").chars().take(200).collect::<String>();
                                    lines.push(format!("[SYSTEM CONTEXT (static)]: {}...", first_line));
                                }
                                continue;
                            }

                            // Handle assistant messages with tool_calls - VARIABLE
                            if role == "assistant" {
                                if let Some(tool_calls) = msg_obj.get("tool_calls") {
                                    if let Some(arr) = tool_calls.as_array() {
                                        for tc in arr {
                                            if let Some(tc_obj) = tc.as_object() {
                                                let empty_map = serde_json::Map::new();
                                                let func = tc_obj.get("function").and_then(|v| v.as_object()).unwrap_or(&empty_map);
                                                let name = func.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                let args = func.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
                                                lines.push(format!("[TOOL CALL (variable)]: {}({})", name, args));
                                            }
                                        }
                                    }
                                }
                                continue;
                            }

                            // Handle tool result messages - VARIABLE
                            if role == "tool" {
                                if let Some(content) = msg_obj.get("content").and_then(|v| v.as_str()) {
                                    let truncated = if content.len() > 500 {
                                        format!("{}...", &content[..500])
                                    } else {
                                        content.to_string()
                                    };
                                    lines.push(format!("[TOOL RESULT (variable)]: {}", truncated));
                                }
                                continue;
                            }

                            // USER messages are VARIABLE - emphasize these
                            if role == "user" {
                                if let Some(content) = msg_obj.get("content").and_then(|v| v.as_str()) {
                                    let truncated = if content.len() > 1500 {
                                        format!("{}...", &content[..1500])
                                    } else {
                                        content.to_string()
                                    };
                                    lines.push(format!("[USER INPUT (variable)]: {}", truncated));
                                }
                                continue;
                            }

                            // Other messages
                            if let Some(content) = msg_obj.get("content").and_then(|v| v.as_str()) {
                                let truncated = if content.len() > 1000 {
                                    format!("{}...", &content[..1000])
                                } else {
                                    content.to_string()
                                };
                                lines.push(format!("[{}]: {}", role.to_uppercase(), truncated));
                            }
                        }
                    }
                }
            }

            // Tools (with descriptions and parameter names)
            if let Some(tools) = input.get("tools") {
                if let Some(arr) = tools.as_array() {
                    lines.push("[TOOLS AVAILABLE]:".to_string());
                    for t in arr {
                        if let Some(t_obj) = t.as_object() {
                            let func_obj = t_obj.get("function").and_then(|v| v.as_object());
                            let empty_map = serde_json::Map::new();
                            let func = func_obj.unwrap_or(&empty_map);
                            let name = func.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                            let desc = func.get("description").and_then(|v| v.as_str()).unwrap_or("").trim();
                            
                            let params = func.get("parameters").and_then(|v| v.as_object())
                                .and_then(|p| p.get("properties"))
                                .and_then(|v| v.as_object())
                                .map(|props| {
                                    props.keys().cloned().collect::<Vec<_>>().join(", ")
                                })
                                .unwrap_or_default();

                            let mut tool_line = format!("- {}", name);
                            if !desc.is_empty() {
                                tool_line.push_str(&format!(": {}", desc));
                            }
                            if !params.is_empty() {
                                tool_line.push_str(&format!(" (inputs: {})", params));
                            }
                            lines.push(tool_line);
                        }
                    }
                }
            }
        }

        // -------- OUTPUT --------
        if let Some(output) = data.get("output") {
            if let Some(messages) = output.get("messages") {
                if let Some(arr) = messages.as_array() {
                    for msg in arr {
                        if let Some(msg_obj) = msg.as_object() {
                            let role = msg_obj.get("role").and_then(|v| v.as_str()).unwrap_or("unknown");
                            if let Some(content) = msg_obj.get("content").and_then(|v| v.as_str()) {
                                let truncated = if content.len() > 1000 {
                                    format!("{}...", &content[..1000])
                                } else {
                                    content.to_string()
                                };
                                lines.push(format!("[{} OUTPUT]: {}", role.to_uppercase(), truncated));
                            }
                        }
                    }
                }
            }

            if let Some(tool_calls) = output.get("tool_calls") {
                if let Some(arr) = tool_calls.as_array() {
                    for tc in arr {
                        if let Some(tc_obj) = tc.as_object() {
                            let empty_map = serde_json::Map::new();
                            let func = tc_obj.get("function").and_then(|v| v.as_object()).unwrap_or(&empty_map);
                            let name = func.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                            let args = func.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
                            lines.push(format!("[TOOL CALL]: {}({})", name, args));
                        }
                    }
                }
            }
        }
    }

    lines.join("\n")
}

/// Build prompt for topic extraction
fn build_topic_extraction_prompt(trace_content: &str, objective: &str, max_topics: u32) -> String {
    format!(
        r#"TASK:
Identify up to {} DIMENSIONS OF VARIATION for synthetic data generation.

OBJECTIVE:
{}

TRACE EVIDENCE:
{}

GUIDANCE:
- Topics must describe things that can vary across future interactions:
  - user intents / tasks / requests
  - input types and parameter ranges
  - tool selection and tool-call sequencing patterns
  - missing/incorrect tool usage patterns
  - structured-output compliance patterns
  - grounding/citation fidelity patterns
  - single-turn vs multi-turn interaction structure
- If the trace set is very small (e.g., 1 trace), propose COVERAGE dimensions that would increase dataset diversity while staying consistent with the objective and tools.

AVOID:
- Repeating static system capabilities or static format requirements as topics
- Purely meta training knobs (temperature, depth, etc.)
- Specific literal values from the trace (IDs, exact arguments, etc.)

OUTPUT:
Return ONLY a JSON object with a "topics" array of topic names (lowercase_with_underscores)."#,
        max_topics, objective, trace_content
    )
}

/// Build prompt for subtopic expansion
fn build_subtopic_expansion_prompt(
    parent_path: &[String],
    trace_content: &str,
    objective: &str,
    degree: u32,
) -> String {
    let path_str = parent_path.join(" -> ");

    format!(
        r#"TASK:
Expand the variation dimension "{}" into up to {} more specific sub-dimensions.

OBJECTIVE:
{}

CURRENT VARIATION PATH:
{}

TRACE EVIDENCE:
{}

GUIDANCE:
- Each child should be a distinct "way this varies" in real usage.
- If evidence is weak (small trace set), propose COVERAGE sub-variations that are plausible and helpful for generating diverse records.
- Prefer variations tied to:
  - user request types
  - input parameter patterns
  - tool call selection and ordering
  - tool argument correctness
  - structured-output adherence
  - grounding/citation correctness

OUTPUT:
Return ONLY a JSON object with a "subtopics" array of subtopic names (lowercase_with_underscores)."#,
        parent_path.last().unwrap_or(&String::new()),
        degree,
        objective,
        path_str,
        trace_content
    )
}

/// Call LLM to extract topics
async fn call_llm_for_topics(
    prompt: &str,
    executor_context: &ExecutorContext,
    project_slug: &str,
    model: &str,
    temperature: f32,
) -> Result<Vec<String>, String> {
    // Create JSON schema for TopicListResponse
    let json_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "topics": {
                "type": "array",
                "items": {
                    "type": "string"
                }
            }
        },
        "required": ["topics"],
        "additionalProperties": false
    });

    let response_format = Some(OpenaiResponseFormat::JsonSchema {
        json_schema: vllora_llm::types::gateway::ResponseFormatJsonSchema {
            name: "topic_list_response".to_string(),
            schema: Some(json_schema),
            strict: Some(true),
            description: None,
        },
    });

    let request: ChatCompletionRequestWithTools<()> = ChatCompletionRequestWithTools {
        request: ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![
                ChatCompletionMessage {
                    role: "system".to_string(),
                    content: Some(ChatCompletionContent::Text(TOPIC_EXTRACTION_SYSTEM_PROMPT.to_string())),
                    ..Default::default()
                },
                ChatCompletionMessage {
                    role: "user".to_string(),
                    content: Some(ChatCompletionContent::Text(prompt.to_string())),
                    ..Default::default()
                },
            ],
            response_format: Some(response_format.clone().unwrap().into()),
            temperature: Some(temperature),
            ..Default::default()
        },
        ..Default::default()
    };

    let response_content = execute_llm_request(request, executor_context, project_slug).await?;
    
    // Parse response
    let response: TopicListResponse = serde_json::from_str(&response_content)
        .map_err(|e| format!("Failed to parse topic list response: {}", e))?;

    Ok(response.topics)
}

/// Call LLM to expand subtopics
async fn call_llm_for_subtopics(
    prompt: &str,
    executor_context: &ExecutorContext,
    project_slug: &str,
    model: &str,
    temperature: f32,
) -> Result<Vec<String>, String> {
    // Create JSON schema for SubtopicListResponse
    let json_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "subtopics": {
                "type": "array",
                "items": {
                    "type": "string"
                }
            }
        },
        "required": ["subtopics"],
        "additionalProperties": false
    });

    let response_format = Some(OpenaiResponseFormat::JsonSchema {
        json_schema: vllora_llm::types::gateway::ResponseFormatJsonSchema {
            name: "subtopic_list_response".to_string(),
            schema: Some(json_schema),
            strict: Some(true),
            description: None,
        },
    });

    let request: ChatCompletionRequestWithTools<()> = ChatCompletionRequestWithTools {
        request: ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![
                ChatCompletionMessage {
                    role: "system".to_string(),
                    content: Some(ChatCompletionContent::Text(SUBTOPIC_EXPANSION_SYSTEM_PROMPT.to_string())),
                    ..Default::default()
                },
                ChatCompletionMessage {
                    role: "user".to_string(),
                    content: Some(ChatCompletionContent::Text(prompt.to_string())),
                    ..Default::default()
                },
            ],
            response_format: Some(response_format.clone().unwrap().into()),
            temperature: Some(temperature),
            ..Default::default()
        },
        ..Default::default()
    };

    let response_content = execute_llm_request(request, executor_context, project_slug).await?;
    
    // Parse response
    let response: SubtopicListResponse = serde_json::from_str(&response_content)
        .map_err(|e| format!("Failed to parse subtopic list response: {}", e))?;

    Ok(response.subtopics)
}

/// Execute LLM request and extract content
async fn execute_llm_request(
    request: ChatCompletionRequestWithTools<()>,
    executor_context: &ExecutorContext,
    project_slug: &str,
) -> Result<String, String> {
    let llm_model = executor_context
        .model_metadata_factory
        .get_model_metadata(&request.request.model, false, false, None)
        .await
        .map_err(|e| format!("Failed to get model metadata: {}", e))?;

    let key = GatewayCredentials::extract_key_from_model(
        &llm_model,
        project_slug,
        "default",
        executor_context.key_storage.as_ref().as_ref(),
    )
    .await
    .map_err(|e| format!("Failed to extract key: {}", e))?;

    let resolved_model_context = resolve_model_instance(
        executor_context,
        &request,
        HashMap::new(),
        ModelTools(vec![]),
        Span::current(),
        None,
        request.request.messages.clone(),
        None,
        None,
        &llm_model,
        key.as_ref(),
    )
    .await
    .map_err(|e| format!("Failed to resolve model instance: {}", e))?;

    let mut messages = vec![];

    for message in &request.request.messages {
        messages.push(
            MessageMapper::map_completions_message_to_vllora_message(
                message,
                &request.request.model,
                "topic_generator",
            )
            .map_err(|e| format!("Failed to map message: {}", e))?,
        );
    }

    let (tx, _rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(10000);
    let result = basic_executor::execute(
        request.request,
        resolved_model_context.model_instance,
        messages,
        HashMap::new(),
        tx,
        Span::current(),
        None,
        HashMap::new(),
        BasicCacheContext::default(),
        Some(resolved_model_context.db_model),
    )
    .await
    .map_err(|e| format!("LLM execution failed: {}", e))?;

    // Extract content from response
    let message = &result.choices[0].message;
    match &message.content {
        Some(ChatCompletionContent::Text(text)) => Ok(text.clone()),
        Some(ChatCompletionContent::Content(parts)) => {
            // Extract text from array of content parts
            let text_parts: Vec<String> = parts
                .iter()
                .filter_map(|part| part.text.clone())
                .collect();
            Ok(text_parts.join(""))
        }
        None => Err("No content in LLM response".to_string()),
    }
}


/// Build a node ID from parent ID and name
fn build_node_id(parent_id: Option<&str>, name: &str) -> String {
    let safe_name = name.to_lowercase().replace(' ', "_").replace('/', "_");
    if let Some(parent) = parent_id {
        format!("{}/{}", parent, safe_name)
    } else {
        safe_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_node_id() {
        assert_eq!(build_node_id(None, "Test Topic"), "test_topic");
        assert_eq!(
            build_node_id(Some("parent"), "Child Topic"),
            "parent/child_topic"
        );
    }

    #[test]
    fn test_format_trace_for_prompt() {
        let records = vec![DatasetRecord {
            data: serde_json::json!({
                "input": {
                    "messages": [
                        {"role": "user", "content": "Hello"}
                    ]
                },
                "output": {
                    "messages": [
                        {"role": "assistant", "content": "Hi there!"}
                    ]
                }
            }),
        }];

        let formatted = format_trace_for_prompt(&records);
        assert!(formatted.contains("[USER INPUT (variable)]"));
        assert!(formatted.contains("Hello"));
    }
}
