use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Debug, Error)]
pub enum DistriError {
    #[error("Failed to get home directory: {0}")]
    HomeDirError(String),
    #[error("Failed to create directory: {0}")]
    CreateDirError(#[from] std::io::Error),
    #[error("Failed to download Distri: {0}")]
    DownloadError(String),
    #[error("Failed to extract Distri: {0}")]
    ExtractError(String),
    #[error("Failed to start Distri: {0}")]
    StartError(String),
    #[error("Distri binary not found at: {0}")]
    BinaryNotFound(String),
}

/// Get the path where Distri binary should be stored
fn get_distri_dir() -> Result<PathBuf, DistriError> {
    let home_dir = std::env::var("HOME")
        .map_err(|_| DistriError::HomeDirError("HOME environment variable not set".to_string()))?;
    Ok(PathBuf::from(home_dir).join(".vllora").join("distri"))
}

/// Get the path to the Distri binary
fn get_distri_binary_path() -> Result<PathBuf, DistriError> {
    let distri_dir = get_distri_dir()?;
    Ok(distri_dir.join("distri"))
}

/// Get the path to the Distri server binary
fn get_distri_server_binary_path() -> Result<PathBuf, DistriError> {
    let distri_dir = get_distri_dir()?;
    Ok(distri_dir.join("distri-server"))
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
}

/// Get the latest Distri version from GitHub releases
async fn get_latest_distri_version() -> Result<String, DistriError> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/repos/distrihub/distri/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "vllora-ai-gateway")
        .send()
        .await
        .map_err(|e| DistriError::DownloadError(format!("GitHub API error: {}", e)))?;

    if !resp.status().is_success() {
        return Err(DistriError::DownloadError(format!(
            "GitHub API returned status: {}",
            resp.status()
        )));
    }

    let release: GithubRelease = resp.json().await.map_err(|e| {
        DistriError::DownloadError(format!("Failed to parse GitHub release JSON: {}", e))
    })?;

    // tag names are typically "v0.2.7" – strip leading 'v' to compare with `distri --version`
    let version = release.tag_name.trim_start_matches('v').to_string();
    Ok(version)
}

/// Get the local Distri binary version by calling `distri --version`
fn get_local_distri_version(binary_path: &Path) -> Option<String> {
    if !binary_path.exists() {
        return None;
    }

    let output = Command::new(binary_path).arg("--version").output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut parts = stdout.split_whitespace();

    // assume format like: "distri 0.2.7"
    let _program = parts.next();
    parts.next().map(|s| s.to_string())
}

/// Detect the OS and architecture for downloading the correct binary
fn detect_platform() -> Result<(String, String, String), DistriError> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let platform = match os {
        "linux" => "linux",
        "macos" => "darwin",
        _ => {
            return Err(DistriError::DownloadError(format!(
                "Unsupported OS: {}",
                os
            )))
        }
    };

    let asset_arch = match arch {
        "x86_64" => "amd64",
        "aarch64" | "arm64" => "arm64",
        _ => {
            return Err(DistriError::DownloadError(format!(
                "Unsupported architecture: {}",
                arch
            )))
        }
    };

    let slug = format!("{}-{}", platform, asset_arch);
    Ok((platform.to_string(), asset_arch.to_string(), slug))
}

