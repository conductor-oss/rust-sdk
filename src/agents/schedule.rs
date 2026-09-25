// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

// Cron schedules for agents, wired through `super::AgentRuntime::deploy_with_schedules`.
//
// `Schedule` is what a caller declares; `ScheduleInfo` is what the server reports back.
// `reconcile` applies the tri-state semantics `deploy_with_schedules` documents.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::client::SchedulerClient;
use crate::error::{ConductorError, Result};
use crate::models::{SaveScheduleRequest, WorkflowSchedule};

/// One cron trigger to attach to an agent, as declared by a caller of
/// [`super::AgentRuntime::deploy_with_schedules`].
///
/// `name` is a short identifier, unique per agent -- the wire-level schedule name the server
/// actually stores is `"{agent_name}-{name}"` (see [`wire_name`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    /// Short identifier, unique per agent.
    pub name: String,
    /// Cron expression (5- or 6-field; the server validates the exact grammar).
    pub cron: String,
    /// IANA timezone id. Defaults to `"UTC"`.
    pub timezone: String,
    /// Workflow input passed when the cron fires.
    pub input: HashMap<String, Value>,
    /// Replay missed fires on resume.
    pub catchup: bool,
    /// Start in paused state.
    pub paused: bool,
    /// Active window start (epoch ms).
    pub start_at: Option<i64>,
    /// Active window end (epoch ms).
    pub end_at: Option<i64>,
    /// Human-readable note.
    pub description: Option<String>,
}

impl Schedule {
    /// Create a new schedule. `name` and `cron` are required and validated immediately; every
    /// other field defaults and is set via the `with_*` builders below.
    ///
    /// # Errors
    ///
    /// Returns [`ConductorError::Agent`] if `name` or `cron` is empty/whitespace-only.
    pub fn new(name: impl Into<String>, cron: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let cron = cron.into();
        if name.trim().is_empty() {
            return Err(ConductorError::agent("Schedule.name must be non-empty"));
        }
        if cron.trim().is_empty() {
            return Err(ConductorError::agent("Schedule.cron must be non-empty"));
        }
        Ok(Self {
            name,
            cron,
            timezone: "UTC".to_owned(),
            input: HashMap::new(),
            catchup: false,
            paused: false,
            start_at: None,
            end_at: None,
            description: None,
        })
    }

    /// Set the IANA timezone id.
    #[must_use]
    pub fn with_timezone(mut self, timezone: impl Into<String>) -> Self {
        self.timezone = timezone.into();
        self
    }

    /// Set the workflow input passed when the cron fires.
    #[must_use]
    pub fn with_input(mut self, input: HashMap<String, Value>) -> Self {
        self.input = input;
        self
    }

    /// Replay missed fires on resume.
    #[must_use]
    pub fn with_catchup(mut self, catchup: bool) -> Self {
        self.catchup = catchup;
        self
    }

    /// Start in paused state.
    #[must_use]
    pub fn with_paused(mut self, paused: bool) -> Self {
        self.paused = paused;
        self
    }

    /// Set the active window start (epoch ms).
    #[must_use]
    pub fn with_start_at(mut self, start_at: i64) -> Self {
        self.start_at = Some(start_at);
        self
    }

    /// Set the active window end (epoch ms).
    #[must_use]
    pub fn with_end_at(mut self, end_at: i64) -> Self {
        self.end_at = Some(end_at);
        self
    }

    /// Set a human-readable description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    // Checks that `start_at < end_at` when both are set.
    fn validate(&self) -> Result<()> {
        if let (Some(start), Some(end)) = (self.start_at, self.end_at) {
            if start >= end {
                return Err(ConductorError::agent(format!(
                    "Schedule '{}': start_at ({start}) must be < end_at ({end})",
                    self.name
                )));
            }
        }
        Ok(())
    }
}

