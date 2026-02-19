// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;
use crate::http::ApiClient;
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
        let path = "/scheduler/schedules";
        self.api.post_no_response(path, request).await
    }

    /// Get a schedule by name
    pub async fn get_schedule(&self, name: &str) -> Result<WorkflowSchedule> {
        let path = format!("/scheduler/schedules/{}", name);
        self.api.get(&path).await
    }

    /// Get all schedules, optionally filtered by workflow name
    pub async fn get_all_schedules(
        &self,
        workflow_name: Option<&str>,
    ) -> Result<Vec<WorkflowSchedule>> {
        let path = "/scheduler/schedules";
        if let Some(wf_name) = workflow_name {
            self.api
                .get_with_params(path, &[("workflowName", wf_name)])
                .await
        } else {
            self.api.get(path).await
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
        let path = "/scheduler/nextFewSchedules";
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
        self.api.get_with_params(path, &params_ref).await
    }

    /// Delete a schedule
    pub async fn delete_schedule(&self, name: &str) -> Result<()> {
        let path = format!("/scheduler/schedules/{}", name);
        self.api.delete_no_content(&path).await
    }

    /// Pause a schedule
    pub async fn pause_schedule(&self, name: &str) -> Result<()> {
        let path = format!("/scheduler/schedules/{}/pause", name);
        self.api.get_no_response(&path).await
    }

    /// Pause all schedules
    pub async fn pause_all_schedules(&self) -> Result<()> {
        let path = "/scheduler/admin/pause";
        self.api.get_no_response(path).await
    }

    /// Resume a schedule
    pub async fn resume_schedule(&self, name: &str) -> Result<()> {
        let path = format!("/scheduler/schedules/{}/resume", name);
        self.api.get_no_response(&path).await
    }

    /// Resume all schedules
    pub async fn resume_all_schedules(&self) -> Result<()> {
        let path = "/scheduler/admin/resume";
        self.api.get_no_response(path).await
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
        let path = "/scheduler/search/executions";
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
        self.api.get_with_params(path, &params_ref).await
    }

    /// Requeue all execution records
    pub async fn requeue_all_execution_records(&self) -> Result<()> {
        let path = "/scheduler/admin/requeue";
        self.api.get_no_response(path).await
    }

    /// Set tags for a schedule
    pub async fn set_scheduler_tags(&self, tags: &[MetadataTag], name: &str) -> Result<()> {
        let path = format!("/scheduler/schedules/{}/tags", name);
        self.api.put_no_response(&path, tags).await
    }

    /// Get tags for a schedule
    pub async fn get_scheduler_tags(&self, name: &str) -> Result<Vec<MetadataTag>> {
        let path = format!("/scheduler/schedules/{}/tags", name);
        self.api.get(&path).await
    }

    /// Delete tags from a schedule
    pub async fn delete_scheduler_tags(&self, tags: &[MetadataTag], name: &str) -> Result<()> {
        let path = format!("/scheduler/schedules/{}/tags", name);
        self.api.delete_with_body(&path, tags).await
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
