// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{ConductorError, Result};

use super::def::AgentDef;
use super::serializer::AgentConfigSerializer;
use super::tool::ToolDef;

/// Evaluation context for a [`GraphConditionFn`] — the state-so-far a conditional edge's
/// predicate reads to pick the next node.
///
/// Deliberately **not** a reuse of any existing crate type (not [`super::swarm::SwarmContext`],
/// not [`super::memory::ConversationMemory`]): those model different niches (rule-based
/// agent-to-agent handoff context, conversation history) and forcing this into either would carry
/// fields a graph predicate has no use for. `accumulated` is a plain bag of whatever prior nodes
/// chose to publish; `last_output` is the most recently produced node's raw output, kept separate
/// since "what did the node I just left produce" is the single most common thing a routing
/// predicate needs and forcing callers to know that node's name to look it up in `accumulated`
/// would be needless ceremony.
///
/// **Out of scope / deferred**: shared-state-across-nodes reducers (LangGraph's `StateGraph`
/// state-channel/reducer machinery, where each node's return value is merged into shared state via
/// a per-key reduce function) are not modeled here. `accumulated` is a plain last-write-wins map a
/// caller populates however it sees fit between node executions; there is no `AgentRuntime` yet to
/// drive that population automatically, and designing a reducer API ahead of that runtime existing
/// would be speculative. See `docs/agents/framework-support.md`'s `GraphAgentDef` entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphContext {
    /// Accumulated state published by nodes executed so far, keyed by whatever name the caller
    /// chose when publishing it. Last write wins — there is no merge/reduce step (see the
    /// deferred-reducers note above).
    pub accumulated: HashMap<String, Value>,
    /// Raw output of the most recently executed node, if any.
    pub last_output: Option<Value>,
}

/// Boxed synchronous predicate backing [`ConditionalGraphEdge`], returning the *name* of the
/// target node to route to next.
///
/// Modeled after [`super::swarm::SwarmConditionFn`] (`Arc`-boxed so a `ConditionalGraphEdge` stays
/// `Clone`, `Send + Sync` for the same cross-thread-usability reasons, synchronous for the same
/// "no I/O implied by the shape being ported" reasoning) with one deliberate signature change:
/// `SwarmConditionFn` returns `bool` because a `SwarmTransition` variant already carries its own
/// single fixed `target`, so the closure only ever needs to answer "does *this* transition fire."
/// A graph node, by contrast, may have edges to several differently-named targets from one
/// conditional branch point (LangGraph's `add_conditional_edges(source, path_fn, path_map)|`
/// shape); a `bool` predicate cannot select *among* them; it can only gate a single fixed target.
/// So this type returns the chosen target's `String` name directly instead.
pub type GraphConditionFn = Arc<dyn Fn(&GraphContext) -> String + Send + Sync>;

/// A single node in a [`GraphAgentDef`].
///
/// Three variants, mirroring the three things a LangGraph node concretely is in practice:
///
/// - [`GraphNode::Agent`]: an LLM/agent call — boxed like [`AgentDef::router`]/
///   [`AgentDef::planner`]'s sub-agent nesting, for the same reason (an `AgentDef` embedding
///   itself by value would be an infinitely-sized type).
/// - [`GraphNode::Tool`]: a tool/worker call, reusing [`ToolDef`] as-is rather than inventing a
///   parallel "graph tool node" shape.
/// - [`GraphNode::Human`]: a human-in-the-loop pause, carrying the prompt to show. This overlaps
///   in *purpose* with [`ToolType::Human`](super::tool::ToolType::Human) (both pause for human
///   input), but is kept as its own `GraphNode` variant rather than forced through
///   `GraphNode::Tool(ToolDef::human(...))`: a graph node's identity is "what kind of node is
///   this in the graph," and a bare prompt string is a lighter-weight, more direct way to express
///   "pause here and ask" than constructing a full `ToolDef` (with a name, description, and JSON
///   input schema it doesn't need) just to hold one string.
#[derive(Clone)]
pub enum GraphNode {
    /// An LLM/agent call node.
    Agent {
        /// Name identifying this node within the graph (must be unique — see
        /// [`GraphAgentDef::with_node`]).
        name: String,
        agent: Box<AgentDef>,
    },
    /// A tool/worker call node.
    Tool {
        /// Name identifying this node within the graph (must be unique — see
        /// [`GraphAgentDef::with_node`]).
        name: String,
        tool: Box<ToolDef>,
    },
    /// A human-in-the-loop pause node.
    Human {
        /// Name identifying this node within the graph (must be unique — see
        /// [`GraphAgentDef::with_node`]).
        name: String,
        /// The prompt shown to the human at this node.
        prompt: String,
    },
}

