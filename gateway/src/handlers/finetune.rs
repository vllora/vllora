use actix_web::{web, HttpRequest, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Dataset metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub id: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
}

/// Evaluation information for a dataset record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetEvaluation {
    pub score: Option<f64>,
    pub feedback: Option<String>,
    #[serde(rename = "evaluatedAt")]
    pub evaluated_at: Option<i64>,
}

/// Individual dataset record with span data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRecord {
    pub id: String,
    #[serde(rename = "datasetId")]
    pub dataset_id: String,
    pub data: Value, // Span data as JSON
    #[serde(rename = "spanId")]
    pub span_id: Option<String>,
    pub topic: Option<String>,
    pub evaluation: Option<DatasetEvaluation>,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

/// Combined view for API (dataset + its records)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetWithRecords {
    // Dataset fields (flattened)
    pub id: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at: i64,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    // Records
    pub records: Vec<DatasetRecord>,
}

/// Hyperparameters for finetuning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hyperparameters {
    #[serde(rename = "batch_size")]
    pub batch_size: Option<u32>,
    #[serde(rename = "learning_rate_multiplier")]
    pub learning_rate_multiplier: Option<f64>,
    #[serde(rename = "n_epochs")]
    pub n_epochs: Option<u32>,
}

/// Request to create a finetuning job via wrapper API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFinetuningJobWrapperRequest {
    pub dataset: DatasetWithRecords,
    #[serde(rename = "base_model")]
    pub base_model: String,
    pub provider: String,
    pub hyperparameters: Option<Hyperparameters>,
    pub suffix: Option<String>,
}

/// Message in OpenAI format
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

/// JSONL entry for OpenAI finetuning
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JSONLEntry {
    messages: Vec<OpenAIMessage>,
}

/// Extract messages from span attribute
fn extract_messages_from_span(span: &Value) -> Option<Vec<OpenAIMessage>> {
    let attribute = span.get("attribute")?;
    
    // Try to extract request messages
    let request_str = attribute.get("request")
        .or_else(|| attribute.get("input"))
        .and_then(|v| v.as_str());
    
    let mut messages = Vec::new();
    
    if let Some(request_str) = request_str {
        if let Ok(request_json) = serde_json::from_str::<Value>(request_str) {
            // Extract messages array
            let request_messages = request_json
                .get("messages")
                .or_else(|| request_json.get("contents"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_else(|| {
                    // If messages is not an array, try parsing the whole thing as an array
                    if request_json.is_array() {
                        request_json.as_array().cloned().unwrap_or_default()
                    } else {
                        vec![]
                    }
                });
            
            // Convert each message
            for msg in request_messages {
                if let Some(role) = msg.get("role").and_then(|v| v.as_str()) {
                    let content = extract_message_content(&msg);
                    if !content.is_empty() {
                        messages.push(OpenAIMessage {
                            role: role.to_string(),
                            content,
                        });
                    }
                }
            }
        }
    }
    
    // Extract response/output to create assistant message
    let output_str = attribute.get("output").and_then(|v| v.as_str());
    if let Some(output_str) = output_str {
        if let Ok(output_json) = serde_json::from_str::<Value>(output_str) {
            if let Some(response_content) = extract_response_content(&output_json, attribute) {
                messages.push(OpenAIMessage {
                    role: "assistant".to_string(),
                    content: response_content,
                });
            }
        }
    }
    
    // If no messages extracted, try alternative extraction
    if messages.is_empty() {
        if let Some(content) = attribute.get("content").and_then(|v| v.as_str()) {
            messages.push(OpenAIMessage {
                role: "assistant".to_string(),
                content: content.to_string(),
            });
        }
    }
    
    if messages.is_empty() {
        None
    } else {
        Some(messages)
    }
}

/// Extract message content from a message object
fn extract_message_content(msg: &Value) -> String {
    // Try direct content string
    if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
        return content.to_string();
    }
    
    // Try content array
    if let Some(content_array) = msg.get("content").and_then(|v| v.as_array()) {
        let mut parts = Vec::new();
        for part in content_array {
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                parts.push(text);
            } else if let Some(text) = part.as_str() {
                parts.push(text);
            }
        }
        if !parts.is_empty() {
            return parts.join("\n");
        }
    }
    
    // Try parts array
    if let Some(parts_array) = msg.get("parts").and_then(|v| v.as_array()) {
        let mut texts = Vec::new();
        for part in parts_array {
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                texts.push(text);
            }
        }
        if !texts.is_empty() {
            return texts.join("\n");
        }
    }
    
    // Try content.text
    if let Some(text) = msg.get("content")
        .and_then(|c| c.get("text"))
        .and_then(|v| v.as_str())
    {
        return text.to_string();
    }
    
    // Fallback: serialize as JSON string
    serde_json::to_string(msg).unwrap_or_default()
}

