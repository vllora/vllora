# Contributing

Thank you for your interest in contributing to vLLora! We welcome contributions from the community and are excited to have you on board.

## Table of Contents

- [Contributing](#contributing)
  - [Table of Contents](#table-of-contents)
  - [Code of Conduct](#code-of-conduct)
  - [Getting Started](#getting-started)
  - [Development Setup](#development-setup)
    - [Prerequisites](#prerequisites)
    - [Local Development](#local-development)
  - [Making Changes](#making-changes)
  - [Submitting Pull Requests](#submitting-pull-requests)
  - [Reporting Issues](#reporting-issues)
  - [License](#license)

## Code of Conduct

This project and everyone participating in it is governed by our Code of Conduct. By participating, you are expected to uphold this code. Please report unacceptable behavior to the project maintainers.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/vllora.git
   cd vllora
   ```
3. Add the upstream repository as a remote:
   ```bash
   git remote add upstream https://github.com/vllora/vllora.git
   ```

## Development Setup

### Prerequisites

- Rust toolchain (latest stable version)
- Docker (optional, for containerized development)
- API keys for LLM providers you plan to use

### Local Development

1. Create a `.env` file with necessary API keys:
   ```env
   OPENAI_API_KEY=your-openai-key-here
   RUST_LOG=debug
   ```

2. Build the project:
   ```bash
   cargo build --release
   ```

3. Run tests:
   ```bash
   cargo test
   ```

4. Start the development server:
   ```bash
   cargo run serve
   ```

## Making Changes

1. Create a new branch for your changes:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. Make your changes following these guidelines:
   - Follow the existing code style and formatting
   - Add tests for new functionality
   - Update documentation as needed
   - Keep commits focused and atomic
   - Write clear commit messages

3. Run the test suite to ensure nothing is broken:
   ```bash
   cargo test
   cargo clippy
   ```

## Submitting Pull Requests

1. Push your changes to your fork:
   ```bash
   git push origin feature/your-feature-name
   ```

2. Open a Pull Request with the following information:
   - Clear title and description
   - Reference any related issues
   - List notable changes
   - Include any necessary documentation updates

3. Respond to any code review feedback

## Reporting Issues

When reporting issues, please include:

- A clear description of the problem
- Steps to reproduce the issue
- Expected vs actual behavior
- Version information:
  - Rust version
  - vLLora version
  - Operating system
  - Any relevant configuration

## License

By contributing to vLLora, you agree that your contributions will be licensed under its project license.

---

Thank you for contributing to vLLora! Your efforts help make this debugging tool better for everyone.