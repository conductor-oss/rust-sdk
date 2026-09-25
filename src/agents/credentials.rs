// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

// Resolved tool/agent credentials, read inside a tool body.
//
// Credentials::from_task builds a read-only view over the values the Conductor server
// attached to a crate::models::Task's runtime_metadata; Credentials::get reads it,
// failing closed on a declared name the server didn't attach. Credentials never fall back to
// env vars, and there's no accessor that returns every value at once.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{ConductorError, Result};
use crate::models::Task;

/// Read-only view over the credential values the Conductor server resolved and attached to a
/// [`Task`]'s `runtime_metadata` for one poll.
///
/// Cheap to clone (`Arc<HashMap<String, String>>` under the hood). `Debug`/`Display` show only
/// the credential *names* this view holds, never the values, and there is no accessor that
/// exposes the full underlying map.
#[derive(Clone, Default)]
pub struct Credentials(Arc<HashMap<String, String>>);

impl Credentials {
    /// Build a `Credentials` view from the resolved values the server attached to
    /// `task.runtime_metadata`. Build one fresh per poll from the `Task` handed to that poll.
    #[must_use]
    pub fn from_task(task: &Task) -> Self {
        Self(Arc::new(task.runtime_metadata.clone()))
    }

    /// Build a `Credentials` view directly from a resolved-name-to-value map. Mainly useful for
    /// tests, or when a `Task` isn't in hand at the construction site.
    #[must_use]
    pub fn new(values: HashMap<String, String>) -> Self {
        Self(Arc::new(values))
    }

    /// Look up a resolved credential value by its declared name.
    ///
    /// Fails closed: a `name` not present in the underlying map is always an error, never a
    /// fallback to `std::env::var()` and never a silent `None`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::CredentialNotFound`] if `name` isn't present.
    pub fn get(&self, name: &str) -> Result<&str> {
        self.0
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| ConductorError::credential_not_found([name]))
    }

    /// True if `name` is present in this credential view, without exposing its value.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// The credential names held by this view, in arbitrary order. Safe to log.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Credentials")
            .field(&self.names().collect::<Vec<_>>())
            .finish()
    }
}

impl std::fmt::Display for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Credentials(names=[{}])",
            self.names().collect::<Vec<_>>().join(", ")
        )
    }
}

impl From<&Task> for Credentials {
    fn from(task: &Task) -> Self {
        Self::from_task(task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_with_runtime_metadata(entries: &[(&str, &str)]) -> Task {
        Task {
            runtime_metadata: entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn get_returns_resolved_value_for_declared_name() {
        let task = task_with_runtime_metadata(&[("GH_TOKEN", "ghp_super_secret")]);
        let creds = Credentials::from_task(&task);

        assert_eq!(creds.get("GH_TOKEN").unwrap(), "ghp_super_secret");
    }

    #[test]
    fn get_fails_closed_on_missing_name() {
        let task = task_with_runtime_metadata(&[("GH_TOKEN", "ghp_super_secret")]);
        let creds = Credentials::from_task(&task);

        let err = creds.get("OPENAI_API_KEY").unwrap_err();
        match err {
            ConductorError::CredentialNotFound(names) => {
                assert_eq!(names, vec!["OPENAI_API_KEY".to_owned()]);
            }
            other => panic!("expected CredentialNotFound, got: {other:?}"),
        }
    }

    #[test]
    fn get_fails_closed_when_task_has_no_runtime_metadata_at_all() {
        let task = Task::default();
        let creds = Credentials::from_task(&task);

        creds.get("ANYTHING").unwrap_err();
    }

    #[test]
    fn never_falls_back_to_process_environment() {
        // A declared name that happens to also be set as a real env var must still fail closed —
        // the whole point of the design is "no fallback to std::env::var()".
        std::env::set_var("CONDUCTOR_CREDENTIALS_TEST_VAR", "leaked-from-env");
        let task = Task::default();
        let creds = Credentials::from_task(&task);

        creds.get("CONDUCTOR_CREDENTIALS_TEST_VAR").unwrap_err();
        std::env::remove_var("CONDUCTOR_CREDENTIALS_TEST_VAR");
    }

    #[test]
    fn contains_reports_presence_without_exposing_value() {
        let task = task_with_runtime_metadata(&[("GH_TOKEN", "ghp_super_secret")]);
        let creds = Credentials::from_task(&task);

        assert!(creds.contains("GH_TOKEN"));
        assert!(!creds.contains("OTHER"));
    }

    #[test]
    fn debug_and_display_show_names_but_never_values() {
        let task = task_with_runtime_metadata(&[("GH_TOKEN", "ghp_super_secret")]);
        let creds = Credentials::from_task(&task);

        let debug = format!("{creds:?}");
        let display = format!("{creds}");

        assert!(debug.contains("GH_TOKEN"));
        assert!(!debug.contains("ghp_super_secret"));
        assert!(display.contains("GH_TOKEN"));
        assert!(!display.contains("ghp_super_secret"));
    }

    #[test]
    fn new_builds_directly_from_a_map_for_tests_without_a_task() {
        let mut values = HashMap::new();
        values.insert("GH_TOKEN".to_owned(), "ghp_super_secret".to_owned());
        let creds = Credentials::new(values);

        assert_eq!(creds.get("GH_TOKEN").unwrap(), "ghp_super_secret");
        creds.get("MISSING").unwrap_err();
    }
}
