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

## Configuration

Create a `rufio-hooks.yaml` in your project root:
```yaml
presets:
  - cargo

checks:
  - name: meow-fmt
    when:
      paths_changed: "journal/**/*.md"
    then:
      ensure_commands:
        - meow fmt
```

### Presets

Built-in presets provide common check configurations:
| Preset | Triggers | Commands/Checks |
|--------|----------|-----------------|
| `cargo` | `**/*.rs` | `cargo test`, `cargo fmt`, `cargo clippy`; version bump if `package.nix` exists |
| `pnpm` | `**/*.ts` | `pnpm lint`, `pnpm typecheck`, `pnpm test`; version bump if `package.nix` exists |
| `meow` | `**/*.md` | `meow fmt` |
| `ledger` | `**/*.ledger` | `hledger check`, `folio validate` |
| `terraform` | `**/*.tf` | `tofu fmt`, `tflint`, `trivy config .` |
Custom presets can be added to `$XDG_CONFIG_HOME/rufio/presets/{name}.yaml`.

### Custom Checks

Define checks with `when` conditions and `then` actions:
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

## Output

To block Claude from stopping, output JSON to stdout:
```json
{"decision":"block","reason":"Message Claude will see"}
```
Exit code 0 with valid JSON required for the decision to be processed.
