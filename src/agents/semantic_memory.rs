// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Semantic memory — long-term, similarity-based recall across agent sessions.
//!
//! Ports python-sdk's `conductor.ai.agents.semantic_memory` module. Like [`super::memory`]'s
//! `ConversationMemory`, this is a standalone, opt-in type: grepping python-sdk confirms nothing
//! in `agent.py`/`runtime.py` references `SemanticMemory`, so there is no `AgentDef` field or
//! `AgentRuntime` wiring to port here — only the type itself, for callers who want it.
//!
//! ## Deliberate differences from python
//!
//! - **Memory IDs**: python hashes `content + time.time()` with SHA-256 and truncates to 16 hex
//!   chars when no ID is supplied. This crate has no `sha2` dependency, and the ID is never a
//!   wire-format value (it never crosses the Conductor server boundary), so a random UUIDv4
//!   (already a dependency, used elsewhere in this crate) truncated to 16 hex chars is used
//!   instead — same shape (16 lowercase hex chars), different generation mechanism, since
//!   collision-resistance is what actually matters here, not reproducibility.
//! - **Store ordering**: python's `InMemoryStore` is a `dict` keyed by ID, and CPython dicts
//!   preserve insertion order, including keeping an existing key's original position when its
//!   value is overwritten. [`InMemoryStore`] here uses a `Vec<MemoryEntry>` with the same
//!   overwrite-in-place-else-append rule, matching that ordering behavior exactly rather than
//!   using a `HashMap`, whose iteration order is unspecified.

use std::collections::HashMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use uuid::Uuid;

/// A single memory entry, matching python's `MemoryEntry` dataclass field-for-field.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub metadata: HashMap<String, Value>,
    pub embedding: Option<Vec<f64>>,
    pub created_at: f64,
}

impl MemoryEntry {
    /// Build a new entry with just its content set; `id`/`created_at` are assigned by whichever
    /// [`MemoryStore`] the entry is added to, matching python's `MemoryEntry(content=...)` plus
    /// `InMemoryStore.add`'s fill-in-if-empty behavior.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..Default::default()
        }
    }
}

/// Abstract memory storage backend, matching python's `MemoryStore` ABC. Implement this to
/// integrate with an external vector database instead of the in-process [`InMemoryStore`]
/// default.
pub trait MemoryStore: Send + Sync {
    /// Store an entry, filling in `id`/`created_at` if unset, and return the (possibly
    /// generated) ID.
    fn add(&mut self, entry: MemoryEntry) -> String;
    /// Return up to `top_k` entries most similar to `query`.
    fn search(&self, query: &str, top_k: usize) -> Vec<MemoryEntry>;
    /// Remove an entry by ID; returns whether one was found and removed.
    fn delete(&mut self, memory_id: &str) -> bool;
    /// Remove every stored entry.
    fn clear(&mut self);
    /// Return every stored entry.
    fn list_all(&self) -> Vec<MemoryEntry>;
}

/// Simple in-process store using keyword-overlap (Jaccard) similarity, matching python's
/// `InMemoryStore` exactly. A lightweight fallback for when no real vector database is wired up;
/// production use should implement [`MemoryStore`] against one instead.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    memories: Vec<MemoryEntry>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

fn unix_timestamp() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn generate_memory_id() -> String {
    Uuid::new_v4().simple().to_string()[..16].to_string()
}

impl MemoryStore for InMemoryStore {
    fn add(&mut self, mut entry: MemoryEntry) -> String {
        if entry.id.is_empty() {
            entry.id = generate_memory_id();
        }
        if entry.created_at == 0.0 {
            entry.created_at = unix_timestamp();
        }
        let id = entry.id.clone();
        match self.memories.iter().position(|e| e.id == id) {
            Some(pos) => self.memories[pos] = entry,
            None => self.memories.push(entry),
        }
        id
    }

    fn search(&self, query: &str, top_k: usize) -> Vec<MemoryEntry> {
        if self.memories.is_empty() {
            return Vec::new();
        }

        let query_words: std::collections::HashSet<&str> = query.split_whitespace().collect();
        let query_words: std::collections::HashSet<String> =
            query_words.into_iter().map(|w| w.to_lowercase()).collect();

        let mut scored: Vec<(f64, &MemoryEntry)> = self
            .memories
            .iter()
            .map(|entry| {
                let entry_words: std::collections::HashSet<String> = entry
                    .content
                    .split_whitespace()
                    .map(|w| w.to_lowercase())
                    .collect();
                let score = if query_words.is_empty() || entry_words.is_empty() {
                    0.0
                } else {
                    let intersection = query_words.intersection(&entry_words).count();
                    let union = query_words.union(&entry_words).count();
                    if union == 0 {
                        0.0
                    } else {
                        intersection as f64 / union as f64
                    }
                };
                (score, entry)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .into_iter()
            .take(top_k)
            .filter(|(score, _)| *score > 0.0)
            .map(|(_, entry)| entry.clone())
            .collect()
    }

    fn delete(&mut self, memory_id: &str) -> bool {
        let len_before = self.memories.len();
        self.memories.retain(|e| e.id != memory_id);
        self.memories.len() != len_before
    }

    fn clear(&mut self) {
        self.memories.clear();
    }

    fn list_all(&self) -> Vec<MemoryEntry> {
        self.memories.clone()
    }
}

/// High-level semantic memory for agents, matching python's `SemanticMemory`. Manages
/// similarity-based retrieval over a pluggable [`MemoryStore`]; not currently wired into
/// [`super::AgentDef`] (see module doc) — callers use this directly to build/search memories and
/// inject the result into their own prompts.
pub struct SemanticMemory {
    store: Box<dyn MemoryStore>,
    max_results: usize,
    session_id: Option<String>,
}

impl fmt::Debug for SemanticMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SemanticMemory")
            .field("entries", &self.store.list_all().len())
            .field("max_results", &self.max_results)
            .finish()
    }
}

