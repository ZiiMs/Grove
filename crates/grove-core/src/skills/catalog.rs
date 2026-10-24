//! The skill catalog: public GitHub repositories, discovered through the GitHub API, cached on disk
//! with a TTL and an ETag, and served from that cache when the network is down.
//!
//! Only directories containing a `SKILL.md` are entries. Preview and install read blobs through the
//! same cache (keyed by the immutable git blob SHA), so a skill that has been previewed or installed
//! once still installs with the network down.

use super::{SkillError, SkillService};
use crate::paths::{display_path, AppPaths};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const GITHUB_ACCEPT: &str = "application/vnd.github+json";
const RAW_ACCEPT: &str = "application/vnd.github.raw";
const GITHUB_API_VERSION: &str = "2022-11-28";
const DEFAULT_TTL: Duration = Duration::from_secs(15 * 60);

/// One catalog repository. Defaults live in [`SkillCatalogConfig::default`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSource {
    /// `owner/name`, exactly two safe path segments.
    pub repo: String,
    /// `None` resolves the repository's default branch through the API.
    pub branch: Option<String>,
}

impl SkillSource {
    pub fn new(repo: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            branch: None,
        }
    }

    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }
}

/// Catalog configuration. `home` is a test seam: production uses the OS home directory.
#[derive(Debug, Clone)]
pub struct SkillCatalogConfig {
    pub sources: Vec<SkillSource>,
    pub api_base: String,
    pub ttl: Duration,
    pub home: Option<PathBuf>,
}

impl Default for SkillCatalogConfig {
    fn default() -> Self {
        Self {
            sources: vec![
                SkillSource::new("anthropics/skills"),
                SkillSource::new("mattpocock/skills"),
            ],
            api_base: "https://api.github.com".to_string(),
            ttl: DEFAULT_TTL,
            home: None,
        }
    }
}

/// One skill directory. Pinned by `contract/fixtures/skill-catalog.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    /// Stable key: `<source>::<repo-relative path>`.
    pub id: String,
    /// Directory name, e.g. `pdf`.
    pub name: String,
    /// `owner/name`.
    pub source: String,
    /// Repository-relative directory path, e.g. `skills/pdf`.
    pub path: String,
}

/// The catalog plus the state the UI needs to explain itself: served from cache, and whether that is
/// because the network is unavailable. Pinned by `contract/fixtures/skill-catalog.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSnapshot {
    pub entries: Vec<CatalogEntry>,
    /// RFC 3339, `None` only before the first successful fetch (an error is returned instead).
    pub fetched_at: Option<String>,
    /// True when every entry came from the disk cache.
    pub from_cache: bool,
    /// True when the network failed and a stale cache kept the panel usable.
    pub offline: bool,
}

/// A `SKILL.md` unpacked for the preview pane. Pinned by `contract/fixtures/skill-preview.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreview {
    pub entry: CatalogEntry,
    /// Flat `key: value` frontmatter. Nested YAML is intentionally out of scope.
    pub frontmatter: BTreeMap<String, String>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TreeFile {
    pub path: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub sha: String,
    #[serde(rename = "type", default)]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    pub source: String,
    pub branch: String,
    pub etag: Option<String>,
    pub fetched_at: String,
    pub files: Vec<TreeFile>,
}

#[derive(Debug, Deserialize)]
struct TreeResponse {
    #[serde(default)]
    tree: Vec<TreeFile>,
}

#[derive(Debug, Deserialize)]
struct RepoResponse {
    #[serde(default)]
    default_branch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceState {
    /// Fetched from the network now.
    Fresh,
    /// Served from a fresh cache, or revalidated by ETag (304).
    Cached,
    /// The network failed and a stale cache was used.
    Offline,
}

enum Fetched {
    Fresh(Manifest),
    NotModified(Manifest),
}

pub(crate) async fn catalog(
    service: &SkillService,
    refresh: bool,
) -> Result<CatalogSnapshot, SkillError> {
    let config = service.config_snapshot();
    let token = service.github_token();
    let mut entries = Vec::new();
    let mut from_cache = true;
    let mut offline = false;
    let mut fetched_at = None;
    let mut reached_any = false;
    let mut failures = Vec::new();

    for source in &config.sources {
        match manifest_for(service, &config, source, token.as_deref(), refresh).await {
            Ok((manifest, state)) => {
                reached_any = true;
                if state == SourceState::Fresh {
                    from_cache = false;
                }
                if state == SourceState::Offline {
                    offline = true;
                }
                fetched_at = Some(manifest.fetched_at.clone());
                entries.extend(entries_from(&manifest));
            }
            Err(err) => failures.push(format!("{}: {err}", source.repo)),
        }
    }

    if !reached_any {
        return Err(SkillError::Unavailable(if failures.is_empty() {
            "no skill sources are configured".to_string()
        } else {
            failures.join("; ")
        }));
    }

    entries.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.path.cmp(&b.path))
    });
    entries.dedup_by(|a, b| a.id == b.id);

    Ok(CatalogSnapshot {
        entries,
        fetched_at: fetched_at.or_else(|| Some(now())),
        from_cache,
        offline,
    })
}