/// Download Distri binary directly from GitHub releases
async fn download_distri() -> Result<PathBuf, DistriError> {
    let distri_dir = get_distri_dir()?;
    let binary_path = get_distri_binary_path()?;
    let server_binary_path = get_distri_server_binary_path()?;

    // Check if both binaries already exist and are up to date
    if binary_path.exists() && server_binary_path.exists() {
        match get_latest_distri_version().await {
            Ok(latest_version) => match get_local_distri_version(&binary_path) {
                Some(local_ver) if local_ver == latest_version => {
                    info!(
                        "Distri binaries already exist and are up to date at {:?} and {:?} (version {}).",
                        binary_path, server_binary_path, local_ver
                    );
                    return Ok(binary_path);
                }
                Some(local_ver) => {
                    info!(
                        "Distri binaries out of date. Local version: {}, latest version: {}. Re-downloading.",
                        local_ver, latest_version
                    );
                }
                None => {
                    info!(
                        "Distri binaries exist but local version is unknown. Re-downloading latest {}.",
                        latest_version
                    );
                }
            },
            Err(e) => {
                // If we cannot reach GitHub to check for updates, keep using existing binaries
                warn!(
                    "Failed to get latest Distri version from GitHub: {}. Using existing binaries at {:?} and {:?}.",
                    e, binary_path, server_binary_path
                );
                return Ok(binary_path);
            }
        }
    }

    info!("Downloading Distri binary...");
    std::fs::create_dir_all(&distri_dir)?;

    // Detect platform
    let (platform, asset_arch, slug) = detect_platform()?;
    let asset = format!("distri-{}-{}.tar.gz", platform, asset_arch);
    let download_url = format!(
        "https://github.com/distrihub/distri/releases/latest/download/{}",
        asset
    );
    println!("Downloading from: {}", download_url);

    debug!("Downloading from: {}", download_url);

    // Download the tarball
    let client = reqwest::Client::new();
    let tarball_bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| DistriError::DownloadError(format!("Failed to download tarball: {}", e)))?
        .bytes()
        .await
        .map_err(|e| DistriError::DownloadError(format!("Failed to read tarball: {}", e)))?;

    // Create temp directory for extraction
    let temp_dir = distri_dir.join("temp");
    std::fs::create_dir_all(&temp_dir)?;

    // Write tarball to temp file
    let tarball_path = temp_dir.join(&asset);
    std::fs::write(&tarball_path, &tarball_bytes)
        .map_err(|e| DistriError::DownloadError(format!("Failed to write tarball: {}", e)))?;

    // Extract tarball
    let extract_dir = temp_dir.join("extract");
    std::fs::create_dir_all(&extract_dir)?;

    let output = Command::new("tar")
        .arg("-xzf")
        .arg(&tarball_path)
        .arg("-C")
        .arg(&extract_dir)
        .output()
        .map_err(|e| DistriError::ExtractError(format!("Failed to extract tarball: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DistriError::ExtractError(format!(
            "tar extraction failed: {}",
            stderr
        )));
    }

    // Helper function to find a binary by name recursively
    fn find_binary_recursive(
        dir: &Path,
        binary_name: &str,
    ) -> Result<Option<PathBuf>, std::io::Error> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Some(name) = path.file_name() {
                    if name == binary_name {
                        return Ok(Some(path));
                    }
                }
            } else if path.is_dir() {
                if let Some(found) = find_binary_recursive(&path, binary_name)? {
                    return Ok(Some(found));
                }
            }
        }
        Ok(None)
    }

    // Find the distri binary in the extracted files
    // The install script looks for: EXTRACT_ROOT/distri or searches for it
    let extract_root = extract_dir.join(&slug);

    // Try the expected path first (matching install script logic)
    let expected_binary = extract_root.join("distri");
    let found_distri = if expected_binary.exists() {
        Some(expected_binary)
    } else {
        find_binary_recursive(&extract_dir, "distri").map_err(|e| {
            DistriError::ExtractError(format!("Failed to search for distri binary: {}", e))
        })?
    };

    let source_distri = found_distri.ok_or_else(|| {
        DistriError::BinaryNotFound(format!(
            "distri binary not found in extracted archive at {:?}",
            extract_dir
        ))
    })?;

    // Find the distri-server binary (typically in server/distri-server)
    let expected_server_binary = extract_root.join("server").join("distri-server");
    let found_server = if expected_server_binary.exists() {
        Some(expected_server_binary)
    } else {
        find_binary_recursive(&extract_dir, "distri-server").map_err(|e| {
            DistriError::ExtractError(format!("Failed to search for distri-server binary: {}", e))
        })?
    };

    let source_server = found_server.ok_or_else(|| {
        DistriError::BinaryNotFound(format!(
            "distri-server binary not found in extracted archive at {:?}",
            extract_dir
        ))
    })?;

    let server_binary_path = get_distri_server_binary_path()?;

    // Copy both binaries to final location
    std::fs::copy(&source_distri, &binary_path)
        .map_err(|e| DistriError::DownloadError(format!("Failed to copy distri binary: {}", e)))?;

    std::fs::copy(&source_server, &server_binary_path).map_err(|e| {
        DistriError::DownloadError(format!("Failed to copy distri-server binary: {}", e))
    })?;

    // Make both binaries executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        // Set permissions for distri binary
        let mut perms = std::fs::metadata(&binary_path)
            .map_err(|e| {
                DistriError::DownloadError(format!("Failed to get distri binary metadata: {}", e))
            })?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&binary_path, perms).map_err(|e| {
            DistriError::DownloadError(format!("Failed to set distri binary permissions: {}", e))
        })?;

        // Set permissions for distri-server binary
        let mut perms = std::fs::metadata(&server_binary_path)
            .map_err(|e| {
                DistriError::DownloadError(format!(
                    "Failed to get distri-server binary metadata: {}",
                    e
                ))
            })?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&server_binary_path, perms).map_err(|e| {
            DistriError::DownloadError(format!(
                "Failed to set distri-server binary permissions: {}",
                e
            ))
        })?;
    }

    // Clean up temp directory
    let _ = std::fs::remove_dir_all(&temp_dir);

    info!(
        "✅ Distri binaries downloaded successfully to {:?} and {:?}",
        binary_path, server_binary_path
    );
    Ok(binary_path)
}

