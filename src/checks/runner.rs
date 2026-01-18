use crate::config::{Check, LoadedConfig};
use crate::transcript::ToolUseEvent;
use glob::Pattern;
use std::path::Path;

/// Result of running a single check
#[derive(Debug)]
pub struct CheckResult {
    pub check_name: String,
    pub reason: Option<String>,
}

/// Run all checks from a loaded config against changed files.
/// Returns a list of check results (only failures have reasons).
pub fn run_checks(
    loaded: &LoadedConfig,
    changed_files: &[String],
    events: &[ToolUseEvent],
) -> Vec<CheckResult> {
    let mut results = Vec::new();

    for check in &loaded.config.checks {
        let result = run_single_check(check, &loaded.config_dir, changed_files, events);
        results.push(result);
    }

    results
}

/// Run a single check against the changed files
fn run_single_check(
    check: &Check,
    config_dir: &Path,
    changed_files: &[String],
    events: &[ToolUseEvent],
) -> CheckResult {
    // Check path_exists condition first
    if let Some(path_exists) = &check.when.path_exists {
        let required_path = config_dir.join(path_exists);
        if !required_path.exists() {
            return CheckResult {
                check_name: check.name.clone(),
                reason: None,
            };
        }
    }

    // Parse the glob pattern
    let pattern = match Pattern::new(&check.when.paths_changed) {
        Ok(p) => p,
        Err(_) => {
            return CheckResult {
                check_name: check.name.clone(),
                reason: Some(format!(
                    "Invalid glob pattern '{}' in check '{}'",
                    check.when.paths_changed, check.name
                )),
            };
        }
    };

    // Find matching files
    let matching_files: Vec<&String> = changed_files
        .iter()
        .filter(|f| file_matches_pattern(f, &pattern))
        .collect();

    if matching_files.is_empty() {
        return CheckResult {
            check_name: check.name.clone(),
            reason: None,
        };
    }

    // Dispatch to the appropriate check type
    if let Some(commands) = &check.then.ensure_commands {
        check_ensure_commands(check, &pattern, commands, events)
    } else if let Some(paths) = &check.then.ensure_changed {
        check_ensure_changed(check, paths, changed_files)
    } else {
        CheckResult {
            check_name: check.name.clone(),
            reason: None,
        }
    }
}

/// Check if a file path matches a glob pattern
fn file_matches_pattern(file_path: &str, pattern: &Pattern) -> bool {
    // Try matching against the path as-is
    if pattern.matches(file_path) {
        return true;
    }

    // Also try matching just the filename for simple patterns
    if let Some(filename) = Path::new(file_path).file_name() {
        if pattern.matches(filename.to_string_lossy().as_ref()) {
            return true;
        }
    }

    false
}

/// Check that required commands were run after the last matching edit
fn check_ensure_commands(
    check: &Check,
    pattern: &Pattern,
    required_commands: &[String],
    events: &[ToolUseEvent],
) -> CheckResult {
    // Find the index of the last matching file write
    let last_write_idx = events.iter().rposition(|e| {
        (e.tool_name == "Edit" || e.tool_name == "Write")
            && e.file_path
                .as_ref()
                .is_some_and(|p| file_matches_pattern(p, pattern))
    });

    // Check which required commands are missing (must run AFTER last write)
    let mut missing: Vec<&str> = Vec::new();

    for cmd in required_commands {
        let was_run_after_write = events.iter().any(|e| {
            e.tool_name == "Bash"
                && e.command.as_ref().is_some_and(|c| c.contains(cmd.as_str()))
                && e.index > last_write_idx.unwrap_or(0)
        });
        if !was_run_after_write {
            missing.push(cmd);
        }
    }

    if missing.is_empty() {
        CheckResult {
            check_name: check.name.clone(),
            reason: None,
        }
    } else {
        CheckResult {
            check_name: check.name.clone(),
            reason: Some(format!(
                "[{}] Required commands not run after last edit: {}",
                check.name,
                missing.join(", ")
            )),
        }
    }
}

