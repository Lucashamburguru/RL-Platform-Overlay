#[cfg(target_os = "windows")]
use crate::state::config_dir;
use crate::state::{AppState, AutoUpdateStatus, VersionCheck};
use serde_json::Value;
#[cfg(any(target_os = "windows", test))]
use sha2::{Digest, Sha256};
#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/Lucashamburguru/RL-Platform-Overlay/releases/latest";
const WINDOWS_ASSET_NAME: &str = "rl-platform-overlay.exe";
const WINDOWS_CHECKSUM_ASSET_NAME: &str = "rl-platform-overlay.exe.sha256";
#[cfg(target_os = "windows")]
const UPDATE_SCRIPT: &str = r#"
param(
    [Parameter(Mandatory=$true)][int]$ProcessId,
    [Parameter(Mandatory=$true)][string]$Source,
    [Parameter(Mandatory=$true)][string]$Destination
)

$ErrorActionPreference = "Stop"

try {
    Wait-Process -Id $ProcessId -ErrorAction SilentlyContinue
} catch {}

Start-Sleep -Milliseconds 500
Copy-Item -LiteralPath $Source -Destination $Destination -Force
Start-Process -FilePath $Destination
Remove-Item -LiteralPath $Source -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
"#;

pub fn start_version_check(state: Arc<AppState>) {
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let client = state.system.http_client.clone();
        let version_check = match check_latest_release(&client).await {
            Ok(check) => check,
            Err(error) => VersionCheck {
                checked: true,
                error: format!("Version check failed: {error}"),
                ..Default::default()
            },
        };
        state.system.version_check.store(Arc::new(version_check));
    });
}

pub fn start_auto_update(state: Arc<AppState>) {
    if state.system.auto_update_status.load().running {
        return;
    }

    state
        .system
        .auto_update_status
        .store(Arc::new(AutoUpdateStatus {
            running: true,
            message: "Preparing update...".to_string(),
            error: String::new(),
        }));

    tokio::spawn(async move {
        match download_and_apply_update(state.clone()).await {
            Ok(()) => {}
            Err(error) => {
                state
                    .system
                    .auto_update_status
                    .store(Arc::new(AutoUpdateStatus {
                        running: false,
                        message: String::new(),
                        error,
                    }));
            }
        }
    });
}

async fn download_and_apply_update(state: Arc<AppState>) -> Result<(), String> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state;
        Err("Automatic updates are currently only supported on Windows.".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        let version_check = state.system.version_check.load();
        if !version_check.update_available {
            return Err("No update is currently available.".to_string());
        }
        if version_check.windows_download_url.is_empty() {
            return Err("Latest release does not include a Windows executable asset.".to_string());
        }
        if version_check.windows_checksum_url.is_empty() {
            return Err("Latest release does not include a Windows checksum asset.".to_string());
        }

        let download_url = version_check.windows_download_url.clone();
        let checksum_url = version_check.windows_checksum_url.clone();
        let latest_tag = version_check.latest_tag.clone();
        drop(version_check);

        set_update_status(&state, true, "Downloading update...", "");
        let client = state.system.http_client.clone();

        let checksum_response = client
            .get(&checksum_url)
            .header("User-Agent", "RL-Platform-Overlay")
            .send()
            .await
            .map_err(|error| format!("Checksum download failed: {error}"))?;
        let checksum_status = checksum_response.status();
        if !checksum_status.is_success() {
            return Err(format!(
                "Checksum download failed with status {checksum_status}."
            ));
        }
        let checksum_text = checksum_response
            .text()
            .await
            .map_err(|error| format!("Could not read checksum: {error}"))?;
        let expected_sha = parse_checksum(&checksum_text, WINDOWS_ASSET_NAME)
            .ok_or_else(|| format!("Checksum file does not contain {WINDOWS_ASSET_NAME}."))?;

        let download_response = client
            .get(&download_url)
            .header("User-Agent", "RL-Platform-Overlay")
            .send()
            .await
            .map_err(|error| format!("Update download failed: {error}"))?;
        let download_status = download_response.status();
        if !download_status.is_success() {
            return Err(format!(
                "Update download failed with status {download_status}."
            ));
        }
        let bytes = download_response
            .bytes()
            .await
            .map_err(|error| format!("Could not read update download: {error}"))?;

        set_update_status(&state, true, "Verifying update...", "");
        let actual_sha = sha256_hex(&bytes);
        if !actual_sha.eq_ignore_ascii_case(&expected_sha) {
            return Err(format!(
                "Update checksum mismatch. Expected {expected_sha}, got {actual_sha}."
            ));
        }

        let update_dir = config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("update");
        std::fs::create_dir_all(&update_dir)
            .map_err(|error| format!("Could not create update directory: {error}"))?;

        let staged_exe = update_dir.join(format!("rl-platform-overlay-{latest_tag}.exe"));
        std::fs::write(&staged_exe, &bytes)
            .map_err(|error| format!("Could not stage update: {error}"))?;

        set_update_status(&state, true, "Restarting to apply update...", "");
        spawn_update_script(&update_dir, &staged_exe)
            .map_err(|error| format!("Could not start updater: {error}"))?;

        std::process::exit(0);
    }
}