impl Default for SemanticMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticMemory {
    /// New memory backed by the default [`InMemoryStore`], `max_results` 5, no session scoping —
    /// matching python's constructor defaults.
    pub fn new() -> Self {
        Self {
            store: Box::new(InMemoryStore::new()),
            max_results: 5,
            session_id: None,
        }
    }

    /// Use a custom [`MemoryStore`] backend instead of the default [`InMemoryStore`].
    pub fn with_store(mut self, store: Box<dyn MemoryStore>) -> Self {
        self.store = store;
        self
    }

    /// Cap on memories retrieved per query.
    pub fn with_max_results(mut self, max_results: usize) -> Self {
        self.max_results = max_results;
        self
    }

    /// Scope added memories to a session ID (stamped into each entry's metadata).
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Add a memory, stamping `session_id` into its metadata when this instance is
    /// session-scoped, matching python's `add`.
    pub fn add(
        &mut self,
        content: impl Into<String>,
        metadata: Option<HashMap<String, Value>>,
    ) -> String {
        let mut meta = metadata.unwrap_or_default();
        if let Some(session_id) = &self.session_id {
            meta.insert("session_id".to_string(), Value::String(session_id.clone()));
        }
        let entry = MemoryEntry {
            content: content.into(),
            metadata: meta,
            ..Default::default()
        };
        self.store.add(entry)
    }

    /// Search for relevant memories, returning just their content strings, most relevant first.
    pub fn search(&self, query: &str, top_k: Option<usize>) -> Vec<String> {
        self.search_entries(query, top_k)
            .into_iter()
            .map(|e| e.content)
            .collect()
    }

    /// Search and return full [`MemoryEntry`] values.
    pub fn search_entries(&self, query: &str, top_k: Option<usize>) -> Vec<MemoryEntry> {
        let k = top_k.unwrap_or(self.max_results);
        self.store.search(query, k)
    }

    /// Delete a memory by ID.
    pub fn delete(&mut self, memory_id: &str) -> bool {
        self.store.delete(memory_id)
    }

    /// Delete all memories.
    pub fn clear(&mut self) {
        self.store.clear();
    }

    /// Return every stored memory.
    pub fn list_all(&self) -> Vec<MemoryEntry> {
        self.store.list_all()
    }

    /// Relevant memories formatted for injection into a prompt, or an empty string when none
    /// match — matching python's `get_context`.
    pub fn get_context(&self, query: &str) -> String {
        let memories = self.search(query, None);
        if memories.is_empty() {
            return String::new();
        }
        let mut lines = vec!["Relevant context from memory:".to_string()];
        for (i, mem) in memories.iter().enumerate() {
            lines.push(format!("  {}. {}", i + 1, mem));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_entry_new_sets_only_content() {
        let entry = MemoryEntry::new("hello");
        assert_eq!(entry.content, "hello");
        assert_eq!(entry.id, "");
        assert_eq!(entry.created_at, 0.0);
        assert!(entry.metadata.is_empty());
        assert!(entry.embedding.is_none());
    }

    #[test]
    fn test_in_memory_store_add_generates_id_and_timestamp() {
        let mut store = InMemoryStore::new();
        let id = store.add(MemoryEntry::new("hello world"));
        assert_eq!(id.len(), 16);
        let entries = store.list_all();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].created_at > 0.0);
    }

    #[test]
    fn test_in_memory_store_add_preserves_given_id_and_timestamp() {
        let mut store = InMemoryStore::new();
        let entry = MemoryEntry {
            id: "fixed-id".to_string(),
            content: "hello".to_string(),
            created_at: 42.0,
            ..Default::default()
        };
        let id = store.add(entry);
        assert_eq!(id, "fixed-id");
        assert_eq!(store.list_all()[0].created_at, 42.0);
    }

    #[test]
    fn test_in_memory_store_add_overwrites_in_place_preserving_position() {
        let mut store = InMemoryStore::new();
        store.add(MemoryEntry {
            id: "a".to_string(),
            content: "first".to_string(),
            ..Default::default()
        });
        store.add(MemoryEntry {
            id: "b".to_string(),
            content: "second".to_string(),
            ..Default::default()
        });
        store.add(MemoryEntry {
            id: "a".to_string(),
            content: "first-updated".to_string(),
            ..Default::default()
        });
        let all = store.list_all();
        assert_eq!(all[0].content, "first-updated");
        assert_eq!(all[1].content, "second");
    }

