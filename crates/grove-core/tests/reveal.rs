//! Revealing a path in the platform's file manager. The command is resolved without a shell, so the
//! arguments can be asserted per platform on any host; nothing here launches an external program.

use grove_core::reveal::{command_for_platform, reveal};
use std::path::Path;

#[test]
fn windows_selects_a_file_and_opens_a_folder() {
    let file = Path::new(r"C:\work\grove\src\main.ts");
    let selected = command_for_platform("windows", file, false);
    assert_eq!(selected.program, "explorer.exe");
    assert_eq!(selected.args, vec![r"/select,C:\work\grove\src\main.ts"]);

    let folder = Path::new(r"C:\work\grove");
    let opened = command_for_platform("windows", folder, true);
    assert_eq!(opened.program, "explorer.exe");
    assert_eq!(opened.args, vec![r"C:\work\grove"]);
}

#[test]
fn windows_converts_display_paths_to_native_separators() {
    // Grove stores and shows paths with `/`; Explorer's `/select,` opens Documents for those.
    let file = Path::new("C:/work/grove/src/main.ts");
    assert_eq!(
        command_for_platform("windows", file, false).args,
        vec![r"/select,C:\work\grove\src\main.ts"]
    );

    let folder = Path::new("C:/work/grove");
    assert_eq!(
        command_for_platform("windows", folder, true).args,
        vec![r"C:\work\grove"]
    );

    let unc = Path::new("//server/share/grove");
    assert_eq!(
        command_for_platform("windows", unc, true).args,
        vec![r"\\server\share\grove"]
    );
}

#[test]
fn macos_reveals_a_file_and_opens_a_folder() {
    let file = Path::new("/work/grove/src/main.ts");
    let revealed = command_for_platform("macos", file, false);
    assert_eq!(revealed.program, "open");
    assert_eq!(revealed.args, vec!["-R", "/work/grove/src/main.ts"]);

    let folder = Path::new("/work/grove");
    let opened = command_for_platform("macos", folder, true);
    assert_eq!(opened.args, vec!["/work/grove"]);
}

#[test]
fn linux_opens_the_parent_of_a_file() {
    let file = Path::new("/work/grove/src/main.ts");
    let revealed = command_for_platform("linux", file, false);
    assert_eq!(revealed.program, "xdg-open");
    assert_eq!(revealed.args, vec!["/work/grove/src"]);

    let folder = Path::new("/work/grove");
    let opened = command_for_platform("linux", folder, true);
    assert_eq!(opened.args, vec!["/work/grove"]);
}

#[test]
fn a_missing_path_is_rejected_before_anything_is_launched() {
    let missing = std::env::temp_dir().join("grove-reveal-does-not-exist");
    let _ = std::fs::remove_file(&missing);
    let error = reveal(missing.to_str().unwrap()).expect_err("missing paths are refused");
    assert_eq!(error.code, grove_core::ipc::ErrorCode::Invalid);
}
