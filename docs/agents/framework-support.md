# Framework Support — design doc

Scope requested: OpenAI, Anthropic, LangGraph. Python-sdk also supports LangChain and Google
ADK; both are covered briefly below for context and explicitly deprioritized.

## First, disentangle two things python-sdk's naming blurs together

"Framework support" and "LLM provider support" are different concerns, and conflating them is
the biggest risk in scoping this wrong:

- **LLM provider support** — `AgentDef.model = "openai/gpt-4o"` / `"anthropic/claude-opus-4"`.
  This is core `AgentDef` functionality (a `provider/model` string resolved server-side against a
  Conductor "integration"), already required for the native agent path in
  [`rust-sdk-design.md`](rust-sdk-design.md), and has nothing to do with framework adapters. It
  ships with v1 regardless of any framework-adapter decision.
- **Framework adapter support** — taking an object authored against *someone else's* agent SDK
  (an `openai-agents` `Agent`, a LangGraph `CompiledStateGraph`, an Anthropic Claude Agent SDK
  session) and running it through Conductor. This is what this document is actually about.

Confirmed from python-sdk: there is no direct `anthropic` Messages-API usage anywhere in that
codebase (`import anthropic` never appears despite an unused poetry extra) — "Anthropic support"
in python-sdk's *framework* sense means the **Claude Agent SDK** (formerly Claude Code SDK), a
subprocess wrapper around the `claude` CLI, not the Anthropic API client. Don't let "Anthropic"
in a Rust scoping conversation default to "the raw API" without checking which one is meant —
raw-API Anthropic is already covered by the provider-string path above.

## What python-sdk actually does, and why most of it can't transliterate

Two structurally different modes, chosen per-object by a priority-ordered heuristic with silent
fallback:

1. **Full extraction** — walk the foreign object (Pydantic/dataclass field introspection, exact
   type-name checks, and for LangGraph specifically: `__closure__` cell contents and `co_names`
   bytecode scanning to find a model/tool variable a node function closes over), produce a real
   `agentConfig`. Conductor's own guardrail/termination/`DoWhile` loop then drives execution —
   "Conductor orchestrates, framework was only used to author."
2. **Passthrough** — can't extract (checkpointer present, non-flat model/tools, or the framework
   is fundamentally opaque like the Claude Agent SDK CLI). The foreign framework's own
   `.invoke()`/`.stream()` loop runs inside one Conductor `SIMPLE` task; Conductor observes via
   callback/hook events pushed non-blockingly to `/agent/events/{id}` and injects cosmetic
   display tasks for the UI, but **cannot** apply guardrails/termination/handoffs inside that
   black box.

Mode 1 for LangGraph specifically depends on CPython internals with **no Rust analog whatsoever**:
mutable `__globals__`, inspectable `__closure__` cells, and bytecode-level `co_names` scanning of
arbitrary compiled functions. Porting that isn't a translation exercise — it would require
whatever Rust graph-orchestration library is targeted to expose its structure through an
explicit, typed API instead, which is a different (and better) starting point anyway — see
Phase 2 below.

One thing this buys Rust for free: **there is no need for a runtime `detect_framework()`
dispatcher at all.** Python needs it because `Agent`/`Runner.run(x)` accepts an untyped `x` and
has to figure out at runtime what it received. Rust's caller already knows the type at compile
time — "detection" is just which `impl From<T> for AgentDef` or which adapter function got
called. An entire subsystem (type-name sniffing, module-prefix fallback tables, bytecode
scanning) simply isn't needed; don't build a Rust equivalent of it.

## Recommendation

### Phase 1 (ships with the native `AgentDef` port)

**1. Provider/model string support for OpenAI and Anthropic** (and whatever else the provider
registry lists) — this is core, not a framework adapter; see above. Also: unify what python-sdk
keeps as three separate, drifting lists (`_internal/provider_registry.py`'s 3 entries,
`_internal/model_parser.py`'s 11-entry `KNOWN_PROVIDERS`, and duplicated bare-model-name→provider
inference logic copy-pasted into `openai_compat.py` and `frameworks/langgraph.py`) into **one**
`ProviderRegistry` table in the Rust port. Don't port the duplication as three call sites just
because python has three.

