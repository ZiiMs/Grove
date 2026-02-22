use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("version.txt");

    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=.git/HEAD");
    if Path::new(".git/refs/heads").exists() {
        println!("cargo:rerun-if-changed=.git/refs/heads/");
    }

    let git_hash = git_hash().unwrap_or_else(|_| "unknown".to_string());
    let git_tag = git_tag().unwrap_or(None);
    let date = format_date();

    let version = match git_tag {
        Some(tag) => tag,
        None => format!("{}-{}", date, git_hash),
    };

    fs::write(&dest_path, &version).unwrap();
    println!("cargo:rustc-env=BUILD_VERSION={}", version);
}

fn git_hash() -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()?;
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn git_tag() -> Result<Option<String>, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--exact-match"])
        .output();
    match output {
        Ok(o) if o.status.success() => Ok(Some(String::from_utf8(o.stdout)?.trim().to_string())),
        _ => Ok(None),
    }
}

fn format_date() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0));
    let total_secs = duration.as_secs();
    let days_since_epoch = total_secs / 86400;
    let (year, month, day) = days_to_ymd(days_since_epoch as i64);
    format!("{:04}.{:02}.{:02}", year, month, day)
}

fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let mut year = 1970;
    let mut remaining = days;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let days_in_months = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &dim in &days_in_months {
        if remaining < dim as i64 {
            break;
        }
        remaining -= dim as i64;
        month += 1;
    }

    let day = remaining + 1;
    (year, month, day as u32)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
