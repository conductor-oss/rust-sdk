// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::{ConductorError, Result};
use serde_json::Value;

/// Composable rule that decides when an agent should stop.
///
/// A recursive enum: `And`/`Or` variants hold other `TerminationCondition`s. The server compiles
/// each condition into a Conductor worker task participating in the agent's `DoWhile` loop;
/// [`TerminationCondition::should_terminate`] evaluates a condition locally, used by the
/// `{agent_name}_termination` worker [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve)
/// registers.
///
/// Combine conditions with `&` / `|`, which flatten automatically — `a & b & c` produces one
/// three-element `And`, never a nested `And(And(a, b), c)`:
///
/// ```
/// use conductor::agents::TerminationCondition;
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
    /// [`TerminationCondition::text_mention_case_sensitive`].
    TextMention { text: String, case_sensitive: bool },

    /// Terminate when the LLM output, after stripping surrounding whitespace, exactly equals
    /// `stop_message` — an exact match, unlike [`TerminationCondition::TextMention`]'s substring
    /// search.
    StopMessage { stop_message: String },

    /// Terminate once the conversation reaches `max_messages` messages (all roles counted).
    MaxMessage { max_messages: u32 },

    /// Terminate once cumulative token usage crosses any of the configured budgets.
    ///
    /// At least one of the three must be `Some` — enforced by
    /// [`TerminationCondition::token_usage`] at construction time.
    TokenUsage {
        max_total_tokens: Option<u32>,
        max_prompt_tokens: Option<u32>,
        max_completion_tokens: Option<u32>,
    },

    /// AND combinator — terminates only once every child condition triggers. Built by
    /// [`TerminationCondition::and`] or the `&` operator.
    And {
        conditions: Vec<TerminationCondition>,
    },

    /// OR combinator — terminates as soon as any child condition triggers. Built by
    /// [`TerminationCondition::or`] or the `|` operator.
    Or {
        conditions: Vec<TerminationCondition>,
    },
}

impl TerminationCondition {
    /// Case-insensitive substring match.
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

    /// Exact-match stop signal. Use [`TerminationCondition::stop_message_default`] for the
    /// common `"TERMINATE"` case.
    pub fn stop_message(stop_message: impl Into<String>) -> Self {
        TerminationCondition::StopMessage {
            stop_message: stop_message.into(),
        }
    }

    /// `stop_message("TERMINATE")`.
    #[must_use]
    pub fn stop_message_default() -> Self {
        Self::stop_message("TERMINATE")
    }

    /// Terminate after `max_messages` messages. Rejects `max_messages < 1`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `max_messages` is 0.
    pub fn max_message(max_messages: u32) -> Result<Self> {
        if max_messages < 1 {
            return Err(ConductorError::agent("max_messages must be >= 1"));
        }
        Ok(TerminationCondition::MaxMessage { max_messages })
    }

    /// Terminate once total token usage (prompt + completion) reaches `max_total_tokens`.
    /// Infallible: a single `Some` limit always satisfies
    /// [`TerminationCondition::token_usage`]'s "at least one limit" requirement.
    #[must_use]
    pub fn max_total_tokens(max_total_tokens: u32) -> Self {
        TerminationCondition::TokenUsage {
            max_total_tokens: Some(max_total_tokens),
            max_prompt_tokens: None,
            max_completion_tokens: None,
        }
    }

    /// Terminate once cumulative token usage crosses any of the given budgets. At least one of
    /// the three must be `Some`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `max_total_tokens`, `max_prompt_tokens`, and `max_completion_tokens` are all `None`.
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
    /// for the common two-condition case; this exists for building an `And` directly from a
    /// `Vec`.
    #[must_use]
    pub fn and(conditions: Vec<TerminationCondition>) -> Self {
        TerminationCondition::And { conditions }
    }

    /// Explicit OR combinator over an arbitrary number of conditions. See
    /// [`TerminationCondition::and`].
    #[must_use]
    pub fn or(conditions: Vec<TerminationCondition>) -> Self {
        TerminationCondition::Or { conditions }
    }

    /// Wire-format discriminant for the `"type"` field.
    #[must_use]
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

    /// Combine with AND — both must trigger to terminate. Flattens: an existing `And` on either
    /// side gets its conditions spliced in rather than nested.
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

    /// Combine with OR — either one triggers termination. Flattens the same way as the `BitAnd`
    /// impl.
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

/// Result of evaluating a [`TerminationCondition`] against a runtime context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminationOutcome {
    pub should_terminate: bool,
    pub reason: String,
}