impl GraphNode {
    /// The node's name, common to every variant.
    pub fn name(&self) -> &str {
        match self {
            GraphNode::Agent { name, .. } => name,
            GraphNode::Tool { name, .. } => name,
            GraphNode::Human { name, .. } => name,
        }
    }
}

impl std::fmt::Debug for GraphNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphNode::Agent { name, agent } => f
                .debug_struct("Agent")
                .field("name", name)
                .field("agent", agent)
                .finish(),
            GraphNode::Tool { name, tool } => f
                .debug_struct("Tool")
                .field("name", name)
                .field("tool", tool)
                .finish(),
            GraphNode::Human { name, prompt } => f
                .debug_struct("Human")
                .field("name", name)
                .field("prompt", prompt)
                .finish(),
        }
    }
}

/// A static (unconditional) edge from `source` to `target`, both node names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

/// A conditional edge: from `source`, evaluate `condition` against the current [`GraphContext`]
/// to pick which of `targets` to route to next.
///
/// `targets` is the declared, closed set of node names `condition` is allowed to return — kept
/// explicit (rather than inferred from whatever string the closure happens to produce at runtime)
/// so [`GraphAgentDef::with_conditional_edge`] can validate every possible destination exists as
/// a real node at build time, the same fail-fast guarantee [`GraphEdge`] gets for free from
/// having only one target.
///
/// `Debug` is implemented by hand (rather than derived) for the same reason as
/// [`super::swarm::SwarmTransition`]'s `OnCondition` variant: `condition` holds an
/// `Arc<dyn Fn(..)>` trait object with no meaningful `Debug` impl of its own.
#[derive(Clone)]
pub struct ConditionalGraphEdge {
    pub source: String,
    pub targets: Vec<String>,
    pub condition: GraphConditionFn,
}

impl std::fmt::Debug for ConditionalGraphEdge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConditionalGraphEdge")
            .field("source", &self.source)
            .field("targets", &self.targets)
            .field("condition", &"<Fn>")
            .finish()
    }
}

/// Explicit-graph authoring type for the LangGraph-shaped "arbitrary DAG of nodes with per-node
/// LLM calls" niche.
///
/// **Read this before touching this type:**
///
/// - This serves *only* that one niche. It is not a general-purpose workflow DAG type, not a
///   replacement for [`AgentDef`]'s `Strategy`-based orchestration (`Sequential`/`Parallel`/
///   `Router`/etc.), and not intended to grow into one.
/// - It is **explicitly authored, not extracted**. Unlike a hypothetical adapter that reads
///   structure out of a compiled LangGraph `StateGraph`, there is no bytecode/closure
///   introspection here (Rust has no analog for that even if it were desired — see
///   `docs/agents/framework-support.md`'s Phase 2 entry) — a caller builds a `GraphAgentDef`
///   directly, node by node, edge by edge.
/// - **Wire compatibility with the Conductor server is an open follow-up, not an assumption.**
///   [`GraphAgentDef::serialize`] produces a plain, directly-structured JSON object (`name`,
///   `nodes`, `edges`, `conditionalEdges`) for inspection and testing. This has **not** been
///   verified against whatever shape the Conductor server actually expects for graph-based
///   execution — there is no server-side graph-execution contract this shape has been checked
///   against yet. Do not assume a server can execute this JSON as-is.
/// - This is **not** feature-complete parity with python-sdk's LangGraph support (which itself
///   works by extracting structure from an already-compiled external graph, a fundamentally
///   different mechanism than this explicitly-authored type).
/// - Shared-state-across-nodes (state reducers, LangGraph's per-key merge-function channels) is a
///   **deferred follow-up** — see [`GraphContext`]'s doc comment.
///
/// Construct via [`GraphAgentDef::new`], compose with consuming `with_*` builders (matching
/// [`AgentDef`]'s 100%-consuming-builder convention), and serialize with
/// [`GraphAgentDef::serialize`].
#[derive(Clone)]
pub struct GraphAgentDef {
    pub name: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub conditional_edges: Vec<ConditionalGraphEdge>,
}

