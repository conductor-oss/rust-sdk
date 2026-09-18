// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::sync::Arc;

/// Rule-based agent-to-agent transition for `Strategy::Swarm`.
///
/// Ports python-sdk's `HandoffCondition` hierarchy (`conductor.ai.agents.handoff`:
/// `HandoffCondition` base + `OnToolResult` / `OnTextMention` / `OnCondition` subclasses) as a
/// single Rust enum, one variant per python subclass.
///
/// Named `SwarmTransition` rather than `HandoffCondition` — **deliberately, do not "fix" this
/// back to match python**. python-sdk has two unrelated mechanisms that happen to share the word
/// "handoff": `Strategy::HANDOFF` (see [`super::Strategy`]), where the LLM freely picks the next
/// agent, and `HandoffCondition` (this type), which only ever applies under `Strategy::Swarm` and
/// is rule-driven, not model-driven. That's a real naming confusion in python's own codebase
/// (readers reasonably expect `HandoffCondition` to configure `Strategy::HANDOFF`; it doesn't —
/// it's swarm-only), and this port disambiguates it by giving the swarm-only, rule-driven type its
/// own name instead of reproducing the collision. See `docs/agents/README.md`'s Strategy
/// section for the worked example and the full rationale.
///
/// Each variant carries a `target` — the name of the agent to hand off to — plus whatever that
/// variant matches against:
///
/// - [`SwarmTransition::OnToolResult`]: fires when a specific tool was just called, optionally
///   narrowed to only fire if the tool's result contains a substring.
/// - [`SwarmTransition::OnTextMention`]: fires when the agent's latest text output mentions a
///   substring (case-insensitive), e.g. the triage agent in the parity-plan example saying
///   "ACTIONABLE".
/// - [`SwarmTransition::OnCondition`]: fires when an arbitrary predicate over the run context
///   returns `true` — the escape hatch for anything the first two can't express.
///
/// Construct variants directly as struct literals (as in the parity-plan example,
/// `SwarmTransition::OnTextMention { text: "ACTIONABLE".into(), target: "filer".into() }`) —
/// unlike [`super::AgentDef`]/[`super::ToolDef`], there's no invalid state a `with_x` builder
/// needs to guard against here (every field is a plain, unconstrained value), so a consuming
/// builder would only add ceremony over what a field-literal already does directly.
#[derive(Clone)]
pub enum SwarmTransition {
    /// Hand off after a specific tool is called, regardless of its return value unless
    /// `result_contains` narrows it.
    ///
    /// Matches python's `OnToolResult(tool_name, target, result_contains=None)`.
    OnToolResult {
        /// Name of the agent to hand off to.
        target: String,
        /// The tool whose invocation triggers the handoff.
        tool_name: String,
        /// If set, only trigger when the tool's result contains this substring
        /// (case-sensitive substring match, matching python's plain `in` check).
        result_contains: Option<String>,
    },
    /// Hand off when the agent's latest text output mentions `text` (case-insensitive).
    ///
    /// Matches python's `OnTextMention(text, target)`.
    OnTextMention {
        /// Name of the agent to hand off to.
        target: String,
        /// The text to look for (case-insensitive substring match).
        text: String,
    },
    /// Hand off when an arbitrary predicate over the run context returns `true`.
    ///
    /// Matches python's `OnCondition(condition, target)`, where `condition` is a
    /// `Callable[[Dict[str, Any]], bool]`. Unlike python, which swallows any exception raised by
    /// the callable and treats it as `false` (`except Exception: return False` in
    /// `HandoffCondition.should_handoff`), a panicking closure here panics normally — this port
    /// does not add a `catch_unwind` to imitate python's blanket exception-swallowing, since
    /// silently converting arbitrary panics into `false` would hide bugs rather than port a
    /// real semantic.
    OnCondition {
        /// Name of the agent to hand off to.
        target: String,
        /// Predicate evaluated against the current [`SwarmContext`].
        condition: SwarmConditionFn,
    },
}