/// Server-reported view of one agent's schedule, as returned by [`list_schedules`].
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleInfo {
    /// Wire name (prefixed with `"{agent}-"`).
    pub name: String,
    /// User-supplied short name (the part after the `"{agent}-"` prefix).
    pub short_name: String,
    /// Agent (workflow) name this schedule fires.
    pub agent: String,
    /// Cron expression.
    pub cron: String,
    /// IANA timezone id.
    pub timezone: String,
    /// Workflow input passed when the cron fires.
    pub input: HashMap<String, Value>,
    /// Whether the schedule is currently paused.
    pub paused: bool,
    /// Why the schedule is paused, if the server recorded a reason.
    pub paused_reason: Option<String>,
    /// Whether missed fires are replayed on resume.
    pub catchup: bool,
    /// Active window start (epoch ms).
    pub start_at: Option<i64>,
    /// Active window end (epoch ms).
    pub end_at: Option<i64>,
    /// Human-readable description.
    pub description: Option<String>,
    /// Next scheduled run time (epoch ms), if computed.
    pub next_run: Option<i64>,
    /// Create time (epoch ms).
    pub create_time: Option<i64>,
    /// Last update time (epoch ms).
    pub update_time: Option<i64>,
    /// Who created the schedule.
    pub created_by: Option<String>,
    /// Who last updated the schedule.
    pub updated_by: Option<String>,
}

impl ScheduleInfo {
    /// Build a [`ScheduleInfo`] from the server's [`WorkflowSchedule`] shape.
    #[must_use]
    pub fn from_workflow_schedule(ws: &WorkflowSchedule, agent_name: &str) -> Self {
        let input = ws
            .start_workflow_request
            .as_ref()
            .map(|swr| swr.input.clone())
            .unwrap_or_default();

        Self {
            short_name: unprefix(agent_name, &ws.name),
            name: ws.name.clone(),
            agent: agent_name.to_owned(),
            cron: ws.cron_expression.clone(),
            timezone: ws.zone_id.clone().unwrap_or_else(|| "UTC".to_owned()),
            input,
            paused: ws.paused,
            paused_reason: ws.paused_reason.clone(),
            catchup: ws.run_catchup_schedule_instances,
            start_at: ws.schedule_start_time,
            end_at: ws.schedule_end_time,
            description: ws.description.clone(),
            next_run: ws.next_run_time,
            create_time: ws.create_time,
            update_time: ws.updated_time,
            created_by: ws.created_by.clone(),
            updated_by: ws.updated_by.clone(),
        }
    }
}

/// The wire-level schedule name the server stores: `"{agent_name}-{short_name}"`.
///
/// Known, non-blocking collision: this plain `-`-join means two different `(agent_name,
/// short_name)` pairs can produce the same wire name if either contains a `-` at the right
/// spot (e.g. agent `"foo"` + schedule `"bar-baz"` collides with agent `"foo-bar"` + schedule
/// `"baz"`). This matches the wire format every Conductor SDK already uses, so it isn't fixable
/// on the Rust side alone without breaking cross-SDK compatibility.
#[must_use]
pub fn wire_name(agent_name: &str, short_name: &str) -> String {
    format!("{agent_name}-{short_name}")
}

fn unprefix(agent_name: &str, wire_name: &str) -> String {
    let prefix = format!("{agent_name}-");
    wire_name
        .strip_prefix(&prefix)
        .unwrap_or(wire_name)
        .to_owned()
}

/// List the schedules currently registered for `agent_name`.
///
/// # Errors
///
/// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
pub async fn list_schedules(
    scheduler: &SchedulerClient,
    agent_name: &str,
) -> Result<Vec<ScheduleInfo>> {
    let schedules = scheduler.get_all_schedules(Some(agent_name)).await?;
    Ok(schedules
        .iter()
        .map(|ws| ScheduleInfo::from_workflow_schedule(ws, agent_name))
        .collect())
}

// Apply the declarative tri-state semantics `super::AgentRuntime::deploy_with_schedules`
// documents:
//
// - `desired` is `None`: no-op, existing schedules for `agent_name` are left untouched.
// - `desired` is `Some(&[])`: every schedule for `agent_name` is deleted.
// - `desired` is `Some(non-empty)`: each listed schedule is upserted; any existing schedule for
//   `agent_name` *not* in the list is deleted (pruned).
//
// Errors: returns `ConductorError::Agent` if `desired` has duplicate `Schedule::name`s, if any
// schedule fails its own `Schedule::validate` check, or any of the transport/server errors
// `SchedulerClient::save_schedule`/`SchedulerClient::delete_schedule`/
// `SchedulerClient::get_all_schedules` return.
pub async fn reconcile(
    scheduler: &SchedulerClient,
    agent_name: &str,
    desired: Option<&[Schedule]>,
) -> Result<()> {
    let Some(desired) = desired else {
        return Ok(());
    };

    let mut seen = HashSet::with_capacity(desired.len());
    for schedule in desired {
        schedule.validate()?;
        if !seen.insert(schedule.name.as_str()) {
            return Err(ConductorError::agent(format!(
                "Duplicate schedule name '{}' -- names must be unique per agent",
                schedule.name
            )));
        }
    }

    let existing = list_schedules(scheduler, agent_name).await?;
    let desired_short: HashSet<&str> = desired.iter().map(|s| s.name.as_str()).collect();

    // Save every desired schedule before deleting anything pruned: `save_schedule` is an
    // upsert, so if a transient error aborts this partway through, the worst case is some
    // stale schedules are left un-pruned -- saving after deleting instead could abort with
    // every existing schedule already deleted and none of the desired ones created yet.
    for schedule in desired {
        let request = to_save_request(schedule, agent_name);
        scheduler.save_schedule(&request).await?;
    }

    for info in &existing {
        if !desired_short.contains(info.short_name.as_str()) {
            scheduler.delete_schedule(&info.name).await?;
        }
    }

    Ok(())
}

