use anyhow::Result;
use std::fs::OpenOptions;
use std::io::{self, Read, Write as IoWrite};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info};

mod checks;
mod config;
mod input;
mod transcript;

use config::group_files_by_config;
use input::HookInput;

/// Write to a log file when RUFIO_LOG is set, e.g. RUFIO_LOG=/tmp/rufio.log.
fn log(msg: &str) {
    let path = match std::env::var("RUFIO_LOG") {
        Ok(p) if !p.is_empty() => p,
        _ => return,
    };
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", msg);
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("rufio=debug".parse()?),
        )
        .init();

    log("rufio invoked");

    let input = read_input()?;

    log(&format!(
        "hook_event={} cwd={} transcript={}",
        input.hook_event_name, input.cwd, input.transcript_path
    ));

    info!(hook_event = %input.hook_event_name, cwd = %input.cwd);

    if input.hook_event_name == "Stop" {
        if input.stop_hook_active {
            log("stop_hook_active=true, skipping checks to avoid loop");
        } else {
            run_stop_checks(&input)?;
        }
    } else {
        log(&format!("ignoring event: {}", input.hook_event_name));
    }

    Ok(())
}

fn run_stop_checks(input: &HookInput) -> Result<()> {
    log("running stop checks");
    let changed_files = get_changed_files(&input.cwd);
    let events = transcript::extract_tool_events(&input.transcript_path)?;

    log(&format!("changed_files={:?}", changed_files));
    log(&format!("transcript_events={}", events.len()));
    for e in &events {
        log(&format!(
            "  event: tool={} cmd={:?} file={:?} idx={}",
            e.tool_name, e.command, e.file_path, e.index
        ));
    }

    debug!(?changed_files);

    let mut reasons: Vec<String> = Vec::new();

    let cwd_path = Path::new(&input.cwd);
    let repo_root = get_git_root(&input.cwd).unwrap_or_else(|| cwd_path.to_path_buf());

    // Group files by their nearest config and run each config's checks
    let groups = group_files_by_config(&changed_files, cwd_path, &repo_root);

    log(&format!("groups={}", groups.len()));
    for (loaded, files) in &groups {
        log(&format!(
            "  group config_dir={} files={:?}",
            loaded.config_dir.display(),
            files
        ));
    }

    debug!(groups = groups.len());

    for (loaded, files) in &groups {
        let results = checks::run_checks(loaded, files, &events, cwd_path);

        for result in results {
            log(&format!(
                "  check={} reason={:?}",
                result.check_name, result.reason
            ));
            if let Some(reason) = result.reason {
                reasons.push(reason);
            }
        }
    }

    if !reasons.is_empty() {
        let combined = reasons.join(" | ");
        log(&format!("BLOCKING: {}", combined));
        #[allow(clippy::print_stdout)]
        {
            println!(r#"{{"decision":"block","reason":"{}"}}"#, combined);
        }
    } else {
        log("all checks passed, not blocking");
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
    let all_files: Vec<String> = stdout
        .lines()
        .filter_map(|line| line.get(3..))
        .map(String::from)
        .collect();

    filter_to_project(cwd, all_files)
}

/// Filter files to only those within the project boundary.
/// Returns files with the project prefix stripped if applicable.
fn filter_to_project(cwd: &str, files: Vec<String>) -> Vec<String> {
    let git_root = match get_git_root(cwd) {
        Some(root) => root,
        None => return files,
    };

    let project_root = match find_project_root(cwd, &git_root) {
        Some(root) => root,
        None => return files,
    };

    strip_project_prefix(files, &git_root, &project_root)
}

/// Strip the project prefix from git-root-relative file paths.
/// When project_root == git_root, returns files unchanged.
/// Otherwise filters to files under the project and strips the prefix.
fn strip_project_prefix(files: Vec<String>, git_root: &Path, project_root: &Path) -> Vec<String> {
    if project_root == git_root {
        return files;
    }

    let prefix = match project_root.strip_prefix(git_root) {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return files,
    };

    let prefix_with_slash = format!("{}/", prefix);

    files
        .into_iter()
        .filter(|f| f.starts_with(&prefix_with_slash))
        .map(|f| f[prefix_with_slash.len()..].to_string())
        .collect()
}

/// Find the project root by walking up from cwd looking for marker files.
/// Stops at git_root. Returns None if no marker found.
fn find_project_root(cwd: &str, git_root: &Path) -> Option<PathBuf> {
    let mut current = PathBuf::from(cwd);

    loop {
        if current.join("shell.nix").exists() || current.join("CLAUDE.md").exists() {
            return Some(current);
        }

        if current == git_root {
            return None;
        }

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

    #[test]
    fn test_strip_project_prefix_in_monorepo() {
        let git_root = PathBuf::from("/repo");
        let project_root = PathBuf::from("/repo/projects/foo");

        let files = vec![
            "projects/foo/src/main.rs".to_string(),
            "projects/foo/src/lib.rs".to_string(),
            "projects/bar/other.rs".to_string(),
        ];

        let result = strip_project_prefix(files, &git_root, &project_root);

        assert_eq!(result, vec!["src/main.rs", "src/lib.rs"]);
    }

    #[test]
    fn test_strip_project_prefix_no_strip_when_at_git_root() {
        let git_root = PathBuf::from("/repo");
        let project_root = PathBuf::from("/repo");

        let files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];

        let result = strip_project_prefix(files, &git_root, &project_root);

        assert_eq!(result, vec!["src/main.rs", "src/lib.rs"]);
    }
}