impl TerminationOutcome {
    fn no() -> Self {
        Self {
            should_terminate: false,
            reason: String::new(),
        }
    }

    fn yes(reason: impl Into<String>) -> Self {
        Self {
            should_terminate: true,
            reason: reason.into(),
        }
    }
}

impl TerminationCondition {
    /// Evaluate this condition against a runtime context shaped `{"result": <text>, "messages":
    /// [...], "iteration": <n>, "token_usage": {"total_tokens": ..., "prompt_tokens": ...,
    /// "completion_tokens": ...}}`. [`TerminationCondition::MaxMessage`] falls back to
    /// `iteration` when `messages` is empty/absent; [`TerminationCondition::And`] joins reasons
    /// with `" AND "`.
    ///
    /// Used by the `{agent_name}_termination` worker
    /// [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve) registers.
    pub fn should_terminate(&self, context: &Value) -> TerminationOutcome {
        match self {
            TerminationCondition::TextMention {
                text,
                case_sensitive,
            } => {
                let result = context.get("result").and_then(Value::as_str).unwrap_or("");
                let (haystack, needle) = if *case_sensitive {
                    (result.to_owned(), text.clone())
                } else {
                    (result.to_lowercase(), text.to_lowercase())
                };
                if haystack.contains(&needle) {
                    TerminationOutcome::yes(format!("Text '{text}' found in output"))
                } else {
                    TerminationOutcome::no()
                }
            }
            TerminationCondition::StopMessage { stop_message } => {
                let result = context
                    .get("result")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if result == stop_message {
                    TerminationOutcome::yes(format!("Stop message '{stop_message}' received"))
                } else {
                    TerminationOutcome::no()
                }
            }
            TerminationCondition::MaxMessage { max_messages } => {
                let from_messages = context
                    .get("messages")
                    .and_then(Value::as_array)
                    .map(|a| a.len() as u64)
                    .filter(|&count| count > 0);
                let count = from_messages.unwrap_or_else(|| {
                    context
                        .get("iteration")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                });
                if count >= u64::from(*max_messages) {
                    TerminationOutcome::yes(format!(
                        "Message count ({count}) >= limit ({max_messages})"
                    ))
                } else {
                    TerminationOutcome::no()
                }
            }
            TerminationCondition::TokenUsage {
                max_total_tokens,
                max_prompt_tokens,
                max_completion_tokens,
            } => {
                let Some(usage) = context.get("token_usage").filter(|u| u.is_object()) else {
                    return TerminationOutcome::no();
                };
                let field = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
                let total = field("total_tokens");
                let prompt = field("prompt_tokens");
                let completion = field("completion_tokens");

                if let Some(max) = max_total_tokens {
                    if total >= u64::from(*max) {
                        return TerminationOutcome::yes(format!(
                            "Total tokens ({total}) >= limit ({max})"
                        ));
                    }
                }
                if let Some(max) = max_prompt_tokens {
                    if prompt >= u64::from(*max) {
                        return TerminationOutcome::yes(format!(
                            "Prompt tokens ({prompt}) >= limit ({max})"
                        ));
                    }
                }
                if let Some(max) = max_completion_tokens {
                    if completion >= u64::from(*max) {
                        return TerminationOutcome::yes(format!(
                            "Completion tokens ({completion}) >= limit ({max})"
                        ));
                    }
                }
                TerminationOutcome::no()
            }
            TerminationCondition::And { conditions } => {
                let mut reasons = Vec::new();
                for cond in conditions {
                    let outcome = cond.should_terminate(context);
                    if !outcome.should_terminate {
                        return TerminationOutcome::no();
                    }
                    if !outcome.reason.is_empty() {
                        reasons.push(outcome.reason);
                    }
                }
                TerminationOutcome::yes(reasons.join(" AND "))
            }
            TerminationCondition::Or { conditions } => {
                for cond in conditions {
                    let outcome = cond.should_terminate(context);
                    if outcome.should_terminate {
                        return outcome;
                    }
                }
                TerminationOutcome::no()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_terminate_text_mention_case_insensitive_default() {
        let cond = TerminationCondition::text_mention("terminate");
        let outcome = cond.should_terminate(&serde_json::json!({"result": "OK, TERMINATE now."}));
        assert!(outcome.should_terminate);
        assert!(outcome.reason.contains("terminate"));

        let outcome = cond.should_terminate(&serde_json::json!({"result": "still working"}));
        assert!(!outcome.should_terminate);
    }

    #[test]
    fn test_should_terminate_text_mention_case_sensitive() {
        let cond = TerminationCondition::text_mention_case_sensitive("TERMINATE");
        let outcome = cond.should_terminate(&serde_json::json!({"result": "please terminate"}));
        assert!(
            !outcome.should_terminate,
            "lowercase must not match case-sensitive TERMINATE"
        );
    }

    #[test]
    fn test_should_terminate_stop_message_exact_match_after_trim() {
        let cond = TerminationCondition::stop_message("DONE");
        let outcome = cond.should_terminate(&serde_json::json!({"result": "  DONE  "}));
        assert!(outcome.should_terminate);

        let outcome = cond.should_terminate(&serde_json::json!({"result": "DONE for real"}));
        assert!(
            !outcome.should_terminate,
            "must be an exact match, not a substring"
        );
    }

    #[test]
    fn test_should_terminate_max_message_counts_messages() {
        let cond = TerminationCondition::max_message(2).unwrap();
        let outcome = cond.should_terminate(&serde_json::json!({"messages": ["a", "b"]}));
        assert!(outcome.should_terminate);

        let outcome = cond.should_terminate(&serde_json::json!({"messages": ["a"]}));
        assert!(!outcome.should_terminate);
    }

    #[test]
    fn test_should_terminate_max_message_falls_back_to_iteration_when_messages_empty() {
        let cond = TerminationCondition::max_message(3).unwrap();
        let outcome = cond.should_terminate(&serde_json::json!({"messages": [], "iteration": 5}));
        assert!(outcome.should_terminate);
    }

    #[test]
    fn test_should_terminate_token_usage_checks_total() {
        let cond = TerminationCondition::max_total_tokens(100);
        let outcome =
            cond.should_terminate(&serde_json::json!({"token_usage": {"total_tokens": 150}}));
        assert!(outcome.should_terminate);

        let outcome =
            cond.should_terminate(&serde_json::json!({"token_usage": {"total_tokens": 50}}));
        assert!(!outcome.should_terminate);
    }

    #[test]
    fn test_should_terminate_token_usage_missing_never_terminates() {
        let cond = TerminationCondition::max_total_tokens(1);
        let outcome = cond.should_terminate(&serde_json::json!({}));
        assert!(!outcome.should_terminate);
    }

    #[test]
    fn test_should_terminate_and_requires_all_and_joins_reasons() {
        let cond = TerminationCondition::text_mention("done")
            & TerminationCondition::max_message(1).unwrap();
        let outcome =
            cond.should_terminate(&serde_json::json!({"result": "done", "messages": ["a"]}));
        assert!(outcome.should_terminate);
        assert!(outcome.reason.contains(" AND "));

        let outcome = cond.should_terminate(&serde_json::json!({"result": "done", "messages": []}));
        assert!(
            !outcome.should_terminate,
            "only one of two AND-ed conditions triggered"
        );
    }

    #[test]
    fn test_should_terminate_or_short_circuits_on_first_match() {
        let cond = TerminationCondition::text_mention("done")
            | TerminationCondition::max_message(100).unwrap();
        let outcome = cond.should_terminate(&serde_json::json!({"result": "done", "messages": []}));
        assert!(outcome.should_terminate);
        assert!(!outcome.reason.contains(" AND "));
    }

    #[test]
    fn test_text_mention_case_sensitivity_default() {
        let cond = TerminationCondition::text_mention("DONE");
        assert_eq!(
            cond,
            TerminationCondition::TextMention {
                text: "DONE".to_owned(),
                case_sensitive: false,
            }
        );

        let cond = TerminationCondition::text_mention_case_sensitive("DONE");
        assert_eq!(
            cond,
            TerminationCondition::TextMention {
                text: "DONE".to_owned(),
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
        TerminationCondition::max_message(0).unwrap_err();
        TerminationCondition::max_message(1).unwrap();
        assert_eq!(
            TerminationCondition::max_message(20).unwrap(),
            TerminationCondition::MaxMessage { max_messages: 20 }
        );
    }

    #[test]
    fn test_token_usage_requires_at_least_one_limit() {
        TerminationCondition::token_usage(None, None, None).unwrap_err();
        TerminationCondition::token_usage(Some(1000), None, None).unwrap();
        TerminationCondition::token_usage(None, Some(500), None).unwrap();
        TerminationCondition::token_usage(None, None, Some(500)).unwrap();
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

    /// Nested And/Or composition: (`TextMention` OR `StopMessage`) AND `MaxMessage`, built
    /// directly rather than via operator-flattening.
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
