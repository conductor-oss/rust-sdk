// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::{ConductorError, Result};

/// Composable rule that decides when an agent should stop.
///
/// Ports python-sdk's `conductor.ai.agents.termination` module — see
/// `rust-sdk/docs/agents/parity-plan.md` (search "TerminationCondition") for where this sits in
/// the overall `AgentDef` shape. Python models this as a small class hierarchy: an abstract
/// `TerminationCondition` base with concrete `TextMentionTermination`, `StopMessageTermination`,
/// `MaxMessageTermination`, `TokenUsageTermination` leaves, plus private `_AndTermination` /
/// `_OrTermination` combinators built via the `&` and `|` operators. This crate collapses that
/// hierarchy into a single recursive enum instead, matching the parity-plan's class diagram
/// exactly (`TerminationCondition "1" o-- "0..*" TerminationCondition` — `And`/`Or` hold other
/// `TerminationCondition`s, recursively) — there's no trait to implement, just data.
///
/// Termination itself is evaluated server-side: each condition compiles into a Conductor worker
/// task that participates in the agent's `DoWhile` loop (see the module docstring in
/// `termination.py`). So unlike python's `should_terminate(context)` method on every subclass,
/// this type has no local evaluation logic at all — it is pure wire-format data, serialized into
/// the same `TerminationConfig` JSON shape python's
/// `AgentConfigSerializer._serialize_termination` produces (see [`TerminationCondition::type_str`]
/// for the exact `"type"` discriminant values).
///
/// Construct via the associated functions below (mirroring python's constructors) and combine
/// with `&` / `|`, which mirror python's `__and__` / `__or__` operator overloads exactly,
/// including the flattening behavior — `a & b & c` produces one three-element `And`, never a
/// nested `And(And(a, b), c)`:
///
/// ```
/// use conductor::agents::termination::TerminationCondition;
///
/// // Stop when the LLM says "DONE" OR after 50 messages.
/// let stop = TerminationCondition::text_mention("DONE")
///     | TerminationCondition::max_message(50).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminationCondition {
    /// Terminate when the LLM output contains `text` as a substring.
    ///
    /// Case-insensitive unless constructed via
    /// [`TerminationCondition::text_mention_case_sensitive`] — matches python's
    /// `TextMentionTermination(text, case_sensitive=False)` default.
    TextMention { text: String, case_sensitive: bool },

    /// Terminate when the LLM output, after stripping surrounding whitespace, exactly equals
    /// `stop_message` — an exact match, unlike [`TerminationCondition::TextMention`]'s substring
    /// search.
    StopMessage { stop_message: String },

    /// Terminate once the conversation reaches `max_messages` messages (all roles counted,
    /// matching python's `MaxMessageTermination`).
    MaxMessage { max_messages: u32 },

    /// Terminate once cumulative token usage crosses any of the configured budgets.
    ///
    /// At least one of the three must be `Some` — enforced by
    /// [`TerminationCondition::token_usage`] at construction time, matching the `ValueError`
    /// python's `TokenUsageTermination.__init__` raises when all three are `None`.
    TokenUsage {
        max_total_tokens: Option<u32>,
        max_prompt_tokens: Option<u32>,
        max_completion_tokens: Option<u32>,
    },

    /// AND combinator — terminates only once every child condition triggers. Built by
    /// [`TerminationCondition::and`] or the `&` operator; matches python's `_AndTermination`.
    And {
        conditions: Vec<TerminationCondition>,
    },

    /// OR combinator — terminates as soon as any child condition triggers. Built by
    /// [`TerminationCondition::or`] or the `|` operator; matches python's `_OrTermination`.
    Or {
        conditions: Vec<TerminationCondition>,
    },
}

impl TerminationCondition {
    /// Case-insensitive substring match (python's default: `case_sensitive=False`).
    pub fn text_mention(text: impl Into<String>) -> Self {
        TerminationCondition::TextMention {
            text: text.into(),
            case_sensitive: false,
        }
    }

    /// Case-sensitive substring match.
    pub fn text_mention_case_sensitive(text: impl Into<String>) -> Self {
        TerminationCondition::TextMention {
            text: text.into(),
            case_sensitive: true,
        }
    }

    /// Exact-match stop signal. Python defaults `stop_message` to `"TERMINATE"`; since Rust has
    /// no default-argument syntax, use [`TerminationCondition::stop_message_default`] for that
    /// case instead of repeating the literal at every call site.
    pub fn stop_message(stop_message: impl Into<String>) -> Self {
        TerminationCondition::StopMessage {
            stop_message: stop_message.into(),
        }
    }

