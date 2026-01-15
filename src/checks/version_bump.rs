use std::path::Path;

/// Check if a file is a Rust source file (requires version bump when changed)
fn is_rust_source_file(path: &str) -> bool {
    path.ends_with(".rs") || path == "build.rs" || path.ends_with("/build.rs")
}

/// Run the version bump check
/// Returns Some(reason) if blocking, None if OK
pub fn check(cwd: &str, changed_files: &[String]) -> Option<String> {
    let package_nix = Path::new(cwd).join("package.nix");

    // Skip silently if no package.nix (not a Nix-packaged project)
    if !package_nix.exists() {
        return None;
    }

    // Check if version.toml was modified (source of truth for version)
    let version_toml_changed = changed_files
        .iter()
        .any(|f| f == "version.toml" || f.ends_with("/version.toml"));

    // Filter to Rust source files only
    let rust_files: Vec<&str> = changed_files
        .iter()
        .map(|s| s.as_str())
        .filter(|f| is_rust_source_file(f))
        .collect();

    // If Rust files changed but version.toml wasn't bumped, block and remind
    if !rust_files.is_empty() && !version_toml_changed {
        return Some(
            "Rust source files were modified but version.toml was not bumped. Please bump the version following semver.".to_string()
        );
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_source_files() {
        assert!(is_rust_source_file("src/main.rs"));
        assert!(is_rust_source_file("src/lib.rs"));
        assert!(is_rust_source_file("src/checks/version_bump.rs"));
        assert!(is_rust_source_file("build.rs"));
        assert!(is_rust_source_file("crates/foo/build.rs"));
    }

    #[test]
    fn test_non_rust_files() {
        assert!(!is_rust_source_file("README.md"));
        assert!(!is_rust_source_file("Cargo.toml"));
        assert!(!is_rust_source_file("Cargo.lock"));
        assert!(!is_rust_source_file("package.nix"));
        assert!(!is_rust_source_file("index.js"));
        assert!(!is_rust_source_file("app/page.tsx"));
        assert!(!is_rust_source_file("script.py"));
        assert!(!is_rust_source_file(".envrc"));
    }
}
