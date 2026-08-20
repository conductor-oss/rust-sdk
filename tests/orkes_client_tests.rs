// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.
//
// Scheduler and event-handler calls are OSS-compatible (confirmed
// empirically) and are asserted for real below. Secret reads are also
// OSS-compatible against an env-backed secret seeded in
// scripts/docker-compose-oss.yaml (see the Secret Client Tests section for
// details); secret writes, Prompt, and event-queue-configuration calls are
// not implemented by plain OSS Conductor (confirmed empirically: 404 "No
// static resource ..." for Prompt/event-queue-config, 501 read-only backend
// for secret writes) and skip/assert explicitly via `is_oss()` instead of
// silently swallowing the resulting error the way this file used to.

mod common;

use common::*;
use conductor::client::ConductorClient;
use conductor::error::ConductorError;
use conductor::models::SaveScheduleRequest;
use std::time::Duration;

// =============================================================================
// Scheduler Client Tests
//
// Scheduler is an OSS-compatible feature, so these run and assert
// unconditionally rather than gating on is_oss().
// =============================================================================

#[tokio::test]
async fn test_scheduler_save_and_get() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let scheduler = client.scheduler_client();

    let schedule_name = generate_unique_name("test_schedule");
    let workflow_name = generate_unique_workflow_name("schedule_wf");

    // Create a simple workflow for the schedule
    let metadata = client.metadata_client();
    let workflow_def = conductor::models::WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(conductor::models::WorkflowTask::wait("wait_ref"));
    metadata
        .register_workflow_def(&workflow_def)
        .await
        .expect("register_workflow_def should succeed");

    // Save Schedule using the builder methods
    let schedule_request = SaveScheduleRequest::new(&schedule_name, "0 0 0 * * ?", &workflow_name)
        .with_version(1)
        .paused(true); // Create paused so it doesn't run

    scheduler
        .save_schedule(&schedule_request)
        .await
        .expect("save_schedule should succeed");

    let schedule = scheduler
        .get_schedule(&schedule_name)
        .await
        .expect("get_schedule should succeed");
    assert_eq!(schedule.name, schedule_name);

    scheduler.delete_schedule(&schedule_name).await.ok();
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
}

#[tokio::test]
async fn test_scheduler_pause_resume() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let scheduler = client.scheduler_client();

    let schedule_name = generate_unique_name("test_schedule_pr");
    let workflow_name = generate_unique_workflow_name("schedule_wf_pr");

    // Create workflow
    let metadata = client.metadata_client();
    let workflow_def = conductor::models::WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(conductor::models::WorkflowTask::wait("wait_ref"));
    metadata
        .register_workflow_def(&workflow_def)
        .await
        .expect("register_workflow_def should succeed");

    // Save schedule
    let schedule_request = SaveScheduleRequest::new(&schedule_name, "0 0 0 * * ?", &workflow_name)
        .with_version(1)
        .paused(false);

    scheduler
        .save_schedule(&schedule_request)
        .await
        .expect("save_schedule should succeed");

    scheduler
        .pause_schedule(&schedule_name)
        .await
        .expect("pause_schedule should succeed");
    scheduler
        .resume_schedule(&schedule_name)
        .await
        .expect("resume_schedule should succeed");

    scheduler.delete_schedule(&schedule_name).await.ok();
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
}

#[tokio::test]
async fn test_scheduler_search_executions() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let scheduler = client.scheduler_client();

    // Search schedule executions
    let results = scheduler
        .search_schedule_executions(Some(0), Some(10), None, None, None)
        .await
        .expect("search_schedule_executions should succeed");
    assert!(results.total_hits >= 0);
}

#[tokio::test]
async fn test_scheduler_get_next_execution_times() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let scheduler = client.scheduler_client();

    // Conductor's scheduler parses cron via Quartz, which requires a seconds
    // field (6-7 parts); a plain 5-field Unix cron is rejected with "Invalid
    // cron expression" (confirmed empirically), regardless of server type.
    let times = scheduler
        .get_next_few_schedule_execution_times("0 0 0 * * ?", None, None, Some(5))
        .await
        .expect("get_next_few_schedule_execution_times should succeed");
    assert!(!times.is_empty());
    assert!(times.len() <= 5);
}

