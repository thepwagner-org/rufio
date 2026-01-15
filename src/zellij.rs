use std::io::Write;
use std::process::Command;

/// Braille spinner frames (10-frame cycle)
const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ASKING_CHAR: &str = "⣿"; // Full block - all 8 dots
const DONE_CHAR: &str = "⠶"; // 4-dot square pattern

/// Get the path to the spinner state file for a session
fn spinner_state_path(session_id: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("/tmp/rufio-spinner-{}", session_id))
}

/// Get current spinner frame index, defaulting to 0
fn get_spinner_index(session_id: &str) -> usize {
    let path = spinner_state_path(session_id);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Advance spinner to next frame and return current frame character
fn advance_spinner(session_id: &str) -> &'static str {
    let index = get_spinner_index(session_id);
    let frame = SPINNER_FRAMES[index];
    let next_index = (index + 1) % SPINNER_FRAMES.len();
    let _ = std::fs::write(spinner_state_path(session_id), next_index.to_string());
    frame
}

/// Reset spinner state for a session
fn reset_spinner(session_id: &str) {
    let _ = std::fs::remove_file(spinner_state_path(session_id));
}

/// State of the Claude Code pane for visual indication
pub enum PaneState {
    /// Claude stopped normally - ready for review
    Stopped,
    /// Claude is asking a question
    AskingQuestion,
    /// User resumed - reset to normal (needs session_id for spinner)
    Active,
}

impl PaneState {
    fn name(&self) -> &'static str {
        match self {
            PaneState::Stopped => "Stopped",
            PaneState::AskingQuestion => "AskingQuestion",
            PaneState::Active => "Active",
        }
    }
}

/// Log a message to the session-specific log file
fn log(session_id: &str, message: &str) {
    let path = format!("/tmp/rufio-{}.txt", session_id);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let timestamp = chrono::Local::now().format("%H:%M:%S%.3f");
        let _ = writeln!(file, "[{}] zellij: {}", timestamp, message);
    }
}

/// Derive tab name from cwd.
/// - ~/src/... -> project directory name
/// - ~/.meow/trees/... -> branch name (directory under trees)
/// - fallback -> last path component
fn derive_name_from_cwd(cwd: &str) -> String {
    let path = std::path::Path::new(cwd);

    // Check for ~/.meow/trees/<branch>/...
    if let Some(home) = std::env::var_os("HOME") {
        let meow_trees = std::path::PathBuf::from(&home).join(".meow/trees");
        if let Ok(relative) = path.strip_prefix(&meow_trees) {
            // First component is the branch name
            if let Some(branch) = relative.iter().next() {
                return branch.to_string_lossy().to_string();
            }
        }

        // Check for ~/src/.../<project>/...
        let src_dir = std::path::PathBuf::from(&home).join("src");
        if let Ok(relative) = path.strip_prefix(&src_dir) {
            // Could be src/projects/foo or src/foo - we want the deepest relevant name
            // Walk up from cwd to find a good project name
            let components: Vec<_> = relative.iter().collect();
            if components.len() >= 2 {
                // e.g. projects/rufio -> rufio, or work/myproject -> myproject
                return components[1].to_string_lossy().to_string();
            } else if !components.is_empty() {
                return components[0].to_string_lossy().to_string();
            }
        }
    }

    // Fallback: last path component
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "claude".to_string())
}

/// Find zellij binary, checking common locations
fn find_zellij() -> Option<std::path::PathBuf> {
    // Try PATH first
    if let Ok(output) = Command::new("which").arg("zellij").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(std::path::PathBuf::from(path));
            }
        }
    }

    // Try common nix/homebrew locations
    let candidates = [
        "/etc/profiles/per-user/pwagner/bin/zellij",
        "/run/current-system/sw/bin/zellij",
        "/usr/local/bin/zellij",
        "/opt/homebrew/bin/zellij",
    ];

    for path in candidates {
        let p = std::path::Path::new(path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }

    None
}

/// Update Zellij tab indicator based on Claude state.
/// Uses zellij-plugin to rename the tab by pane ID (works even when tab is not focused).
/// Fails silently - this is a non-critical notification feature.
pub fn update_tab_name(state: PaneState, cwd: &str, session_id: &str) {
    let pane_id = match std::env::var("ZELLIJ_PANE_ID") {
        Ok(id) => id,
        Err(_) => {
            log(session_id, "ZELLIJ_PANE_ID not set, skipping tab update");
            return;
        }
    };

    let zellij_path = match find_zellij() {
        Some(p) => p,
        None => {
            log(session_id, "zellij binary not found, skipping tab update");
            return;
        }
    };

    let name = derive_name_from_cwd(cwd);
    if name == "tmp" {
        log(session_id, "cwd is tmp, skipping tab update");
        return;
    }

    // Get prefix based on state
    let prefix = match &state {
        PaneState::Stopped => {
            reset_spinner(session_id);
            DONE_CHAR
        }
        PaneState::AskingQuestion => ASKING_CHAR,
        PaneState::Active => advance_spinner(session_id),
    };
    let tab_title = format!("{} {}", prefix, name);
    let payload = format!(r#"{{"pane_id": "{}", "name": "{}"}}"#, pane_id, tab_title);

    log(
        session_id,
        &format!(
            "updating tab: state={} pane_id={} title=\"{}\"",
            state.name(),
            pane_id,
            tab_title
        ),
    );

    let result = Command::new(&zellij_path)
        .args(["pipe", "--name", "rename-tab", "--", &payload])
        .output();

    match result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log(session_id, &format!("zellij pipe failed: {}", stderr));
            }
        }
        Err(e) => {
            log(session_id, &format!("zellij pipe error: {}", e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_constants() {
        assert_eq!(SPINNER_FRAMES.len(), 10);
        assert_eq!(ASKING_CHAR, "⣿");
        assert_eq!(DONE_CHAR, "⠶");
    }

    #[test]
    fn test_spinner_advances() {
        let session_id = "test-spinner-advance";
        // Clean up any existing state
        let _ = std::fs::remove_file(spinner_state_path(session_id));

        // First call should return first frame and advance to 1
        assert_eq!(advance_spinner(session_id), "⠋");
        assert_eq!(get_spinner_index(session_id), 1);

        // Second call should return second frame and advance to 2
        assert_eq!(advance_spinner(session_id), "⠙");
        assert_eq!(get_spinner_index(session_id), 2);

        // Clean up
        reset_spinner(session_id);
        assert_eq!(get_spinner_index(session_id), 0);
    }

    #[test]
    fn test_spinner_wraps() {
        let session_id = "test-spinner-wrap";
        // Set spinner to last frame
        let _ = std::fs::write(spinner_state_path(session_id), "9");

        // Should return last frame and wrap to 0
        assert_eq!(advance_spinner(session_id), "⠏");
        assert_eq!(get_spinner_index(session_id), 0);

        // Clean up
        reset_spinner(session_id);
    }

    #[test]
    fn test_derive_name_from_cwd() {
        // Test fallback to last component
        assert_eq!(derive_name_from_cwd("/some/random/path"), "path");
    }
}