/// Boxed synchronous predicate backing [`SwarmTransition::OnCondition`].
///
/// Kept synchronous (unlike [`super::ToolHandler`], which is async) because python's
/// `HandoffCondition.should_handoff` is a plain synchronous function over an in-memory context
/// dict — no I/O is implied by the shape being ported, so there's nothing here that needs an
/// `async fn` / `Future` the way a tool call (which may hit the network) does.
pub type SwarmConditionFn = Arc<dyn Fn(&SwarmContext) -> bool + Send + Sync>;

impl std::fmt::Debug for SwarmTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwarmTransition::OnToolResult {
                target,
                tool_name,
                result_contains,
            } => f
                .debug_struct("OnToolResult")
                .field("target", target)
                .field("tool_name", tool_name)
                .field("result_contains", result_contains)
                .finish(),
            SwarmTransition::OnTextMention { target, text } => f
                .debug_struct("OnTextMention")
                .field("target", target)
                .field("text", text)
                .finish(),
            SwarmTransition::OnCondition { target, condition } => {
                let _ = condition;
                f.debug_struct("OnCondition")
                    .field("target", target)
                    .field("condition", &"Fn(..)")
                    .finish()
            }
        }
    }
}

/// Evaluation context for [`SwarmTransition::should_transition`].
///
/// Mirrors the subset of python's `context: Dict[str, Any]` (see
/// `HandoffCondition.should_handoff`'s docstring: `result`, `tool_name`, `tool_result`,
/// `messages`) that the three variants above actually read. `messages` (python's full
/// conversation history) is intentionally left out here: this crate has no `AgentRuntime` or
/// message model yet to type it against, so carrying a stable type for it is deferred to
/// whoever wires transition evaluation into the runtime — an arbitrary [`SwarmTransition::OnCondition`]
/// closure that needs more than `result`/`tool_name`/`tool_result` isn't expressible yet, same
/// as every other runtime-dependent piece this file intentionally excludes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SwarmContext {
    /// Latest LLM output text, if any.
    pub result: Option<String>,
    /// Name of the last tool called, if any.
    pub tool_name: Option<String>,
    /// Result of the last tool call, if any.
    pub tool_result: Option<String>,
}

