<div align="center">

# LangDB AI Gateway

A Rust-based gateway service for interacting with various LLM providers (OpenAI, Anthropic, etc.) with unified API interface.

<img src="assets/langdb-models.gif" width="550px" alt="LangDB AI Gateway Demo showing LLM Switching">
</div>

## Features

- OpenAI-compatible API endpoints
- Model configuration via YAML
- Support for multiple LLM providers
- Debug-level event logging
- OpenTelemetry integration
- Cost tracking and usage monitoring

## Setup

### Quick Start

Choose one of the following scenarios to get started:

#### Direct installation

1. Install using cargo:
```bash
RUSTFLAGS="--cfg tracing_unstable --cfg aws_sdk_unstable" cargo install --git https://github.com/langdb/ai-gateway.git
```

2. Run the server:
```bash
ai-gateway serve
```

#### Build from source

1. Clone the repository:
```bash
git clone https://github.com/langdb/ai-gateway.git
cd ai-gateway
```

2. Run the server with default configuration:
```bash
cargo run -- serve
```

Both scenarios will start the server on `127.0.0.1:8080` with default settings.

### Environment Variables

Create a `.env` file in the project root directory and add the API keys for the providers you plan to use:
```
# API Keys for different providers (set the ones you plan to use)
LANGDB_OPENAI_API_KEY=your-openai-key-here
# Other providers

# Optional: Set log level (default: info)
RUST_LOG=debug
```

The service will automatically load these environment variables from the `.env` file when starting up.

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
cargo run -- serve --clickhouse-url "clickhouse://localhost:9000"
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
cargo run
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

## Running with Docker Compose

For a complete setup including ClickHouse for analytics and tracing, follow these steps:

1. Start the services using Docker Compose:
```bash
docker-compose up -d
```

This will start:
- ClickHouse server on ports 8123 (HTTP)
- All necessary configurations will be loaded from `docker/clickhouse/server/config.d`

2. Build and run the gateway:
```bash
cargo run
```

The gateway will now be running with full analytics and logging capabilities, storing data in ClickHouse.

## Using MCP Tools
```bash
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
- [x] Provide example docker-compose (simple / full (clickhouse))
- [x] cost control
- [x] rate limiting
- [ ] cargo install / curl -sH install
- [ ] CI/CD for ubuntu / mac silicon
- [ ] postman 
- [ ] Include Model selection config (All / Filter)
- [ ] usage command (runs a query and prints model usage)
- [ ] README has explanations each of them.
- [ ] Docs (opensource section) / Mrunmay
- [ ] 
