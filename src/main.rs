use anyhow::Result;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

mod checks;
mod input;
mod transcript;
mod zellij;

use input::HookInput;

/// Log a message to /tmp/rufio-{session_id}.txt if running in Zellij.
/// This is for debugging hook behavior.
fn log_if_zellij(session_id: &str, message: &str) {
    if std::env::var("ZELLIJ_PANE_ID").is_ok() {
        let path = format!("/tmp/rufio-{}.txt", session_id);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let timestamp = chrono::Local::now().format("%H:%M:%S%.3f");
            let _ = writeln!(file, "[{}] {}", timestamp, message);
        }
    }
}

fn main() -> Result<()> {
    let input = read_input()?;

    log_if_zellij(
        &input.session_id,
        &format!(
            "event={} tool={:?}",
            input.hook_event_name,
            input.tool_name.as_deref().unwrap_or("None")
        ),
    );

    match input.hook_event_name.as_str() {
        "Stop" => {
            run_stop_checks(&input)?;
        }
        "PostToolUse" => {
            // Clear asking marker if it exists
            let marker = asking_marker_path(&input.session_id);
            if marker.exists() {
                log_if_zellij(
                    &input.session_id,
                    &format!(
                        "PostToolUse: {} completed, clearing asking marker",
                        input.tool_name.as_deref().unwrap_or("unknown")
                    ),
                );
                let _ = std::fs::remove_file(&marker);
            }
            // Always tick spinner
            log_if_zellij(
                &input.session_id,
                &format!(
                    "PostToolUse: {} -> ticking spinner",
                    input.tool_name.as_deref().unwrap_or("unknown")
                ),
            );
            zellij::update_tab_name(zellij::PaneState::Active, &input.cwd, &input.session_id);
        }
        "PermissionRequest" => {
            log_if_zellij(
                &input.session_id,
                "PermissionRequest -> setting question state",
            );
            zellij::update_tab_name(
                zellij::PaneState::AskingQuestion,
                &input.cwd,
                &input.session_id,
            );
            let marker = asking_marker_path(&input.session_id);
            let _ = std::fs::write(&marker, "");
        }
        "PreToolUse" => {
            // Clear asking marker if it exists (permission was granted)
            let marker = asking_marker_path(&input.session_id);
            if marker.exists() {
                log_if_zellij(&input.session_id, "PreToolUse: clearing asking marker");
                let _ = std::fs::remove_file(&marker);
            }
            // Always tick spinner
            log_if_zellij(
                &input.session_id,
                &format!(
                    "PreToolUse: {} -> ticking spinner",
                    input.tool_name.as_deref().unwrap_or("unknown")
                ),
            );
            zellij::update_tab_name(zellij::PaneState::Active, &input.cwd, &input.session_id);
        }
        "UserPromptSubmit" => {
            // Clear asking marker if present
            let marker = asking_marker_path(&input.session_id);
            if marker.exists() {
                log_if_zellij(
                    &input.session_id,
                    "UserPromptSubmit: clearing asking marker",
                );
                let _ = std::fs::remove_file(&marker);
            }
            log_if_zellij(&input.session_id, "UserPromptSubmit -> active state");
            zellij::update_tab_name(zellij::PaneState::Active, &input.cwd, &input.session_id);
        }
        _ => {
            log_if_zellij(
                &input.session_id,
                &format!("unhandled event: {}", input.hook_event_name),
            );
        }
    }

    Ok(())
}

