pub const VERSION: &str = env!("BUILD_VERSION");

pub fn version() -> &'static str {
    VERSION
}