impl std::fmt::Debug for GraphAgentDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphAgentDef")
            .field("name", &self.name)
            .field("nodes", &self.nodes)
            .field("edges", &self.edges)
            .field("conditional_edges", &self.conditional_edges)
            .finish()
    }
}

impl GraphAgentDef {
    /// Create a new, empty graph definition. Validates `name` against
    /// `^[a-zA-Z_][a-zA-Z0-9_-]*$` up front, mirroring [`AgentDef::new`]'s validation exactly
    /// (same rationale: the name doubles as the Conductor workflow name once compiled).
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if !is_valid_graph_name(&name) {
            return Err(ConductorError::agent(format!(
                "invalid graph name '{name}': must match ^[a-zA-Z_][a-zA-Z0-9_-]*$"
            )));
        }
        Ok(Self {
            name,
            nodes: Vec::new(),
            edges: Vec::new(),
            conditional_edges: Vec::new(),
        })
    }

    /// Add a node. Fails if its name collides with an already-added node's name (matches
    /// [`AgentDef::with_sub_agent`]'s duplicate-name check, moved to construction time).
    pub fn with_node(mut self, node: GraphNode) -> Result<Self> {
        if self.nodes.iter().any(|n| n.name() == node.name()) {
            return Err(ConductorError::agent(format!(
                "duplicate node name '{}': node names must be unique",
                node.name()
            )));
        }
        self.nodes.push(node);
        Ok(self)
    }

    /// Add a static edge from `source` to `target`. Fails fast if either name does not already
    /// name a node added via [`GraphAgentDef::with_node`] — mirroring
    /// [`AgentDef::with_sub_agent`]'s "reject the invalid state at build time" convention rather
    /// than allowing a dangling edge to only surface as a failure at execution time.
    pub fn with_edge(
        mut self,
        source: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<Self> {
        let source = source.into();
        let target = target.into();
        self.require_known_node(&source)?;
        self.require_known_node(&target)?;
        self.edges.push(GraphEdge { source, target });
        Ok(self)
    }

    /// Add a conditional edge from `source`, routing at runtime to one of `targets` via
    /// `condition`. Fails fast if `source` or any entry in `targets` does not already name a node
    /// added via [`GraphAgentDef::with_node`] — same rationale as [`GraphAgentDef::with_edge`].
    pub fn with_conditional_edge(
        mut self,
        source: impl Into<String>,
        targets: impl IntoIterator<Item = impl Into<String>>,
        condition: GraphConditionFn,
    ) -> Result<Self> {
        let source = source.into();
        let targets: Vec<String> = targets.into_iter().map(Into::into).collect();
        self.require_known_node(&source)?;
        for target in &targets {
            self.require_known_node(target)?;
        }
        self.conditional_edges.push(ConditionalGraphEdge {
            source,
            targets,
            condition,
        });
        Ok(self)
    }

    fn require_known_node(&self, name: &str) -> Result<()> {
        if !self.nodes.iter().any(|n| n.name() == name) {
            return Err(ConductorError::agent(format!(
                "unknown node '{name}': add it via with_node(...) before referencing it in an edge"
            )));
        }
        Ok(())
    }

    /// Serialize this graph to a plain JSON object: `name`, `nodes` (each tagged by kind), static
    /// `edges`, and `conditionalEdges`.
    ///
    /// This is **not** python-sdk's `_graph` prep/finish-split wire shape — no partial wire
    /// format is invented here (see this type's top-level doc comment). The LLM/agent node's
    /// nested [`AgentDef`] is serialized via the existing, already-public
    /// [`AgentConfigSerializer::serialize`] so this file requires no change to `serializer.rs`.
    pub fn serialize(&self) -> Value {
        let mut map = Map::new();
        map.insert("name".to_string(), Value::String(self.name.clone()));
        map.insert(
            "nodes".to_string(),
            Value::Array(self.nodes.iter().map(serialize_node).collect()),
        );
        map.insert(
            "edges".to_string(),
            Value::Array(self.edges.iter().map(serialize_edge).collect()),
        );
        map.insert(
            "conditionalEdges".to_string(),
            Value::Array(
                self.conditional_edges
                    .iter()
                    .map(serialize_conditional_edge)
                    .collect(),
            ),
        );
        Value::Object(map)
    }
}

