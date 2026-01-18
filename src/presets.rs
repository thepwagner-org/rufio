use crate::config::{Check, Then, When};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Built-in presets that can be referenced in rufio-hooks.yaml via `presets: ["name"]`
pub static PRESETS: LazyLock<HashMap<&'static str, Vec<Check>>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    m.insert(
        "cargo",
        vec![
            Check {
                name: "cargo-checks".to_string(),
                when: When {
                    paths_changed: "**/*.rs".to_string(),
                    path_exists: None,
                },
                then: Then {
                    ensure_commands: Some(vec![
                        "cargo test".to_string(),
                        "cargo fmt".to_string(),
                        "cargo clippy".to_string(),
                    ]),
                    ensure_changed: None,
                },
            },
            Check {
                name: "cargo-version-bump".to_string(),
                when: When {
                    paths_changed: "**/*.rs".to_string(),
                    path_exists: Some("package.nix".to_string()),
                },
                then: Then {
                    ensure_commands: None,
                    ensure_changed: Some(vec!["version.toml".to_string()]),
                },
            },
        ],
    );

    m.insert(
        "meow",
        vec![Check {
            name: "meow-fmt".to_string(),
            when: When {
                paths_changed: "**/*.md".to_string(),
                path_exists: None,
            },
            then: Then {
                ensure_commands: Some(vec!["meow fmt".to_string()]),
                ensure_changed: None,
            },
        }],
    );

    m.insert(
        "pnpm",
        vec![
            Check {
                name: "pnpm-checks".to_string(),
                when: When {
                    paths_changed: "**/*.ts".to_string(),
                    path_exists: None,
                },
                then: Then {
                    ensure_commands: Some(vec![
                        "pnpm lint".to_string(),
                        "pnpm typecheck".to_string(),
                        "pnpm test".to_string(),
                    ]),
                    ensure_changed: None,
                },
            },
            Check {
                name: "pnpm-version-bump".to_string(),
                when: When {
                    paths_changed: "**/*.ts".to_string(),
                    path_exists: Some("package.nix".to_string()),
                },
                then: Then {
                    ensure_commands: None,
                    ensure_changed: Some(vec!["version.toml".to_string()]),
                },
            },
        ],
    );

    m.insert(
        "ledger",
        vec![Check {
            name: "ledger-checks".to_string(),
            when: When {
                paths_changed: "**/*.ledger".to_string(),
                path_exists: None,
            },
            then: Then {
                ensure_commands: Some(vec![
                    "hledger check".to_string(),
                    "folio validate".to_string(),
                ]),
                ensure_changed: None,
            },
        }],
    );

    m.insert(
        "terraform",
        vec![Check {
            name: "terraform-checks".to_string(),
            when: When {
                paths_changed: "**/*.tf".to_string(),
                path_exists: None,
            },
            then: Then {
                ensure_commands: Some(vec![
                    "tofu fmt".to_string(),
                    "tflint".to_string(),
                    "trivy config .".to_string(),
                ]),
                ensure_changed: None,
            },
        }],
    );

    m
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presets_exist() {
        assert!(PRESETS.contains_key("cargo"));
        assert!(PRESETS.contains_key("meow"));
        assert!(PRESETS.contains_key("pnpm"));
        assert!(PRESETS.contains_key("ledger"));
        assert!(PRESETS.contains_key("terraform"));
    }

    #[test]
    fn test_cargo_preset_has_checks() {
        let cargo = PRESETS.get("cargo").unwrap();
        assert_eq!(cargo.len(), 2);
        assert_eq!(cargo[0].name, "cargo-checks");
        assert_eq!(cargo[1].name, "cargo-version-bump");
    }

    #[test]
    fn test_check_fields_valid() {
        for (name, checks) in PRESETS.iter() {
            for check in checks {
                assert!(
                    !check.name.is_empty(),
                    "preset {} has empty check name",
                    name
                );
                assert!(
                    !check.when.paths_changed.is_empty(),
                    "preset {} check {} has empty paths_changed",
                    name,
                    check.name
                );
                assert!(
                    check.then.ensure_commands.is_some() || check.then.ensure_changed.is_some(),
                    "preset {} check {} has no then action",
                    name,
                    check.name
                );
            }
        }
    }
}
