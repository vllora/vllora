# LangDB MCP Server Deployment Request Guidelines

These guidelines will help you request deployment of an MCP server on LangDB. Deployment through LangDB ensures your server is monitored, maintained, and available to the community.

## Prerequisites

- Repository Prepared: Your MCP server code must be hosted in a public GitHub repository.
- Documentation: Include a README with installation steps, configuration, and usage examples.
- Docker Support: Provide a Dockerfile or similar container definition for consistent deployment.

## Find the Listing Page

Hidden pages exist for all MCP servers to help with SEO. Locate your server’s listing page at:

https://langdb.ai/mcp/{your-server-name}

## Request Deployment


On the listing page, click *Request Deployment*. You will be directed to a form that creates a GitHub issue in the LangDB deployment repository.

### Required Information

- Server Name: Friendly identifier (e.g., airtable-mcp, agentset-mcp).
- Repository URL: Full GitHub URL to your server’s code.
- Docker Registry (optional): If you’ve published a container image (e.g., docker.io/username/mcp-server:latest).
- Deployment Command: The command to deploy or run your MCP server (e.g., `npx -y @21st-dev/magic@latest API_KEY="your-api-key"` or `uvx ...`). Make sure to specify the exact command needed to start your server.

## Post Submission

- **Review**: The LangDB team will review your issue and may ask for clarifications.

- **Deployment**: Once approved, automatic CI/CD will build and deploy your server.

- **Monitoring**: We’ll set up basic logging and health monitoring.

Maintenance: You will receive notifications for failures or updates.

### Troubleshooting & Support

Check the GitHub issue for CI logs if deployment fails.

Once approved, automatic CI/CD will build and deploy your server.

### Monitoring

We’ll set up basic logging and health monitoring.

Maintenance: You will receive notifications for failures or updates.

### Troubleshooting & Support

Check the GitHub issue for CI logs if deployment fails.

Ensure your Docker image builds locally without errors.

Confirm health endpoint returns HTTP 200.

