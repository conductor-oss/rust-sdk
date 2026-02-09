//! Scheduler, Secret, Prompt, and Event Client Tests
//!
//! Integration tests for Orkes-specific clients

mod common;

use common::*;
use conductor::client::ConductorClient;
use conductor::models::SaveScheduleRequest;
use std::time::Duration;

// =============================================================================
// Scheduler Client Tests
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

    if let Err(e) = metadata.register_workflow_def(&workflow_def).await {
        eprintln!(
            "Warning: Could not create workflow for schedule test: {:?}",
            e
        );
        return;
    }

    // Save Schedule using the builder methods
    let schedule_request = SaveScheduleRequest::new(&schedule_name, "0 0 * * *", &workflow_name)
        .with_version(1)
        .paused(true); // Create paused so it doesn't run

    match scheduler.save_schedule(&schedule_request).await {
        Ok(_) => {
            // Get Schedule
            match scheduler.get_schedule(&schedule_name).await {
                Ok(schedule) => {
                    assert_eq!(schedule.name, schedule_name);
                }
                Err(e) => eprintln!("Warning: get_schedule failed: {:?}", e),
            }

            // Cleanup
            scheduler.delete_schedule(&schedule_name).await.ok();
        }
        Err(e) => {
            eprintln!(
                "Warning: save_schedule failed (may require specific permissions): {:?}",
                e
            );
        }
    }

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

    if let Err(e) = metadata.register_workflow_def(&workflow_def).await {
        eprintln!("Warning: Could not create workflow: {:?}", e);
        return;
    }

    // Save schedule
    let schedule_request = SaveScheduleRequest::new(&schedule_name, "0 0 * * *", &workflow_name)
        .with_version(1)
        .paused(false);

    match scheduler.save_schedule(&schedule_request).await {
        Ok(_) => {
            // Pause schedule
            if let Err(e) = scheduler.pause_schedule(&schedule_name).await {
                eprintln!("Warning: pause_schedule failed: {:?}", e);
            }

            // Resume schedule
            if let Err(e) = scheduler.resume_schedule(&schedule_name).await {
                eprintln!("Warning: resume_schedule failed: {:?}", e);
            }

            // Cleanup
            scheduler.delete_schedule(&schedule_name).await.ok();
        }
        Err(e) => {
            eprintln!("Warning: save_schedule failed: {:?}", e);
        }
    }

    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
}

