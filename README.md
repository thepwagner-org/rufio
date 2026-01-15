# rufio

Claude Code hook handler that enforces code quality checks.

## Usage

Add to `~/.claude/settings.json`:
```json
{
  "hooks": {
    "Notification": [{"matcher": "", "hooks": [{"type": "command", "command": "/path/to/rufio"}]}],
    "PermissionRequest": [{"matcher": "", "hooks": [{"type": "command", "command": "/path/to/rufio"}]}],
    "PostToolUse": [{"matcher": "", "hooks": [{"type": "command", "command": "/path/to/rufio"}]}],
    "PreToolUse": [{"matcher": "", "hooks": [{"type": "command", "command": "/path/to/rufio"}]}],
    "SessionEnd": [{"matcher": "", "hooks": [{"type": "command", "command": "/path/to/rufio"}]}],
    "SessionStart": [{"matcher": "", "hooks": [{"type": "command", "command": "/path/to/rufio"}]}],
    "Stop": [{"matcher": "", "hooks": [{"type": "command", "command": "/path/to/rufio"}]}],
    "SubagentStop": [{"matcher": "", "hooks": [{"type": "command", "command": "/path/to/rufio"}]}],
    "UserPromptSubmit": [{"matcher": "", "hooks": [{"type": "command", "command": "/path/to/rufio"}]}]
  }
}
```

Only `Stop` is required for the quality checks. The other events enable the status spinner.

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
