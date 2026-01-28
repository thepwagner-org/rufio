use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILENAME: &str = "rufio-hooks.yaml";

/// Conditions that trigger a check
#[derive(Debug, Clone, Deserialize)]
pub struct When {
    /// Glob pattern for files that trigger this check (relative to config dir)
    pub paths_changed: String,
    /// Optional: check only applies if this path exists (relative to config dir)
    pub path_exists: Option<String>,
}

/// Actions required when check triggers - mutually exclusive
#[derive(Debug, Clone, Deserialize)]
pub struct Then {
    /// Commands that must ALL run after the last matching edit
    pub ensure_commands: Option<Vec<String>>,
    /// At least one of these paths must have been edited this session
    pub ensure_changed: Option<Vec<String>>,
}

/// A single check definition
#[derive(Debug, Clone, Deserialize)]
pub struct Check {
    /// Name of the check (for error messages)
    pub name: String,
    /// Conditions that trigger this check
    pub when: When,
    /// Required actions
    pub then: Then,
}

/// Raw configuration structure (as parsed from YAML)
#[derive(Debug, Deserialize)]
struct RufioConfigRaw {
    /// Built-in preset names to include
    presets: Option<Vec<String>>,
    /// Custom check definitions
    checks: Option<Vec<Check>>,
}

/// Preset file structure
#[derive(Debug, Deserialize)]
struct PresetFile {
    checks: Vec<Check>,
}

/// Resolved configuration (presets expanded, checks always defined)
#[derive(Debug)]
pub struct RufioConfig {
    pub checks: Vec<Check>,
}

/// Parsed config with its location
#[derive(Debug)]
pub struct LoadedConfig {
    pub config: RufioConfig,
    /// Directory containing the config file
    pub config_dir: PathBuf,
}

/// Resolves preset names to their check definitions from XDG config
fn resolve_presets(preset_names: &[String], config_path: &Path) -> Result<Vec<Check>> {
    let mut checks = Vec::new();

    for name in preset_names {
        match load_preset_from_xdg(name)? {
            Some(xdg_checks) => checks.extend(xdg_checks),
            None => {
                let expected_path = get_preset_path(name);
                bail!(
                    "Invalid config at {}: preset '{}' not found at {}",
                    config_path.display(),
                    name,
                    expected_path.display()
                );
            }
        }
    }

    Ok(checks)
}

/// Get the expected path for a preset in XDG config
fn get_preset_path(name: &str) -> PathBuf {
    let xdg_config = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config")
        });

    xdg_config
        .join("rufio")
        .join("presets")
        .join(format!("{}.yaml", name))
}

/// Try to load a preset from $XDG_CONFIG_HOME/rufio/presets/{name}.yaml
fn load_preset_from_xdg(name: &str) -> Result<Option<Vec<Check>>> {
    let preset_path = get_preset_path(name);

    if !preset_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&preset_path)
        .with_context(|| format!("Failed to read preset file: {}", preset_path.display()))?;

    let preset: PresetFile = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse preset file: {}", preset_path.display()))?;

    Ok(Some(preset.checks))
}

/// Validates a check definition
fn validate_check(check: &Check, config_path: &Path) -> Result<()> {
    if check.name.is_empty() {
        bail!(
            "Invalid config at {}: check missing 'name'",
            config_path.display()
        );
    }
    if check.when.paths_changed.is_empty() {
        bail!(
            "Invalid config at {}: check '{}' missing 'when.paths_changed'",
            config_path.display(),
            check.name
        );
    }
    if check.then.ensure_commands.is_none() && check.then.ensure_changed.is_none() {
        bail!(
            "Invalid config at {}: check '{}' must have 'then.ensure_commands' or 'then.ensure_changed'",
            config_path.display(),
            check.name
        );
    }
    if check.then.ensure_commands.is_some() && check.then.ensure_changed.is_some() {
        bail!(
            "Invalid config at {}: check '{}' cannot have both 'then.ensure_commands' and 'then.ensure_changed'",
            config_path.display(),
            check.name
        );
    }
    Ok(())
}

/// Loads and parses a rufio-hooks.yaml config file.
/// Resolves presets and merges them with custom checks.
pub fn load_config(config_path: &Path) -> Result<RufioConfig> {
    let content = fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config: {}", config_path.display()))?;

    let parsed: RufioConfigRaw = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse config: {}", config_path.display()))?;

    // Resolve presets first
    let preset_checks = if let Some(presets) = &parsed.presets {
        resolve_presets(presets, config_path)?
    } else {
        Vec::new()
    };

    let user_checks = parsed.checks.unwrap_or_default();

    // Merge: presets first, then user checks
    let mut merged_checks = preset_checks;
    merged_checks.extend(user_checks.iter().cloned());

    if merged_checks.is_empty() {
        bail!(
            "Invalid config at {}: no checks defined (add 'presets' or 'checks')",
            config_path.display()
        );
    }

    // Validate user checks (preset checks are trusted)
    for check in &user_checks {
        validate_check(check, config_path)?;
    }

    Ok(RufioConfig {
        checks: merged_checks,
    })
}