pub(crate) async fn preview(
    service: &SkillService,
    source: &str,
    path: &str,
) -> Result<SkillPreview, SkillError> {
    let relative = checked_relative(path)?;
    let config = service.config_snapshot();
    let source = find_source(&config, source)?.clone();
    let token = service.github_token();
    let (manifest, _) = manifest_for(service, &config, &source, token.as_deref(), false).await?;

    let manifest_path = format!("{}/SKILL.md", display_path(&relative));
    let file = manifest
        .files
        .iter()
        .find(|file| file.kind != "tree" && file.path == manifest_path)
        .ok_or_else(|| SkillError::NotFound(format!("{} in {}", manifest_path, source.repo)))?;

    let raw = read_blob(
        service,
        &source.repo,
        &manifest.branch,
        &file.path,
        &file.sha,
        token.as_deref(),
    )
    .await?;
    let (frontmatter, body) = parse_frontmatter(&raw);
    let entry = entry_for(&source.repo, &relative)
        .ok_or_else(|| SkillError::Invalid(format!("not a skill directory: {path}")))?;

    Ok(SkillPreview {
        entry,
        frontmatter,
        body,
    })
}

/// Read one file, preferring the immutable blob cache. A cache hit never touches the network, which
/// is what makes preview and re-install work offline.
pub(crate) async fn read_blob(
    service: &SkillService,
    repo: &str,
    branch: &str,
    path: &str,
    sha: &str,
    token: Option<&str>,
) -> Result<String, SkillError> {
    let cached = blob_path(service.paths(), path, sha);
    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(String::from_utf8_lossy(&bytes).into_owned());
    }

    let url = format!(
        "{}/repos/{}/contents/{}?ref={}",
        api_base(&service.config_snapshot()),
        repo,
        encode_path(path),
        encode_path(branch)
    );
    let mut request = service
        .http()
        .get(url)
        .header("accept", RAW_ACCEPT)
        .header("x-github-api-version", GITHUB_API_VERSION);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|err| SkillError::Unavailable(err.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(SkillError::NotFound(format!("{path} in {repo}")));
        }
        return Err(SkillError::Http {
            status: status.as_u16(),
            message: truncate(&body, 300),
        });
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| SkillError::Unavailable(err.to_string()))?;
    // A cache write failure must not fail the read; the next call simply fetches again.
    let _ = super::atomic_write(&cached, &bytes);
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) async fn manifest_for(
    service: &SkillService,
    config: &SkillCatalogConfig,
    source: &SkillSource,
    token: Option<&str>,
    refresh: bool,
) -> Result<(Manifest, SourceState), SkillError> {
    validate_repo(&source.repo)?;
    let cached = load_manifest(service.paths(), &source.repo);

    if !refresh {
        if let Some(manifest) = &cached {
            if is_fresh(manifest, config.ttl) {
                return Ok((manifest.clone(), SourceState::Cached));
            }
        }
    }

    match fetch_manifest(service, config, source, cached.as_ref(), token).await {
        Ok(Fetched::Fresh(manifest)) => {
            store_manifest(service.paths(), &manifest)?;
            Ok((manifest, SourceState::Fresh))
        }
        Ok(Fetched::NotModified(mut manifest)) => {
            manifest.fetched_at = now();
            store_manifest(service.paths(), &manifest)?;
            Ok((manifest, SourceState::Cached))
        }
        Err(err) => match cached {
            Some(manifest) => {
                tracing::warn!(
                    target: "grove::skills",
                    source = %source.repo,
                    "skill manifest fetch failed; serving the disk cache: {err}"
                );
                Ok((manifest, SourceState::Offline))
            }
            None => Err(err),
        },
    }
}

