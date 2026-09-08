# Agents — Design Docs (Rust SDK port)

Status: **design phase**, no implementation merged yet. These documents plan the port of
the python-sdk "Agents" feature (`conductor.ai.agents`) to the Rust SDK, per
[`Agent_Parity_checklist.txt`](../../Agent_Parity_checklist.txt). Source of truth for behavior
is the python-sdk; java-sdk (`conductor-client-ai`) is the secondary typed-language reference
where mentioned, though it wasn't available locally at research time — diff against it before
finalizing implementation if possible.

| Doc | Purpose |
|---|---|
| [`python-sdk-reference.md`](python-sdk-reference.md) | One-pager: what exists today in python-sdk — classes, fields, wire format. Ground truth for parity. |
| [`rust-sdk-design.md`](rust-sdk-design.md) | One-pager: proposed Rust types, module layout, and how they map onto existing rust-sdk conventions. |
| [`secrets-and-credentials.md`](secrets-and-credentials.md) | How credentials/secrets work today (python) and the proposed Rust design — flagged up front because this is **not** a 1:1 port. |
| [`framework-support.md`](framework-support.md) | Which external agent frameworks (OpenAI, Anthropic, LangGraph, …) we support and how, phased by ROI. |
| [`examples.md`](examples.md) | 2-3 worked examples of the proposed Rust API. Illustrative — the types shown are proposed, not yet implemented. |
| [`parity-plan.md`](parity-plan.md) | Condensed single-page summary (classes / examples / secrets / frameworks), same format as the Ruby SDK's parity plan — for cross-SDK comparison at a glance. |

## How to use these docs

Read `python-sdk-reference.md` first if you're unfamiliar with the feature. Read
`rust-sdk-design.md` for the actual proposal. The other three docs go deep on the parts of the
proposal that need the most scrutiny before implementation starts.

## Open items that need a decision before implementation

These came up repeatedly across the research and aren't yet resolved:

1. Two real bugs in python-sdk's own schema/serializer contract (`allowedTransitions` and
   `planSource` emit shapes that don't match `agent-schema.json`) — worth flagging upstream so
   the Rust port doesn't faithfully copy a bug. See `rust-sdk-design.md` § Wire compatibility.
2. Whether `Task.runtimeMetadata` (the server→worker credential-delivery contract) is already
   supported by the Conductor server versions this SDK targets. The entire secrets design
   depends on it. See `secrets-and-credentials.md` § Prerequisite.
3. Scope of framework support for v1 — recommendation and reasoning in `framework-support.md`.
