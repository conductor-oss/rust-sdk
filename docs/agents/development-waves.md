Rust SDK → Python SDK Parity Checklist
Grouped into waves. Within a wave, every item is independent (own file/type, no shared-file edits) and can run in parallel. Later waves depend on earlier ones being merged (noted per item).

Wave 1 — Agents: new composition types (no dependencies on each other; each is a new file under src/agents/)

- [x] Guardrail types — RegexGuardrail, LlmGuardrail, GuardrailCheck trait, Position/OnFail/max_retries config. New src/agents/guardrail.rs.
- [x] TerminationCondition enum — TextMention, StopMessage, MaxMessage, TokenUsage, And, Or (recursive). New src/agents/termination.rs.
- [x] SwarmTransition enum — OnToolResult, OnTextMention, OnCondition. New src/agents/swarm.rs. (Named SwarmTransition, not python's HandoffCondition — see parity-plan.md rationale, don't rename to match python.)
- [x] CallbackHandler trait — on_agent_start(ctx), on_tool_start(ctx), etc., returning Option<Value>. New src/agents/callback.rs.
- [x] ConversationMemory struct — messages: Vec<Message>, max_messages: Option<u32>. New src/agents/memory.rs.
- [x] Credentials type — creds.get(name) reads Task.runtime_metadata, fails closed with ConductorError::CredentialNotFound on a miss. New src/agents/credentials.rs. No dependency on the tool macro change (Wave 2) — the type can exist and be unit-tested against a hand-built Task before anything calls it.

Wave 2 — Agents: wire up Wave-1 types into AgentDef (touches shared files def.rs/serializer.rs, so sequence these serially with each other, but each can be scoped as one focused PR)

