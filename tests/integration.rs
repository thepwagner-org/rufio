#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::Write;
use std::process::{Command, Stdio};

fn run_rufio(json: &str) -> (String, String, i32) {
    let mut child = Command::new("cargo")
        .args(["run", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(json.as_bytes()).expect("failed to write");
    }

    let output = child.wait_with_output().expect("failed to wait");

    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn test_stop_event_no_package_nix() {
    // Use a temp directory without package.nix
    let json =
        r#"{"hook_event_name":"Stop","cwd":"/tmp","session_id":"test","transcript_path":"/tmp/t"}"#;
    let (stdout, _stderr, code) = run_rufio(json);

    assert_eq!(code, 0);
    assert!(
        stdout.is_empty(),
        "Should produce no output when no package.nix"
    );
}

#[test]
fn test_unknown_event_noop() {
    let json = r#"{"hook_event_name":"Start","cwd":"/tmp","session_id":"test","transcript_path":"/tmp/t"}"#;
    let (stdout, _stderr, code) = run_rufio(json);

    assert_eq!(code, 0);
    assert!(stdout.is_empty(), "Unknown events should be no-op");
}

#[test]
fn test_invalid_json_fails() {
    let json = "not valid json";
    let (_stdout, _stderr, code) = run_rufio(json);

    assert_ne!(code, 0, "Invalid JSON should cause non-zero exit");
}
