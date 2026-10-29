//! Reveal a file or folder in the platform's file manager.
//!
//! The program and its arguments are assembled here, never through a shell, so a path containing
//! spaces, quotes or `&` reaches the file manager as one argv entry. Resolution is a pure function of
//! the platform and the path, which is what lets the tests pin every platform on any host; only
//! [`reveal`] launches anything.

use crate::ipc::{IpcError, IpcResult};
use std::path::Path;

/// One external program invocation: a program name and its argv, with no shell in between.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// The command for a path on `platform` (`windows`, `macos`, else the freedesktop convention).
///
/// A directory opens itself; a file is selected in its folder. Windows gets one `/select,<path>`
/// argument so the comma and the path survive the command line intact; macOS uses `open -R`; on
/// Linux the parent folder is the closest equivalent, because `xdg-open` has no select verb.
pub fn command_for_platform(platform: &str, path: &Path, is_dir: bool) -> RevealCommand {
    let shown = path.to_string_lossy().to_string();
    match platform {
        "windows" => RevealCommand {
            program: "explorer.exe".into(),
            args: if is_dir {
                vec![shown]
            } else {
                vec![format!("/select,{shown}")]
            },
        },
        "macos" => RevealCommand {
            program: "open".into(),
            args: if is_dir {
                vec![shown]
            } else {
                vec!["-R".into(), shown]
            },
        },
        _ => RevealCommand {
            program: "xdg-open".into(),
            args: if is_dir {
                vec![shown]
            } else {
                vec![parent_of(path)]
            },
        },
    }
}

/// The command for this host.
pub fn command_for(path: &Path, is_dir: bool) -> RevealCommand {
    command_for_platform(std::env::consts::OS, path, is_dir)
}

/// Show `path` in the platform's file manager.
///
/// The path must exist: a missing path is a user-facing invalid-argument error, not a spawned process
/// that silently does nothing.
pub fn reveal(path: &str) -> IpcResult<()> {
    let raw = path.trim();
    if raw.is_empty() {
        return Err(IpcError::invalid("a path is required"));
    }
    let path = Path::new(raw);
    if !path.exists() {
        return Err(IpcError::invalid(format!("{raw} does not exist")));
    }
    let command = command_for(path, path.is_dir());
    std::process::Command::new(&command.program)
        .args(&command.args)
        .spawn()
        .map_err(|error| {
            IpcError::unavailable(format!("could not open {}: {error}", command.program))
        })?;
    Ok(())
}

fn parent_of(path: &Path) -> String {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