/// Extract response content from output JSON
fn extract_response_content(output_json: &Value, attribute: &Value) -> Option<String> {
    // Try choices[0].message.content (OpenAI format)
    if let Some(content) = output_json
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|v| v.as_str())
    {
        return Some(content.to_string());
    }
    
    // Try content array (Anthropic format)
    if let Some(content_array) = output_json.get("content").and_then(|v| v.as_array()) {
        let mut texts = Vec::new();
        for item in content_array {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                texts.push(text);
            }
        }
        if !texts.is_empty() {
            return Some(texts.join("\n"));
        }
    }
    
    // Try direct content string
    if let Some(content) = output_json.get("content").and_then(|v| v.as_str()) {
        return Some(content.to_string());
    }
    
    // Try attribute.response
    if let Some(response) = attribute.get("response").and_then(|v| v.as_str()) {
        return Some(response.to_string());
    }
    
    None
}

/// Generate JSONL from dataset records
fn generate_jsonl(dataset: &DatasetWithRecords) -> Result<Vec<u8>, String> {
    let mut jsonl_lines = Vec::new();
    
    for record in &dataset.records {
        if let Some(messages) = extract_messages_from_span(&record.data) {
            let entry = JSONLEntry { messages };
            let json_line = serde_json::to_string(&entry)
                .map_err(|e| format!("Failed to serialize JSONL entry: {}", e))?;
            jsonl_lines.push(json_line);
        }
    }
    
    if jsonl_lines.is_empty() {
        return Err("No valid messages found in dataset records".to_string());
    }
    
    Ok(jsonl_lines.join("\n").into_bytes())
}

/// Get cloud API URL
fn get_api_url() -> String {
    std::env::var("LANGDB_API_URL")
        .unwrap_or_else(|_| vllora_core::types::LANGDB_API_URL.to_string())
}

/// Create a finetuning job
/// This is a wrapper API that accepts DatasetWithRecords, generates JSONL, and forwards to cloud API
pub async fn create_finetuning_job(
    request: web::Json<CreateFinetuningJobWrapperRequest>,
    req: HttpRequest,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
) -> Result<HttpResponse> {
    // Generate JSONL from dataset records
    let jsonl_data = generate_jsonl(&request.dataset)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Failed to generate JSONL: {}", e)))?;
    
    // Build job configuration JSON
    let config = serde_json::json!({
        "base_model": request.base_model,
        "provider": request.provider,
        "hyperparameters": request.hyperparameters,
        "suffix": request.suffix,
    });
    
    let config_json = serde_json::to_string(&config)
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("Failed to serialize config: {}", e)))?;
    
    // Extract authorization header
    let auth_header = req.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    
    // Create multipart form data
    let client = reqwest::Client::new();
    let mut form = reqwest::multipart::Form::new();
    
    // Add config as JSON string
    form = form.text("config", config_json);
    
    // Add JSONL file
    let jsonl_part = reqwest::multipart::Part::bytes(jsonl_data)
        .file_name("training.jsonl")
        .mime_str("application/x-ndjson")
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("Failed to create multipart part: {}", e)))?;
    form = form.part("file", jsonl_part);
    
    // Build request to cloud API
    let mut cloud_request = client
        .post(format!("{}/finetune/jobs", get_api_url()))
        .multipart(form);
    
    let api_key = std::env::var("LANGDB_API_KEY").unwrap_or_default();
    cloud_request = cloud_request.header("Authorization", api_key);
    
    // Forward other relevant headers
    for (key, value) in req.headers().iter() {
        if key.as_str().starts_with("x-") || key.as_str() == "User-Agent" {
            if let Ok(value_str) = value.to_str() {
                cloud_request = cloud_request.header(key.as_str(), value_str);
            }
        }
    }
    
    // Send request to cloud API
    let response = cloud_request
        .send()
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("Failed to call cloud API: {}", e)))?;
    
    // Forward response status and body
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("Failed to read response: {}", e)))?;
    
    // Copy response headers
    // Note: We don't have access to response headers in this simple forwarding approach
    // In production, you might want to forward important headers
    
    Ok(HttpResponse::Ok().json(body))
}