// =============================================================================
// Secret Client Tests
//
// OSS Conductor registers a full secrets CRUD controller by default (the
// `agentspan` module's `conductor.integrations.ai.enabled=true` default), but
// only ships read-only `SecretsDAO` backends: writes (put/delete) return a
// real 501 "read-only backend" rather than a 404. Reads work against an
// env-backed secret seeded via `CONDUCTOR_SECRET_RUST_SDK_INTEGRATION_TEST`
// in scripts/docker-compose-oss.yaml. OSS images old enough to predate this
// feature entirely still 404 on every call; the OSS branches below treat any
// error on the first read as "secrets API unavailable on this server" and
// skip with a clear message rather than assuming Enterprise-only.
//
// This is safe to test against with an unauthenticated OSS server: OSS
// Conductor has no authentication/authorization at all (see
// authorization_client_tests.rs), so an unauthenticated GET on
// /api/secrets/{key} doesn't change the threat model versus any other
// unauthenticated OSS endpoint. Only a dummy, non-sensitive value is seeded.
// =============================================================================

/// Name/value seeded via `CONDUCTOR_SECRET_RUST_SDK_INTEGRATION_TEST` in
/// scripts/docker-compose-oss.yaml -- keep these two in sync.
const OSS_SEEDED_SECRET_NAME: &str = "RUST_SDK_INTEGRATION_TEST";
const OSS_SEEDED_SECRET_VALUE: &str = "rust-sdk-oss-secret-value";

#[tokio::test]
async fn test_secret_put_and_get() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let secret = client.secret_client();

    if client.is_oss().await {
        let value = match secret.get_secret(OSS_SEEDED_SECRET_NAME).await {
            Ok(value) => value,
            Err(e) => {
                println!("Skipping: OSS secrets API unavailable on this server ({e:?})");
                return;
            }
        };
        assert_eq!(value, OSS_SEEDED_SECRET_VALUE);

        // The only bundled SecretsDAO backends (env-var, noop) are
        // read-only, so writes are expected to fail with a real 501. Accept
        // success too in case a future OSS release ships a writable backend.
        let throwaway_key = generate_unique_name("test_secret_put");
        match secret.put_secret(&throwaway_key, "value").await {
            Ok(()) => {
                secret.delete_secret(&throwaway_key).await.ok();
            }
            Err(ConductorError::Server { status: 501, .. }) => {}
            Err(e) => {
                panic!("expected put_secret to fail with 501 on a read-only backend, got: {e:?}")
            }
        }
        return;
    }

    let secret_key = generate_unique_name("test_secret");
    let secret_value = "my_secret_value_123";

    secret
        .put_secret(&secret_key, secret_value)
        .await
        .expect("put_secret should succeed");
    let value = secret
        .get_secret(&secret_key)
        .await
        .expect("get_secret should succeed");
    assert_eq!(value, secret_value);

    secret.delete_secret(&secret_key).await.ok();
}

#[tokio::test]
async fn test_secret_list_all() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let secret = client.secret_client();

    if client.is_oss().await {
        let secrets = match secret.list_all_secret_names().await {
            Ok(secrets) => secrets,
            Err(e) => {
                println!("Skipping: OSS secrets API unavailable on this server ({e:?})");
                return;
            }
        };
        assert!(
            secrets.contains(OSS_SEEDED_SECRET_NAME),
            "expected seeded secret {:?} in {:?}",
            OSS_SEEDED_SECRET_NAME,
            secrets
        );
        return;
    }

    let secrets = secret
        .list_all_secret_names()
        .await
        .expect("list_all_secret_names should succeed");
    println!("Found {} secrets", secrets.len());
}

