//! Golden-fixture half of the skill contract. The TS side parses the same files with zod
//! (`packages/contract/test/skills.test.ts`), so either side drifting breaks a test.

use grove_core::skills::{CatalogEntry, CatalogSnapshot, InstallResult, SkillPreview, SkillTarget};
use std::collections::BTreeMap;

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract/fixtures")
        .join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read fixture {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("fixture is valid JSON")
}

fn entry(name: &str, path: &str) -> CatalogEntry {
    CatalogEntry {
        id: format!("anthropics/skills::{path}"),
        name: name.to_string(),
        source: "anthropics/skills".to_string(),
        path: path.to_string(),
    }
}

#[test]
fn catalog_snapshot_matches_fixture() {
    let sample = CatalogSnapshot {
        entries: vec![entry("pdf", "skills/pdf"), entry("slides", "skills/slides")],
        fetched_at: Some("2026-10-24T18:00:00Z".to_string()),
        from_cache: false,
        offline: false,
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("skill-catalog.json")
    );
}

#[test]
fn preview_matches_fixture() {
    let mut frontmatter = BTreeMap::new();
    frontmatter.insert("name".to_string(), "pdf".to_string());
    frontmatter.insert(
        "description".to_string(),
        "Create and inspect PDF files".to_string(),
    );
    let sample = SkillPreview {
        entry: entry("pdf", "skills/pdf"),
        frontmatter,
        body: "# PDF\n\nUse this skill to work with PDF files.\n".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("skill-preview.json")
    );
}

#[test]
fn targets_match_fixture() {
    // Registry order: the dropdown is built from `builtin_registry()` and each adapter's own
    // `skills_dir()`, so the golden file follows the adapter list rather than a list of its own.
    let sample = vec![
        SkillTarget {
            id: "claude".into(),
            label: "Claude Code".into(),
            path: "C:/Users/demo/.claude/skills".into(),
            exists: true,
        },
        SkillTarget {
            id: "pi".into(),
            label: "pi".into(),
            path: "C:/Users/demo/.pi/skills".into(),
            exists: false,
        },
        SkillTarget {
            id: "opencode".into(),
            label: "OpenCode".into(),
            path: "C:/Users/demo/.config/opencode/skills".into(),
            exists: false,
        },
        SkillTarget {
            id: "codex".into(),
            label: "Codex".into(),
            path: "C:/Users/demo/.codex/skills".into(),
            exists: false,
        },
        SkillTarget {
            id: "agents".into(),
            label: "Project .agents/skills".into(),
            path: "C:/Users/demo/project/.agents/skills".into(),
            exists: true,
        },
    ];
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("skill-targets.json")
    );
}

#[test]
fn install_result_matches_fixture() {
    let sample = InstallResult {
        skill: "pdf".into(),
        source: "anthropics/skills".into(),
        target: "claude".into(),
        path: "C:/Users/demo/.claude/skills/pdf".into(),
        overwritten: true,
        installed_at: "2026-10-24T18:00:00Z".into(),
    };
    assert_eq!(
        serde_json::to_value(&sample).unwrap(),
        fixture("skill-install.json")
    );
}