/// Finds the nearest rufio-hooks.yaml config file by walking up from a directory.
/// Stops at the repository root (does not leave the repo).
///
/// Returns LoadedConfig if found, None otherwise.
pub fn find_nearest_config(start_dir: &Path, repo_root: &Path) -> Option<LoadedConfig> {
    let mut current = start_dir.to_path_buf();

    loop {
        let config_path = current.join(CONFIG_FILENAME);

        if config_path.exists() {
            match load_config(&config_path) {
                Ok(config) => {
                    return Some(LoadedConfig {
                        config,
                        config_dir: current,
                    });
                }
                Err(_) => {
                    // Invalid config, skip and continue searching
                    // (Could log this error if needed)
                }
            }
        }

        // Stop if we've reached repo root
        if current == repo_root {
            return None;
        }

        // Move up
        if !current.pop() {
            return None;
        }

        // Safety: don't go above repo root
        if !current.starts_with(repo_root) {
            return None;
        }
    }
}

/// Groups changed files by their nearest config.
/// Returns a map of config_dir -> (LoadedConfig, files)
#[allow(dead_code)]
pub fn group_files_by_config(
    changed_files: &[String],
    cwd: &Path,
    repo_root: &Path,
) -> Vec<(LoadedConfig, Vec<String>)> {
    use std::collections::HashMap;

    let mut groups: HashMap<PathBuf, (LoadedConfig, Vec<String>)> = HashMap::new();

    for file in changed_files {
        // Resolve the file path to find its config
        let file_path = if Path::new(file).is_absolute() {
            PathBuf::from(file)
        } else {
            cwd.join(file)
        };

        let file_dir = file_path.parent().unwrap_or(&file_path);

        if let Some(loaded) = find_nearest_config(file_dir, repo_root) {
            let config_dir = loaded.config_dir.clone();
            groups
                .entry(config_dir)
                .or_insert_with(|| (loaded, Vec::new()))
                .1
                .push(file.clone());
        }
    }

    groups.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_load_config_with_checks() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join(CONFIG_FILENAME);
        fs::write(
            &config_path,
            r#"
checks:
  - name: test-check
    when:
      paths_changed: "**/*.rs"
    then:
      ensure_commands:
        - cargo test
"#,
        )
        .unwrap();

        let config = load_config(&config_path).unwrap();
        assert_eq!(config.checks.len(), 1);
        assert_eq!(config.checks[0].name, "test-check");
    }

    #[test]
    fn test_load_config_with_ensure_changed() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join(CONFIG_FILENAME);
        fs::write(
            &config_path,
            r#"
checks:
  - name: version-bump
    when:
      paths_changed: "**/*.rs"
      path_exists: package.nix
    then:
      ensure_changed:
        - version.toml
"#,
        )
        .unwrap();

        let config = load_config(&config_path).unwrap();
        assert_eq!(config.checks.len(), 1);
        assert!(config.checks[0].then.ensure_changed.is_some());
    }

    #[test]
    fn test_load_config_empty_fails() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join(CONFIG_FILENAME);
        fs::write(&config_path, "{}").unwrap();

        assert!(load_config(&config_path).is_err());
    }

    #[test]
    fn test_load_config_both_then_fields_fails() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join(CONFIG_FILENAME);
        fs::write(
            &config_path,
            r#"
checks:
  - name: bad-check
    when:
      paths_changed: "**/*.rs"
    then:
      ensure_commands:
        - cargo test
      ensure_changed:
        - version.toml
"#,
        )
        .unwrap();

        assert!(load_config(&config_path).is_err());
    }

    #[test]
    fn test_find_nearest_config() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path();
        let subdir = repo_root.join("src/lib");
        fs::create_dir_all(&subdir).unwrap();

        let config_path = repo_root.join(CONFIG_FILENAME);
        fs::write(
            &config_path,
            r#"
checks:
  - name: test
    when:
      paths_changed: "**/*.rs"
    then:
      ensure_commands:
        - cargo test
"#,
        )
        .unwrap();

        let loaded = find_nearest_config(&subdir, repo_root);
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().config_dir, repo_root);
    }

    #[test]
    fn test_find_nearest_config_nested() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path();
        let pkg_dir = repo_root.join("packages/foo");
        let src_dir = pkg_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        // Config at package level, not repo root
        let config_path = pkg_dir.join(CONFIG_FILENAME);
        fs::write(
            &config_path,
            r#"
checks:
  - name: pkg-check
    when:
      paths_changed: "**/*.ts"
    then:
      ensure_commands:
        - pnpm test
"#,
        )
        .unwrap();

        let loaded = find_nearest_config(&src_dir, repo_root);
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().config_dir, pkg_dir);
    }

    #[test]
    fn test_find_nearest_config_none() {
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path();
        let subdir = repo_root.join("src");
        fs::create_dir_all(&subdir).unwrap();

        let loaded = find_nearest_config(&subdir, repo_root);
        assert!(loaded.is_none());
    }
}
