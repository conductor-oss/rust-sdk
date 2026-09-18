// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Worker stall detection for [`super::AgentHandle::join`] -- ports the server-side half of
//! python-sdk's `runtime/_liveness.py` (`ServerLivenessMonitor`).
//!
//! ## What's ported, and what isn't
//!
//! Python's `ServerLivenessMonitor` watches for `SCHEDULED` tasks with `pollCount == 0` *in the
//! execution's own worker domain* -- each stateful agent execution gets a random per-execution
//! domain (`run_id`) so its local tool workers only ever see tasks meant for that specific
//! execution. This crate's `AgentRuntime`/`TaskHandler` has no equivalent per-execution domain
//! concept at all: `AgentRuntime::serve`/`register_agent_workers` registers workers keyed only
//! by task-type name, shared across every concurrent execution of the same agent. There is
//! therefore no "our domain" to scope the check to here.
//!
//! What this module ports instead: a workflow-scoped check -- any `SCHEDULED` task in *this
//! execution's* workflow that's been queued past the stall threshold with zero polls. This is
//! strictly more general than python's domain-scoped check (it also catches a stall in a task
//! this handle's own runtime was never going to serve in the first place), so a positive here
//! is a reliable signal that *some* worker is missing, even though it can't always say the
//! stall is specifically about *your* local tool workers the way python's can.
//!
//! `LocalLivenessCheck` (verifying a registered worker's subprocess is alive right after
//! registration) and `WorkerRestarter` (SIGKILL + let a process supervisor respawn) aren't
//! ported here at all -- both depend on python's one-OS-process-per-worker model, which this
//! crate's tokio-task-per-worker model has no equivalent of. See `docs/agents/README.md`'s
//! Wave 8 for the follow-up items tracking those separately.

use std::collections::HashSet;

use crate::error::StalledTaskInfo;
use crate::models::{TaskStatus, Workflow};

/// What [`super::AgentHandle::join`] does when it detects a new stall.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StallPolicy {
    /// Log a `tracing::warn!` and keep waiting -- the default. A stall is a strong signal
    /// something is wrong, but `join()` staying up gives a caller-supplied `timeout` (if any)
    /// the chance to be the thing that actually gives up.
    #[default]
    Warn,
    /// Return `Err(`[`crate::error::ConductorError::WorkerStall`]`)` immediately on the first
    /// newly-detected stall, instead of continuing to wait.
    Raise,
}

/// Scan `workflow`'s tasks for ones stuck `SCHEDULED` with no poller for at least
/// `stall_seconds`, skipping any `task_id` already present in `seen` (so a stall already
/// reported once isn't reported again on the next tick). Newly-found stalls are added to
/// `seen` before returning.
pub(super) fn find_new_stalls(
    workflow: &Workflow,
    stall_seconds: f64,
    now_millis: i64,
    seen: &mut HashSet<String>,
) -> Vec<StalledTaskInfo> {
    let threshold_millis = (stall_seconds * 1000.0) as i64;
    let mut found = Vec::new();

    for task in &workflow.tasks {
        if task.status != TaskStatus::Scheduled || task.poll_count != 0 {
            continue;
        }
        if seen.contains(&task.task_id) {
            continue;
        }
        let queued_millis = now_millis - task.scheduled_time;
        if queued_millis < threshold_millis {
            continue;
        }
        found.push(StalledTaskInfo {
            task_def_name: task.task_def_name.clone(),
            task_id: task.task_id.clone(),
            seconds_queued: queued_millis as f64 / 1000.0,
        });
        seen.insert(task.task_id.clone());
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Task;

    fn now_millis() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    fn scheduled_task(task_id: &str, task_def_name: &str, queued_for_millis: i64) -> Task {
        Task {
            task_id: task_id.to_owned(),
            task_def_name: task_def_name.to_owned(),
            status: TaskStatus::Scheduled,
            poll_count: 0,
            scheduled_time: now_millis() - queued_for_millis,
            ..Default::default()
        }
    }

    #[test]
    fn test_no_stalls_below_threshold() {
        let workflow = Workflow {
            tasks: vec![scheduled_task("t1", "my_task", 5_000)],
            ..Default::default()
        };
        let mut seen = HashSet::new();
        let stalls = find_new_stalls(&workflow, 30.0, now_millis(), &mut seen);
        assert!(stalls.is_empty());
    }

    #[test]
    fn test_detects_a_stall_past_threshold() {
        let workflow = Workflow {
            tasks: vec![scheduled_task("t1", "my_task", 45_000)],
            ..Default::default()
        };
        let mut seen = HashSet::new();
        let stalls = find_new_stalls(&workflow, 30.0, now_millis(), &mut seen);
        assert_eq!(stalls.len(), 1);
        assert_eq!(stalls[0].task_id, "t1");
        assert_eq!(stalls[0].task_def_name, "my_task");
        assert!(stalls[0].seconds_queued >= 44.0);
    }

    #[test]
    fn test_ignores_tasks_that_have_been_polled() {
        let mut task = scheduled_task("t1", "my_task", 45_000);
        task.poll_count = 1;
        let workflow = Workflow {
            tasks: vec![task],
            ..Default::default()
        };
        let mut seen = HashSet::new();
        let stalls = find_new_stalls(&workflow, 30.0, now_millis(), &mut seen);
        assert!(stalls.is_empty());
    }

    #[test]
    fn test_ignores_non_scheduled_tasks() {
        let mut task = scheduled_task("t1", "my_task", 45_000);
        task.status = TaskStatus::InProgress;
        let workflow = Workflow {
            tasks: vec![task],
            ..Default::default()
        };
        let mut seen = HashSet::new();
        let stalls = find_new_stalls(&workflow, 30.0, now_millis(), &mut seen);
        assert!(stalls.is_empty());
    }

    #[test]
    fn test_a_stall_is_only_reported_once() {
        let workflow = Workflow {
            tasks: vec![scheduled_task("t1", "my_task", 45_000)],
            ..Default::default()
        };
        let mut seen = HashSet::new();
        let first = find_new_stalls(&workflow, 30.0, now_millis(), &mut seen);
        assert_eq!(first.len(), 1);
        let second = find_new_stalls(&workflow, 30.0, now_millis(), &mut seen);
        assert!(second.is_empty());
    }
}
