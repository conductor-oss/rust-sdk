# Connection and Authentication

`Configuration::from_env()` reads `CONDUCTOR_SERVER_URL`, defaulting to
`http://localhost:8080/api`. It reads `CONDUCTOR_AUTH_KEY` and `CONDUCTOR_AUTH_SECRET` together
when the server requires key/secret auth.

```rust
use conductor::configuration::Configuration;

let config = Configuration::from_env();
println!("{}", config.server_api_url);
```

**OSS:** a local development server may allow anonymous access (leave the auth env vars unset).
**Orkes:** use the tenant API endpoint and an application access key/secret pair.

Never put credentials in workflow inputs, agent prompts, task output, or source control — see
[SECURITY.md](SECURITY.md) and [docs/agents/secrets-and-credentials.md](docs/agents/secrets-and-credentials.md).

If requests fail, verify that the URL ends in `/api`, the server is reachable, and the
credentials belong to that endpoint. Next: [SERVER_SETUP.md](SERVER_SETUP.md).

## Related Documentation

- **[src/configuration/](src/configuration/)** — `Configuration`
- **[WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md)** — the same hierarchical override pattern applied to worker settings
