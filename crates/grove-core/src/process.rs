//! Child-process flags that keep Grove quiet on Windows.
//!
//! The release build is a GUI-subsystem executable (`windows_subsystem = "windows"`), so it owns no
//! console. Windows therefore allocates a console window for every console child (`git`, `gh`,
//! `cmd`, an MCP server), which a user sees as a terminal flashing open. `CREATE_NO_WINDOW` at spawn
//! time is the only thing that prevents it. On every other platform these are no-ops.

/// Spawn `command` without giving it a console window of its own.
pub fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `CREATE_NO_WINDOW` — the child runs with no console, so Windows never creates a window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

/// [`hide_console`] for a `tokio::process::Command`.
pub fn hide_console_async(command: &mut tokio::process::Command) {
    hide_console(command.as_std_mut());
}
