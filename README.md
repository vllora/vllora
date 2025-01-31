# LangDB AI Gateway

A Rust-based gateway service for interacting with various LLM providers (OpenAI, Anthropic, etc.) with unified API interface.

## Features

- OpenAI-compatible API endpoints
- Model configuration via YAML
- Support for multiple LLM providers
- Debug-level event logging
- OpenTelemetry integration
- Cost tracking and usage monitoring

## Setup

### Quick Start

1. Clone the repository:
```bash
git clone https://github.com/langdb/ai-gateway.git
cd ai-gateway
```

2. Run the server with default configuration:
```bash
cargo run -- serve
```

This will start the server on `127.0.0.1:8080` with default settings.

### Environment Variables

Set the API keys for the providers you plan to use:
```bash
# API Keys for different providers (set the ones you plan to use)
export LANGDB_OPENAI_API_KEY=your-openai-key-here
export LANGDB_BEDROCK_API_KEY=your-bedrock-key-here
export LANGDB_ANTHROPIC_API_KEY=your-anthropic-key-here
export LANGDB_GEMINI_API_KEY=your-gemini-key-here
export LANGDB_TOGETHERAI_API_KEY=your-togetherai-key-here
export LANGDB_XAI_API_KEY=your-xai-key-here
export LANGDB_DEEPSEEK_API_KEY=your-deepseek-key-here

# Optional: Set log level (default: info)
export RUST_LOG=debug
```

### Running with Options

The server supports various configuration options that can be specified either via command line arguments or through a config file.

#### Command Line Options

```bash
# Run with custom host and port
cargo run -- serve --host 0.0.0.0 --port 3000

# Run with CORS origins
cargo run -- serve --cors-origins "http://localhost:3000,http://example.com"

# Run with rate limiting
cargo run -- serve --rate-hourly 1000

# Run with cost limits
cargo run -- serve --cost-daily 100.0 --cost-monthly 1000.0

# Run with custom database connections
cargo run -- serve --clickhouse-url "clickhouse://localhost:9000" --redis-url "redis://localhost:6379"
```

#### Using Config File

1. Copy the example config file:
```bash
cp config.sample.yaml config.yaml
```

2. Run the server:
```bash
cargo run -- serve
```

Command line options will override corresponding config file settings when both are specified.

## API Endpoints

The gateway provides the following OpenAI-compatible endpoints:

- `POST /v1/chat/completions` - Chat completions
- `GET /v1/models` - List available models
- `POST /v1/embeddings` - Generate embeddings
- `POST /v1/images/generations` - Generate images

## Example Usage

1. Run the server with your OpenAI API key:
```bash
LANGDB_OPENAI_API_KEY=your-api-key cargo run
```

2. Make a chat completion request:
```bash
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o-mini",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

The gateway will now be running with basic functionality on http://localhost:8080.

## Tracing

The gateway supports OpenTelemetry tracing with ClickHouse as the storage backend. All traces are stored in the `langdb.traces` table.

### Setting up Tracing

1. Create the traces table in ClickHouse:
```bash
# Create langdb database if it doesn't exist
clickhouse-client --query "CREATE DATABASE IF NOT EXISTS langdb"

# Import the traces table schema
clickhouse-client --query "$(cat sql/traces.sql)"
```

2. Enable tracing by providing the ClickHouse URL when running the server:
```bash
cargo run -- serve --clickhouse-url "clickhouse://localhost:9000"
```

You can also set the URL in your `config.yaml`:
```yaml
clickhouse:
  url: "http://localhost:8123"
```

### Querying Traces

The traces are stored in the `langdb.traces` table. Here are some example queries:

```sql
-- Get recent traces
SELECT 
    trace_id,
    operation_name,
    start_time_us,
    finish_time_us,
    (finish_time_us - start_time_us) as duration_us
FROM langdb.traces
WHERE finish_date >= today() - 1
ORDER BY finish_time_us DESC
LIMIT 10;

