use git2::Repository;

use crate::app::config::GitProvider;

pub struct RemoteInfo {
    pub owner: String,
    pub repo: String,
    pub provider: GitProvider,
    pub base_url: Option<String>,
}

pub fn parse_remote_info(repo_path: &str) -> Option<RemoteInfo> {
    let repo = Repository::open(repo_path).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    let url = remote.url()?;

    parse_git_url(url)
}

fn parse_git_url(url: &str) -> Option<RemoteInfo> {
    let url = url.trim_end_matches(".git");

    if url.starts_with("git@") {
        parse_ssh_url(url)
    } else if url.starts_with("https://") || url.starts_with("http://") {
        parse_https_url(url)
    } else {
        None
    }
}

fn parse_ssh_url(url: &str) -> Option<RemoteInfo> {
    let url = url.strip_prefix("git@")?;
    let parts: Vec<&str> = url.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let host = parts[0];
    let path = parts[1];

    let path_parts: Vec<&str> = path.split('/').collect();
    if path_parts.len() < 2 {
        return None;
    }

    let owner = path_parts[0].to_string();
    let repo = path_parts[path_parts.len() - 1].to_string();

    let (provider, base_url) = detect_provider_from_host(host);

    Some(RemoteInfo {
        owner,
        repo,
        provider,
        base_url,
    })
}

fn parse_https_url(url: &str) -> Option<RemoteInfo> {
    let url = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;

    let parts: Vec<&str> = url.split('/').collect();
    if parts.len() < 3 {
        return None;
    }

    let host = parts[0];
    let owner = parts[1].to_string();
    let repo = parts[parts.len() - 1].to_string();

    let (provider, base_url) = detect_provider_from_host(host);

    Some(RemoteInfo {
        owner,
        repo,
        provider,
        base_url,
    })
}

fn detect_provider_from_host(host: &str) -> (GitProvider, Option<String>) {
    let host_lower = host.to_lowercase();

    if host_lower == "github.com" {
        (GitProvider::GitHub, None)
    } else if host_lower == "gitlab.com" {
        (GitProvider::GitLab, None)
    } else if host_lower == "codeberg.org" {
        (GitProvider::Codeberg, None)
    } else if host_lower.contains("gitlab") || host_lower.contains("gitlab.") {
        (GitProvider::GitLab, Some(format!("https://{}", host)))
    } else if host_lower.contains("codeberg") || host_lower.contains("forgejo") {
        (GitProvider::Codeberg, Some(format!("https://{}", host)))
    } else if host_lower.contains("github") || host_lower.contains("ghe") {
        (GitProvider::GitHub, Some(format!("https://{}", host)))
    } else {
        (GitProvider::GitLab, Some(format!("https://{}", host)))
    }
}
