// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::{ConductorError, Result};
use crate::http::{ApiClient, ApiPath};
use crate::models::{
    MetadataTag, SaveScheduleRequest, SearchResultWorkflowScheduleExecution, WorkflowSchedule,
};

/// Client for managing workflow schedules
#[derive(Clone)]
pub struct SchedulerClient {
    api: ApiClient,
}

impl SchedulerClient {
    /// Create a new scheduler client
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    /// Save (create or update) a schedule
    pub async fn save_schedule(&self, request: &SaveScheduleRequest) -> Result<()> {
        self.api
            .post_no_response("/scheduler/schedules", request)
            .await
    }

    /// Get a schedule by name
    pub async fn get_schedule(&self, name: &str) -> Result<WorkflowSchedule> {
        let path = format!("/scheduler/schedules/{}", name);
        self.api
            .get(ApiPath::templated(&path, "/scheduler/schedules/{name}"))
            .await
    }

    /// Get all schedules, optionally filtered by workflow name
    pub async fn get_all_schedules(
        &self,
        workflow_name: Option<&str>,
    ) -> Result<Vec<WorkflowSchedule>> {
        if let Some(wf_name) = workflow_name {
            self.api
                .get_with_params("/scheduler/schedules", &[("workflowName", wf_name)])
                .await
        } else {
            self.api.get("/scheduler/schedules").await
        }
    }

    /// Get next few schedule execution times for a cron expression
    pub async fn get_next_few_schedule_execution_times(
        &self,
        cron_expression: &str,
        schedule_start_time: Option<i64>,
        schedule_end_time: Option<i64>,
        limit: Option<i32>,
    ) -> Result<Vec<i64>> {
        let mut params: Vec<(&str, String)> = vec![("cronExpression", cron_expression.to_string())];

        if let Some(start) = schedule_start_time {
            params.push(("scheduleStartTime", start.to_string()));
        }
        if let Some(end) = schedule_end_time {
            params.push(("scheduleEndTime", end.to_string()));
        }
        if let Some(l) = limit {
            params.push(("limit", l.to_string()));
        }

        let params_ref: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.api
            .get_with_params("/scheduler/nextFewSchedules", &params_ref)
            .await
    }

    /// Delete a schedule
    pub async fn delete_schedule(&self, name: &str) -> Result<()> {
        let path = format!("/scheduler/schedules/{}", name);
        self.api
            .delete_no_content(ApiPath::templated(&path, "/scheduler/schedules/{name}"))
            .await
    }

    /// Pause a schedule
    ///
    /// Per-schedule pause/resume is `PUT`-mapped on OSS Conductor
    /// (`scheduler/core/.../rest/SchedulerResource.java` maps only `@PutMapping`).
    /// Orkes Conductor accepts both `GET` and `PUT` as of the dual
    /// `@RequestMapping(method = {GET, PUT})` added in 2026-07; deployments older
    /// than that are `GET`-only. So PUT is tried first and a `405` falls back to
    /// GET, which covers every server family without an extra probe.
    pub async fn pause_schedule(&self, name: &str) -> Result<()> {
        let path = format!("/scheduler/schedules/{}/pause", name);
        let template = "/scheduler/schedules/{name}/pause";
        match self
            .api
            .put_no_body(ApiPath::templated(&path, template))
            .await
        {
            Err(ConductorError::Server { status: 405, .. }) => {
                self.api
                    .get_no_response(ApiPath::templated(&path, template))
                    .await
            }
            result => result,
        }
    }

    /// Pause all schedules
    ///
    /// `GET`-mapped on both server families (admin/debug endpoint), unlike the
    /// per-schedule pause/resume calls above -- no PUT is ever sent here.
    pub async fn pause_all_schedules(&self) -> Result<()> {
        self.api.get_no_response("/scheduler/admin/pause").await
    }

    /// Resume a schedule
    ///
    /// See [`Self::pause_schedule`] for the PUT-with-GET-fallback rationale.
    pub async fn resume_schedule(&self, name: &str) -> Result<()> {
        let path = format!("/scheduler/schedules/{}/resume", name);
        let template = "/scheduler/schedules/{name}/resume";
        match self
            .api
            .put_no_body(ApiPath::templated(&path, template))
            .await
        {
            Err(ConductorError::Server { status: 405, .. }) => {
                self.api
                    .get_no_response(ApiPath::templated(&path, template))
                    .await
            }
            result => result,
        }
    }

    /// Resume all schedules
    ///
    /// `GET`-mapped on both server families (admin/debug endpoint), unlike the
    /// per-schedule pause/resume calls above -- no PUT is ever sent here.
    pub async fn resume_all_schedules(&self) -> Result<()> {
        self.api.get_no_response("/scheduler/admin/resume").await
    }

    /// Search schedule executions
    pub async fn search_schedule_executions(
        &self,
        start: Option<i32>,
        size: Option<i32>,
        sort: Option<&str>,
        free_text: Option<&str>,
        query: Option<&str>,
    ) -> Result<SearchResultWorkflowScheduleExecution> {
        let mut params: Vec<(&str, String)> = Vec::new();

        if let Some(s) = start {
            params.push(("start", s.to_string()));
        }
        if let Some(s) = size {
            params.push(("size", s.to_string()));
        }
        if let Some(s) = sort {
            params.push(("sort", s.to_string()));
        }
        if let Some(ft) = free_text {
            params.push(("freeText", ft.to_string()));
        }
        if let Some(q) = query {
            params.push(("query", q.to_string()));
        }

        let params_ref: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.api
            .get_with_params("/scheduler/search/executions", &params_ref)
            .await
    }

    /// Requeue all execution records
    ///
    /// `GET`-mapped on both server families (admin/debug endpoint), unlike the
    /// per-schedule pause/resume calls above -- no PUT is ever sent here.
    pub async fn requeue_all_execution_records(&self) -> Result<()> {
        self.api.get_no_response("/scheduler/admin/requeue").await
    }

    /// Set tags for a schedule
    pub async fn set_scheduler_tags(&self, tags: &[MetadataTag], name: &str) -> Result<()> {
        let path = format!("/scheduler/schedules/{}/tags", name);
        self.api
            .put_no_response(
                ApiPath::templated(&path, "/scheduler/schedules/{name}/tags"),
                tags,
            )
            .await
    }

    /// Get tags for a schedule
    pub async fn get_scheduler_tags(&self, name: &str) -> Result<Vec<MetadataTag>> {
        let path = format!("/scheduler/schedules/{}/tags", name);
        self.api
            .get(ApiPath::templated(
                &path,
                "/scheduler/schedules/{name}/tags",
            ))
            .await
    }

    /// Delete tags from a schedule
    pub async fn delete_scheduler_tags(&self, tags: &[MetadataTag], name: &str) -> Result<()> {
        let path = format!("/scheduler/schedules/{}/tags", name);
        self.api
            .delete_with_body(
                ApiPath::templated(&path, "/scheduler/schedules/{name}/tags"),
                tags,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Configuration;

    #[test]
    fn test_scheduler_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let _client = SchedulerClient::new(api);
    }
}
