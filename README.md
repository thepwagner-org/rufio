# rufio

Claude Code hook handler that enforces code quality checks.

## Usage

Pipe hook JSON to stdin:
```bash
echo '{"hook_event_name":"Stop","cwd":"/path","session_id":"x","transcript_path":"/t"}' | cargo run
```

## Checks

### Version Bump (Nix projects)

For projects with `package.nix`: blocks if functional code changed but `Cargo.toml` version wasn't bumped. Ignores meta files (`.md`, `.lock`, `.nix`, `.claude/`, `.envrc`, `.gitignore`).

### Cargo Commands (Rust projects)

For projects with `Cargo.toml`: requires `cargo test`, `cargo fmt`, and `cargo clippy` to run after any `.rs` file edits. Parses the transcript to verify commands ran in correct order.

## Output

To block Claude from stopping, output JSON to stdout:
```json
{"decision":"block","reason":"Message Claude will see"}
```
Exit code 0 with valid JSON required for the decision to be processed.
