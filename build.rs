use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("version.txt");

    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=.git/HEAD");
    if Path::new(".git/refs/heads").exists() {
        println!("cargo:rerun-if-changed=.git/refs/heads/");
    }

    let version = build_version();

    fs::write(&dest_path, &version).unwrap();
    println!("cargo:rustc-env=BUILD_VERSION={}", version);
}

fn build_version() -> String {
    if let Ok(v) = env::var("VERSION") {
        return v;
    }
    git_describe().unwrap_or_else(|_| "dev".to_string())
}

fn git_describe() -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()?;
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}
