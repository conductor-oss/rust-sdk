// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

// Worker stall detection for super::AgentHandle::join.
//
// Scans a workflow for SCHEDULED tasks with zero polls that have sat past a stall threshold.
// The check is workflow-scoped: a positive is a reliable signal that some worker is missing,
// but it can't pin the stall to a specific local worker.

use std::collections::HashSet;

use crate::error::StalledTaskInfo;
use crate::models::{TaskStatus, Workflow};

/// What [`super::AgentHandle::join`] does when it detects a new stall.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StallPolicy {
    /// Log a `tracing::warn!` and keep waiting -- the default. A caller-supplied `timeout` on
    /// `join()`, if any, still applies.
    #[default]
    Warn,
    /// Return `Err(`[`crate::error::ConductorError::WorkerStall`]`)` on the first newly-detected
    /// stall, instead of continuing to wait.
    Raise,
}

// Scan workflow's tasks for ones stuck SCHEDULED with no poller for at least
// stall_seconds, skipping any task_id already in seen. Newly-found stalls are added to
// seen before returning.
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
