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
/// Changed files are relative to repo_root.
pub fn run_checks(
    loaded: &LoadedConfig,
    changed_files: &[String],
    events: &[ToolUseEvent],
    repo_root: &Path,
) -> Vec<CheckResult> {
    let mut results = Vec::new();

    for check in &loaded.config.checks {
        let result = run_single_check(check, &loaded.config_dir, changed_files, events, repo_root);
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
    repo_root: &Path,
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

    // Find matching files (make paths relative to config dir before matching)
    let matching_files: Vec<&String> = changed_files
        .iter()
        .filter(|f| file_matches_relative(f, &pattern, config_dir, repo_root))
        .collect();

    if matching_files.is_empty() {
        return CheckResult {
            check_name: check.name.clone(),
            reason: None,
        };
    }

    // Dispatch to the appropriate check type
    if let Some(commands) = &check.then.ensure_commands {
        check_ensure_commands(check, &pattern, commands, events, config_dir)
    } else if let Some(paths) = &check.then.ensure_changed {
        check_ensure_changed(check, paths, changed_files, config_dir, repo_root)
    } else {
        CheckResult {
            check_name: check.name.clone(),
            reason: None,
        }
    }
}

/// Check if a file (relative to repo root) matches a glob pattern
/// after converting to be relative to config dir.
/// Files outside the config directory are skipped.
fn file_matches_relative(
    file_path: &str,
    pattern: &Pattern,
    config_dir: &Path,
    repo_root: &Path,
) -> bool {
    let absolute = repo_root.join(file_path);
    let relative = match absolute.strip_prefix(config_dir) {
        Ok(r) => r,
        Err(_) => return false, // File is outside config dir
    };

    let relative_str = relative.to_string_lossy();
    pattern.matches(relative_str.as_ref())
}

/// Check if a transcript file path (absolute) matches a glob pattern
/// relative to config dir.
fn transcript_path_matches(path: &str, pattern: &Pattern, config_dir: &Path) -> bool {
    let absolute = Path::new(path);
    let relative = match absolute.strip_prefix(config_dir) {
        Ok(r) => r,
        Err(_) => return false,
    };

    let relative_str = relative.to_string_lossy();
    pattern.matches(relative_str.as_ref())
}

/// Check that required commands were run after the last matching edit
fn check_ensure_commands(
    check: &Check,
    pattern: &Pattern,
    required_commands: &[String],
    events: &[ToolUseEvent],
    config_dir: &Path,
) -> CheckResult {
    // Find the index of the last matching file write
    let last_write_idx = events.iter().rposition(|e| {
        (e.tool_name == "Edit" || e.tool_name == "Write")
            && e.file_path
                .as_ref()
                .is_some_and(|p| transcript_path_matches(p, pattern, config_dir))
    });

    // If no matching file was edited in this session, skip the check
    let last_write_idx = match last_write_idx {
        Some(idx) => idx,
        None => {
            return CheckResult {
                check_name: check.name.clone(),
                reason: None,
            };
        }
    };

    // Find the event index (not vec position) of the last write
    let last_write_event_idx = events[last_write_idx].index;

    // Check which required commands are missing (must run AFTER last write)
    let mut missing: Vec<&str> = Vec::new();

    for cmd in required_commands {
        let was_run_after_write = events.iter().any(|e| {
            e.tool_name == "Bash"
                && e.command.as_ref().is_some_and(|c| c.contains(cmd.as_str()))
                && e.index > last_write_event_idx
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
                "Check '{}' failed: these commands must run after editing {}: {}",
                check.name,
                check.when.paths_changed,
                missing.join(", ")
            )),
        }
    }
}

