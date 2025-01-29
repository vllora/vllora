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

1. Clone the repository:
```bash
git clone https://github.com/langdb/ai-gateway.git
cd ai-gateway
```

1. Copy file `config.yaml`:
```yaml
rest:
  host: "127.0.0.1"
  port: 8080
  cors_allowed_origins:
    - "http://localhost:3000"
    - "http://127.0.0.1:3000"
```

3. Set up environment variables:
```bash
# OpenAI API Key
export LANGDB_OPENAI_API_KEY=your-api-key-here

# Optional: Set log level (default: info)
export RUST_LOG=debug
```

4. Build and run:
```bash
cargo run
```

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

## Basic Run

For a simple setup without analytics and tracing, follow these steps:

1. Copy `config.sample.basic.yaml` to `config.yaml`:
```bash
cp config.sample.basic.yaml config.yaml
```

2. Set up environment variables:
```bash
# OpenAI API Key
export LANGDB_OPENAI_API_KEY=your-api-key-here

# Optional: Set log level (default: info)
export RUST_LOG=debug
```

3. Build and run the gateway:
```bash
cargo run
```

The gateway will now be running with basic functionality on http://localhost:8080.

## Running with Docker Compose

For a complete setup including ClickHouse for analytics and tracing, follow these steps:

1. Copy `config.sample.full.yaml` to `config.yaml`:
```bash
cp config.sample.full.yaml config.yaml
```

2. Start the services using Docker Compose:
```bash
docker-compose up -d
```

This will start:
- ClickHouse server on ports 8123 (HTTP) and 9000 (native protocol)
- All necessary configurations will be loaded from `docker/clickhouse/server/config.d`

3. Set up environment variables:
```bash
# OpenAI API Key
export LANGDB_OPENAI_API_KEY=your-api-key-here

# Optional: Set log level (default: info)
export RUST_LOG=debug
```

4. Build and run the gateway:
```bash
cargo run
```

The gateway will now be running with full analytics and logging capabilities, storing data in ClickHouse.

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
- [ ] cost control
- [ ] rate limiting (redis configuration)
- [ ] cargo install / curl -sH install
- [ ] CI/CD for ubuntu / mac silicon
- [ ] postman 
- [ ] Include Model selection config (All / Filter)
- [ ] usage command (runs a query and prints model usage)
- [ ] README has explanations each of them.
- [ ] Docs (opensource section) / Mrunmay
- [ ] 
