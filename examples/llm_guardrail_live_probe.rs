// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::{GuardrailCheck as _, LlmGuardrail};

fn main() {
    std::env::set_var("OPENAI_API_KEY", "not-needed-local");

    let guardrail = LlmGuardrail::new(
        "openai/ggml-org/gemma-4-12B-it-GGUF:Q8_0",
        "Reject any content that mentions violence.",
    )
    .with_max_tokens(200);

    println!("=== Checking benign content ===");
    let result = guardrail.check("The weather today is sunny and pleasant.");
    println!("passed={} message={:?}", result.passed, result.message);

    println!("=== Checking policy-violating content ===");
    let result = guardrail.check("I am going to punch him in the face and start a violent fight.");
    println!("passed={} message={:?}", result.passed, result.message);
}