#[cfg(target_os = "windows")]
fn set_update_status(state: &AppState, running: bool, message: &str, error: &str) {
    state
        .system
        .auto_update_status
        .store(Arc::new(AutoUpdateStatus {
            running,
            message: message.to_string(),
            error: error.to_string(),
        }));
}

#[cfg(target_os = "windows")]
fn spawn_update_script(update_dir: &Path, staged_exe: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let script_path = update_dir.join("apply_update.ps1");
    std::fs::write(&script_path, UPDATE_SCRIPT)?;

    let current_exe = std::env::current_exe()?;
    let process_id = std::process::id().to_string();

    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(&script_path)
        .arg("-ProcessId")
        .arg(process_id)
        .arg("-Source")
        .arg(staged_exe)
        .arg("-Destination")
        .arg(current_exe)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;

    Ok(())
}

async fn check_latest_release(client: &wreq::Client) -> Result<VersionCheck, String> {
    let response = client
        .get(LATEST_RELEASE_URL)
        .header("User-Agent", "RL-Platform-Overlay")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Ok(VersionCheck {
            checked: true,
            error: format!("GitHub version check failed with status {status}."),
            ..Default::default()
        });
    }

    let body = response.text().await.map_err(|error| error.to_string())?;
    let Ok(release) = serde_json::from_str::<Value>(&body) else {
        return Ok(VersionCheck {
            checked: true,
            error: "Could not parse GitHub release response.".to_string(),
            ..Default::default()
        });
    };

    if release["draft"].as_bool().unwrap_or(false)
        || release["prerelease"].as_bool().unwrap_or(false)
    {
        return Ok(VersionCheck {
            checked: true,
            error: "Latest GitHub release is not a public stable release.".to_string(),
            ..Default::default()
        });
    }

    let Some(latest_tag) = latest_release_tag(&release) else {
        return Ok(VersionCheck {
            checked: true,
            error: "No release tag found on GitHub.".to_string(),
            ..Default::default()
        });
    };

    let update_available = compare_versions(&latest_tag, CURRENT_VERSION)
        .is_some_and(|ordering| ordering == std::cmp::Ordering::Greater);
    let windows_asset = release_asset_url(&release, WINDOWS_ASSET_NAME).unwrap_or_default();
    let windows_checksum =
        release_asset_url(&release, WINDOWS_CHECKSUM_ASSET_NAME).unwrap_or_default();

    Ok(VersionCheck {
        checked: true,
        update_available,
        latest_tag: latest_tag.clone(),
        release_url: release["html_url"]
            .as_str()
            .unwrap_or("https://github.com/Lucashamburguru/RL-Platform-Overlay/releases/latest")
            .to_string(),
        windows_download_url: windows_asset,
        windows_checksum_url: windows_checksum,
        error: String::new(),
    })
}