async fn fetch_manifest(
    service: &SkillService,
    config: &SkillCatalogConfig,
    source: &SkillSource,
    cached: Option<&Manifest>,
    token: Option<&str>,
) -> Result<Fetched, SkillError> {
    let branch = match &source.branch {
        Some(branch) => branch.clone(),
        None => default_branch(service, config, &source.repo, token).await?,
    };

    let url = format!(
        "{}/repos/{}/git/trees/{}?recursive=1",
        api_base(config),
        source.repo,
        encode_path(&branch)
    );
    let mut request = service
        .http()
        .get(url)
        .header("accept", GITHUB_ACCEPT)
        .header("x-github-api-version", GITHUB_API_VERSION);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(etag) = cached.and_then(|manifest| manifest.etag.as_deref()) {
        request = request.header("if-none-match", etag);
    }

    let response = request
        .send()
        .await
        .map_err(|err| SkillError::Unavailable(err.to_string()))?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        let manifest = cached.cloned().ok_or_else(|| {
            SkillError::Unavailable("GitHub returned 304 without a cached manifest".to_string())
        })?;
        return Ok(Fetched::NotModified(manifest));
    }

    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response
        .text()
        .await
        .map_err(|err| SkillError::Unavailable(err.to_string()))?;
    if !status.is_success() {
        return Err(SkillError::Http {
            status: status.as_u16(),
            message: truncate(&body, 300),
        });
    }
    let parsed: TreeResponse = serde_json::from_str(&body).map_err(|err| SkillError::Http {
        status: status.as_u16(),
        message: format!("invalid tree response: {err}"),
    })?;

    Ok(Fetched::Fresh(Manifest {
        source: source.repo.clone(),
        branch,
        etag,
        fetched_at: now(),
        files: parsed.tree,
    }))
}

async fn default_branch(
    service: &SkillService,
    config: &SkillCatalogConfig,
    repo: &str,
    token: Option<&str>,
) -> Result<String, SkillError> {
    let url = format!("{}/repos/{}", api_base(config), repo);
    let mut request = service
        .http()
        .get(url)
        .header("accept", GITHUB_ACCEPT)
        .header("x-github-api-version", GITHUB_API_VERSION);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|err| SkillError::Unavailable(err.to_string()))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| SkillError::Unavailable(err.to_string()))?;
    if !status.is_success() {
        return Err(SkillError::Http {
            status: status.as_u16(),
            message: truncate(&body, 300),
        });
    }
    let parsed: RepoResponse = serde_json::from_str(&body).map_err(|err| SkillError::Http {
        status: status.as_u16(),
        message: format!("invalid repository response: {err}"),
    })?;
    if parsed.default_branch.is_empty() {
        return Err(SkillError::Http {
            status: status.as_u16(),
            message: "repository has no default branch".to_string(),
        });
    }
    Ok(parsed.default_branch)
}

fn entries_from(manifest: &Manifest) -> Vec<CatalogEntry> {
    manifest
        .files
        .iter()
        .filter(|file| file.kind != "tree")
        .filter_map(|file| {
            let dir = file.path.strip_suffix("/SKILL.md")?;
            let relative = checked_relative(dir).ok()?;
            entry_for(&manifest.source, &relative)
        })
        .collect()
}

pub(crate) fn entry_for(source: &str, relative: &Path) -> Option<CatalogEntry> {
    let path = display_path(relative);
    let name = relative.file_name()?.to_str()?.to_string();
    Some(CatalogEntry {
        id: format!("{source}::{path}"),
        name,
        source: source.to_string(),
        path,
    })
}

pub(crate) fn find_source<'a>(
    config: &'a SkillCatalogConfig,
    repo: &str,
) -> Result<&'a SkillSource, SkillError> {
    config
        .sources
        .iter()
        .find(|source| source.repo == repo)
        .ok_or_else(|| SkillError::Invalid(format!("source is not configured: {repo}")))
}

