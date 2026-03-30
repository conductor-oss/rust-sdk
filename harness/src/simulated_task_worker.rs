// Copyright 2024 Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde_json::Value;

use conductor::error::Result;
use conductor::models::Task;
use conductor::worker::{Worker, WorkerOutput};

const ALPHANUMERIC_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

fn instance_id() -> &'static str {
    use std::sync::OnceLock;
    static INSTANCE_ID: OnceLock<String> = OnceLock::new();
    INSTANCE_ID.get_or_init(|| {
        std::env::var("HOSTNAME").unwrap_or_else(|_| {
            uuid::Uuid::new_v4().to_string()[..8].to_string()
        })
    })
}

pub struct SimulatedTaskWorker {
    task_name: String,
    codename: String,
    default_delay_ms: u64,
    batch_size: usize,
    poll_interval_ms: u64,
    worker_id: String,
    rng: Mutex<StdRng>,
}

impl SimulatedTaskWorker {
    pub fn new(
        task_name: &str,
        codename: &str,
        sleep_seconds: u64,
        batch_size: usize,
        poll_interval_ms: u64,
    ) -> Self {
        let worker_id = format!("{}-{}", task_name, instance_id());

        println!(
            "[{}] Initialized worker [workerId={}, codename={}, batchSize={}, pollInterval={}ms]",
            task_name, worker_id, codename, batch_size, poll_interval_ms
        );

        Self {
            task_name: task_name.to_string(),
            codename: codename.to_string(),
            default_delay_ms: sleep_seconds * 1000,
            batch_size,
            poll_interval_ms,
            worker_id,
            rng: Mutex::new(StdRng::from_entropy()),
        }
    }

    fn calculate_delay(&self, delay_type: &str, min_delay: i64, max_delay: i64, mean_delay: i64, std_deviation: i64) -> i64 {
        let mut rng = self.rng.lock().unwrap_or_else(|e| e.into_inner());
        match delay_type.to_lowercase().as_str() {
            "fixed" => min_delay,
            "random" => {
                if max_delay <= min_delay {
                    return min_delay;
                }
                rng.gen_range(min_delay..=max_delay)
            }
            "normal" => {
                let u1: f64 = 1.0 - rng.gen::<f64>();
                let u2: f64 = rng.gen::<f64>();
                let gaussian = ((-2.0 * u1.ln()).sqrt()) * (2.0 * PI * u2).sin();
                let delay = (mean_delay as f64 + gaussian * std_deviation as f64).round();
                if delay < 1.0 { 1 } else { delay as i64 }
            }
            "exponential" => {
                let exp = -(mean_delay as f64) * (1.0 - rng.gen::<f64>()).ln();
                let result = exp as i64;
                result.clamp(min_delay, max_delay)
            }
            _ => min_delay,
        }
    }

    fn should_task_succeed(&self, success_rate: f64, failure_mode: &str, input: &HashMap<String, Value>) -> bool {
        if let Some(v) = input.get("forceSuccess") {
            if let Some(b) = to_bool(v) {
                return b;
            }
        }
        if let Some(v) = input.get("forceFail") {
            if let Some(b) = to_bool(v) {
                return !b;
            }
        }

        let mut rng = self.rng.lock().unwrap_or_else(|e| e.into_inner());
        match failure_mode.to_lowercase().as_str() {
            "random" => rng.gen::<f64>() < success_rate,
            "conditional" => self.should_conditional_succeed(success_rate, input, &mut *rng),
            "sequential" => {
                let attempt = get_int_or_default(input, "attempt", 1);
                let fail_until_attempt = get_int_or_default(input, "failUntilAttempt", 2);
                attempt >= fail_until_attempt
            }
            _ => rng.gen::<f64>() < success_rate,
        }
    }

    fn should_conditional_succeed(&self, success_rate: f64, input: &HashMap<String, Value>, rng: &mut impl Rng) -> bool {
        let task_index = get_int_or_default(input, "taskIndex", -1);
        if task_index >= 0 {
            if let Some(Value::Array(arr)) = input.get("failIndexes") {
                for idx in arr {
                    if to_int(idx) == task_index {
                        return false;
                    }
                }
            }
            let fail_every = get_int_or_default(input, "failEvery", 0);
            if fail_every > 0 && task_index % fail_every == 0 {
                return false;
            }
        }
        rng.gen::<f64>() < success_rate
    }