    /// `stop_message("TERMINATE")` — matches python's `StopMessageTermination()` default.
    pub fn stop_message_default() -> Self {
        Self::stop_message("TERMINATE")
    }

    /// Terminate after `max_messages` messages. Rejects `max_messages < 1`, matching python's
    /// `ValueError("max_messages must be >= 1")`.
    pub fn max_message(max_messages: u32) -> Result<Self> {
        if max_messages < 1 {
            return Err(ConductorError::agent("max_messages must be >= 1"));
        }
        Ok(TerminationCondition::MaxMessage { max_messages })
    }

    /// Terminate once total token usage (prompt + completion) reaches `max_total_tokens` — the
    /// common case from python's `TokenUsageTermination(max_total_tokens=...)` example.
    /// Infallible: a single `Some` limit always satisfies
    /// [`TerminationCondition::token_usage`]'s "at least one limit" requirement.
    pub fn max_total_tokens(max_total_tokens: u32) -> Self {
        TerminationCondition::TokenUsage {
            max_total_tokens: Some(max_total_tokens),
            max_prompt_tokens: None,
            max_completion_tokens: None,
        }
    }

    /// Terminate once cumulative token usage crosses any of the given budgets. At least one of
    /// the three must be `Some` — matches the `ValueError` python's
    /// `TokenUsageTermination.__init__` raises when all three are `None`.
    pub fn token_usage(
        max_total_tokens: Option<u32>,
        max_prompt_tokens: Option<u32>,
        max_completion_tokens: Option<u32>,
    ) -> Result<Self> {
        if max_total_tokens.is_none()
            && max_prompt_tokens.is_none()
            && max_completion_tokens.is_none()
        {
            return Err(ConductorError::agent(
                "at least one token limit must be specified",
            ));
        }
        Ok(TerminationCondition::TokenUsage {
            max_total_tokens,
            max_prompt_tokens,
            max_completion_tokens,
        })
    }

    /// Explicit AND combinator over an arbitrary number of conditions. Prefer the `&` operator
    /// (see [`TerminationCondition`]'s docs) for the common two-condition case — this exists for
    /// building an `And` directly from a `Vec`, matching the `"0..*"` cardinality in the
    /// parity-plan's class diagram.
    pub fn and(conditions: Vec<TerminationCondition>) -> Self {
        TerminationCondition::And { conditions }
    }

    /// Explicit OR combinator over an arbitrary number of conditions. See
    /// [`TerminationCondition::and`].
    pub fn or(conditions: Vec<TerminationCondition>) -> Self {
        TerminationCondition::Or { conditions }
    }

    /// Wire-format discriminant, matching the `"type"` value python's
    /// `AgentConfigSerializer._serialize_termination` emits for each variant exactly.
    pub fn type_str(&self) -> &'static str {
        match self {
            TerminationCondition::TextMention { .. } => "text_mention",
            TerminationCondition::StopMessage { .. } => "stop_message",
            TerminationCondition::MaxMessage { .. } => "max_message",
            TerminationCondition::TokenUsage { .. } => "token_usage",
            TerminationCondition::And { .. } => "and",
            TerminationCondition::Or { .. } => "or",
        }
    }
}

impl std::ops::BitAnd for TerminationCondition {
    type Output = TerminationCondition;

    /// Combine with AND — both must trigger to terminate. Mirrors python's `__and__`, including
    /// the flattening: an existing `And` on either side gets its conditions spliced in rather
    /// than nested, so `a & b & c` is one `And` of three, not `And(And(a, b), c)`.
    fn bitand(self, rhs: TerminationCondition) -> TerminationCondition {
        let mut conditions = match self {
            TerminationCondition::And { conditions } => conditions,
            other => vec![other],
        };
        match rhs {
            TerminationCondition::And {
                conditions: rhs_conditions,
            } => conditions.extend(rhs_conditions),
            other => conditions.push(other),
        }
        TerminationCondition::And { conditions }
    }
}

impl std::ops::BitOr for TerminationCondition {
    type Output = TerminationCondition;

