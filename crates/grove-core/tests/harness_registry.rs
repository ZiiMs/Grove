//! The registry is the one list of harnesses. These assertions keep it that way.
//!
//! A sixth adapter must be able to appear in the picker, the doctor and the skills dropdown by
//! implementing `HarnessAdapter` and adding its one registry line — no label `match` to extend and no
//! skills list to keep in step.

use grove_core::adapters::registry::builtin_registry;
use grove_core::paths::AppPaths;
use grove_core::secrets::MemorySecretStore;
use grove_core::skills::{SkillCatalogConfig, SkillService};
use std::sync::Arc;

fn service(home: &std::path::Path) -> SkillService {
    let directory = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(directory.path().join("app-data"));
    paths.ensure().expect("create app data");
    SkillService::with_config(
        paths,
        Arc::new(MemorySecretStore::new()),
        SkillCatalogConfig {
            home: Some(home.to_path_buf()),
            ..Default::default()
        },
    )
}

#[test]
fn every_registered_harness_has_a_display_name() {
    for adapter in builtin_registry() {
        let label = adapter.display_name();
        assert!(
            !label.trim().is_empty(),
            "harness {} has no display name: the doctor and the picker would show a raw id",
            adapter.id()
        );
        assert_eq!(
            label,
            label.trim(),
            "harness {} display name has stray whitespace",
            adapter.id()
        );
    }
}

#[test]
fn every_harness_that_declares_a_skills_directory_gets_a_target_row() {
    let home = std::path::Path::new("C:/Users/demo");
    let project = std::path::Path::new("C:/Users/demo/project");
    let service = service(home);
    let targets = service.targets(Some(project));
    let ids: Vec<&str> = targets.iter().map(|target| target.id.as_str()).collect();

    for adapter in builtin_registry() {
        let id = adapter.id().as_str();
        if adapter.skills_dir().is_some() {
            let row = targets
                .iter()
                .find(|target| target.id == id)
                .unwrap_or_else(|| {
                    panic!(
                        "harness {id} declares a skills directory but has no target row: {ids:?}"
                    )
                });
            assert_eq!(
                row.label,
                adapter.display_name(),
                "the dropdown label is the adapter's own display name"
            );
        }
    }

    // The project-local destination is harness-agnostic and always last.
    assert_eq!(ids.last().copied(), Some("agents"));
}

#[test]
fn every_target_row_belongs_to_a_registered_harness() {
    let targets = service(std::path::Path::new("C:/Users/demo")).targets(None);
    let registered: Vec<&'static str> = builtin_registry()
        .iter()
        .map(|adapter| adapter.id().as_str())
        .collect();
    for target in &targets {
        if target.id == "agents" {
            continue;
        }
        assert!(
            registered.contains(&target.id.as_str()),
            "skill target {} is not a registered harness: the dropdown and the picker disagree",
            target.id
        );
    }
}
