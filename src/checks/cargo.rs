use crate::checks::common::{check_commands_after_changes, FileChangeCheck};
use crate::transcript::ToolUseEvent;

/// Required cargo commands when Rust files change
const REQUIRED_COMMANDS: &[(&str, &[&str])] = &[
    ("cargo test", &["cargo test", "cargo t "]),
    ("cargo fmt", &["cargo fmt"]),
    ("cargo clippy", &["cargo clippy"]),
];

fn is_rust_file(f: &str) -> bool {
    f.ends_with(".rs")
}

fn missing_message(missing: &[&str]) -> String {
    format!(
        "Rust files changed but these commands were not run (after last edit): {}",
        missing.join(", ")
    )
}

/// Check if required cargo commands were run when Rust files changed.
/// Returns Some(reason) if blocking, None if OK.
pub fn check(changed_files: &[String], events: &[ToolUseEvent]) -> Option<String> {
    let config = FileChangeCheck {
        file_matcher: is_rust_file,
        required_commands: REQUIRED_COMMANDS,
        missing_message,
    };

    check_commands_after_changes(changed_files, events, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_commands_defined() {
        assert!(!REQUIRED_COMMANDS.is_empty());
        for (name, patterns) in REQUIRED_COMMANDS {
            assert!(!name.is_empty());
            assert!(!patterns.is_empty());
        }
    }
}