fn run_stop_checks(input: &HookInput) -> Result<()> {
    // Get changed files ONCE
    let changed_files = get_changed_files(&input.cwd);
    log_if_zellij(
        &input.session_id,
        &format!("Stop: {} changed files", changed_files.len()),
    );

    // Parse transcript ONCE
    let events = transcript::extract_tool_events(&input.transcript_path)?;
    log_if_zellij(
        &input.session_id,
        &format!("Stop: {} transcript events", events.len()),
    );

    let mut reasons: Vec<String> = Vec::new();

    // Run checks FIRST before updating Zellij state
    if let Some(reason) = checks::version_bump::check(&input.cwd, &changed_files) {
        log_if_zellij(
            &input.session_id,
            &format!("check version_bump: BLOCK - {}", reason),
        );
        reasons.push(reason);
    } else {
        log_if_zellij(&input.session_id, "check version_bump: pass");
    }
    if let Some(reason) = checks::cargo::check(&changed_files, &events) {
        log_if_zellij(
            &input.session_id,
            &format!("check cargo: BLOCK - {}", reason),
        );
        reasons.push(reason);
    } else {
        log_if_zellij(&input.session_id, "check cargo: pass");
    }
    if let Some(reason) = checks::meow::check(&changed_files, &events) {
        log_if_zellij(
            &input.session_id,
            &format!("check meow: BLOCK - {}", reason),
        );
        reasons.push(reason);
    } else {
        log_if_zellij(&input.session_id, "check meow: pass");
    }

    // Update Zellij AFTER checks - only show Stopped if not blocking
    let marker = asking_marker_path(&input.session_id);
    if marker.exists() {
        log_if_zellij(&input.session_id, "Stop: asking marker exists, removing it");
        let _ = std::fs::remove_file(&marker);
    } else if reasons.is_empty() {
        log_if_zellij(&input.session_id, "Stop: all checks pass -> stopped state");
        zellij::update_tab_name(zellij::PaneState::Stopped, &input.cwd, &input.session_id);
    } else {
        log_if_zellij(
            &input.session_id,
            "Stop: checks failed -> active state (blocking)",
        );
        zellij::update_tab_name(zellij::PaneState::Active, &input.cwd, &input.session_id);
    }

    // Output blocking JSON if any reasons
    if !reasons.is_empty() {
        let combined = reasons.join(" | ");
        log_if_zellij(
            &input.session_id,
            &format!("Stop: outputting block JSON: {}", combined),
        );
        #[allow(clippy::print_stdout)]
        {
            println!(r#"{{"decision":"block","reason":"{}"}}"#, combined);
        }
    }

    Ok(())
}

fn get_changed_files(cwd: &str) -> Vec<String> {
    let output = match Command::new("git")
        .args(["status", "--porcelain", "-uall"])
        .current_dir(cwd)
        .output()
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Porcelain format: "XY filename" where XY is 2-char status, then space, then filename
    let all_files: Vec<String> = stdout
        .lines()
        .filter_map(|line| line.get(3..))
        .map(String::from)
        .collect();

    // Filter to files within project boundary
    filter_to_project(cwd, all_files)
}

/// Filter files to only those within the project boundary.
/// Returns files with the project prefix stripped if applicable.
fn filter_to_project(cwd: &str, files: Vec<String>) -> Vec<String> {
    let git_root = match get_git_root(cwd) {
        Some(root) => root,
        None => return files, // Not in a git repo, return as-is
    };

    let project_root = match find_project_root(cwd, &git_root) {
        Some(root) => root,
        None => return files, // No marker found, use git root (current behavior)
    };

    // If project root IS the git root, no filtering needed
    if project_root == git_root {
        return files;
    }

    // Compute relative path from git root to project root
    let prefix = match project_root.strip_prefix(&git_root) {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return files, // Shouldn't happen, but be safe
    };

    // Filter files that start with the project prefix
    files
        .into_iter()
        .filter(|f| f.starts_with(&prefix))
        .collect()
}

/// Find the project root by walking up from cwd looking for marker files.
/// Stops at git_root. Returns None if no marker found.
fn find_project_root(cwd: &str, git_root: &Path) -> Option<PathBuf> {
    let mut current = PathBuf::from(cwd);

    loop {
        // Check for marker files
        if current.join("shell.nix").exists() || current.join("CLAUDE.md").exists() {
            return Some(current);
        }

        // Stop if we've reached git root
        if current == git_root {
            return None;
        }

        // Move up
        if !current.pop() {
            return None;
        }
    }
}

/// Get the git repository root directory.
fn get_git_root(cwd: &str) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout);
        Some(PathBuf::from(path.trim()))
    } else {
        None
    }
}

fn read_input() -> Result<HookInput> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    let input: HookInput = serde_json::from_str(&buffer)?;
    Ok(input)
}

/// Get path to marker file that indicates AskUserQuestion was just used
fn asking_marker_path(session_id: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/rufio-asking-{}", session_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_find_project_root_with_shell_nix() {
        let temp = TempDir::new().unwrap();
        let git_root = temp.path();
        let subproject = git_root.join("projects/foo");
        fs::create_dir_all(&subproject).unwrap();
        fs::write(subproject.join("shell.nix"), "").unwrap();

        let result = find_project_root(subproject.to_str().unwrap(), git_root);
        assert_eq!(result, Some(subproject));
    }

    #[test]
    fn test_find_project_root_with_claude_md() {
        let temp = TempDir::new().unwrap();
        let git_root = temp.path();
        let subproject = git_root.join("projects/bar");
        fs::create_dir_all(&subproject).unwrap();
        fs::write(subproject.join("CLAUDE.md"), "").unwrap();

        let result = find_project_root(subproject.to_str().unwrap(), git_root);
        assert_eq!(result, Some(subproject));
    }

    #[test]
    fn test_find_project_root_walks_up() {
        let temp = TempDir::new().unwrap();
        let git_root = temp.path();
        let subproject = git_root.join("projects/baz");
        let deep_dir = subproject.join("src/lib");
        fs::create_dir_all(&deep_dir).unwrap();
        fs::write(subproject.join("shell.nix"), "").unwrap();

        // Start from deep_dir, should find shell.nix in subproject
        let result = find_project_root(deep_dir.to_str().unwrap(), git_root);
        assert_eq!(result, Some(subproject));
    }

    #[test]
    fn test_find_project_root_no_marker_returns_none() {
        let temp = TempDir::new().unwrap();
        let git_root = temp.path();
        let subdir = git_root.join("some/path");
        fs::create_dir_all(&subdir).unwrap();

        let result = find_project_root(subdir.to_str().unwrap(), git_root);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_project_root_marker_at_git_root() {
        let temp = TempDir::new().unwrap();
        let git_root = temp.path();
        fs::write(git_root.join("CLAUDE.md"), "").unwrap();

        let result = find_project_root(git_root.to_str().unwrap(), git_root);
        assert_eq!(result, Some(git_root.to_path_buf()));
    }
}