fn serialize_node(node: &GraphNode) -> Value {
    let mut map = Map::new();
    map.insert("name".to_string(), Value::String(node.name().to_string()));
    match node {
        GraphNode::Agent { agent, .. } => {
            map.insert("kind".to_string(), Value::String("agent".to_string()));
            map.insert("agent".to_string(), AgentConfigSerializer::serialize(agent));
        }
        GraphNode::Tool { tool, .. } => {
            map.insert("kind".to_string(), Value::String("tool".to_string()));
            map.insert("tool".to_string(), Value::String(tool.name.clone()));
        }
        GraphNode::Human { prompt, .. } => {
            map.insert("kind".to_string(), Value::String("human".to_string()));
            map.insert("prompt".to_string(), Value::String(prompt.clone()));
        }
    }
    Value::Object(map)
}

fn serialize_edge(edge: &GraphEdge) -> Value {
    let mut map = Map::new();
    map.insert("source".to_string(), Value::String(edge.source.clone()));
    map.insert("target".to_string(), Value::String(edge.target.clone()));
    Value::Object(map)
}

fn serialize_conditional_edge(edge: &ConditionalGraphEdge) -> Value {
    let mut map = Map::new();
    map.insert("source".to_string(), Value::String(edge.source.clone()));
    map.insert(
        "targets".to_string(),
        Value::Array(edge.targets.iter().cloned().map(Value::String).collect()),
    );
    Value::Object(map)
}

