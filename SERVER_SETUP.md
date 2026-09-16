# Connect the Rust SDK to a Conductor Server

**Audience:** developers running workflows or agents locally or against a hosted cluster. The
server itself is language-agnostic — these steps are the same regardless of which Conductor SDK
you're using.

## Hosted or remote server

Set the API URL (including `/api`), then add credentials only when the target requires them:

```shell
export CONDUCTOR_SERVER_URL=https://your-server.example/api
export CONDUCTOR_AUTH_KEY=<key-id>
export CONDUCTOR_AUTH_SECRET=<key-secret>
```

## Local development

Docker is the simplest local path:

```shell
docker run --rm -p 8080:8080 conductoross/conductor:latest
export CONDUCTOR_SERVER_URL=http://localhost:8080/api
```

Verify it's up:

```shell
curl http://localhost:8080/health
```

Stop the container with `docker stop <container>`.

For agent runs, configure the LLM provider credential on the server before starting it. Continue
with [CONNECTION_AUTHENTICATION.md](CONNECTION_AUTHENTICATION.md).

## Related Documentation

- **[examples/hello_world.rs](examples/hello_world.rs)** — minimal end-to-end example against a local server
- **[TESTING.md](TESTING.md)** — running this crate's own test suite against a live server