    #[test]
    fn test_in_memory_store_search_empty_store_returns_empty() {
        let store = InMemoryStore::new();
        assert!(store.search("anything", 5).is_empty());
    }

    #[test]
    fn test_in_memory_store_search_ranks_by_keyword_overlap() {
        let mut store = InMemoryStore::new();
        store.add(MemoryEntry::new("User's name is Alice"));
        store.add(MemoryEntry::new("User prefers Python over JavaScript"));
        store.add(MemoryEntry::new("Completely unrelated sentence"));

        let results = store.search("What language does the user like", 5);
        assert!(!results.is_empty());
        assert!(results[0].content.contains("Python"));
    }

    #[test]
    fn test_in_memory_store_search_excludes_zero_score_entries() {
        let mut store = InMemoryStore::new();
        store.add(MemoryEntry::new("apple banana"));
        store.add(MemoryEntry::new("completely different words"));

        let results = store.search("apple banana", 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "apple banana");
    }

    #[test]
    fn test_in_memory_store_search_respects_top_k() {
        let mut store = InMemoryStore::new();
        for i in 0..5 {
            store.add(MemoryEntry::new(format!("shared word {i}")));
        }
        let results = store.search("shared word", 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_in_memory_store_delete_removes_and_reports() {
        let mut store = InMemoryStore::new();
        let id = store.add(MemoryEntry::new("hello"));
        assert!(store.delete(&id));
        assert!(!store.delete(&id));
        assert!(store.list_all().is_empty());
    }

    #[test]
    fn test_in_memory_store_clear_removes_everything() {
        let mut store = InMemoryStore::new();
        store.add(MemoryEntry::new("one"));
        store.add(MemoryEntry::new("two"));
        store.clear();
        assert!(store.list_all().is_empty());
    }

    #[test]
    fn test_semantic_memory_default_settings() {
        let memory = SemanticMemory::new();
        assert_eq!(memory.max_results, 5);
        assert!(memory.session_id.is_none());
    }

    #[test]
    fn test_semantic_memory_add_and_search_round_trip() {
        let mut memory = SemanticMemory::new();
        memory.add("User's name is Alice", None);
        memory.add("User prefers Python over JavaScript", None);

        let results = memory.search("What language does the user like?", None);
        assert!(!results.is_empty());
        assert!(results[0].contains("Python"));
    }

    #[test]
    fn test_semantic_memory_add_stamps_session_id_into_metadata() {
        let mut memory = SemanticMemory::new().with_session_id("sess-1");
        memory.add("hello", None);
        let entries = memory.list_all();
        assert_eq!(
            entries[0].metadata.get("session_id"),
            Some(&Value::String("sess-1".to_string()))
        );
    }

    #[test]
    fn test_semantic_memory_add_without_session_id_omits_metadata_key() {
        let mut memory = SemanticMemory::new();
        memory.add("hello", None);
        assert!(!memory.list_all()[0].metadata.contains_key("session_id"));
    }

    #[test]
    fn test_semantic_memory_search_top_k_overrides_max_results() {
        let mut memory = SemanticMemory::new().with_max_results(1);
        for i in 0..5 {
            memory.add(format!("shared word {i}"), None);
        }
        assert_eq!(memory.search("shared word", None).len(), 1);
        assert_eq!(memory.search("shared word", Some(3)).len(), 3);
    }

    #[test]
    fn test_semantic_memory_delete_and_clear() {
        let mut memory = SemanticMemory::new();
        let id = memory.add("hello", None);
        assert!(memory.delete(&id));
        memory.add("one", None);
        memory.add("two", None);
        memory.clear();
        assert!(memory.list_all().is_empty());
    }

    #[test]
    fn test_semantic_memory_get_context_empty_when_no_matches() {
        let memory = SemanticMemory::new();
        assert_eq!(memory.get_context("anything"), "");
    }

    #[test]
    fn test_semantic_memory_get_context_formats_numbered_lines() {
        let mut memory = SemanticMemory::new();
        memory.add("shared apple", None);
        memory.add("shared banana", None);
        let context = memory.get_context("shared");
        assert!(context.starts_with("Relevant context from memory:\n  1. "));
        assert!(context.contains("2. "));
    }

    #[test]
    fn test_semantic_memory_debug_impl_reports_entry_count() {
        let mut memory = SemanticMemory::new();
        memory.add("hello", None);
        let debug_str = format!("{memory:?}");
        assert!(debug_str.contains("entries: 1"));
        assert!(debug_str.contains("max_results: 5"));
    }

    #[test]
    fn test_semantic_memory_with_custom_store() {
        let mut memory = SemanticMemory::new().with_store(Box::new(InMemoryStore::new()));
        memory.add("hello", None);
        assert_eq!(memory.list_all().len(), 1);
    }
}