fn is_valid_graph_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triage_agent() -> AgentDef {
        AgentDef::new("triage").unwrap().with_model("gpt-4")
    }

    #[test]
    fn test_new_validates_name() {
        assert!(GraphAgentDef::new("valid_name-1").is_ok());
        assert!(GraphAgentDef::new("1invalid").is_err());
        assert!(GraphAgentDef::new("in valid").is_err());
        assert!(GraphAgentDef::new("").is_err());
    }

    #[test]
    fn test_build_small_graph_with_static_and_conditional_edge() {
        let graph = GraphAgentDef::new("triage_graph")
            .unwrap()
            .with_node(GraphNode::Agent {
                name: "triage".into(),
                agent: Box::new(triage_agent()),
            })
            .unwrap()
            .with_node(GraphNode::Human {
                name: "ask_human".into(),
                prompt: "Please clarify".into(),
            })
            .unwrap()
            .with_node(GraphNode::Tool {
                name: "file_ticket".into(),
                tool: Box::new(ToolDef::human("file_ticket", "files a ticket")),
            })
            .unwrap()
            .with_edge("triage", "ask_human")
            .unwrap()
            .with_conditional_edge(
                "ask_human",
                vec!["file_ticket", "triage"],
                Arc::new(|ctx: &GraphContext| {
                    if ctx.last_output.is_some() {
                        "file_ticket".to_string()
                    } else {
                        "triage".to_string()
                    }
                }),
            )
            .unwrap();

        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.conditional_edges.len(), 1);
        assert_eq!(
            graph.conditional_edges[0].targets,
            vec!["file_ticket", "triage"]
        );
    }

    #[test]
    fn test_with_edge_rejects_unknown_target() {
        let graph = GraphAgentDef::new("g")
            .unwrap()
            .with_node(GraphNode::Human {
                name: "a".into(),
                prompt: "hi".into(),
            })
            .unwrap();
        assert!(graph.clone().with_edge("a", "does_not_exist").is_err());
        assert!(graph.with_edge("does_not_exist", "a").is_err());
    }

    #[test]
    fn test_with_conditional_edge_rejects_unknown_target() {
        let graph = GraphAgentDef::new("g")
            .unwrap()
            .with_node(GraphNode::Human {
                name: "a".into(),
                prompt: "hi".into(),
            })
            .unwrap();
        let result = graph.with_conditional_edge(
            "a",
            vec!["a", "missing"],
            Arc::new(|_: &GraphContext| "a".to_string()),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_with_node_rejects_duplicate_name() {
        let graph = GraphAgentDef::new("g")
            .unwrap()
            .with_node(GraphNode::Human {
                name: "a".into(),
                prompt: "hi".into(),
            })
            .unwrap();
        let dup = graph.with_node(GraphNode::Human {
            name: "a".into(),
            prompt: "bye".into(),
        });
        assert!(dup.is_err());
    }

    #[test]
    fn test_serialize_plain_json_shape() {
        let graph = GraphAgentDef::new("g")
            .unwrap()
            .with_node(GraphNode::Agent {
                name: "llm".into(),
                agent: Box::new(triage_agent()),
            })
            .unwrap()
            .with_node(GraphNode::Human {
                name: "human".into(),
                prompt: "clarify?".into(),
            })
            .unwrap()
            .with_edge("llm", "human")
            .unwrap()
            .with_conditional_edge(
                "human",
                vec!["llm"],
                Arc::new(|_: &GraphContext| "llm".to_string()),
            )
            .unwrap();

        let json = graph.serialize();
        let obj = json.as_object().unwrap();

        assert_eq!(obj.get("name"), Some(&Value::String("g".to_string())));

        let nodes = obj.get("nodes").unwrap().as_array().unwrap();
        assert_eq!(nodes.len(), 2);
        let llm_node = nodes[0].as_object().unwrap();
        assert_eq!(
            llm_node.get("name"),
            Some(&Value::String("llm".to_string()))
        );
        assert_eq!(
            llm_node.get("kind"),
            Some(&Value::String("agent".to_string()))
        );
        assert_eq!(
            llm_node
                .get("agent")
                .unwrap()
                .as_object()
                .unwrap()
                .get("name"),
            Some(&Value::String("triage".to_string()))
        );
        let human_node = nodes[1].as_object().unwrap();
        assert_eq!(
            human_node.get("kind"),
            Some(&Value::String("human".to_string()))
        );
        assert_eq!(
            human_node.get("prompt"),
            Some(&Value::String("clarify?".to_string()))
        );

        let edges = obj.get("edges").unwrap().as_array().unwrap();
        assert_eq!(
            edges[0],
            serde_json::json!({"source": "llm", "target": "human"})
        );

        let conditional_edges = obj.get("conditionalEdges").unwrap().as_array().unwrap();
        assert_eq!(
            conditional_edges[0],
            serde_json::json!({"source": "human", "targets": ["llm"]})
        );
    }

    #[test]
    fn test_debug_does_not_panic() {
        let graph = GraphAgentDef::new("g")
            .unwrap()
            .with_node(GraphNode::Human {
                name: "a".into(),
                prompt: "hi".into(),
            })
            .unwrap()
            .with_conditional_edge("a", vec!["a"], Arc::new(|_: &GraphContext| "a".to_string()))
            .unwrap();
        let debug_str = format!("{graph:?}");
        assert!(debug_str.contains("GraphAgentDef"));
        assert!(debug_str.contains("<Fn>"));
    }
}
