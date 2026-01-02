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

    // Check if binary already exists
    if binary_path.exists() {
        info!("Distri binary already exists at {:?}", binary_path);
        return Ok(binary_path);
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

    // Find the distri binary in the extracted files
    // The install script looks for: EXTRACT_ROOT/distri or searches for it
    let extract_root = extract_dir.join(&slug);

    // Try the expected path first (matching install script logic)
    let expected_binary = extract_root.join("distri");
    let found_binary = if expected_binary.exists() {
        Some(expected_binary)
    } else {
        // Search recursively for the binary (matching install script's find command)
        fn find_binary_recursive(dir: &Path) -> Result<Option<PathBuf>, std::io::Error> {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_file() {
                    if let Some(name) = path.file_name() {
                        if name == "distri" {
                            return Ok(Some(path));
                        }
                    }
                } else if path.is_dir() {
                    if let Some(found) = find_binary_recursive(&path)? {
                        return Ok(Some(found));
                    }
                }
            }
            Ok(None)
        }

        find_binary_recursive(&extract_dir)
            .map_err(|e| DistriError::ExtractError(format!("Failed to search for binary: {}", e)))?
    };

    let source_binary = found_binary.ok_or_else(|| {
        DistriError::BinaryNotFound(format!(
            "distri binary not found in extracted archive at {:?}",
            extract_dir
        ))
    })?;

    // Copy binary to final location
    std::fs::copy(&source_binary, &binary_path)
        .map_err(|e| DistriError::DownloadError(format!("Failed to copy binary: {}", e)))?;

    // Make binary executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&binary_path)
            .map_err(|e| {
                DistriError::DownloadError(format!("Failed to get binary metadata: {}", e))
            })?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&binary_path, perms).map_err(|e| {
            DistriError::DownloadError(format!("Failed to set binary permissions: {}", e))
        })?;
    }

    // Clean up temp directory
    let _ = std::fs::remove_dir_all(&temp_dir);

    info!(
        "✅ Distri binary downloaded successfully to {:?}",
        binary_path
    );
    Ok(binary_path)
}

/// Start Distri server as a background process
pub async fn start_distri_server(port: u16) -> Result<tokio::process::Child, DistriError> {
    let binary_path = download_distri().await?;

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
