use crate::metadata::pool::DbPool;
use crate::metadata::services::mcp_config::McpConfigService;
use crate::rmcp::model::Tool;
use crate::types::mcp::McpConfig;
use crate::types::GatewayTenant;
use crate::GatewayApiError;
use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use vllora_llm::mcp::get_tools;

#[derive(Deserialize)]
pub struct CreateMcpConfigRequest {
    pub config: McpConfig,
}

#[derive(Deserialize)]
pub struct UpdateMcpConfigRequest {
    pub config: Option<McpConfig>,
    pub tools: Option<HashMap<String, Vec<Tool>>>,
}

#[derive(Serialize)]
pub struct McpConfigResponse {
    pub id: String,
    pub company_slug: String,
    pub config: McpConfig,
    pub tools: Value,
    pub tools_refreshed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct McpConfigListResponse {
    pub configs: Vec<McpConfigResponse>,
}

impl From<crate::metadata::models::mcp_config::DbMcpConfig> for McpConfigResponse {
    fn from(db_config: crate::metadata::models::mcp_config::DbMcpConfig) -> Self {
        let config = db_config.to_mcp_config().unwrap_or_default();
        let tools = db_config
            .get_tools()
            .unwrap_or_else(|_| Value::Array(vec![]));

        Self {
            id: db_config.id.to_string(),
            company_slug: db_config.company_slug,
            config,
            tools,
            tools_refreshed_at: db_config.tools_refreshed_at.map(|t| t.to_rfc3339()),
            created_at: db_config.created_at.to_rfc3339(),
            updated_at: db_config.updated_at.to_rfc3339(),
        }
    }
}

/// List all MCP configurations
pub async fn list_mcp_configs(
    db_pool: web::Data<DbPool>,
    tenant: web::ReqData<GatewayTenant>,
) -> Result<HttpResponse, GatewayApiError> {
    let service = McpConfigService::new(db_pool.get_ref().clone());

    let db_configs = service
        .get_all(&tenant.name)
        .map_err(|e| GatewayApiError::CustomError(format!("Failed to fetch MCP configs: {}", e)))?;

    let configs: Vec<McpConfigResponse> = db_configs.into_iter().map(|c| c.into()).collect();

    Ok(HttpResponse::Ok().json(McpConfigListResponse { configs }))
}

/// Get MCP configuration by ID
pub async fn get_mcp_config(
    path: web::Path<uuid::Uuid>,
    db_pool: web::Data<DbPool>,
    tenant: web::ReqData<GatewayTenant>,
) -> Result<HttpResponse, GatewayApiError> {
    let service = McpConfigService::new(db_pool.get_ref().clone());
    let id = path.into_inner();

    let db_config = service
        .get_by_id(&id, &tenant.name)
        .map_err(|e| GatewayApiError::CustomError(format!("Failed to fetch MCP config: {}", e)))?;

    Ok(HttpResponse::Ok().json(McpConfigResponse::from(db_config)))
}

/// Upsert MCP configuration for a company (create or update)
pub async fn upsert_mcp_config(
    req: web::Json<CreateMcpConfigRequest>,
    db_pool: web::Data<DbPool>,
    tenant: web::ReqData<GatewayTenant>,
) -> Result<HttpResponse, GatewayApiError> {
    let service = McpConfigService::new(db_pool.get_ref().clone());

    let db_config = service
        .upsert(tenant.name.clone(), &req.config)
        .map_err(|e| GatewayApiError::CustomError(format!("Failed to upsert MCP config: {}", e)))?;

    Ok(HttpResponse::Ok().json(McpConfigResponse::from(db_config)))
}

/// Update MCP configuration by ID
pub async fn update_mcp_config(
    path: web::Path<uuid::Uuid>,
    req: web::Json<UpdateMcpConfigRequest>,
    db_pool: web::Data<DbPool>,
    tenant: web::ReqData<GatewayTenant>,
) -> Result<HttpResponse, GatewayApiError> {
    let service = McpConfigService::new(db_pool.get_ref().clone());
    let id = path.into_inner();

    // Get existing config to check what needs to be updated
    let _existing_config = service.get_by_id(&id, &tenant.name).map_err(|e| {
        GatewayApiError::CustomError(format!("Failed to fetch existing MCP config: {}", e))
    })?;

    let result = match (&req.config, &req.tools) {
        (Some(config), Some(tools)) => {
            // Update both config and tools
            service.update_config_and_tools(&id, &tenant.name, config, tools)
        }
        (Some(config), None) => {
            // Update config only
            service.update_config(&id, &tenant.name, config)
        }
        (None, Some(tools)) => {
            // Update tools only
            service.update_tools(&id, &tenant.name, tools)
        }
        (None, None) => {
            return Err(GatewayApiError::CustomError(
                "No fields to update".to_string(),
            ));
        }
    };

    let _updated_rows = result
        .map_err(|e| GatewayApiError::CustomError(format!("Failed to update MCP config: {}", e)))?;

    // Return the updated config
    let db_config = service.get_by_id(&id, &tenant.name).map_err(|e| {
        GatewayApiError::CustomError(format!("Failed to fetch updated MCP config: {}", e))
    })?;

    Ok(HttpResponse::Ok().json(McpConfigResponse::from(db_config)))
}

/// Delete MCP configuration by ID
pub async fn delete_mcp_config(
    path: web::Path<uuid::Uuid>,
    db_pool: web::Data<DbPool>,
    tenant: web::ReqData<GatewayTenant>,
) -> Result<HttpResponse, GatewayApiError> {
    let service = McpConfigService::new(db_pool.get_ref().clone());
    let id = path.into_inner();

    let deleted_rows = service
        .delete(&id, &tenant.name)
        .map_err(|e| GatewayApiError::CustomError(format!("Failed to delete MCP config: {}", e)))?;

    if deleted_rows == 0 {
        return Err(GatewayApiError::CustomError(
            "MCP config not found".to_string(),
        ));
    }

    Ok(HttpResponse::NoContent().finish())
}

pub async fn update_mcp_config_tools(
    path: web::Path<uuid::Uuid>,
    db_pool: web::Data<DbPool>,
    tenant: web::ReqData<GatewayTenant>,
) -> Result<HttpResponse, GatewayApiError> {
    let service = McpConfigService::new(db_pool.get_ref().clone());
    let id = path.into_inner();

    let db_config = service
        .get_by_id(&id, &tenant.name)
        .map_err(|e| GatewayApiError::CustomError(format!("Failed to fetch MCP config: {}", e)))?;

    let config = db_config.to_mcp_config().unwrap_or_default();

    let mut tools_result = HashMap::new();
    for (name, config) in config.mcp_servers.iter() {
        let definition = config.to_mcp_definition();
        let tools = get_tools(&[definition])
            .await
            .map_err(|e| GatewayApiError::CustomError(format!("Failed to fetch MCP tools: {}", e)));

        match tools {
            Ok(tools) => {
                let mut tools_list = vec![];
                for server_tools in tools {
                    for tool in server_tools.tools {
                        tools_list.push(tool.0);
                    }
                }

                tools_result.insert(name.clone(), tools_list);
            }
            Err(e) => {
                tracing::error!("Failed to fetch MCP tools: {}", e);
            }
        }
    }

    service
        .update_tools(&id, &tenant.name, &tools_result)
        .map_err(|e| {
            GatewayApiError::CustomError(format!("Failed to update MCP config tools: {}", e))
        })?;

    Ok(HttpResponse::Ok().json(tools_result))
}

pub async fn get_mcp_config_tools(
    json: web::Json<McpConfig>,
) -> Result<HttpResponse, GatewayApiError> {
    let mut tools_result = HashMap::new();
    for (name, config) in json.mcp_servers.iter() {
        let definition = config.to_mcp_definition();
        let tools = get_tools(&[definition])
            .await
            .map_err(|e| GatewayApiError::CustomError(format!("Failed to fetch MCP tools: {}", e)));
        match tools {
            Ok(tools) => {
                let mut tools_list = vec![];
                for server_tools in tools {
                    for tool in server_tools.tools {
                        tools_list.push(tool.0);
                    }
                }
                tools_result.insert(name.clone(), tools_list);
            }
            Err(e) => {
                tracing::error!("Failed to fetch MCP tools: {}", e);
            }
        }
    }

    Ok(HttpResponse::Ok().json(tools_result))
}