fn to_save_request(schedule: &Schedule, agent_name: &str) -> SaveScheduleRequest {
    let mut request = SaveScheduleRequest::new(
        wire_name(agent_name, &schedule.name),
        schedule.cron.clone(),
        agent_name,
    )
    .with_timezone(schedule.timezone.clone())
    .with_input(schedule.input.clone())
    .paused(schedule.paused)
    .with_catchup(schedule.catchup);

    if let Some(start_at) = schedule.start_at {
        request = request.with_start_time(start_at);
    }
    if let Some(end_at) = schedule.end_at {
        request = request.with_end_time(end_at);
    }
    if let Some(description) = &schedule.description {
        request = request.with_description(description.clone());
    }

    request
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schedule_new_rejects_empty_name_or_cron() {
        Schedule::new("", "0 0 * * * *").unwrap_err();
        Schedule::new("daily", "").unwrap_err();
        Schedule::new("daily", "0 0 * * * *").unwrap();
    }

    #[test]
    fn test_schedule_validate_rejects_start_at_after_end_at() {
        let schedule = Schedule::new("daily", "0 0 * * * *")
            .unwrap()
            .with_start_at(100)
            .with_end_at(50);
        schedule.validate().unwrap_err();
    }

    #[test]
    fn test_schedule_validate_allows_start_at_before_end_at() {
        let schedule = Schedule::new("daily", "0 0 * * * *")
            .unwrap()
            .with_start_at(50)
            .with_end_at(100);
        schedule.validate().unwrap();
    }

    #[test]
    fn test_wire_name_and_unprefix_round_trip() {
        let wire = wire_name("my-agent", "daily");
        assert_eq!(wire, "my-agent-daily");
        assert_eq!(unprefix("my-agent", &wire), "daily");
    }

    #[test]
    fn test_unprefix_leaves_non_matching_name_untouched() {
        assert_eq!(
            unprefix("my-agent", "someone-elses-schedule"),
            "someone-elses-schedule"
        );
    }

    #[test]
    fn test_to_save_request_maps_fields() {
        let schedule = Schedule::new("daily", "0 0 * * * *")
            .unwrap()
            .with_timezone("America/New_York")
            .with_catchup(true)
            .with_paused(true)
            .with_description("daily digest");

        let request = to_save_request(&schedule, "digest-agent");

        assert_eq!(request.name, "digest-agent-daily");
        assert_eq!(request.cron_expression, "0 0 * * * *");
        assert_eq!(request.start_workflow_request.name, "digest-agent");
        assert_eq!(request.zone_id, Some("America/New_York".to_owned()));
        assert!(request.run_catchup_schedule_instances);
        assert!(request.paused);
        assert_eq!(request.description, Some("daily digest".to_owned()));
    }

    #[test]
    fn test_schedule_info_from_workflow_schedule_strips_prefix() {
        let ws = WorkflowSchedule {
            name: "digest-agent-daily".to_owned(),
            cron_expression: "0 0 * * * *".to_owned(),
            paused: true,
            paused_reason: Some("manually paused".to_owned()),
            ..WorkflowSchedule::default()
        };

        let info = ScheduleInfo::from_workflow_schedule(&ws, "digest-agent");

        assert_eq!(info.short_name, "daily");
        assert_eq!(info.agent, "digest-agent");
        assert!(info.paused);
        assert_eq!(info.paused_reason, Some("manually paused".to_owned()));
    }
}