/// Check that at least one of the specified paths was changed.
/// Resolves required paths relative to config dir, compares against
/// changed files resolved relative to repo root.
fn check_ensure_changed(
    check: &Check,
    required_paths: &[String],
    changed_files: &[String],
    config_dir: &Path,
    repo_root: &Path,
) -> CheckResult {
    let any_changed = required_paths.iter().any(|required| {
        let absolute_required = config_dir.join(required);
        changed_files.iter().any(|f| {
            let absolute_changed = repo_root.join(f);
            absolute_changed == absolute_required
        })
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
                "Check '{}' failed: one of these files must be changed when editing {}: {}",
                check.name,
                check.when.paths_changed,
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

    fn make_loaded_config(checks: Vec<Check>, config_dir: &Path) -> LoadedConfig {
        LoadedConfig {
            config: RufioConfig { checks },
            config_dir: config_dir.to_path_buf(),
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
        let repo_root = PathBuf::from("/repo");
        let config_dir = repo_root.clone();
        let loaded = make_loaded_config(
            vec![make_check(
                "test",
                "**/*.rs",
                Some(vec!["cargo test"]),
                None,
            )],
            &config_dir,
        );
        let changed_files = vec!["README.md".to_string()];
        let events = vec![];

        let results = run_checks(&loaded, &changed_files, &events, &repo_root);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_none());
    }

    #[test]
    fn test_matching_files_command_run() {
        let repo_root = PathBuf::from("/repo");
        let config_dir = repo_root.clone();
        let loaded = make_loaded_config(
            vec![make_check(
                "test",
                "**/*.rs",
                Some(vec!["cargo test"]),
                None,
            )],
            &config_dir,
        );
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![
            ToolUseEvent {
                tool_name: "Write".to_string(),
                command: None,
                file_path: Some("/repo/src/main.rs".to_string()),
                index: 0,
            },
            ToolUseEvent {
                tool_name: "Bash".to_string(),
                command: Some("cargo test".to_string()),
                file_path: None,
                index: 1,
            },
        ];

        let results = run_checks(&loaded, &changed_files, &events, &repo_root);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_none());
    }

    #[test]
    fn test_matching_files_command_not_run() {
        let repo_root = PathBuf::from("/repo");
        let config_dir = repo_root.clone();
        let loaded = make_loaded_config(
            vec![make_check(
                "test",
                "**/*.rs",
                Some(vec!["cargo test"]),
                None,
            )],
            &config_dir,
        );
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![ToolUseEvent {
            tool_name: "Write".to_string(),
            command: None,
            file_path: Some("/repo/src/main.rs".to_string()),
            index: 0,
        }];

        let results = run_checks(&loaded, &changed_files, &events, &repo_root);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0].reason.as_ref().unwrap().contains("cargo test"));
    }

    #[test]
    fn test_ensure_changed_satisfied() {
        let repo_root = PathBuf::from("/repo");
        let config_dir = repo_root.clone();
        let loaded = make_loaded_config(
            vec![make_check(
                "version",
                "**/*.rs",
                None,
                Some(vec!["version.toml"]),
            )],
            &config_dir,
        );
        let changed_files = vec!["src/main.rs".to_string(), "version.toml".to_string()];
        let events = vec![];

        let results = run_checks(&loaded, &changed_files, &events, &repo_root);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_none());
    }

    #[test]
    fn test_ensure_changed_not_satisfied() {
        let repo_root = PathBuf::from("/repo");
        let config_dir = repo_root.clone();
        let loaded = make_loaded_config(
            vec![make_check(
                "version",
                "**/*.rs",
                None,
                Some(vec!["version.toml"]),
            )],
            &config_dir,
        );
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![];

        let results = run_checks(&loaded, &changed_files, &events, &repo_root);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
        assert!(results[0].reason.as_ref().unwrap().contains("version.toml"));
    }

    #[test]
    fn test_command_run_before_write() {
        let repo_root = PathBuf::from("/repo");
        let config_dir = repo_root.clone();
        let loaded = make_loaded_config(
            vec![make_check(
                "test",
                "**/*.rs",
                Some(vec!["cargo test"]),
                None,
            )],
            &config_dir,
        );
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
                file_path: Some("/repo/src/main.rs".to_string()),
                index: 1,
            },
        ];

        let results = run_checks(&loaded, &changed_files, &events, &repo_root);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_some());
    }

    #[test]
    fn test_multiple_checks() {
        let repo_root = PathBuf::from("/repo");
        let config_dir = repo_root.clone();
        let loaded = make_loaded_config(
            vec![
                make_check("test", "**/*.rs", Some(vec!["cargo test"]), None),
                make_check("fmt", "**/*.rs", Some(vec!["cargo fmt"]), None),
            ],
            &config_dir,
        );
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![
            ToolUseEvent {
                tool_name: "Write".to_string(),
                command: None,
                file_path: Some("/repo/src/main.rs".to_string()),
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

        let results = run_checks(&loaded, &changed_files, &events, &repo_root);
        assert_eq!(results.len(), 2);
        assert!(results[0].reason.is_none()); // cargo test passed
        assert!(results[1].reason.is_some()); // cargo fmt failed
    }

    #[test]
    fn test_no_edit_in_transcript_skips_ensure_commands() {
        // Files are dirty in git but no edits in this session's transcript
        let repo_root = PathBuf::from("/repo");
        let config_dir = repo_root.clone();
        let loaded = make_loaded_config(
            vec![make_check(
                "test",
                "**/*.rs",
                Some(vec!["cargo test"]),
                None,
            )],
            &config_dir,
        );
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![]; // No edits in transcript

        let results = run_checks(&loaded, &changed_files, &events, &repo_root);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_none()); // Should pass - no edit means skip
    }

    #[test]
    fn test_files_outside_config_dir_ignored() {
        // Config is in /repo/packages/foo, files in /repo/packages/bar should be ignored
        let repo_root = PathBuf::from("/repo");
        let config_dir = PathBuf::from("/repo/packages/foo");
        let loaded = make_loaded_config(
            vec![make_check(
                "test",
                "**/*.rs",
                Some(vec!["cargo test"]),
                None,
            )],
            &config_dir,
        );
        // File is in a sibling package - should not match
        let changed_files = vec!["packages/bar/src/lib.rs".to_string()];
        let events = vec![];

        let results = run_checks(&loaded, &changed_files, &events, &repo_root);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_none()); // No match, check skipped
    }

    #[test]
    fn test_ensure_changed_relative_to_config_dir() {
        // Config in a subdirectory, ensure_changed paths relative to it
        let repo_root = PathBuf::from("/repo");
        let config_dir = PathBuf::from("/repo/packages/foo");
        let loaded = make_loaded_config(
            vec![make_check(
                "version",
                "**/*.rs",
                None,
                Some(vec!["version.toml"]),
            )],
            &config_dir,
        );
        // Both files are in the package
        let changed_files = vec![
            "packages/foo/src/main.rs".to_string(),
            "packages/foo/version.toml".to_string(),
        ];
        let events = vec![];

        let results = run_checks(&loaded, &changed_files, &events, &repo_root);
        assert_eq!(results.len(), 1);
        assert!(results[0].reason.is_none());
    }

    #[test]
    fn test_project_relative_files_after_prefix_strip() {
        // After filter_to_project strips the monorepo prefix, files are relative
        // to the project root (== config_dir), not the git root.
        // run_stop_checks passes cwd (project root) as repo_root in this case.
        let project_root = PathBuf::from("/repo/projects/foo");
        let config_dir = project_root.clone();
        let loaded = make_loaded_config(
            vec![make_check(
                "test",
                "**/*.rs",
                Some(vec!["cargo test"]),
                None,
            )],
            &config_dir,
        );
        // Files are project-relative (prefix already stripped)
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![
            ToolUseEvent {
                tool_name: "Edit".to_string(),
                command: None,
                file_path: Some("/repo/projects/foo/src/main.rs".to_string()),
                index: 0,
            },
        ];

        // Pass project_root as repo_root (as run_stop_checks now does)
        let results = run_checks(&loaded, &changed_files, &events, &project_root);
        assert_eq!(results.len(), 1);
        // Should block because cargo test wasn't run after the edit
        assert!(results[0].reason.is_some());
        assert!(results[0].reason.as_ref().unwrap().contains("cargo test"));
    }
}