**2. A generic `FrameworkAgent` trait + blanket `From` conversion** — the direct equivalent of
python's generic deep-serializer (the code path that actually carries Google ADK and most
OpenAI-agents objects today, with no dedicated per-framework file). This is the best
coverage-per-engineering-hour bet in the whole surface: one adapter, and any Rust struct shaped
like "name + instructions + model + tool list" converts into an `AgentDef` for free.

```rust
pub trait FrameworkAgent {
    fn name(&self) -> &str;
    fn instructions(&self) -> &str;
    fn model(&self) -> &str;
    fn tools(&self) -> Vec<ToolDef>;
}

impl<T: FrameworkAgent> From<T> for AgentDef { /* full extraction, always — no silent passthrough fallback */ }
```

Ship one concrete adapter against this trait for the `async-openai` crate's function-calling /
tool shape (`ChatCompletionTool`), since that's the most-adopted Rust OpenAI client today and is
the most direct match for what "OpenAI framework support" concretely means for a Rust user.

**Deliberate divergence from python:** don't replicate "best-effort extraction, silent fallback to
opaque passthrough." A design-doc audience (and downstream users) benefit more from predictable
behavior than magic — if a type doesn't implement `FrameworkAgent` and there's no adapter for it,
that's a compile error, not a runtime guess. This trades python's "it just works, sometimes
opaquely" for "it's obvious at compile time whether Conductor can see inside your agent."

### Phase 2 (after v1 native path + Phase 1 land)

**3. A typed graph-declaration API for the LangGraph-shaped need** — explicit nodes/edges/state,
authored directly against a Conductor type rather than reverse-engineered from an opaque compiled
graph:

```rust
pub struct GraphAgentDef { /* nodes, edges, conditional_edges, state reducers — all declared, not extracted */ }
```

This serves the same niche LangGraph's "graph-structure" mode serves in python (arbitrary DAGs
with per-node LLM calls that Conductor's guardrails/termination can actually attach to, because
the LLM call is a real node, not hidden inside a black box) without depending on bytecode
introspection or on a specific external crate reaching adoption first. If/when a Rust
graph-orchestration crate with LangGraph-like ambitions gains real adoption, revisit whether it's
worth adding an adapter *from* that crate's compiled-graph type *into* `GraphAgentDef` — but don't
block this on that crate existing.

**4. Claude Agent SDK / CLI passthrough** (subprocess + `stream-json` protocol) — feasible in
Rust, arguably cleaner than python's version: `tokio::process::Command` speaks the same stdio
JSON-streaming protocol just as well as python's subprocess wrapper does, and credential passing
is strictly better (see [`secrets-and-credentials.md`](secrets-and-credentials.md) — scoped
`Command::env()`, no global mutation). Ranked below the graph API anyway because it delivers the
least "Conductor orchestrates" value of everything here — it's pure black-box passthrough with
event-sniffing, so it doesn't showcase anything differentiated, and it drags in a hard
non-Rust runtime dependency (Node.js + the `claude` CLI binary) that's orthogonal to what makes
a Rust SDK compelling. Build it when a concrete user asks for it, not speculatively.

### Explicitly deprioritized

