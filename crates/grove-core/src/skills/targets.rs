//! Where a skill can be installed.
//!
//! Each harness owns its own skills directory; the project-local `.agents/skills` is the
//! harness-agnostic destination. Directories are reported with an `exists` flag so the UI can explain
//! whether an install creates the folder or drops into an existing one.

use crate::paths::display_path;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One destination in the install dropdown. Pinned by `contract/fixtures/skill-targets.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTarget {
    /// Stable id used by `skill_install`: `claude` | `codex` | `pi` | `opencode` | `agents`.
    pub id: String,
    pub label: String,
    /// Forward-slash display path, so the fixture is identical on Windows and CI.
    pub path: String,
    /// True when the directory already exists; installing creates it otherwise.
    pub exists: bool,
}

/// Internal form: the real path, so install never re-parses a display string.
#[derive(Debug, Clone)]
pub(crate) struct TargetDir {
    pub id: &'static str,
    pub label: &'static str,
    pub path: PathBuf,
}

/// Home directory from `config.home` (tests) or the environment. Windows is the first target, so
/// `USERPROFILE` is checked before `HOME`.
pub(crate) fn home_dir(configured: Option<&Path>) -> PathBuf {
    if let Some(home) = configured {
        return home.to_path_buf();
    }
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The per-harness directories, in registry order, plus the project-local one.
///
/// The harness rows come from `builtin_registry()` and each adapter declares its own `skills_dir()`,
/// so a sixth harness appears in the dropdown by implementing the contract — no list to edit here and
/// no label switch anywhere. A harness that declares no directory (the in-process mock, ACP) simply
/// has no row of its own.
pub(crate) fn target_dirs(home: &Path, project_root: Option<&Path>) -> Vec<TargetDir> {
    let mut dirs: Vec<TargetDir> = crate::adapters::registry::builtin_registry()
        .iter()
        .filter_map(|adapter| {
            adapter.skills_dir().map(|relative| TargetDir {
                id: adapter.id().as_str(),
                label: adapter.display_name(),
                path: home.join(relative),
            })
        })
        .collect();

    let project = project_root
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok());
    if let Some(project) = project {
        dirs.push(TargetDir {
            id: "agents",
            label: "Project .agents/skills",
            path: project.join(".agents").join("skills"),
        });
    }

    dirs
}

pub(crate) fn project_targets(home: &Path, project_root: Option<&Path>) -> Vec<SkillTarget> {
    target_dirs(home, project_root)
        .into_iter()
        .map(|target| SkillTarget {
            id: target.id.to_string(),
            label: target.label.to_string(),
            path: display_path(&target.path),
            exists: target.path.is_dir(),
        })
        .collect()
}
