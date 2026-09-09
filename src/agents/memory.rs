// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Conversation memory — session message history management.
//!
//! Ports python-sdk's `ConversationMemory` (see
//! `python-sdk/src/conductor/ai/agents/memory.py`). There, conversation state is designed to be
//! persisted in Conductor workflow variables so it survives process crashes; this module mirrors
//! the message-accumulation/trimming behavior only — persistence is the caller's/runtime's
//! responsibility, not this struct's.
//!
//! Per `docs/agents/parity-plan.md`'s class diagram, [`AgentDef`](super::AgentDef) holds
//! `ConversationMemory` with an *open* circle (`o--`), not a filled one (`*--`): `AgentDef`
//! doesn't own/construct this value the way it owns `Vec<ToolDef>` or `Vec<Guardrail>`. A
//! `ConversationMemory` is mutable session state a caller builds up turn-by-turn (and may persist
//! across process restarts) and hands to — or reads back from — an agent run; `AgentDef` merely
//! references it, the same reasoning that makes `CallbackHandler` `o--` there too. Wiring a
//! `memory` field onto `AgentDef` itself, plus its `AgentConfigSerializer` support, is tracked as
//! a separate follow-up — this module only defines the type.

use serde_json::Value;

/// The speaker/kind of a [`Message`], matching python-sdk's string literals used as dict
/// `"role"` values (`"user"`, `"assistant"`, `"system"`, `"tool_call"`, `"tool"`) exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    ToolCall,
    Tool,
}

impl MessageRole {
    /// Wire-format string, matching python-sdk's role values exactly.
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::ToolCall => "tool_call",
            MessageRole::Tool => "tool",
        }
    }
}

/// A single entry inside a [`Message`]'s `tool_calls` list (only populated on
/// `MessageRole::ToolCall` messages), matching python-sdk's `{"name", "taskReferenceName",
/// "input"}` dict shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub task_reference_name: String,
    pub input: Value,
}

/// One entry in a [`ConversationMemory`]'s history.
///
/// Kept as a flat struct rather than one enum variant per role, because that is exactly
/// python-sdk's shape: every message is the same dict with role-dependent optional fields
/// populated (`tool_calls` only for `ToolCall`; `tool_call_id`/`task_reference_name` only for
/// `Tool`). Construct these via [`ConversationMemory`]'s `add_*` methods rather than directly —
/// they populate the right combination of fields per role, the same division of responsibility
/// as python's `add_user_message`/`add_tool_call`/etc.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub role: MessageRole,
    pub message: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
    pub task_reference_name: Option<String>,
}

impl Message {
    fn new(role: MessageRole, message: impl Into<String>) -> Self {
        Self {
            role,
            message: message.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            task_reference_name: None,
        }
    }
}

/// Manages conversation history for an agent session.
///
/// Ports python-sdk's `ConversationMemory` dataclass as-is: a plain accumulator of [`Message`]s
/// optionally bounded by `max_messages`. This is runtime session state built up turn-by-turn
/// (not an immutable definition), so its message-adding methods take `&mut self` rather than
/// following this crate's usual consuming `with_*` builder convention — contrast
/// [`AgentDef`](super::AgentDef), which builds an immutable definition.
#[derive(Debug, Clone, Default)]
pub struct ConversationMemory {
    pub messages: Vec<Message>,
    pub max_messages: Option<u32>,
}

impl ConversationMemory {
    /// Create an empty conversation memory with no bound on message count.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of messages to retain; oldest non-system messages are trimmed
    /// first once this is exceeded. See [`ConversationMemory::trim`].
    pub fn with_max_messages(mut self, max_messages: u32) -> Self {
        self.max_messages = Some(max_messages);
        self
    }

