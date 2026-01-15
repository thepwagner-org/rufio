use crate::checks::common::{check_commands_after_changes, FileChangeCheck};
use crate::transcript::ToolUseEvent;

/// Required meow commands when journal files change
const REQUIRED_COMMANDS: &[(&str, &[&str])] = &[("meow fmt", &["meow fmt"])];

fn is_journal_file(f: &str) -> bool {
    // Handle both relative (git diff) and absolute (transcript) paths
    (f.starts_with("journal/") || f.contains("/journal/")) && f.ends_with(".md")
}

fn missing_message(missing: &[&str]) -> String {
    format!(
        "Journal files changed but these commands were not run (after last edit): {}",
        missing.join(", ")
    )
}

/// Check if meow fmt was run when journal files changed.
/// Returns Some(reason) if blocking, None if OK.
pub fn check(changed_files: &[String], events: &[ToolUseEvent]) -> Option<String> {
    let config = FileChangeCheck {
        file_matcher: is_journal_file,
        required_commands: REQUIRED_COMMANDS,
        missing_message,
    };

    check_commands_after_changes(changed_files, events, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_journal_file() {
        // Relative paths (from git diff)
        assert!(is_journal_file("journal/2025-11.md"));
        assert!(is_journal_file("journal/notes.md"));
        // Absolute paths (from transcript)
        assert!(is_journal_file("/Users/foo/project/journal/2025-11.md"));
        assert!(is_journal_file("/home/user/rufio/journal/notes.md"));
        // Non-matches
        assert!(!is_journal_file("journal/foo.txt"));
        assert!(!is_journal_file("src/main.rs"));
        assert!(!is_journal_file("README.md"));
    }
}