/// Check that at least one of the specified paths was changed
fn check_ensure_changed(
    check: &Check,
    required_paths: &[String],
    changed_files: &[String],
) -> CheckResult {
    // Check if any required path was changed
    let any_changed = required_paths.iter().any(|required| {
        changed_files
            .iter()
            .any(|f| f == required || f.ends_with(&format!("/{}", required)))
    });

    if any_changed {
        CheckResult {
            check_name: check.name.clone(),
            reason: None,
        }
    } else {
        CheckResult {
            check_name: check.name.clone(),
            reason: Some(format!(
                "[{}] Required files not modified: {}",
                check.name,
                required_paths.join(", ")
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RufioConfig, Then, When};
    use std::path::PathBuf;

    fn make_loaded_config(checks: Vec<Check>) -> LoadedConfig {
        LoadedConfig {
            config: RufioConfig { checks },
            config_dir: PathBuf::from("/test"),
        }
    }

    fn make_check(
        name: &str,
        pattern: &str,
        commands: Option<Vec<&str>>,
        ensure_changed: Option<Vec<&str>>,
    ) -> Check {
        Check {
            name: name.to_string(),
            when: When {
                paths_changed: pattern.to_string(),
                path_exists: None,
            },
            then: Then {
                ensure_commands: commands.map(|c| c.into_iter().map(String::from).collect()),
                ensure_changed: ensure_changed.map(|c| c.into_iter().map(String::from).collect()),
            },
        }
    }

    #[test]
    fn test_no_matching_files() {
        let loaded = make_loaded_config(vec![make_check(
            "test",
            "**/*.rs",
            Some(vec!["cargo test"]),
            None,
        )]);
        let changed_files = vec!["README.md".to_string()];
        let events = vec![];

        let results = run_checks(&loaded, &changed_files, &events);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_none());
    }

    #[test]
    fn test_matching_files_command_run() {
        let loaded = make_loaded_config(vec![make_check(
            "test",
            "**/*.rs",
            Some(vec!["cargo test"]),
            None,
        )]);
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![
            ToolUseEvent {
                tool_name: "Write".to_string(),
                command: None,
                file_path: Some("src/main.rs".to_string()),
                index: 0,
            },
            ToolUseEvent {
                tool_name: "Bash".to_string(),
                command: Some("cargo test".to_string()),
                file_path: None,
                index: 1,
            },
        ];

        let results = run_checks(&loaded, &changed_files, &events);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_none());
    }

    #[test]
    fn test_matching_files_command_not_run() {
        let loaded = make_loaded_config(vec![make_check(
            "test",
            "**/*.rs",
            Some(vec!["cargo test"]),
            None,
        )]);
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![ToolUseEvent {
            tool_name: "Write".to_string(),
            command: None,
            file_path: Some("src/main.rs".to_string()),
            index: 0,
        }];

        let results = run_checks(&loaded, &changed_files, &events);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0].reason.as_ref().unwrap().contains("cargo test"));
    }

    #[test]
    fn test_ensure_changed_satisfied() {
        let loaded = make_loaded_config(vec![make_check(
            "version",
            "**/*.rs",
            None,
            Some(vec!["version.toml"]),
        )]);
        let changed_files = vec!["src/main.rs".to_string(), "version.toml".to_string()];
        let events = vec![];

        let results = run_checks(&loaded, &changed_files, &events);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_none());
    }

    #[test]
    fn test_ensure_changed_not_satisfied() {
        let loaded = make_loaded_config(vec![make_check(
            "version",
            "**/*.rs",
            None,
            Some(vec!["version.toml"]),
        )]);
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![];

        let results = run_checks(&loaded, &changed_files, &events);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0].reason.as_ref().unwrap().contains("version.toml"));
    }

    #[test]
    fn test_command_run_before_write() {
        let loaded = make_loaded_config(vec![make_check(
            "test",
            "**/*.rs",
            Some(vec!["cargo test"]),
            None,
        )]);
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![
            ToolUseEvent {
                tool_name: "Bash".to_string(),
                command: Some("cargo test".to_string()),
                file_path: None,
                index: 0,
            },
            ToolUseEvent {
                tool_name: "Write".to_string(),
                command: None,
                file_path: Some("src/main.rs".to_string()),
                index: 1,
            },
        ];

        let results = run_checks(&loaded, &changed_files, &events);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
    }

    #[test]
    fn test_multiple_checks() {
        let loaded = make_loaded_config(vec![
            make_check("test", "**/*.rs", Some(vec!["cargo test"]), None),
            make_check("fmt", "**/*.rs", Some(vec!["cargo fmt"]), None),
        ]);
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![
            ToolUseEvent {
                tool_name: "Write".to_string(),
                command: None,
                file_path: Some("src/main.rs".to_string()),
                index: 0,
            },
            ToolUseEvent {
                tool_name: "Bash".to_string(),
                command: Some("cargo test".to_string()),
                file_path: None,
                index: 1,
            },
            // cargo fmt not run
        ];

        let results = run_checks(&loaded, &changed_files, &events);
        assert_eq!(results.len(), 2);
        assert!(results[0].reason.is_none()); // cargo test passed
        assert!(results[1].reason.is_some()); // cargo fmt failed
    }
}
