# Shared Configuration Format

Rufio hooks use a unified configuration format shared between the Rust (`rufio`) and TypeScript (`rufio-ts`) implementations.

## Config File: `rufio-hooks.yaml`

Placed in project directories. Discovery walks up from changed files to repo root.

```yaml
presets:
  - cargo
  - meow

checks:
  - name: custom-check
    when:
      paths_changed: "**/*.py"
      path_exists: pyproject.toml  # optional
    then:
      ensure_commands:
        - pytest
      # OR
      ensure_changed:
        - CHANGELOG.md
```

### Fields

- `presets`: List of preset names to include (resolved from `$XDG_CONFIG_HOME/rufio/presets/`)
- `checks`: List of custom check definitions

### Check Definition

- `name`: Identifier for error messages
- `when.paths_changed`: Glob pattern matching changed files (relative to config dir)
- `when.path_exists`: Optional condition - check only runs if this path exists
- `then.ensure_commands`: Commands that must run after last matching edit (mutually exclusive with `ensure_changed`)
- `then.ensure_changed`: Paths that must be edited in session (mutually exclusive with `ensure_commands`)

## Preset Files: `$XDG_CONFIG_HOME/rufio/presets/{name}.yaml`

Default location: `~/.config/rufio/presets/`

```yaml
checks:
  - name: cargo-checks
    when:
      paths_changed: "**/*.rs"
    then:
      ensure_commands:
        - cargo test
        - cargo fmt
        - cargo clippy

  - name: cargo-version-bump
    when:
      paths_changed: "**/*.rs"
      path_exists: package.nix
    then:
      ensure_changed:
        - version.toml
```

## Behavior

- **No config found**: Spinner/progress tracking only, no validation checks
- **Preset resolution**: User presets override built-ins with same name
- **Command matching**: Exact match only (no aliases)
- **Monorepo support**: Different configs per package, globs relative to config dir

## Implementation Plan (Rust)

1. Add `serde` and `serde_yaml` dependencies
2. Create `src/config.rs`:
   - Define config structs (`RufioConfig`, `Check`, `When`, `Then`)
   - `find_nearest_config(file_path, repo_root)` - walk up to find `rufio-hooks.yaml`
   - `load_config(path)` - parse and validate YAML
   - `resolve_presets(names)` - load from XDG or built-ins
   - `group_files_by_config(files, repo_root)` - group changed files by nearest config
3. Create `src/presets.rs` - built-in preset definitions (temporary, move to XDG later)
4. Refactor `src/checks/`:
   - Remove hardcoded check modules (`cargo.rs`, `meow.rs`, `version_bump.rs`)
   - Create generic check runner that executes checks from config
   - Keep `common.rs` utilities for transcript/file matching
5. Update `run_stop_checks()` in `main.rs`:
   - Load config (or skip validation if none)
   - Run generic check runner against loaded checks
6. Update tests
