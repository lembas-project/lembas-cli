//! Self-update mechanism for the lembas CLI.
//!
//! Downloads pre-built binaries from GitHub releases and replaces the running binary atomically.
//! Binaries are verified using Ed25519 signatures before installation.

use std::io::Write;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use miette::{Context, IntoDiagnostic, Result};
use serde::Deserialize;
use thiserror::Error;

const GITHUB_REPO: &str = "lembas-project/lembas-cli";

/// Trusted public keys for verifying release signatures (hex-encoded).
/// Multiple keys are supported to allow key rotation:
/// 1. Generate new keypair
/// 2. Add new public key here, release new CLI
/// 3. Update GitHub secret to new private key
/// 4. (Later) Remove old public key
const TRUSTED_PUBLIC_KEYS_HEX: &[&str] = &[
    // Key 1 (2026-07-15): Initial signing key
    include_str!("signing_keys/key1.pub"),
];

fn decode_hex_key(hex: &str) -> Option<[u8; 32]> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).ok()?;
        bytes[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(bytes)
}

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

    #[error("Signature file not found for release")]
    #[diagnostic(code(lembas::update::signature_not_found))]
    SignatureNotFound,

    #[error("Invalid signature format")]
    #[diagnostic(code(lembas::update::invalid_signature))]
    InvalidSignature,

    #[error("Signature verification failed - binary not signed by trusted key")]
    #[diagnostic(
        code(lembas::update::signature_verification_failed),
        help("This could indicate a compromised release. Do not install.")
    )]
    SignatureVerificationFailed,
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

    let releases: Vec<Release> = github_releases
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
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

/// Verify a binary's signature against any of the trusted public keys.
fn verify_signature(binary: &[u8], signature_bytes: &[u8]) -> std::result::Result<(), UpdateError> {
    let signature =
        Signature::from_slice(signature_bytes).map_err(|_| UpdateError::InvalidSignature)?;

    for key_hex in TRUSTED_PUBLIC_KEYS_HEX {
        let key_bytes = decode_hex_key(key_hex).ok_or(UpdateError::InvalidSignature)?;
        let verifying_key =
            VerifyingKey::from_bytes(&key_bytes).map_err(|_| UpdateError::InvalidSignature)?;

        if verifying_key.verify(binary, &signature).is_ok() {
            return Ok(());
        }
    }

    Err(UpdateError::SignatureVerificationFailed)
}

async fn download_asset(
    client: &reqwest::Client,
    asset: &GitHubAsset,
    show_progress: bool,
) -> Result<Vec<u8>> {
    let response = client
        .get(&asset.browser_download_url)
        .header("User-Agent", "lembas-cli")
        .send()
        .await
        .into_diagnostic()
        .context("failed to download")?;

    if !response.status().is_success() {
        miette::bail!("download failed: {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(asset.size);

    let pb = if show_progress {
        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
                )
                .unwrap()
                .progress_chars("#>-"),
        );
        Some(pb)
    } else {
        None
    };

    let mut bytes = Vec::with_capacity(total_size as usize);
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.into_diagnostic().context("download interrupted")?;
        if let Some(ref pb) = pb {
            pb.inc(chunk.len() as u64);
        }
        bytes.extend_from_slice(&chunk);
    }

    if let Some(pb) = pb {
        pb.finish_with_message("Downloaded");
    }

    Ok(bytes)
}

async fn download_release(
    client: &reqwest::Client,
    release: &Release,
    skip_verify: bool,
) -> Result<Vec<u8>> {
    let platform = get_platform_asset_name()?;
    let sig_name = format!("{}.sig", platform);

    let binary_asset = release
        .assets
        .iter()
        .find(|a| a.name == platform)
        .ok_or_else(|| UpdateError::AssetNotFound(platform.to_string()))?;

    // Download binary with progress bar
    let binary = download_asset(client, binary_asset, true).await?;

    if skip_verify {
        tracing::warn!("Skipping signature verification - this is dangerous!");
        return Ok(binary);
    }

    let sig_asset = release
        .assets
        .iter()
        .find(|a| a.name == sig_name)
        .ok_or(UpdateError::SignatureNotFound)?;

    // Download signature (small, no progress bar)
    tracing::info!("Verifying signature...");
    let signature = download_asset(client, sig_asset, false).await?;

    // Verify signature before returning
    verify_signature(&binary, &signature)?;
    tracing::info!("Signature verified");

    Ok(binary)
}

pub async fn perform_update(
    client: &reqwest::Client,
    release: &Release,
    force: bool,
    skip_verify: bool,
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

    let binary = download_release(client, release, skip_verify).await?;

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

    #[test]
    fn test_verify_signature_rejects_invalid() {
        let binary = b"hello world";
        let bad_signature = [0u8; 64];
        let result = verify_signature(binary, &bad_signature);
        assert!(matches!(
            result,
            Err(UpdateError::SignatureVerificationFailed)
        ));
    }

    #[test]
    fn test_verify_signature_rejects_wrong_length() {
        let binary = b"hello world";
        let bad_signature = [0u8; 32]; // Wrong length
        let result = verify_signature(binary, &bad_signature);
        assert!(matches!(result, Err(UpdateError::InvalidSignature)));
    }
}
