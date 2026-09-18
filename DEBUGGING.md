# Debugging Incidents

Start with safe evidence: workflow ID, task reference name, status, retry count, and
`reason_for_incompletion`. Confirm server reachability and authentication before changing
application code.

| Symptom | First check |
|---|---|
| Connection error | `CONDUCTOR_SERVER_URL` includes `/api` and the server is healthy (`curl $CONDUCTOR_SERVER_URL/../health`). |
| Task remains `SCHEDULED` | A worker is polling the exact task type (and `domain`, if set). |
| Authentication failure | `CONDUCTOR_AUTH_KEY`/`CONDUCTOR_AUTH_SECRET` target the active server. |
| Task retried repeatedly then fails | Check `reason_for_incompletion` on the failed task before assuming it's transient. |
| Agent can't call a model | The LLM provider credential is configured on the server, not the client. |
| `test_workflow` behaves unexpectedly | Confirm `task_ref_to_mock_output` uses the right task **reference** name, not the task definition name. |

Enable debug logging (see [OBSERVABILITY.md](OBSERVABILITY.md)) and re-run before assuming a bug
in the SDK rather than in configuration:

```shell
RUST_LOG=conductor=debug cargo run --example your_example
```

## Related Documentation

- **[src/client/workflow_client.rs](src/client/workflow_client.rs)** — `get_workflow` for full task-level inspection
- **[TESTING.md](TESTING.md)** — "Debugging Failed Tests" section
- **[RELIABILITY.md](RELIABILITY.md)**