/// A repository path that is safe to join onto a cache or target directory. Rejects absolute paths,
/// `..`, rooted paths (`/x`), drive prefixes and backslashes on every platform.
pub(crate) fn checked_relative(value: &str) -> Result<PathBuf, SkillError> {
    if value.is_empty() {
        return Err(SkillError::Invalid("empty path".to_string()));
    }
    if value.contains('\0') || value.contains('\\') {
        return Err(SkillError::Invalid(format!(
            "repository paths must use forward slashes: {value}"
        )));
    }
    let raw = Path::new(value);
    if raw.is_absolute() || raw.has_root() {
        return Err(SkillError::Invalid(format!(
            "absolute paths are not allowed: {value}"
        )));
    }
    let mut relative = PathBuf::new();
    for component in raw.components() {
        match component {
            std::path::Component::Normal(part) => relative.push(part),
            _ => {
                return Err(SkillError::Invalid(format!(
                    "path traversal is not allowed: {value}"
                )))
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(SkillError::Invalid(format!("empty path: {value}")));
    }
    Ok(relative)
}

fn validate_repo(repo: &str) -> Result<(), SkillError> {
    let parts: Vec<&str> = repo.split('/').collect();
    let valid_part = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if parts.len() != 2 || !parts.iter().all(|part| valid_part(part)) {
        return Err(SkillError::Invalid(format!(
            "invalid GitHub repository: {repo}"
        )));
    }
    Ok(())
}

fn parse_frontmatter(raw: &str) -> (BTreeMap<String, String>, String) {
    let normalized = raw.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return (BTreeMap::new(), normalized);
    };

    let mut frontmatter = BTreeMap::new();
    let mut consumed = 4;
    let mut closed = false;
    for line in rest.lines() {
        if line.trim_end() == "---" {
            consumed += line.len() + 1;
            closed = true;
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            if !key.is_empty() {
                frontmatter.insert(
                    key.to_string(),
                    value
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string(),
                );
            }
        }
        consumed += line.len() + 1;
    }

    if !closed {
        // An opening fence without a closing one is not frontmatter; show the file as written.
        return (BTreeMap::new(), normalized);
    }
    let body = normalized.get(consumed..).unwrap_or_default();
    (
        frontmatter,
        body.strip_prefix('\n').unwrap_or(body).to_string(),
    )
}

fn is_fresh(manifest: &Manifest, ttl: Duration) -> bool {
    if ttl.is_zero() {
        return false;
    }
    let Ok(fetched_at) = DateTime::parse_from_rfc3339(&manifest.fetched_at) else {
        return false;
    };
    let age = Utc::now().signed_duration_since(fetched_at.with_timezone(&Utc));
    age.to_std().map(|age| age < ttl).unwrap_or(false)
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn api_base(config: &SkillCatalogConfig) -> &str {
    config.api_base.trim_end_matches('/')
}

fn cache_dir(paths: &AppPaths) -> PathBuf {
    paths.cache.join("skills")
}

fn manifest_path(paths: &AppPaths, source: &str) -> PathBuf {
    cache_dir(paths).join(format!("{}.json", short_hash(source)))
}

fn load_manifest(paths: &AppPaths, source: &str) -> Option<Manifest> {
    let raw = std::fs::read_to_string(manifest_path(paths, source)).ok()?;
    let manifest: Manifest = serde_json::from_str(&raw).ok()?;
    (manifest.source == source).then_some(manifest)
}

fn store_manifest(paths: &AppPaths, manifest: &Manifest) -> Result<(), SkillError> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|err| SkillError::Unavailable(format!("serialize skill cache: {err}")))?;
    super::atomic_write(&manifest_path(paths, &manifest.source), &bytes)?;
    Ok(())
}

fn blob_path(paths: &AppPaths, path: &str, sha: &str) -> PathBuf {
    let key = if !sha.is_empty() && sha.chars().all(|c| c.is_ascii_hexdigit()) {
        sha.to_string()
    } else {
        short_hash(path)
    };
    cache_dir(paths).join("blobs").join(key)
}

fn short_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

/// Percent-encode a repository path for the contents API. `/` stays a separator; everything outside
/// the unreserved set is escaped, so spaces and non-ASCII skill names work.
fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_is_split_from_the_body() {
        let (frontmatter, body) = parse_frontmatter("---\r\nname: pdf\r\n---\r\n# PDF\r\n");
        assert_eq!(frontmatter.get("name").map(String::as_str), Some("pdf"));
        assert_eq!(body, "# PDF\n");
    }

    #[test]
    fn a_file_without_frontmatter_is_all_body() {
        let (frontmatter, body) = parse_frontmatter("# Just markdown\n");
        assert!(frontmatter.is_empty());
        assert_eq!(body, "# Just markdown\n");
    }

    #[test]
    fn traversal_is_rejected_in_every_form() {
        for path in [
            "../x",
            "a/../../x",
            "/etc/passwd",
            "C:/Windows",
            "a\\b",
            ".",
        ] {
            assert!(checked_relative(path).is_err(), "{path} should be rejected");
        }
        assert!(checked_relative("skills/pdf").is_ok());
    }

    #[test]
    fn repository_names_are_validated() {
        assert!(validate_repo("anthropics/skills").is_ok());
        for repo in ["anthropics", "a/b/c", "../x", "a/", "/b", "a b/c"] {
            assert!(validate_repo(repo).is_err(), "{repo} should be rejected");
        }
    }

    #[test]
    fn entry_ids_are_stable() {
        let entry = entry_for("acme/skills", Path::new("skills/pdf")).unwrap();
        assert_eq!(entry.id, "acme/skills::skills/pdf");
        assert_eq!(entry.name, "pdf");
        assert_eq!(entry.path, "skills/pdf");
    }
}