## Rate Limiting and Cost Control

The gateway provides rate limiting and cost control features to help manage API usage and costs. These features require Redis for state management.

### Setting up Redis

1. Enable Redis by providing the Redis URL when running the server:
```bash
cargo run -- serve --redis-url "redis://localhost:6379"
```

You can also set the URL in your `config.yaml`:
```yaml
redis:
  url: "redis://localhost:6379"
```

### Rate Limiting

Rate limiting helps prevent API abuse by limiting the number of requests within a time window. Configure rate limits using:

```bash
# Limit to 1000 requests per hour
cargo run -- serve --rate-hourly 1000
```

Or in `config.yaml`:
```yaml
rate_limit:
  hourly: 1000
```

When a rate limit is exceeded, the API will return a 429 (Too Many Requests) response.

### Cost Control

Cost control helps manage API spending by setting daily, monthly, or total cost limits. Configure cost limits using:

```bash
# Set daily and monthly limits
cargo run -- serve \
  --cost-daily 100.0 \
  --cost-monthly 1000.0 \
  --cost-total 5000.0
```

Or in `config.yaml`:
```yaml
cost_control:
  daily: 100.0   # $100 per day
  monthly: 1000.0  # $1000 per month
  total: 5000.0    # $5000 total
```

When a cost limit is reached, the API will return a 429 response with a message indicating the limit has been exceeded.

You can monitor current usage through Redis:
```bash
# Get current daily cost
redis-cli GET "default:llm_usage:2025-01-31"

# Get current monthly cost
redis-cli GET "default:llm_usage:2025-01"

# Get total cost
redis-cli GET "default:llm_usage:total"

# Get current rate limit counter
redis-cli GET "default:api_calls:2025-01-31"
```

## Running with Docker Compose

For a complete setup including ClickHouse for tracing and Redis for rate limiting/cost control, follow these steps:

1. Start the services using Docker Compose:
```bash
docker-compose up -d
```

This will start:
- ClickHouse server on port 8123 (HTTP) for tracing
- Redis server on port 6379 for rate limiting and cost control
- All necessary configurations will be loaded from `docker/clickhouse/server/config.d`

2. Build and run the gateway with Docker services:
```bash
cargo run -- serve \
  --clickhouse-url "http://localhost:8123" \
  --redis-url "redis://localhost:6379"
```

You can also configure these in your `config.yaml`:
```yaml
clickhouse:
  url: "http://localhost:8123"
redis:
  url: "redis://localhost:6379"
```

## Using MCP Tools
```sh
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o-mini",
    "messages": [{"role": "user", "content": "Ping the server using the tool and return the response"}],
    "mcp_servers": [{"server_url": "http://localhost:3004"}]
  }'
```

## Development

### Project Structure

- `gateway/` - Core gateway implementation
  - Models and provider integrations
  - API types and handlers
  - OpenTelemetry integration
- `server/` - HTTP server implementation
  - Configuration management
  - REST API endpoints
  - Cost tracking

### Running Tests

```bash
cargo test
```

### Logging

The gateway uses `tracing` for logging. Set the `RUST_LOG` environment variable to control log levels:

```bash
RUST_LOG=debug cargo run    # For detailed logs
RUST_LOG=info cargo run     # For standard logs
```

## License

This project is released under the [Elastic License 2.0](./LICENSE.md). See the license file for more information.


## Roadmap

- [x] Include License (Apache2)
- [x] clickhouse config + traces
- [x] Provide example docker-compose (simple / full (clickhouse + redis))
- [x] cost control
- [x] rate limiting (redis configuration)
- [ ] cargo install / curl -sH install
- [ ] CI/CD for ubuntu / mac silicon
- [ ] postman 
- [ ] Include Model selection config (All / Filter)
- [ ] usage command (runs a query and prints model usage)
- [ ] README has explanations each of them.
- [ ] Docs (opensource section) / Mrunmay
- [ ] 
