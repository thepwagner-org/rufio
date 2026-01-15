use crate::transcript::ToolUseEvent;

/// Configuration for a file change check
pub struct FileChangeCheck<'a> {
    pub file_matcher: fn(&str) -> bool,
    pub required_commands: &'a [(&'a str, &'a [&'a str])],
    pub missing_message: fn(&[&str]) -> String,
}

/// Check if required commands were run after matching files changed.
/// Takes pre-fetched changed_files and transcript events to avoid redundant work.
/// Returns Some(reason) if blocking, None if OK.
pub fn check_commands_after_changes(
    changed_files: &[String],
    events: &[ToolUseEvent],
    config: &FileChangeCheck,
) -> Option<String> {
    // Check if any matching files changed
    let matching_files_changed = changed_files.iter().any(|f| (config.file_matcher)(f));

    if !matching_files_changed {
        return None;
    }

    // Find the index of the last matching file write
    let last_write_idx = events.iter().rposition(|e| {
        (e.tool_name == "Edit" || e.tool_name == "Write")
            && e.file_path
                .as_ref()
                .is_some_and(|p| (config.file_matcher)(p))
    });

    // Check which required commands are missing (must run AFTER last write)
    let mut missing: Vec<&str> = Vec::new();

    for (name, patterns) in config.required_commands {
        let was_run_after_write = events.iter().any(|e| {
            e.tool_name == "Bash"
                && e.command
                    .as_ref()
                    .is_some_and(|cmd| patterns.iter().any(|p| cmd.contains(p)))
                && e.index > last_write_idx.unwrap_or(0)
        });
        if !was_run_after_write {
            missing.push(name);
        }
    }

    if missing.is_empty() {
        None
    } else {
        Some((config.missing_message)(&missing))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rs_matcher(f: &str) -> bool {
        f.ends_with(".rs")
    }

    fn make_message(missing: &[&str]) -> String {
        format!("Missing: {}", missing.join(", "))
    }

    #[test]
    fn test_no_matching_files_returns_none() {
        let changed_files = vec!["README.md".to_string()];
        let events = vec![];
        let config = FileChangeCheck {
            file_matcher: rs_matcher,
            required_commands: &[("cargo test", &["cargo test"])],
            missing_message: make_message,
        };

        assert!(check_commands_after_changes(&changed_files, &events, &config).is_none());
    }

    #[test]
    fn test_matching_files_but_command_run_returns_none() {
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
        let config = FileChangeCheck {
            file_matcher: rs_matcher,
            required_commands: &[("cargo test", &["cargo test"])],
            missing_message: make_message,
        };

        assert!(check_commands_after_changes(&changed_files, &events, &config).is_none());
    }

    #[test]
    fn test_matching_files_command_not_run_returns_reason() {
        let changed_files = vec!["src/main.rs".to_string()];
        let events = vec![ToolUseEvent {
            tool_name: "Write".to_string(),
            command: None,
            file_path: Some("src/main.rs".to_string()),
            index: 0,
        }];
        let config = FileChangeCheck {
            file_matcher: rs_matcher,
            required_commands: &[("cargo test", &["cargo test"])],
            missing_message: make_message,
        };

        let result = check_commands_after_changes(&changed_files, &events, &config);
        assert!(result.is_some());
        assert!(result.as_ref().is_some_and(|r| r.contains("cargo test")));
    }

    #[test]
    fn test_command_run_before_write_returns_reason() {
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
        let config = FileChangeCheck {
            file_matcher: rs_matcher,
            required_commands: &[("cargo test", &["cargo test"])],
            missing_message: make_message,
        };

        let result = check_commands_after_changes(&changed_files, &events, &config);
        assert!(result.is_some());
    }
}