fn release_asset_url(release: &Value, asset_name: &str) -> Option<String> {
    release["assets"].as_array()?.iter().find_map(|asset| {
        let name = asset["name"].as_str()?;
        (name == asset_name).then(|| asset["browser_download_url"].as_str().map(str::to_string))?
    })
}

fn latest_release_tag(release: &Value) -> Option<String> {
    let tag = release["tag_name"].as_str()?;
    parse_version(tag)?;
    Some(tag.to_string())
}

fn compare_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    Some(parse_version(left)?.cmp(&parse_version(right)?))
}

fn parse_version(version: &str) -> Option<Vec<u64>> {
    let clean = version
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()?;
    let mut parts = Vec::new();
    for part in clean.split('.') {
        parts.push(part.parse().ok()?);
    }
    Some(parts)
}

#[cfg(any(target_os = "windows", test))]
fn parse_checksum(content: &str, asset_name: &str) -> Option<String> {
    let content = content.trim_start_matches('\u{feff}');
    let asset_lower = asset_name.to_lowercase();

    // First, try to find a line containing the asset name and a 64-char hex hash
    if let Some(hash) = content.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        let line_lower = trimmed.to_lowercase();
        if !line_lower.contains(&asset_lower) {
            return None;
        }
        trimmed
            .split_whitespace()
            .find(|word| word.len() == 64 && word.chars().all(|c| c.is_ascii_hexdigit()))
            .map(str::to_string)
    }) {
        return Some(hash);
    }

    // Fallback: if there's no line containing the asset name, but a line consists entirely of a 64-char hex hash, return it.
    // This is useful if the checksum file only contains the raw hash.
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

#[cfg(any(target_os = "windows", test))]
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_latest_release_tag_uses_release_tag_name() {
        let release = json!({
            "tag_name": "v0.2.0",
            "html_url": "https://github.com/example/repo/releases/tag/v0.2.0",
            "assets": [
                {
                    "name": "rl-platform-overlay.exe",
                    "browser_download_url": "https://example.com/rl-platform-overlay.exe"
                }
            ]
        });

        assert_eq!(latest_release_tag(&release), Some("v0.2.0".to_string()));
        assert_eq!(
            release_asset_url(&release, WINDOWS_ASSET_NAME),
            Some("https://example.com/rl-platform-overlay.exe".to_string())
        );
    }

    #[test]
    fn test_latest_release_tag_ignores_non_semver_tag_name() {
        let release = json!({ "tag_name": "latest" });

        assert_eq!(latest_release_tag(&release), None);
    }

    #[test]
    fn test_compare_versions_strips_v_prefix() {
        assert_eq!(
            compare_versions("v0.2.0", "0.1.4"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            compare_versions("v0.1.4", "0.1.4"),
            Some(std::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn test_parse_checksum_finds_named_asset_hash() {
        let checksum = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  rl-platform-overlay.exe\n";

        assert_eq!(
            parse_checksum(checksum, WINDOWS_ASSET_NAME),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string())
        );

        // Case-insensitivity test
        let checksum_caps = "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF  RL-PLATFORM-OVERLAY.EXE\n";
        assert_eq!(
            parse_checksum(checksum_caps, WINDOWS_ASSET_NAME),
            Some("0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF".to_string())
        );

        // BOM test
        let checksum_bom = "\u{feff}0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  rl-platform-overlay.exe\n";
        assert_eq!(
            parse_checksum(checksum_bom, WINDOWS_ASSET_NAME),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string())
        );

        // BSD-style checksum
        let checksum_bsd = "SHA256 (rl-platform-overlay.exe) = 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n";
        assert_eq!(
            parse_checksum(checksum_bsd, WINDOWS_ASSET_NAME),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string())
        );

        // Raw hash only (no filename)
        let checksum_raw = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n";
        assert_eq!(
            parse_checksum(checksum_raw, WINDOWS_ASSET_NAME),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string())
        );

        // Raw hash only with BOM
        let checksum_raw_bom =
            "\u{feff}0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\r\n";
        assert_eq!(
            parse_checksum(checksum_raw_bom, WINDOWS_ASSET_NAME),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string())
        );
    }

    #[test]
    fn test_sha256_hex_hashes_bytes() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
