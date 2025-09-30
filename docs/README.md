# LangDB Cloud API Docs

- [Projects](./projects.md)
- [Models](./models.md)
- [Threads](./threads.md)
- [Messages](./messages.md)
- [Traces](./traces.md)
- [Runs](./runs.md)

This is a placeholder for the main documentation entrypoint.

# Request handling

Requests are handled with actix
- Project middleware, reads X-Project-Id header. If header is not present, default project is read. Project is pushed to extensions