impl SwarmTransition {
    /// Wire-format string for the `type` field, matching python-sdk's
    /// `AgentConfigSerializer._serialize_handoff` output exactly (`on_tool_result` /
    /// `on_text_mention` / `on_condition`). Following [`super::Strategy::as_str`]'s pattern:
    /// the wire string lives on the type itself rather than being re-derived at serialization
    /// call sites.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            SwarmTransition::OnToolResult { .. } => "on_tool_result",
            SwarmTransition::OnTextMention { .. } => "on_text_mention",
            SwarmTransition::OnCondition { .. } => "on_condition",
        }
    }

    /// The agent this transition hands off to, common to every variant (python's
    /// `HandoffCondition.target` base-class attribute).
    #[must_use]
    pub fn target(&self) -> &str {
        match self {
            SwarmTransition::OnToolResult { target, .. } => target,
            SwarmTransition::OnTextMention { target, .. } => target,
            SwarmTransition::OnCondition { target, .. } => target,
        }
    }

    /// Evaluate whether this transition should fire, matching python's
    /// `HandoffCondition.should_handoff` semantics per variant:
    ///
    /// - `OnToolResult`: `false` unless `ctx.tool_name` equals `tool_name`; if `result_contains`
    ///   is set, also requires `ctx.tool_result` to contain it (case-sensitive substring).
    /// - `OnTextMention`: `true` iff `ctx.result` contains `text`, case-insensitively.
    /// - `OnCondition`: the closure's return value, called directly against `ctx`.
    #[must_use]
    pub fn should_transition(&self, ctx: &SwarmContext) -> bool {
        match self {
            SwarmTransition::OnToolResult {
                tool_name,
                result_contains,
                ..
            } => {
                if ctx.tool_name.as_deref() != Some(tool_name.as_str()) {
                    return false;
                }
                match result_contains {
                    Some(needle) => ctx.tool_result.as_deref().unwrap_or("").contains(needle),
                    None => true,
                }
            }
            SwarmTransition::OnTextMention { text, .. } => ctx
                .result
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&text.to_lowercase()),
            SwarmTransition::OnCondition { condition, .. } => condition(ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_on_tool_result_matches_tool_name() {
        let transition = SwarmTransition::OnToolResult {
            target: "refund".into(),
            tool_name: "check_order".into(),
            result_contains: None,
        };
        let matching_ctx = SwarmContext {
            tool_name: Some("check_order".into()),
            ..Default::default()
        };
        let other_ctx = SwarmContext {
            tool_name: Some("other_tool".into()),
            ..Default::default()
        };
        assert!(transition.should_transition(&matching_ctx));
        assert!(!transition.should_transition(&other_ctx));
        assert!(!transition.should_transition(&SwarmContext::default()));
    }

    #[test]
    fn test_on_tool_result_result_contains_narrows_match() {
        let transition = SwarmTransition::OnToolResult {
            target: "supervisor".into(),
            tool_name: "escalate".into(),
            result_contains: Some("urgent".into()),
        };
        let ctx_with_needle = SwarmContext {
            tool_name: Some("escalate".into()),
            tool_result: Some("this is urgent".into()),
            ..Default::default()
        };
        let ctx_without_needle = SwarmContext {
            tool_name: Some("escalate".into()),
            tool_result: Some("all good".into()),
            ..Default::default()
        };
        assert!(transition.should_transition(&ctx_with_needle));
        assert!(!transition.should_transition(&ctx_without_needle));
    }

    #[test]
    fn test_on_text_mention_case_insensitive() {
        let transition = SwarmTransition::OnTextMention {
            target: "filer".into(),
            text: "ACTIONABLE".into(),
        };
        let ctx = SwarmContext {
            result: Some("this looks actionable to me".into()),
            ..Default::default()
        };
        assert!(transition.should_transition(&ctx));
        assert!(!transition.should_transition(&SwarmContext::default()));
    }

    #[test]
    fn test_on_condition_calls_closure() {
        let transition = SwarmTransition::OnCondition {
            target: "summarizer".into(),
            condition: Arc::new(|ctx: &SwarmContext| ctx.tool_result.as_deref() == Some("done")),
        };
        let done_ctx = SwarmContext {
            tool_result: Some("done".into()),
            ..Default::default()
        };
        assert!(transition.should_transition(&done_ctx));
        assert!(!transition.should_transition(&SwarmContext::default()));
    }

    #[test]
    fn test_target_accessor() {
        let a = SwarmTransition::OnToolResult {
            target: "a".into(),
            tool_name: "t".into(),
            result_contains: None,
        };
        let b = SwarmTransition::OnTextMention {
            target: "b".into(),
            text: "t".into(),
        };
        let c = SwarmTransition::OnCondition {
            target: "c".into(),
            condition: Arc::new(|_: &SwarmContext| false),
        };
        assert_eq!(a.target(), "a");
        assert_eq!(b.target(), "b");
        assert_eq!(c.target(), "c");
    }

    #[test]
    fn test_as_str_wire_format() {
        assert_eq!(
            SwarmTransition::OnToolResult {
                target: "x".into(),
                tool_name: "t".into(),
                result_contains: None,
            }
            .as_str(),
            "on_tool_result"
        );
        assert_eq!(
            SwarmTransition::OnTextMention {
                target: "x".into(),
                text: "t".into(),
            }
            .as_str(),
            "on_text_mention"
        );
        assert_eq!(
            SwarmTransition::OnCondition {
                target: "x".into(),
                condition: Arc::new(|_: &SwarmContext| false),
            }
            .as_str(),
            "on_condition"
        );
    }

    #[test]
    fn test_debug_does_not_panic() {
        let transition = SwarmTransition::OnCondition {
            target: "x".into(),
            condition: Arc::new(|_: &SwarmContext| false),
        };
        let debug_str = format!("{transition:?}");
        assert!(debug_str.contains("OnCondition"));
    }
}