#[tokio::test]
async fn test_secret_exists() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let secret = client.secret_client();

    if client.is_oss().await {
        let exists = match secret.secret_exists(OSS_SEEDED_SECRET_NAME).await {
            Ok(exists) => exists,
            Err(e) => {
                println!("Skipping: OSS secrets API unavailable on this server ({e:?})");
                return;
            }
        };
        assert!(exists, "seeded secret should exist");

        let missing_name = generate_unique_name("nonexistent_secret");
        assert!(
            !secret
                .secret_exists(&missing_name)
                .await
                .expect("secret_exists should succeed"),
            "never-created secret should not exist"
        );
        return;
    }

    let secret_key = generate_unique_name("test_secret_exists");

    secret
        .put_secret(&secret_key, "value")
        .await
        .expect("put_secret should succeed");
    assert!(
        secret
            .secret_exists(&secret_key)
            .await
            .expect("secret_exists should succeed"),
        "Secret should exist after creation"
    );

    secret.delete_secret(&secret_key).await.ok();
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert!(
        !secret
            .secret_exists(&secret_key)
            .await
            .expect("secret_exists should succeed"),
        "Secret should not exist after deletion"
    );
}

// =============================================================================
// Prompt Client Tests
//
// Not implemented by plain OSS Conductor (confirmed empirically: 404 "No
// static resource api/prompts..."). Skip explicitly when is_oss(), assert for
// real otherwise.
// =============================================================================

#[tokio::test]
async fn test_prompt_save_and_get() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Prompt API requires Orkes Enterprise Conductor");
        return;
    }
    let prompt = client.prompt_client();

    let prompt_name = generate_unique_name("test_prompt");

    prompt
        .save_prompt(
            &prompt_name,
            "Test prompt for integration tests",
            "Please analyze ${input} and provide insights.",
        )
        .await
        .expect("save_prompt should succeed");

    let template = prompt
        .get_prompt(&prompt_name)
        .await
        .expect("get_prompt should succeed");
    assert_eq!(template.name, prompt_name);

    prompt.delete_prompt(&prompt_name).await.ok();
}

#[tokio::test]
async fn test_prompt_get_prompts() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Prompt API requires Orkes Enterprise Conductor");
        return;
    }
    let prompt = client.prompt_client();

    let prompts = prompt
        .get_prompts()
        .await
        .expect("get_prompts should succeed");
    println!("Found {} prompts", prompts.len());
}

#[tokio::test]
async fn test_prompt_test() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Prompt API requires Orkes Enterprise Conductor");
        return;
    }
    let prompt = client.prompt_client();

    let prompt_name = generate_unique_name("test_prompt_test");

    prompt
        .save_prompt(&prompt_name, "Test prompt", "Say hello to ${name}.")
        .await
        .expect("save_prompt should succeed");

    // test_prompt itself additionally requires a valid AI integration/LLM
    // model configured, which this suite does not set up; save/get above
    // already cover the client's request/response handling.
    println!("Prompt created successfully. Skipping test_prompt as it requires AI configuration.");

    prompt.delete_prompt(&prompt_name).await.ok();
}

// =============================================================================
// Event Client Tests
//
// Event handlers are OSS-compatible (confirmed empirically); queue
// configuration is not (404 "No static resource api/event/queue/config").
// =============================================================================

#[tokio::test]
async fn test_get_all_event_handlers() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let event = client.event_client();

    let handlers = event
        .get_all_event_handlers()
        .await
        .expect("get_all_event_handlers should succeed");
    println!("Found {} event handlers", handlers.len());
}

#[tokio::test]
async fn test_event_handlers() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let event = client.event_client();

    let handlers = event
        .get_event_handlers("conductor:test_event", false)
        .await
        .expect("get_event_handlers should succeed");
    println!("Found {} handlers for event", handlers.len());
}

#[tokio::test]
async fn test_event_queue_configuration() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Event queue configuration API requires Orkes Enterprise Conductor");
        return;
    }
    let event = client.event_client();

    let configs = event
        .get_all_queue_configurations()
        .await
        .expect("get_all_queue_configurations should succeed");
    println!("Found {} queue configurations", configs.len());
}
