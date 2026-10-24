//! Filesystem locations. All paths the app owns come from here — never string-concatenated ad hoc,
//! never with a hard-coded separator, so Windows stays first-class.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    /// Tauri app-data dir: `%APPDATA%/Grove` on Windows.
    pub app_data: PathBuf,
    /// Where the SQLite database lives.
    pub database: PathBuf,
    /// Where per-session plan mirrors live, under a project root.
    pub cache: PathBuf,
}

impl AppPaths {
    pub fn new(app_data: impl Into<PathBuf>) -> Self {
        let app_data = app_data.into();
        Self {
            database: app_data.join("grove.db"),
            cache: app_data.join("cache"),
            app_data,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.app_data)?;
        std::fs::create_dir_all(&self.cache)?;
        Ok(())
    }

    /// Stable, forward-slash form shown in the UI and written into contract fixtures.
    pub fn app_data_display(&self) -> String {
        display_path(&self.app_data)
    }
}

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_sits_under_app_data() {
        let paths = AppPaths::new(PathBuf::from("/tmp/grove"));
        assert_eq!(paths.database, PathBuf::from("/tmp/grove/grove.db"));
        assert_eq!(paths.cache, PathBuf::from("/tmp/grove/cache"));
    }

    #[test]
    fn display_path_normalises_separators() {
        assert_eq!(
            display_path(Path::new("C:\\Users\\demo\\Grove")),
            "C:/Users/demo/Grove"
        );
    }
}