    /// Combine with OR — either one triggers termination. Mirrors python's `__or__`, with the
    /// same flattening behavior as [`TerminationCondition::bitand`].
    fn bitor(self, rhs: TerminationCondition) -> TerminationCondition {
        let mut conditions = match self {
            TerminationCondition::Or { conditions } => conditions,
            other => vec![other],
        };
        match rhs {
            TerminationCondition::Or {
                conditions: rhs_conditions,
            } => conditions.extend(rhs_conditions),
            other => conditions.push(other),
        }
        TerminationCondition::Or { conditions }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_mention_case_sensitivity_default() {
        let cond = TerminationCondition::text_mention("DONE");
        assert_eq!(
            cond,
            TerminationCondition::TextMention {
                text: "DONE".to_string(),
                case_sensitive: false,
            }
        );

        let cond = TerminationCondition::text_mention_case_sensitive("DONE");
        assert_eq!(
            cond,
            TerminationCondition::TextMention {
                text: "DONE".to_string(),
                case_sensitive: true,
            }
        );
    }

    #[test]
    fn test_stop_message_default() {
        assert_eq!(
            TerminationCondition::stop_message_default(),
            TerminationCondition::stop_message("TERMINATE")
        );
    }

    #[test]
    fn test_max_message_validation() {
        assert!(TerminationCondition::max_message(0).is_err());
        assert!(TerminationCondition::max_message(1).is_ok());
        assert_eq!(
            TerminationCondition::max_message(20).unwrap(),
            TerminationCondition::MaxMessage { max_messages: 20 }
        );
    }

    #[test]
    fn test_token_usage_requires_at_least_one_limit() {
        assert!(TerminationCondition::token_usage(None, None, None).is_err());
        assert!(TerminationCondition::token_usage(Some(1000), None, None).is_ok());
        assert!(TerminationCondition::token_usage(None, Some(500), None).is_ok());
        assert!(TerminationCondition::token_usage(None, None, Some(500)).is_ok());
    }

    #[test]
    fn test_max_total_tokens_convenience_matches_token_usage() {
        assert_eq!(
            TerminationCondition::max_total_tokens(10_000),
            TerminationCondition::token_usage(Some(10_000), None, None).unwrap()
        );
    }

    #[test]
    fn test_type_str_matches_python_wire_discriminants() {
        assert_eq!(
            TerminationCondition::text_mention("x").type_str(),
            "text_mention"
        );
        assert_eq!(
            TerminationCondition::stop_message_default().type_str(),
            "stop_message"
        );
        assert_eq!(
            TerminationCondition::max_message(1).unwrap().type_str(),
            "max_message"
        );
        assert_eq!(
            TerminationCondition::max_total_tokens(1).type_str(),
            "token_usage"
        );
        assert_eq!(TerminationCondition::and(vec![]).type_str(), "and");
        assert_eq!(TerminationCondition::or(vec![]).type_str(), "or");
    }

    #[test]
    fn test_and_operator_flattens_nested_and() {
        let a = TerminationCondition::text_mention("A");
        let b = TerminationCondition::stop_message_default();
        let c = TerminationCondition::max_message(10).unwrap();

        // a & b & c should flatten to one 3-element And, not And(And(a, b), c).
        let combined = a.clone() & b.clone() & c.clone();
        assert_eq!(
            combined,
            TerminationCondition::And {
                conditions: vec![a, b, c],
            }
        );
    }

    #[test]
    fn test_or_operator_flattens_nested_or() {
        let a = TerminationCondition::text_mention("A");
        let b = TerminationCondition::stop_message_default();
        let c = TerminationCondition::max_message(10).unwrap();

        let combined = a.clone() | b.clone() | c.clone();
        assert_eq!(
            combined,
            TerminationCondition::Or {
                conditions: vec![a, b, c],
            }
        );
    }

    /// Nested And/Or composition: (TextMention OR StopMessage) AND MaxMessage — exercises the
    /// recursive `TerminationCondition "1" o-- "0..*" TerminationCondition` shape from the
    /// parity-plan's class diagram directly (an `And` whose child is itself an `Or`), rather than
    /// relying on operator-flattening to build it.
    #[test]
    fn test_nested_and_or_composition() {
        let inner_or = TerminationCondition::or(vec![
            TerminationCondition::text_mention("DONE"),
            TerminationCondition::stop_message_default(),
        ]);
        let nested = TerminationCondition::and(vec![
            inner_or.clone(),
            TerminationCondition::max_message(50).unwrap(),
        ]);

        match nested {
            TerminationCondition::And { conditions } => {
                assert_eq!(conditions.len(), 2);
                assert_eq!(conditions[0], inner_or);
                assert!(matches!(conditions[0], TerminationCondition::Or { .. }));
                assert_eq!(
                    conditions[1],
                    TerminationCondition::MaxMessage { max_messages: 50 }
                );
            }
            _ => panic!("expected And"),
        }
    }

    #[test]
    fn test_token_usage_all_three_limits() {
        let cond =
            TerminationCondition::token_usage(Some(10_000), Some(6_000), Some(4_000)).unwrap();
        assert_eq!(
            cond,
            TerminationCondition::TokenUsage {
                max_total_tokens: Some(10_000),
                max_prompt_tokens: Some(6_000),
                max_completion_tokens: Some(4_000),
            }
        );
    }
}
