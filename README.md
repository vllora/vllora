<div align="center">

<img src="assets/images/logos/logo_dark.svg" width="200px" alt="vLLora Logo">

#### Lightweight, Real-time Debugging for AI Agents

Debug your AI agents with precision. vLLora provides real-time observability and tracing for AI agent interactions, helping you understand exactly what's happening under the hood.

![vLLora Demo](https://raw.githubusercontent.com/vllora/vllora/main/assets/gifs/traces.gif)

[![GitHub stars](https://img.shields.io/github/stars/vLLora/vLLora?style=social)](https://github.com/vLLora/vLLora)

</div>

### Key Features

🔍 **Real-time Debugging**
- Live observability of AI agent calls and tool interactions
- Inspect tool calls and responses in real-time
- Debug agent decision-making processes
- Track tool usage patterns and performance
- Instant visibility into agent behavior patterns


⚡ **Lightweight Performance**
- Built in Rust for maximum speed and reliability
- Minimal resource footprint
- Zero-configuration debugging setup

## Installation

### Using Homebrew (Recommended)

First, install [Homebrew](https://brew.sh) if you haven't already, then:

```bash
brew tap vllora/vllora
brew install vllora
```

### Build from Source

```bash
git clone https://github.com/vllora/vllora.git
cd vLLora
cargo build --release
```

The binary will be available at `target/release/vlora`.

## Quick Start

Start the debugging server:

```bash
vllora serve
```

The server will start on `http://localhost:8080` and the UI will be available at `http://localhost:8084`. 

vLLora uses OpenAI-compatible chat completions API, so when your AI agents make calls through vLLora, it automatically collects traces and debugging information for every interaction.

### Test Your Setup

1. **Configure API Keys**: Visit `http://localhost:8084` to configure your AI provider API keys through the UI
2. **Make a request** to see debugging in action:

```bash
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o-mini",
    "messages": [{"role": "user", "content": "What is the capital of France?"}]
  }'
```

## Observability

vLLora provides real-time debugging and tracing for your AI agent interactions. All traces and debugging information are available through the web UI at `http://localhost:8084`.

## API Endpoints

vLLora provides debugging endpoints compatible with OpenAI's API format:

- `POST /v1/chat/completions` - Chat completions with tracing
- `GET /v1/models` - List available models
- `POST /v1/embeddings` - Generate embeddings with tracing

## Development

To get started with development:

1. Clone the repository
2. Run `cargo build` to compile
3. Run `cargo test` to run tests

## Contributing

We welcome contributions! Please check out our [Contributing Guide](CONTRIBUTING.md) for guidelines on:

- How to submit issues
- How to submit pull requests
- Code style conventions
- Development workflow
- Testing requirements

## License

This project is released under the [Apache License 2.0](./LICENSE.md). See the license file for more information.