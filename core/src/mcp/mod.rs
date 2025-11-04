//! MCP (Model Context Protocol) package for Vllora
//!
//! This package handles MCP resolution and integration, facilitating the connection
//! between Vllora and external tools and data sources through the Model Context Protocol.

// Re-export MCP types from the types module

pub mod server;
pub mod transport;

pub use crate::types::mcp::*;
