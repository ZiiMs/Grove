//! Architecture guards that a review would otherwise have to remember.
//!
//! `grove-core` must stay independent of the UI shell, and adapters must stay independent of each
//! other. These are cheap text checks on the sources, and they fail loudly when a dependency is added
//! in the wrong direction.

use std::path::{Path, PathBuf};

fn core_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

#[test]
fn core_does_not_depend_on_tauri() {
    let manifest = read(&core_root().join("Cargo.toml"));
    assert!(
        !manifest.contains("tauri"),
        "grove-core must not depend on tauri: the core has to stay testable headlessly"
    );
}

#[test]
fn adapters_do_not_import_each_other_or_tauri() {
    let dir = core_root().join("src/adapters");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        // Adapters land with ticket 08; nothing to guard before then.
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string();
        let source = read(&path);

        assert!(
            !source.contains("use tauri") && !source.contains("tauri::"),
            "adapter {name} must not import tauri (UI code belongs in the shell)"
        );

        for forbidden in ["crate::sessions::manager", "crate::sessions::pump"] {
            assert!(
                !source.contains(forbidden),
                "adapter {name} must not reach into {forbidden}: the session manager drives the adapter, never the other way round"
            );
        }

        for other in ["claude", "codex", "acp", "opencode", "pi", "mock"] {
            if other == name {
                continue;
            }
            let cross_import = format!("crate::adapters::{other}");
            assert!(
                !source.contains(&cross_import),
                "adapter {name} must not import adapter {other}: adapters are isolated"
            );
        }
    }
}
