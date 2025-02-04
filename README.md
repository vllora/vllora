<div align="center">


<img src="https://raw.githubusercontent.com/langdb/ai-gateway/main/assets/images/logos/icon_red.png" width="25px" alt="LangDB Logo">

## AI Gateway
#### OpenSource Enterprise AI Gateway built in Rust

[![GitHub stars](https://img.shields.io/github/stars/langdb/ai-gateway?style=social)](https://github.com/langdb/ai-gateway)
[![Slack](https://img.shields.io/badge/Join-Slack-brightgreen?logo=slack)](https://join.slack.com/t/langdbcommunity/shared_invite/zt-2haf5kj6a-d7NX6TFJUPX45w~Ag4dzlg)
[![Documentation](https://img.shields.io/badge/docs-langdb.ai-blue)](https://docs.langdb.ai)

</div>

Govern, Secure, and Optimize your AI Traffic. LangDB AI Gateway provides unified interface to all LLMs using OpenAI API format. Built with performance and reliability in mind.

### Key Features

🚀 **High Performance**
- Built in Rust for maximum speed and reliability
- Seamless integration with any framework (Langchain, Vercel AI SDK, CrewAI, etc.)
- Integrate with any MCP servers(https://docs.langdb.ai/ai-gateway/features/mcp-support)

📊 **Enterprise Ready**
- [Comprehensive usage analytics and cost tracking](https://docs.langdb.ai/ai-gateway/features/analytics)
- [Rate limiting and cost control](https://docs.langdb.ai/ai-gateway/features/usage)
- [Advanced routing, load balancing and failover](https://docs.langdb.ai/ai-gateway/features/routing)
- [Evaluations](https://docs.langdb.ai/ai-gateway/features/evaluation)

🔒 **Data Control**
- Full ownership of your LLM usage data
- Detailed logging and tracing

### Looking for More? Try Our Hosted & Enterprise Solutions

🌟 **[Hosted Version](https://langdb.ai)** - Get started in minutes with our fully managed solution
- Zero infrastructure management
- Automatic updates and maintenance
- Pay-as-you-go pricing

💼 **[Enterprise Version](https://langdb.ai/)** - Enhanced features for large-scale deployments
- Advanced team management and access controls
- Custom security guardrails and compliance features
- Intuitive monitoring dashboard
- Priority support and SLA guarantees
- Custom deployment options

[Contact our team](https://calendly.com/d/cqs2-cfz-gdn/meet-langdb-team) to learn more about enterprise solutions.

## Getting Started

### 1. Installation

Choose one of these installation methods:

#### Using Docker (Recommended)
```bash
docker run -it \
    -p 8080:8080 \
    -e LANGDB_OPENAI_API_KEY=your-openai-key-here \
    langdb/ai-gateway serve
```

#### Using Cargo
```bash
export RUSTFLAGS="--cfg tracing_unstable --cfg aws_sdk_unstable" 

cargo install ai-gateway

ai-gateway serve
```

### 2. Make Your First Request

Test the gateway with a simple chat completion:

```bash
# Chat completion with GPT-4
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o-mini",
    "messages": [{"role": "user", "content": "What is the capital of France?"}]
  }'

# Or try Claude
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-opus",
    "messages": [
      {"role": "user", "content": "What is the capital of France?"}
    ]
  }'
```

## Providers

LangDB AI Gateway currently supports the following LLM providers. Find all [the available models here](https://app.langdb.ai/models).

|                                                          | Provider                        |
| -------------------------------------------------------- | ------------------------------- |
| <img src="https://raw.githubusercontent.com/langdb/ai-gateway/main/assets/images/openai.png" width="32">          | OpenAI                          |
| <img src="https://raw.githubusercontent.com/langdb/ai-gateway/main/assets/images/gemini.png" width="32">          | Google Gemini                   |
| <img src="https://raw.githubusercontent.com/langdb/ai-gateway/main/assets/images/Anthropic-AI.png" width="32">    | Anthropic                       |
| <img src="https://raw.githubusercontent.com/langdb/ai-gateway/main/assets/images/deepseek.png" width="32">        | DeepSeek                        |
| <img src="https://raw.githubusercontent.com/langdb/ai-gateway/main/assets/images/cohere.875858bb.svg" width="32"> | TogetherAI                      |
| <img src="https://raw.githubusercontent.com/langdb/ai-gateway/main/assets/images/xai.png" width="32">             | XAI                             |
| <img src="https://raw.githubusercontent.com/langdb/ai-gateway/main/assets/images/meta.png" width="32">            | Meta ( Provided by Bedrock )    |
| <img src="https://raw.githubusercontent.com/langdb/ai-gateway/main/assets/images/cohere.png" width="32">          | Cohere ( Provided by Bedrock )  |
| <img src="https://raw.githubusercontent.com/langdb/ai-gateway/main/assets/images/mistral.png" width="32">         | Mistral ( Provided by Bedrock ) |

The gateway supports standard OpenAI-compatible endpoints:
- `/v1/chat/completions` - For chat completions
- `/v1/completions` - For text completions
- `/v1/embeddings` - For generating embeddings

### Advanced Configuration
Create a `config.yaml` file:
```yaml
providers:
  openai: 
    api_key: "your-openai-key-here"
  anthropic: 
    api_key: "your-anthropic-key-here"
  
  # Supports mustache style variables  
  gemini:
    api_key: {{LANGDB_GEMINI_API_KEY}}

http:
  host: "0.0.0.0"
  port: 8080
```

#### Command Line Options

```bash
# Run with custom host and port
ai-gateway serve --host 0.0.0.0 --port 3000

# Run with CORS origins
ai-gateway serve --cors-origins "http://localhost:3000,http://example.com"

# Run with rate limiting
ai-gateway serve --rate-hourly 1000

# Run with cost limits
ai-gateway serve --cost-daily 100.0 --cost-monthly 1000.0

# Run with custom database connections
ai-gateway serve --clickhouse-url "clickhouse://localhost:9000"
```

#### Using Config File
Download the sample configuration from our repo.
1. Copy the example config file:
```bash
curl -sL https://raw.githubusercontent.com/langdb/ai-gateway/main/config.sample.yaml -o config.sample.yaml

cp config.sample.yaml config.yaml
```

Command line options will override corresponding config file settings when both are specified.

### Rate Limiting

Rate limiting helps prevent API abuse by limiting the number of requests within a time window. Configure rate limits using:

```bash
# Limit to 1000 requests per hour
ai-gateway serve --rate-hourly 1000
```

Or in `config.yaml`:
```yaml
rate_limit:
  hourly: 1000
```

When a rate limit is exceeded, the API will return a 429 (Too Many Requests) response.

## API Endpoints

The gateway provides the following OpenAI-compatible endpoints:

- `POST /v1/chat/completions` - Chat completions
- `GET /v1/models` - List available models
- `POST /v1/embeddings` - Generate embeddings
- `POST /v1/images/generations` - Generate images


## Clickhouse Integration
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
ai-gateway serve --clickhouse-url "clickhouse://localhost:9000"
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
### Cost Control

Cost control helps manage API spending by setting daily, monthly, or total cost limits. Configure cost limits using:

```bash
# Set daily and monthly limits
ai-gateway serve \
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
ai-gateway run
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

To get started with development:

1. Clone the repository
2. Copy `config.sample.yaml` to `config.yaml` and configure as needed
3. Run `cargo build` to compile
4. Run `cargo test` to run tests

## Contributing

We welcome contributions! Please check out our [Contributing Guide](CONTRIBUTING.md) for guidelines on:

- How to submit issues
- How to submit pull requests
- Code style conventions
- Development workflow
- Testing requirements
### Logging

The gateway uses `tracing` for logging. Set the `RUST_LOG` environment variable to control log levels:

```bash
RUST_LOG=debug cargo run serve    # For detailed logs
RUST_LOG=info cargo run serve   # For standard logs
```
## License

This project is released under the [Apache License 2.0](./LICENSE.md). See the license file for more information.