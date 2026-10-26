//! Where a skill can be installed.
//!
//! Two scopes share the `.agents/skills` layout: the user's global set and the project-local set.
//! Directories are reported with an `exists` flag so the UI can explain whether an install creates
//! the folder or drops into an existing one.

use crate::paths::display_path;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One destination in the install dropdown. Pinned by `contract/fixtures/skill-targets.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTarget {
    /// Stable id used by `skill_install`: `user` (global) | `agents` (project).
    pub id: String,
    pub label: String,
    /// Forward-slash display path, so the fixture is identical on Windows and CI.
    pub path: String,
    /// True when the directory already exists; installing creates it otherwise.
    pub exists: bool,
}

/// A skill folder found under a target, whether or not Grove put it there. Pinned by
/// `contract/fixtures/skill-inventory.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedSkill {
    /// Folder name, which is also the catalog entry's name.
    pub name: String,
    /// Target id the folder was found under: `user` | `agents`.
    pub target: String,
    /// Forward-slash path of the skill folder.
    pub path: String,
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

/// The skill destinations, in display order: the user's global `.agents/skills`, then the
/// project-local `.agents/skills`.
///
/// Grove runs the agent itself, so there is no per-vendor skills directory to enumerate any more: a
/// skill is installed where Grove's own tool loop can read it.
pub(crate) fn target_dirs(home: &Path, project_root: Option<&Path>) -> Vec<TargetDir> {
    let mut dirs = vec![TargetDir {
        id: "user",
        label: "User .agents/skills",
        path: home.join(".agents").join("skills"),
    }];

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

/// Every skill folder already sitting under a target, whether Grove installed it or not.
///
/// A folder counts as a skill only when it has a `SKILL.md` at its root; hidden names are skipped so
/// an in-flight `.grove-install-*` staging directory never shows up as a skill.
pub(crate) fn detect_installed(home: &Path, project_root: Option<&Path>) -> Vec<DetectedSkill> {
    let mut found = Vec::new();
    for target in target_dirs(home, project_root) {
        let Ok(entries) = std::fs::read_dir(&target.path) else {
            continue;
        };
        let mut here = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.starts_with('.') || !path.is_dir() || !path.join("SKILL.md").is_file() {
                continue;
            }
            here.push(DetectedSkill {
                name: name.to_string(),
                target: target.id.to_string(),
                path: display_path(&path),
            });
        }
        here.sort_by(|left, right| left.name.cmp(&right.name));
        found.extend(here);
    }
    found
}
