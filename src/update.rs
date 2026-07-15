//! Self-update mechanism for the lembas CLI.
//!
//! Downloads pre-built binaries from GitHub releases and replaces the running binary atomically.

use std::io::Write;

use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use miette::{Context, IntoDiagnostic, Result};
use serde::Deserialize;
use thiserror::Error;

const GITHUB_REPO: &str = "lembas-project/lembas-cli";

#[derive(Error, Debug, miette::Diagnostic)]
pub enum UpdateError {
    #[error("No release asset found for platform: {0}")]
    #[diagnostic(code(lembas::update::asset_not_found))]
    AssetNotFound(String),

    #[error("Unsupported platform: {os}-{arch}")]
    #[diagnostic(code(lembas::update::unsupported_platform))]
    UnsupportedPlatform {
        os: &'static str,
        arch: &'static str,
    },

    #[error("No releases found")]
    #[diagnostic(code(lembas::update::no_releases))]
    NoReleases,

    #[error("Version not found: {0}")]
    #[diagnostic(code(lembas::update::version_not_found))]
    VersionNotFound(String),

    #[error("Failed to parse version: {0}")]
    #[diagnostic(code(lembas::update::version_parse))]
    VersionParse(String),
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug)]
pub struct Release {
    pub version: semver::Version,
    #[allow(dead_code)]
    tag: String,
    assets: Vec<GitHubAsset>,
}

pub enum UpdateCheck {
    Available(Release),
    AlreadyUpToDate,
}

fn get_platform_asset_name() -> std::result::Result<&'static str, UpdateError> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    match (os, arch) {
        ("macos", "aarch64") => Ok("lembas-darwin-arm64"),
        ("macos", "x86_64") => Ok("lembas-darwin-x86_64"),
        ("linux", "x86_64") => Ok("lembas-linux-x86_64"),
        ("linux", "aarch64") => Ok("lembas-linux-aarch64"),
        _ => Err(UpdateError::UnsupportedPlatform { os, arch }),
    }
}

fn parse_version(tag: &str) -> std::result::Result<semver::Version, UpdateError> {
    let version_str = tag.strip_prefix('v').unwrap_or(tag);
    // Convert CalVer .devN to semver -dev.N format
    let normalized = if let Some((base, dev_num)) = version_str.split_once(".dev") {
        format!("{}-dev.{}", base, dev_num)
    } else {
        version_str.to_string()
    };
    semver::Version::parse(&normalized).map_err(|_| UpdateError::VersionParse(tag.to_string()))
}

fn current_version() -> semver::Version {
    parse_version(env!("LEMBAS_CLI_VERSION")).unwrap_or_else(|_| semver::Version::new(0, 0, 0))
}

async fn fetch_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page=50",
        GITHUB_REPO
    );

    let response = client
        .get(&url)
        .header("User-Agent", "lembas-cli")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .into_diagnostic()
        .context("failed to fetch releases")?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        miette::bail!("GitHub API error: {} - {}", status, text);
    }

    let github_releases: Vec<GitHubRelease> = response
        .json()
        .await
        .into_diagnostic()
        .context("failed to parse releases")?;

    let include_prereleases = std::env::var("LEMBAS_PRERELEASES").is_ok();

    let releases: Vec<Release> = github_releases
        .into_iter()
        .filter(|r| !r.draft && (include_prereleases || !r.prerelease))
        .filter_map(|r| {
            let version = parse_version(&r.tag_name).ok()?;
            Some(Release {
                version,
                tag: r.tag_name,
                assets: r.assets,
            })
        })
        .collect();

    Ok(releases)
}

pub async fn check_for_update(client: &reqwest::Client) -> Result<UpdateCheck> {
    let releases = fetch_releases(client).await?;

    let latest = releases
        .into_iter()
        .max_by(|a, b| a.version.cmp(&b.version));

    match latest {
        None => Err(UpdateError::NoReleases.into()),
        Some(release) if release.version > current_version() => Ok(UpdateCheck::Available(release)),
        Some(_) => Ok(UpdateCheck::AlreadyUpToDate),
    }
}

pub async fn list_versions(client: &reqwest::Client) -> Result<Vec<Release>> {
    let mut releases = fetch_releases(client).await?;
    releases.sort_by(|a, b| b.version.cmp(&a.version));
    Ok(releases)
}

pub async fn find_version(client: &reqwest::Client, version: &str) -> Result<Release> {
    let target_version = parse_version(version)?;
    let releases = fetch_releases(client).await?;

    releases
        .into_iter()
        .find(|r| r.version == target_version)
        .ok_or_else(|| UpdateError::VersionNotFound(version.to_string()).into())
}

async fn download_release(client: &reqwest::Client, release: &Release) -> Result<Vec<u8>> {
    let platform = get_platform_asset_name()?;

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == platform)
        .ok_or_else(|| UpdateError::AssetNotFound(platform.to_string()))?;

    let response = client
        .get(&asset.browser_download_url)
        .header("User-Agent", "lembas-cli")
        .send()
        .await
        .into_diagnostic()
        .context("failed to download release")?;

    if !response.status().is_success() {
        miette::bail!("download failed: {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(asset.size);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut bytes = Vec::with_capacity(total_size as usize);
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.into_diagnostic().context("download interrupted")?;
        pb.inc(chunk.len() as u64);
        bytes.extend_from_slice(&chunk);
    }

    pb.finish_with_message("Downloaded");

    Ok(bytes)
}

pub async fn perform_update(
    client: &reqwest::Client,
    release: &Release,
    force: bool,
) -> Result<()> {
    if !force && release.version <= current_version() {
        tracing::info!(
            "Already on version {} (target: {})",
            current_version(),
            release.version
        );
        return Ok(());
    }

    tracing::info!("Downloading v{}...", release.version);

    let binary = download_release(client, release).await?;

    // Write to temp file first
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(format!("lembas-update-{}", std::process::id()));

    {
        let mut file = std::fs::File::create(&temp_path)
            .into_diagnostic()
            .context("failed to create temp file")?;
        file.write_all(&binary)
            .into_diagnostic()
            .context("failed to write temp file")?;
    }

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o755))
            .into_diagnostic()
            .context("failed to set executable permissions")?;
    }

    // Atomic self-replace
    tracing::info!("Installing v{}...", release.version);
    self_replace::self_replace(&temp_path)
        .into_diagnostic()
        .context("failed to replace binary")?;

    // Clean up temp file (may fail on Windows, that's ok)
    let _ = std::fs::remove_file(&temp_path);

    tracing::info!(
        "Updated lembas CLI: {} -> {}",
        current_version(),
        release.version
    );

    Ok(())
}

pub fn current_version_string() -> String {
    current_version().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_simple() {
        let v = parse_version("v2026.7.1").unwrap();
        assert_eq!(v.major, 2026);
        assert_eq!(v.minor, 7);
        assert_eq!(v.patch, 1);
    }

    #[test]
    fn test_parse_version_without_v() {
        let v = parse_version("2026.7.1").unwrap();
        assert_eq!(v.major, 2026);
    }

    #[test]
    fn test_parse_version_dev() {
        let v = parse_version("v2026.7.1.dev5").unwrap();
        assert_eq!(v.major, 2026);
        assert!(!v.pre.is_empty());
    }

    #[test]
    fn test_get_platform_asset_name() {
        // Should not error on supported platforms
        let result = get_platform_asset_name();
        // This will pass on macOS/Linux, may fail on Windows (expected)
        if std::env::consts::OS == "macos" || std::env::consts::OS == "linux" {
            assert!(result.is_ok());
        }
    }
}