    fn generate_output(
        &self,
        input: &HashMap<String, Value>,
        task_id: &str,
        task_index: i64,
        delay_ms: i64,
        elapsed_ms: u128,
        output_size: i64,
    ) -> HashMap<String, Value> {
        let mut rng = self.rng.lock().unwrap_or_else(|e| e.into_inner());

        let a_or_b = if rng.gen_range(0..100) > 20 { "a" } else { "b" };
        let c_or_d = if rng.gen_range(0..100) > 33 { "c" } else { "d" };

        let mut output = HashMap::new();
        output.insert("taskId".to_string(), Value::String(task_id.to_string()));
        output.insert("taskIndex".to_string(), serde_json::json!(task_index));
        output.insert("codename".to_string(), Value::String(self.codename.clone()));
        output.insert("status".to_string(), Value::String("completed".to_string()));
        output.insert("configuredDelayMs".to_string(), serde_json::json!(delay_ms));
        output.insert("actualExecutionTimeMs".to_string(), serde_json::json!(elapsed_ms as i64));
        output.insert("a_or_b".to_string(), Value::String(a_or_b.to_string()));
        output.insert("c_or_d".to_string(), Value::String(c_or_d.to_string()));

        if get_bool_or_default(input, "includeInput", false) {
            output.insert("input".to_string(), serde_json::to_value(input).unwrap_or(Value::Null));
        }

        if let Some(prev) = input.get("previousTaskOutput") {
            if !prev.is_null() {
                output.insert("previousTaskData".to_string(), prev.clone());
            }
        }

        if output_size > 0 {
            output.insert("data".to_string(), Value::String(generate_random_data(&mut *rng, output_size as usize)));
        }

        if let Some(Value::Object(tmpl)) = input.get("outputTemplate") {
            for (k, v) in tmpl {
                output.insert(k.clone(), v.clone());
            }
        }

        output
    }
}

#[async_trait]
impl Worker for SimulatedTaskWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    fn identity(&self) -> String {
        self.worker_id.clone()
    }

    fn poll_interval_millis(&self) -> u64 {
        self.poll_interval_ms
    }

    fn thread_count(&self) -> usize {
        self.batch_size
    }

    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let input = &task.input_data;
        let task_id = &task.task_id;
        let task_index = get_int_or_default(input, "taskIndex", -1);

        println!(
            "[{}] Starting simulated task [id={}, index={}, codename={}]",
            self.task_name, task_id, task_index, self.codename
        );

        let start_time = Instant::now();

        let delay_type = get_string_or_default(input, "delayType", "fixed");
        let min_delay = get_int_or_default(input, "minDelay", self.default_delay_ms as i64);
        let max_delay = get_int_or_default(input, "maxDelay", min_delay + 100);
        let mean_delay = get_int_or_default(input, "meanDelay", (min_delay + max_delay) / 2);
        let std_deviation = get_int_or_default(input, "stdDeviation", 30);
        let success_rate = get_float_or_default(input, "successRate", 1.0);
        let failure_mode = get_string_or_default(input, "failureMode", "random");
        let output_size = get_int_or_default(input, "outputSize", 1024);

        let mut delay_ms: i64 = 0;
        if delay_type.to_lowercase() != "wait" {
            delay_ms = self.calculate_delay(&delay_type, min_delay, max_delay, mean_delay, std_deviation);
            println!(
                "[{}] Simulated task [id={}, index={}] sleeping for {} ms",
                self.task_name, task_id, task_index, delay_ms
            );
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms as u64)).await;
        }

        if !self.should_task_succeed(success_rate, &failure_mode, input) {
            println!(
                "[{}] Simulated task [id={}, index={}] failed as configured",
                self.task_name, task_id, task_index
            );
            return Ok(WorkerOutput::failed("Simulated task failure based on configuration"));
        }

        let elapsed = start_time.elapsed().as_millis();
        let output = self.generate_output(input, task_id, task_index, delay_ms, elapsed, output_size);
        Ok(WorkerOutput::completed(output))
    }
}

fn generate_random_data(rng: &mut impl Rng, size: usize) -> String {
    (0..size)
        .map(|_| ALPHANUMERIC_CHARS[rng.gen_range(0..ALPHANUMERIC_CHARS.len())] as char)
        .collect()
}

fn to_int(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn to_float(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn to_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.to_lowercase().as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        Value::Number(n) => n.as_f64().map(|f| f != 0.0),
        _ => None,
    }
}

fn get_int_or_default(input: &HashMap<String, Value>, key: &str, default: i64) -> i64 {
    input.get(key).map(to_int).unwrap_or(default)
}

fn get_float_or_default(input: &HashMap<String, Value>, key: &str, default: f64) -> f64 {
    input.get(key).map(to_float).unwrap_or(default)
}

fn get_string_or_default(input: &HashMap<String, Value>, key: &str, default: &str) -> String {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

fn get_bool_or_default(input: &HashMap<String, Value>, key: &str, default: bool) -> bool {
    input
        .get(key)
        .and_then(|v| to_bool(v))
        .unwrap_or(default)
}
