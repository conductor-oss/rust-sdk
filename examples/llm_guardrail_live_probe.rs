//! One-off probe: exercises LlmGuardrail::check() against a real local OpenAI-compatible LLM
//! server (temporarily patched into guardrail.rs's normally-hardcoded OpenAI URL for this
//! test only). Not part of the crate's example set.

use conductor::agents::{GuardrailCheck, LlmGuardrail};

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