/// Start downloading Distri in the background
/// Returns a JoinHandle that can be awaited to get the binary path
pub fn download_distri_background() -> tokio::task::JoinHandle<Result<PathBuf, DistriError>> {
    tokio::spawn(async move { download_distri().await })
}

/// Start Distri server as a background process
/// If download_handle is provided, it will be awaited to get the binary path
pub async fn start_distri_server(
    port: u16,
    download_handle: Option<tokio::task::JoinHandle<Result<PathBuf, DistriError>>>,
) -> Result<tokio::process::Child, DistriError> {
    let home_dir = std::env::var("HOME").map(PathBuf::from).map_err(|_| {
        DistriError::HomeDirError("HOME environment variable not set".to_string())
    })?;

    let binary_path = if let Some(handle) = download_handle {
        handle
            .await
            .map_err(|e| DistriError::DownloadError(format!("Download task failed: {}", e)))??
    } else {
        download_distri().await?
    };

    if !binary_path.exists() {
        return Err(DistriError::BinaryNotFound(format!(
            "Distri binary not found at {:?}",
            binary_path
        )));
    }

    info!("🚀 Starting Distri server...");

    // Start Distri server
    let mut child = tokio::process::Command::new(&binary_path)
        .arg("serve")
        .current_dir(&home_dir)
        .arg("--headless")
        .arg(format!("--port={port}"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| DistriError::StartError(format!("Failed to start Distri: {}", e)))?;

    // Give it a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Check if process is still running
    match child.try_wait() {
        Ok(Some(status)) => {
            return Err(DistriError::StartError(format!(
                "Distri process exited immediately with status: {:?}",
                status
            )));
        }
        Ok(None) => {
            info!(
                "✅ Distri server started successfully (PID: {:?})",
                child.id()
            );
        }
        Err(e) => {
            warn!("Could not check Distri process status: {}", e);
        }
    }

    Ok(child)
}

/// Check if Distri server is already running
pub async fn is_distri_running(api_url: &str) -> bool {
    let url = format!("{}/v1/agents", api_url);
    let client = reqwest::Client::new();

    match client
        .get(&url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
    {
        Ok(response) => {
            // Any 2xx or 4xx response means the server is running
            response.status().is_client_error() || response.status().is_success()
        }
        Err(_) => false,
    }
}
