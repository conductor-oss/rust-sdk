# Security and Secrets

Keep API keys, provider credentials, and signing secrets in the Conductor server or its
configured secret provider — manage them via `SecretClient`, not application config files or
source control.

```rust
let secrets = client.secret_client();
secrets.put_secret("GITHUB_TOKEN", "...").await?;
```

Do not put credentials in workflow inputs, agent prompts, task output, example source, or
version control.

## Agent tool credentials

Agent tools declare required credentials by *name*; the server delivers resolved values only in
`Task::runtime_metadata` for the specific poll. Missing a declared credential fails the task
before the tool body runs (`ConductorError::CredentialNotFound`) — there is no fallback to
process environment variables. See
[docs/agents/README.md](docs/agents/README.md) for the full
design (declare/register/resolve/deliver/consume contract) and
[docs/agents/README.md](docs/agents/README.md) for how it's wired into `AgentDef`/`ToolDef`.

## Related Documentation

- **[src/client/secret_client.rs](src/client/secret_client.rs)**
- **[src/agents/credentials.rs](src/agents/credentials.rs)**
- **[docs/agents/README.md](docs/agents/README.md)**
