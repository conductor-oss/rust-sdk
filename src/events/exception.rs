// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Bounded-cardinality exception label helpers for metrics.
//!
//! The canonical Conductor SDK metric catalog specifies that any `exception`
//! label on a metric should carry an exception *type name*, never a raw message
//! or stack trace. This mirrors Python's `type(err).__name__`, Go's
//! `fmt.Sprintf("%T", err)`, and Java's `e.getClass().getSimpleName()`.

use crate::error::ConductorError;

/// Return the canonical `exception` label value for a [`ConductorError`].
///
/// Returns the unqualified variant name (e.g. `"Http"`, `"Json"`, `"Auth"`),
/// which is stable, compact, and bounded in cardinality.
pub fn exception_label(err: &ConductorError) -> &'static str {
    match err {
        ConductorError::Http(_) => "Http",
        ConductorError::Json(_) => "Json",
        ConductorError::Config(_) => "Config",
        ConductorError::Auth(_) => "Auth",
        ConductorError::TaskExecution(_) => "TaskExecution",
        ConductorError::TaskNotFound(_) => "TaskNotFound",
        ConductorError::WorkflowNotFound(_) => "WorkflowNotFound",
        ConductorError::Workflow(_) => "Workflow",
        ConductorError::Worker(_) => "Worker",
        ConductorError::Timeout(_) => "Timeout",
        ConductorError::Server { .. } => "Server",
        ConductorError::Api { .. } => "Api",
        ConductorError::Internal(_) => "Internal",
        ConductorError::Io(_) => "Io",
        ConductorError::Channel(_) => "Channel",
    }
}

/// Return the canonical `exception` label value for any type.
///
/// Uses [`std::any::type_name`] with the module path stripped, so generic
/// and nested types still produce a single, short label value. Intended for
/// values that aren't `ConductorError` — for those, prefer [`exception_label`]
/// which is guaranteed to be `&'static str` and doesn't allocate.
pub fn type_name_of<T: ?Sized>(_value: &T) -> &'static str {
    last_type_segment(std::any::type_name::<T>())
}

/// Return the canonical `exception` label value for a panic payload produced
/// by [`std::panic::catch_unwind`] / [`futures::FutureExt::catch_unwind`].
///
/// Panic payloads are `Box<dyn Any + Send>` and don't carry a useful type
/// name by themselves, so we always report `"Panic"` to keep cardinality
/// bounded. Callers that need the panic message should log it separately.
pub fn exception_label_for_panic(_payload: &(dyn std::any::Any + Send)) -> &'static str {
    "Panic"
}

/// Strip everything before the final `::` from a Rust type path.
fn last_type_segment(full: &'static str) -> &'static str {
    // Walk back from the end until we find `::` outside generic `<...>` nesting.
    // Simpler approach: split on `<` first to get the base, then take the last
    // `::`-delimited segment of that base.
    let base = match full.find('<') {
        Some(idx) => &full[..idx],
        None => full,
    };
    match base.rfind("::") {
        Some(idx) => &base[idx + 2..],
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conductor_error_variant_names() {
        assert_eq!(
            exception_label(&ConductorError::Auth("bad creds".into())),
            "Auth"
        );
        assert_eq!(
            exception_label(&ConductorError::Worker("oops".into())),
            "Worker"
        );
        assert_eq!(
            exception_label(&ConductorError::Server {
                status: 500,
                message: "boom".into(),
            }),
            "Server"
        );
    }

    #[test]
    fn strips_module_path() {
        assert_eq!(last_type_segment("std::io::Error"), "Error");
        assert_eq!(last_type_segment("reqwest::Error"), "Error");
        assert_eq!(last_type_segment("Foo"), "Foo");
        assert_eq!(last_type_segment("core::option::Option<String>"), "Option");
    }

    #[test]
    fn panic_payload_label() {
        let payload: Box<dyn std::any::Any + Send> = Box::new("panicked");
        assert_eq!(exception_label_for_panic(payload.as_ref()), "Panic");
    }
}