    /// Append a user message to the conversation.
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.messages.push(Message::new(MessageRole::User, content));
        self.trim();
    }

    /// Append an assistant message to the conversation.
    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.messages
            .push(Message::new(MessageRole::Assistant, content));
        self.trim();
    }

    /// Append a system message to the conversation.
    pub fn add_system_message(&mut self, content: impl Into<String>) {
        self.messages
            .push(Message::new(MessageRole::System, content));
        self.trim();
    }

    /// Record a tool call in the conversation. `task_reference_name` defaults to
    /// `"{tool_name}_ref"` when not given, matching python-sdk.
    pub fn add_tool_call(
        &mut self,
        tool_name: impl Into<String>,
        arguments: Value,
        task_reference_name: Option<String>,
    ) {
        let tool_name = tool_name.into();
        let reference = task_reference_name.unwrap_or_else(|| format!("{tool_name}_ref"));
        let mut message = Message::new(MessageRole::ToolCall, "");
        message.tool_calls.push(ToolCall {
            name: tool_name,
            task_reference_name: reference,
            input: arguments,
        });
        self.messages.push(message);
        self.trim();
    }

    /// Record a tool result in the conversation. `task_reference_name` defaults to
    /// `"{tool_name}_ref"` when not given, matching python-sdk. `result` is stringified,
    /// matching python's `str(result)`.
    pub fn add_tool_result(
        &mut self,
        tool_name: impl Into<String>,
        result: impl std::fmt::Display,
        task_reference_name: Option<String>,
    ) {
        let tool_name = tool_name.into();
        let reference = task_reference_name.unwrap_or_else(|| format!("{tool_name}_ref"));
        let mut message = Message::new(MessageRole::Tool, result.to_string());
        message.tool_call_id = Some(reference.clone());
        message.task_reference_name = Some(reference);
        self.messages.push(message);
        self.trim();
    }

    /// Return a clone of the accumulated messages. Matches python's `to_chat_messages`, which
    /// deep-copies so callers can't mutate this memory's history through the returned list.
    pub fn to_chat_messages(&self) -> Vec<Message> {
        self.messages.clone()
    }

    /// Clear all conversation history.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Trim messages to stay within `max_messages`.
    ///
    /// Preserves original ordering: removes the oldest non-system messages first while keeping
    /// all system messages in their original positions. Ported field-for-field from python-sdk's
    /// `_trim` (`python-sdk/src/conductor/ai/agents/memory.py`), including its quirk that
    /// `max_messages == Some(0)` disables trimming entirely — python's guard is `if
    /// self.max_messages and ...`, and `0` is falsy in Python, so a configured zero is silently
    /// treated the same as unset rather than "keep zero messages".
    fn trim(&mut self) {
        let Some(max_messages) = self.max_messages else {
            return;
        };
        if max_messages == 0 {
            return;
        }
        let max_messages = max_messages as usize;
        if self.messages.len() <= max_messages {
            return;
        }

        let system_count = self
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .count();

        if system_count >= max_messages {
            // More system messages than budget — keep only the latest.
            let system_msgs: Vec<Message> = self
                .messages
                .iter()
                .filter(|m| m.role == MessageRole::System)
                .cloned()
                .collect();
            let start = system_msgs.len() - max_messages;
            self.messages = system_msgs[start..].to_vec();
            return;
        }

        // Number of non-system messages we can keep.
        let keep_non_system = max_messages - system_count;
        // Count non-system messages from the end to find the cutoff.
        let mut non_system_seen = 0;
        let mut cutoff_idx = self.messages.len();
        for i in (0..self.messages.len()).rev() {
            if self.messages[i].role != MessageRole::System {
                non_system_seen += 1;
                if non_system_seen == keep_non_system {
                    cutoff_idx = i;
                    break;
                }
            }
        }

        // Keep all messages from cutoff_idx onward, plus system messages before it.
        let mut result: Vec<Message> = self.messages[..cutoff_idx]
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .cloned()
            .collect();
        result.extend(self.messages[cutoff_idx..].iter().cloned());
        self.messages = result;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_messages_accumulate() {
        let mut memory = ConversationMemory::new();
        memory.add_user_message("hi");
        memory.add_assistant_message("hello");
        assert_eq!(memory.messages.len(), 2);
        assert_eq!(memory.messages[0].role, MessageRole::User);
        assert_eq!(memory.messages[0].message, "hi");
        assert_eq!(memory.messages[1].role, MessageRole::Assistant);
        assert_eq!(memory.messages[1].message, "hello");
    }

    #[test]
    fn test_tool_call_and_result_default_reference_name() {
        let mut memory = ConversationMemory::new();
        memory.add_tool_call("search", serde_json::json!({"q": "rust"}), None);
        memory.add_tool_result("search", 42, None);

        let call = &memory.messages[0];
        assert_eq!(call.role, MessageRole::ToolCall);
        assert_eq!(call.tool_calls.len(), 1);
        assert_eq!(call.tool_calls[0].name, "search");
        assert_eq!(call.tool_calls[0].task_reference_name, "search_ref");
        assert_eq!(call.tool_calls[0].input, serde_json::json!({"q": "rust"}));

        let result = &memory.messages[1];
        assert_eq!(result.role, MessageRole::Tool);
        assert_eq!(result.message, "42");
        assert_eq!(result.tool_call_id.as_deref(), Some("search_ref"));
        assert_eq!(result.task_reference_name.as_deref(), Some("search_ref"));
    }

    #[test]
    fn test_trim_keeps_most_recent_when_no_system_messages() {
        let mut memory = ConversationMemory::new().with_max_messages(2);
        memory.add_user_message("one");
        memory.add_assistant_message("two");
        memory.add_user_message("three");

        assert_eq!(memory.messages.len(), 2);
        assert_eq!(memory.messages[0].message, "two");
        assert_eq!(memory.messages[1].message, "three");
    }

    #[test]
    fn test_trim_preserves_system_messages_in_original_position() {
        let mut memory = ConversationMemory::new().with_max_messages(2);
        memory.add_system_message("sys");
        memory.add_user_message("one");
        memory.add_assistant_message("two");
        memory.add_user_message("three");

        // Budget is 2: 1 system message is always kept, leaving room for exactly 1 non-system
        // message — the most recent one.
        assert_eq!(memory.messages.len(), 2);
        assert_eq!(memory.messages[0].role, MessageRole::System);
        assert_eq!(memory.messages[0].message, "sys");
        assert_eq!(memory.messages[1].message, "three");
    }

    #[test]
    fn test_trim_more_system_messages_than_budget_keeps_latest_system_only() {
        let mut memory = ConversationMemory::new().with_max_messages(2);
        memory.add_system_message("sys1");
        memory.add_system_message("sys2");
        memory.add_system_message("sys3");

        assert_eq!(memory.messages.len(), 2);
        assert!(memory.messages.iter().all(|m| m.role == MessageRole::System));
        assert_eq!(memory.messages[0].message, "sys2");
        assert_eq!(memory.messages[1].message, "sys3");
    }

    #[test]
    fn test_max_messages_zero_disables_trimming() {
        // Matches python-sdk's falsy-zero quirk in `_trim`: max_messages == 0 behaves as unset.
        let mut memory = ConversationMemory::new().with_max_messages(0);
        memory.add_user_message("one");
        memory.add_user_message("two");
        memory.add_user_message("three");
        assert_eq!(memory.messages.len(), 3);
    }

    #[test]
    fn test_clear() {
        let mut memory = ConversationMemory::new();
        memory.add_user_message("hi");
        memory.clear();
        assert!(memory.messages.is_empty());
    }

    #[test]
    fn test_to_chat_messages_is_independent_copy() {
        let mut memory = ConversationMemory::new();
        memory.add_user_message("hi");
        let mut snapshot = memory.to_chat_messages();
        snapshot.push(Message::new(MessageRole::User, "not shared"));
        assert_eq!(memory.messages.len(), 1);
        assert_eq!(snapshot.len(), 2);
    }

    #[test]
    fn test_message_role_as_str_matches_python_wire_values() {
        assert_eq!(MessageRole::User.as_str(), "user");
        assert_eq!(MessageRole::Assistant.as_str(), "assistant");
        assert_eq!(MessageRole::System.as_str(), "system");
        assert_eq!(MessageRole::ToolCall.as_str(), "tool_call");
        assert_eq!(MessageRole::Tool.as_str(), "tool");
    }
}