- [x] Add guardrails: Vec<Guardrail> field + with_guardrail() builder to AgentDef; serialize in AgentConfigSerializer.
- [x] Add termination: Option<TerminationCondition> field + with_termination(); serialize.
- [x] Add router field + lift the with_strategy rejection for Strategy::Router in def.rs:219-230; serialize.
- [x] Add swarm_transitions: Vec<SwarmTransition> + with_swarm_transition(); lift the Strategy::Swarm rejection; serialize.
- [x] Add planner/fallback/fallback_max_turns/planner_context/synthesize fields; lift the Strategy::PlanExecute rejection; serialize.
- [x] Add callbacks: Vec<Box<dyn CallbackHandler>> registration (not serialized — caller-side only, per parity-plan.md's open-circle relationship).
- [x] Add memory: Option<ConversationMemory> field; serialize.
- [x] Add output_type structured-output field; serialize.

Wave 3 — Agents: credentials delivery (each independent; depends only on Wave 1's Credentials type)

- [x] Stamp declared credentials names onto TaskDef.runtime_metadata at registration time (in the existing metadata-registration path task_handler.rs:339).
- [x] #[tool] macro (conductor-macros/src/lib.rs:412): allow a second parameter typed &Credentials, generating the pass-through instead of erroring on fn_inputs.len() != 1.
- [x] with_tool_credentials(tool_name, names) / per-agent with_credentials — verify existing inert field wiring extends cleanly to the new resolution path (currently a no-op per def.rs:262-268).

Wave 4 — Agents: runtime (sequenced after Wave 2/3 land; independent from each other once AgentDef is stable)

- [x] AgentRuntime::new + compile() (→ AgentConfigSerializer::serialize + AgentClient::compile_agent).
- [x] AgentRuntime::deploy() / start_agent() wiring to existing AgentClient methods (src/client/agent_client.rs — already has the transport, just needs a caller).
- [x] AgentRuntime::run() — blocking helper: start + poll get_status/get_execution to completion, return AgentResult.
- [x] AgentRuntime::serve() — composes the existing TaskHandler (reuse, per parity-plan.md's diagram — no new polling loop) for local tool workers.
- [x] AgentHandle — join(), stream(), approve()/reject()/respond() targeting execution_id. (all delivered in src/agents/handle.rs; stream() opens the SSE response via AgentClient::stream() and wraps it in AgentStream::new.)
- [x] AgentEvent enum + AgentStream (SSE parsing over the existing stream endpoint on AgentClient).
- [x] AgentStatus / AgentResult types.

Wave 5 — Agents: framework adapters (fully independent of each other and of Wave 4 internals, only need AgentDef stable)

- [x] FrameworkAgent trait (generic adapter interface) + fallible `TryFrom<T> for AgentDef` (since AgentDef::new returns Result).
- [x] OpenAI Agents SDK adapter for the async-openai crate's tool shape (Phase 1).
- [x] Claude Agent SDK passthrough adapter — subprocess + stream-json over tokio::process (Phase 2, lowest priority per parity-plan.md). (Reduced scope: delivers the bare subprocess / stream-json transport only — the tracking-workflow, server-side event-push, and hook-bridging behavior matching python-sdk's full `claude_agent_sdk.py` adapter is an explicit follow-up, not done here.)
- [x] LangGraph typed GraphAgentDef adapter (Phase 2). (Rust-side authoring type + plain JSON serialization; server-side graph-execution wire compatibility not yet verified — open follow-up).

Wave 6 — Lease extension / automatic heartbeat (independent of all Agents work; touches src/worker/)

- [x] Add lease_extend_enabled: bool (+ threshold, default 80%) to WorkerConfig. (`lease_extend_threshold: f64`, default `0.8`, both env-resolvable per the existing hierarchical config pattern.)
- [x] Heartbeat scheduler: spawns a tokio task alongside the worker future (`TaskRunner::maybe_spawn_lease_heartbeat`/`send_lease_heartbeats` in task_runner.rs) that fires at `lease_extend_threshold` of responseTimeoutSeconds. Deliberately a per-task spawned tokio task rather than python's shared-thread `LeaseManager` singleton — see LEASE_EXTENSION.md for why that's the right call here.
- [x] On fire, send a TaskResult { extend_lease: true, status: InProgress, .. } via task_client.update_task, with its own short/fast retry (3 attempts) rather than update_task_with_retry's 10/20/30s schedule, which is sized for terminal completion updates, not a keep-alive.
- [x] Cancel the heartbeat timer when the task completes/fails (JoinHandle::abort() right after `worker.execute()` returns, before the result is converted/sent).

Wave 7 — Docs (each is a standalone file, fully parallel, zero code dependency)

- [x] SCHEMA_CLIENT.md — document the already-implemented src/client/schema_client.rs (pure doc gap, no code needed).
- [x] LEASE_EXTENSION.md — pairs with Wave 6.
- [x] docs/agents/* status headers updated to reflect Waves 1-5 being implemented (README.md, examples.md, rust-sdk-design.md) — not a line-by-line api-reference rewrite, just correcting the "nothing implemented yet" framing that was no longer true.
- [x] WORKFLOW_TESTING.md, OBSERVABILITY.md, SECURITY.md, DEBUGGING.md, UPGRADING.md, API_MAP.md, CONNECTION_AUTHENTICATION.md, DEPLOYMENT_SCALING.md, RELIABILITY.md, SCHEDULES_EVENTS.md, SERVER_SETUP.md, WORKFLOW_LIFECYCLE.md, WORKFLOW_MESSAGE_QUEUE.md, CORE_QUICKSTART.md — all checked against actual rust-sdk capability before writing. Two turned out to be real code gaps, not doc gaps, and were implemented rather than just flagged:
  - **Workflow Message Queue**: `WorkflowClient::send_message` didn't exist at all, and `WorkflowTask` had no way to construct a `PULL_WORKFLOW_MESSAGES` task (`TaskType` is a closed enum with no generic-string escape hatch). Added `WorkflowClient::send_message`, `TaskType::PullWorkflowMessages`, and `WorkflowTask::pull_workflow_messages`/`.non_blocking()`.
  - **`TestWorkflowRequest` wire format**: didn't match the server's actual `WorkflowTestRequest`/`TaskMock` model at all -- `task_ref_to_mock_output` was `Map<String, Map<String, Value>>` instead of `Map<String, List<TaskMock>>` (no retry-sequence or non-`COMPLETED`-status mocking was possible), `workflow_input` serialized as `"workflowInput"` instead of `"input"`, and several base fields (`correlation_id`, `task_to_domain`, `priority`, `external_input_payload_storage_path`, `sub_workflow_test_request`) were missing entirely. Fixed to match the server's Java model exactly (confirmed against `WorkflowTestRequest.java` directly, not just python-sdk's docs, which also undercounted the valid `TaskMock` status set). `with_mock_output`'s existing call sites (including the example) needed no changes -- it now appends a `TaskMock::completed(..)` to a sequence instead of overwriting a flat map, so its old "one call per task ref" usage is unaffected.

Sizing note: Waves 1–3 (≈16 tasks) are genuinely embarrassingly parallel — new files, no shared-file contention. Wave 4 (runtime) is the riskiest to parallelize cleanly since several items touch the same new runtime.rs; I'd assign it to one person/agent rather than splitting. Wave 5 and 6 are fully independent of everything else and could start on day one in parallel with Wave 1.