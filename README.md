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
Only `Stop` is required for the quality checks. The other events enable a Zellij tab status indicator.

## Configuration

Create a `rufio-hooks.yaml` in your project root:
```yaml
checks:
  - name: my-check
    when:
      paths_changed: "src/**/*.rs"  # glob pattern (required)
      path_exists: "package.nix"    # only run if this path exists (optional)
    then:
      ensure_commands:              # commands that must have run
        - cargo test
      # OR
      ensure_changed:               # files that must have changed
        - version.toml
```
- `ensure_commands`: verifies these commands ran (in any order) after the matching files changed
- `ensure_changed`: verifies these files were also modified in the session

### Presets

Presets are reusable check collections stored at `$XDG_CONFIG_HOME/rufio/presets/{name}.yaml`:
```yaml
# ~/.config/rufio/presets/cargo.yaml
checks:
  - name: cargo-checks
    when:
      paths_changed: "**/*.rs"
    then:
      ensure_commands:
        - cargo test
        - cargo fmt
        - cargo clippy
```
Reference presets in your project config:
```yaml
presets:
  - cargo

checks:
  - name: version-bump
    when:
      paths_changed: "**/*.rs"
    then:
      ensure_changed:
        - version.toml
```

## Output

To block Claude from stopping, output JSON to stdout:
```json
{"decision":"block","reason":"Message Claude will see"}
```
Exit code 0 with valid JSON required for the decision to be processed.