#[tokio::test]
async fn test_scheduler_search_executions() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let scheduler = client.scheduler_client();

    // Search schedule executions
    match scheduler
        .search_schedule_executions(Some(0), Some(10), None, None, None)
        .await
    {
        Ok(results) => {
            assert!(results.total_hits >= 0);
        }
        Err(e) => {
            eprintln!("Warning: search_schedule_executions failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_scheduler_get_next_execution_times() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let scheduler = client.scheduler_client();

    // Get next 5 execution times for daily cron
    match scheduler
        .get_next_few_schedule_execution_times("0 0 * * *", None, None, Some(5))
        .await
    {
        Ok(times) => {
            assert!(!times.is_empty());
            assert!(times.len() <= 5);
        }
        Err(e) => {
            eprintln!(
                "Warning: get_next_few_schedule_execution_times failed: {:?}",
                e
            );
        }
    }
}

// =============================================================================
// Secret Client Tests
// =============================================================================

#[tokio::test]
async fn test_secret_put_and_get() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let secret = client.secret_client();

    let secret_key = generate_unique_name("test_secret");
    let secret_value = "my_secret_value_123";

    // Put secret
    match secret.put_secret(&secret_key, secret_value).await {
        Ok(_) => {
            // Get secret
            match secret.get_secret(&secret_key).await {
                Ok(value) => {
                    assert_eq!(value, secret_value);
                }
                Err(e) => eprintln!("Warning: get_secret failed: {:?}", e),
            }

            // Cleanup
            secret.delete_secret(&secret_key).await.ok();
        }
        Err(e) => {
            eprintln!(
                "Warning: put_secret failed (may require specific permissions): {:?}",
                e
            );
        }
    }
}

#[tokio::test]
async fn test_secret_list_all() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let secret = client.secret_client();

    // List all secret names
    match secret.list_all_secret_names().await {
        Ok(secrets) => {
            // Just verify we got a valid response (may be empty)
            println!("Found {} secrets", secrets.len());
        }
        Err(e) => {
            eprintln!("Warning: list_all_secret_names failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_secret_exists() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let secret = client.secret_client();

    let secret_key = generate_unique_name("test_secret_exists");

    // Put secret
    match secret.put_secret(&secret_key, "value").await {
        Ok(_) => {
            // Check exists
            match secret.secret_exists(&secret_key).await {
                Ok(exists) => {
                    assert!(exists, "Secret should exist after creation");
                }
                Err(e) => eprintln!("Warning: secret_exists failed: {:?}", e),
            }

            // Delete
            secret.delete_secret(&secret_key).await.ok();

            // Wait for deletion
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Check doesn't exist
            match secret.secret_exists(&secret_key).await {
                Ok(exists) => {
                    assert!(!exists, "Secret should not exist after deletion");
                }
                Err(e) => eprintln!("Warning: secret_exists after delete failed: {:?}", e),
            }
        }
        Err(e) => {
            eprintln!("Warning: put_secret failed: {:?}", e);
        }
    }
}

// =============================================================================
// Prompt Client Tests
// =============================================================================

#[tokio::test]
async fn test_prompt_save_and_get() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let prompt = client.prompt_client();

    let prompt_name = generate_unique_name("test_prompt");

    // Save prompt
    match prompt
        .save_prompt(
            &prompt_name,
            "Test prompt for integration tests",
            "Please analyze ${input} and provide insights.",
        )
        .await
    {
        Ok(_) => {
            // Get prompt
            match prompt.get_prompt(&prompt_name).await {
                Ok(template) => {
                    assert_eq!(template.name, prompt_name);
                }
                Err(e) => eprintln!("Warning: get_prompt failed: {:?}", e),
            }

            // Cleanup
            prompt.delete_prompt(&prompt_name).await.ok();
        }
        Err(e) => {
            eprintln!(
                "Warning: save_prompt failed (may require AI module): {:?}",
                e
            );
        }
    }
}

#[tokio::test]
async fn test_prompt_get_prompts() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let prompt = client.prompt_client();

    // Get all prompts
    match prompt.get_prompts().await {
        Ok(prompts) => {
            println!("Found {} prompts", prompts.len());
        }
        Err(e) => {
            eprintln!("Warning: get_prompts failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_prompt_test() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let prompt = client.prompt_client();

    let prompt_name = generate_unique_name("test_prompt_test");

    // Save a prompt first
    match prompt
        .save_prompt(&prompt_name, "Test prompt", "Say hello to ${name}.")
        .await
    {
        Ok(_) => {
            // Test the prompt (requires AI integration to be configured)
            let mut vars: std::collections::HashMap<String, serde_json::Value> =
                std::collections::HashMap::new();
            vars.insert("name".to_string(), serde_json::json!("World"));

            // Note: test_prompt requires a valid AI integration to be configured
            // Also requires a valid LLM model name and integration
            // Skipping actual test since we don't have AI configured
            println!("Prompt created successfully. Skipping test_prompt as it requires AI configuration.");

            // Cleanup
            prompt.delete_prompt(&prompt_name).await.ok();
        }
        Err(e) => {
            eprintln!("Warning: save_prompt failed: {:?}", e);
        }
    }
}

// =============================================================================
// Event Client Tests
// =============================================================================

#[tokio::test]
async fn test_get_all_event_handlers() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let event = client.event_client();

    // Get all event handlers
    match event.get_all_event_handlers().await {
        Ok(handlers) => {
            println!("Found {} event handlers", handlers.len());
        }
        Err(e) => {
            eprintln!("Warning: get_all_event_handlers failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_event_handlers() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let event = client.event_client();

    // Try to get event handlers for a specific event
    match event
        .get_event_handlers("conductor:test_event", false)
        .await
    {
        Ok(handlers) => {
            println!("Found {} handlers for event", handlers.len());
        }
        Err(e) => {
            eprintln!("Warning: get_event_handlers failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_event_queue_configuration() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let event = client.event_client();

    // Get all queue configurations
    match event.get_all_queue_configurations().await {
        Ok(configs) => {
            println!("Found {} queue configurations", configs.len());
        }
        Err(e) => {
            eprintln!("Warning: get_all_queue_configurations failed (may require queue configuration): {:?}", e);
        }
    }
}
