// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Declarative agent definitions and tool declarations ("Agents" feature, `agents` Cargo
//! feature). Same `agentConfig` wire format as python-sdk's `conductor.ai.agents` for the
//! subset of fields this crate currently models — see `docs/agents/` in the repo root for the
//! full design and what's deferred to follow-up PRs.
//!
//! This module has no dependency on a running `AgentRuntime` (that type doesn't exist yet in
//! this crate) — everything here is pure data + serialization, usable standalone to build and
//! inspect `agentConfig` JSON.

mod def;
mod serializer;
mod tool;

pub use def::{AgentDef, RunSettings, Strategy};
pub use serializer::AgentConfigSerializer;
pub use tool::{ToolDef, ToolHandler, ToolType};
