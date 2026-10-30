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

/// Every console child is spawned with [`grove_core::process::hide_console`].
///
/// The release build is a GUI executable with no console, so a missing call does not fail a test on
/// this machine: it shows up as a terminal window flashing open, once per `git` call. `explorer.exe`
/// is a GUI program and has no console window to hide.
#[test]
fn console_children_are_spawned_without_a_window() {
    const WINDOWLESS_PROGRAMS: &[&str] = &["src/reveal.rs"];
    let mut offenders = Vec::new();
    let mut pending = vec![core_root().join("src")];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let relative = path
                .strip_prefix(core_root())
                .expect("path under the crate")
                .to_string_lossy()
                .replace('\\', "/");
            if WINDOWLESS_PROGRAMS.contains(&relative.as_str()) {
                continue;
            }
            let source = read(&path);
            if source.contains("Command::new") && !source.contains("hide_console") {
                offenders.push(relative);
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these spawn a console child without hide_console, so a release build flashes a terminal window: {offenders:?}"
    );
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