- **LangChain `AgentExecutor`-equivalent full extraction.** Lower engineering cost than LangGraph
  in python (find `.tools`, find the chat model, map `BaseTool` → schema → worker), and would
  reuse the same `FrameworkAgent`/extraction machinery as Phase 1 if a comparably-adopted Rust
  crate existed. **Re-checked 2026-09-17** (Wave 8): this premise needs updating — the `rig`
  ecosystem (`rig-core` + `rig-agent`, <https://github.com/0xPlaygrounds/rig>) is no longer a
  fringe crate: `rig-core` has ~2.8M all-time / ~1.56M 90-day downloads on crates.io as of this
  check, dwarfing `langchain-rust` (~155K all-time, last published 2024-10-06 — effectively
  unmaintained) and the brand-new `langgraph` crate (~1.2K downloads, published within the last
  90 days, too immature to call adopted). `rig` is the closest thing Rust has today to a
  dominant, actively-maintained LangChain-equivalent.
  Still not adapting it, but for a *different* reason than "nothing exists": `rig_agent::Agent`'s
  shape doesn't fit `FrameworkAgent`'s synchronous-extraction contract. Its fields are private
  with only `name()`/`description()`/`model_handle()` getters (`model_handle()` returns an opaque
  `&ModelHandle`, not a plain `"provider/model"` string `FrameworkAgent::model()` needs), and tool
  discovery (`tool_definitions()`) is `async` and takes a `prompt: Option<String>` — tools are
  resolved dynamically per-prompt, not held as a static list `FrameworkAgent::tools()`'s
  synchronous `Vec<ToolDef>` can represent. This is structurally the same "can't do full
  extraction, only black-box passthrough" situation `claude_agent_sdk.rs`'s module doc describes,
  not a `FrameworkAgent`-shaped gap. `rig-agent` itself is also very new (published alongside this
  `rig-core` split ~2026-08-17; nearly all its downloads are within the 90-day window) — revisit
  once/if its API stabilizes, but a passthrough adapter (à la Claude Agent SDK) is the more
  plausible shape to build if a concrete user asks, not full extraction.
- **Google ADK.** No Rust equivalent exists. Python's own ADK support is the thinnest of the five
  (no dedicated adapter file, falls through to the generic walker) — there's nothing
  framework-specific to port even if a Rust ADK-alike appears; Phase 1's generic trait already
  covers this shape.

## Guardrails/termination applicability — document this, don't leave it implicit

Python never states this in user-facing docs (only discoverable by reading dispatcher code):
guardrails/termination/handoffs only ever attach to a real Conductor `LlmChatComplete` task. In
full-extraction mode that's true after extraction; in passthrough mode it's never true — the
framework's own loop is opaque, full stop. Whatever Rust ships, **say this explicitly** in the
adapter's own doc comments and in `docs/agents/`: "content run through `FrameworkAgent`
extraction gets Conductor guardrails/termination; content run through a passthrough adapter does
not, and never will unless the loop is unwrapped into a real Conductor task." Don't leave users
to discover this the way python's users currently have to — via source or logs.

## Summary table

| Framework | What it means here | Mode | Phase | Rationale |
|---|---|---|---|---|
| OpenAI (provider) | `model = "openai/..."` | native | v1 | Core `AgentDef`, not a framework adapter |
| OpenAI (framework) | `async-openai` tool/function shape | full extraction via `FrameworkAgent` | 1 | Cheapest, highest-coverage adapter; reuses generic trait |
| Anthropic (provider) | `model = "anthropic/..."` | native | v1 | Core `AgentDef` |
| Anthropic (framework, i.e. Claude Agent SDK / CLI) | subprocess + stream-json | passthrough only | 2, low priority | Least "Conductor orchestrates" value; hard external CLI/Node dependency |
| LangGraph | typed graph declaration (no bytecode introspection possible) | new `GraphAgentDef`, not an adapter from an external crate | 2 | Serves the same niche without depending on Rust ecosystem maturity |
| LangChain | `rig`/`rig-agent` is now the dominant crate (re-checked 2026-09-17), but its shape is passthrough-only | passthrough adapter (à la Claude Agent SDK), not `FrameworkAgent` extraction | deprioritized, build on concrete ask | `rig_agent::Agent` has no static tool list / plain model string to extract — see "Explicitly deprioritized" above |
| Google ADK | — | `FrameworkAgent` trait covers the shape already | deprioritized | No Rust equivalent; python's own support is already the thinnest of the five |
