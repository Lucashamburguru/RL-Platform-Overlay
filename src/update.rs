use crate::state::{AppState, AutoUpdateStatus, VersionCheck};
#[cfg(any(target_os = "windows", test))]
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
#[cfg(any(target_os = "windows", test))]
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use semver::Version;
use serde_json::Value;
#[cfg(any(target_os = "windows", test))]
use sha2::{Digest, Sha256};
#[cfg(target_os = "windows")]
use std::path::Path;
use std::sync::Arc;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/Lucashamburguru/RL-Platform-Overlay/releases/latest";
const WINDOWS_ASSET_NAME: &str = "rl-platform-overlay.exe";
const WINDOWS_CHECKSUM_ASSET_NAME: &str = "rl-platform-overlay.exe.sha256";
const WINDOWS_SIGNATURE_ASSET_NAME: &str = "rl-platform-overlay.exe.sig";
pub const RELEASE_SIGNING_PUBLIC_KEY_B64: &str = "PZxbpTJyQ7EzHd77YcsTT/BbTZecf6T7HVC3XO6AQpw=";
#[cfg(any(target_os = "windows", test))]
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
$logPath = Join-Path -Path (Split-Path -Parent $PSCommandPath) -ChildPath "apply_update.log"
$lastError = $null
for ($attempt = 1; $attempt -le 10; $attempt++) {
    try {
        Add-Content -LiteralPath $logPath -Value ("attempt {0}/10 copying {1} to {2}" -f $attempt, $Source, $Destination)
        Copy-Item -LiteralPath $Source -Destination $Destination -Force
        Add-Content -LiteralPath $logPath -Value ("attempt {0}/10 succeeded" -f $attempt)
        $lastError = $null
        break
    } catch {
        $lastError = $_
        Add-Content -LiteralPath $logPath -Value ("attempt {0}/10 failed: {1}" -f $attempt, $_.Exception.Message)
        Start-Sleep -Milliseconds 500
    }
}
if ($null -ne $lastError) {
    $message = "final failure after 10 attempts: {0}" -f $lastError.Exception.Message
    Add-Content -LiteralPath $logPath -Value $message
    throw $message
}
Start-Process -FilePath $Destination
Remove-Item -LiteralPath $Source -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
"#;

pub fn start_version_check(state: Arc<AppState>) {
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let version_check = match check_latest_release(&state).await {
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
    #[cfg(all(not(target_os = "windows"), not(test)))]
    {
        let _ = state;
        Err("Automatic updates are currently only supported on Windows.".to_string())
    }

    #[cfg(any(target_os = "windows", test))]
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
        if version_check.windows_signature_url.is_empty() {
            return Err("Latest release does not include a Windows signature asset.".to_string());
        }

        let download_url = version_check.windows_download_url.clone();
        let checksum_url = version_check.windows_checksum_url.clone();
        let signature_url = version_check.windows_signature_url.clone();
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

        let signature_response = client
            .get(&signature_url)
            .header("User-Agent", "RL-Platform-Overlay")
            .send()
            .await
            .map_err(|error| format!("Signature download failed: {error}"))?;
        let signature_status = signature_response.status();
        if !signature_status.is_success() {
            return Err(format!(
                "Signature download failed with status {signature_status}."
            ));
        }
        let signature_text = signature_response
            .text()
            .await
            .map_err(|error| format!("Could not read update signature: {error}"))?;
        let signature = parse_signature(&signature_text, WINDOWS_ASSET_NAME)
            .ok_or_else(|| format!("Signature file does not contain {WINDOWS_ASSET_NAME}."))?;

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
        verify_release_signature(&state, &bytes, &signature)?;

        let update_dir = state.paths.config_dir.join("update");
        tokio::fs::create_dir_all(&update_dir)
            .await
            .map_err(|error| format!("Could not create update directory: {error}"))?;

        let staged_exe = update_dir.join(format!("rl-platform-overlay-{latest_tag}.exe"));
        tokio::fs::write(&staged_exe, &bytes)
            .await
            .map_err(|error| format!("Could not stage update: {error}"))?;

        set_update_status(&state, true, "Restarting to apply update...", "");

        #[cfg(target_os = "windows")]
        spawn_update_script(&update_dir, &staged_exe)
            .map_err(|error| format!("Could not start updater: {error}"))?;

        #[cfg(not(target_os = "windows"))]
        {
            // Stub/noop for testing on Linux
            let _ = update_dir;
            let _ = staged_exe;
        }

        state
            .flags
            .should_exit
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(any(target_os = "windows", test))]
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

async fn check_latest_release(state: &AppState) -> Result<VersionCheck, String> {
    let url = state.system.release_url.load().to_string();
    let client = &state.system.http_client;
    let response = client
        .get(&url)
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
    let windows_signature =
        release_asset_url(&release, WINDOWS_SIGNATURE_ASSET_NAME).unwrap_or_default();

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
        windows_signature_url: windows_signature,
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

fn parse_version(version: &str) -> Option<Version> {
    Version::parse(version.trim().trim_start_matches('v')).ok()
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
fn parse_signature(content: &str, asset_name: &str) -> Option<Vec<u8>> {
    let content = content.trim_start_matches('\u{feff}');
    let asset_lower = asset_name.to_lowercase();

    if let Some(signature) = content.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.to_lowercase().contains(&asset_lower) {
            return None;
        }
        trimmed.split_whitespace().find_map(decode_signature_token)
    }) {
        return Some(signature);
    }

    content.split_whitespace().find_map(decode_signature_token)
}

#[cfg(any(target_os = "windows", test))]
fn decode_signature_token(token: &str) -> Option<Vec<u8>> {
    let decoded = BASE64.decode(token.trim()).ok()?;
    (decoded.len() == 64).then_some(decoded)
}

#[cfg(any(target_os = "windows", test))]
fn verify_release_signature(
    state: &AppState,
    bytes: &[u8],
    signature_bytes: &[u8],
) -> Result<(), String> {
    let public_key = state.system.release_public_key.load();
    verify_signature_with_public_key(bytes, signature_bytes, &public_key)
}

#[cfg(any(target_os = "windows", test))]
fn verify_signature_with_public_key(
    bytes: &[u8],
    signature_bytes: &[u8],
    public_key_b64: &str,
) -> Result<(), String> {
    let public_key_bytes = BASE64
        .decode(public_key_b64)
        .map_err(|error| format!("Invalid embedded release public key: {error}"))?;
    let public_key: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| "Invalid embedded release public key length.".to_string())?;
    let signature: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| "Invalid update signature length.".to_string())?;

    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|error| format!("Invalid embedded release public key: {error}"))?;
    verifying_key
        .verify(bytes, &Signature::from_bytes(&signature))
        .map_err(|_| "Update signature verification failed.".to_string())
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
                },
                {
                    "name": "rl-platform-overlay.exe.sig",
                    "browser_download_url": "https://example.com/rl-platform-overlay.exe.sig"
                }
            ]
        });

        assert_eq!(latest_release_tag(&release), Some("v0.2.0".to_string()));
        assert_eq!(
            release_asset_url(&release, WINDOWS_ASSET_NAME),
            Some("https://example.com/rl-platform-overlay.exe".to_string())
        );
        assert_eq!(
            release_asset_url(&release, WINDOWS_SIGNATURE_ASSET_NAME),
            Some("https://example.com/rl-platform-overlay.exe.sig".to_string())
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
    fn stable_release_beats_matching_prerelease() {
        assert_eq!(
            compare_versions("1.2.3", "1.2.3-rc1"),
            Some(std::cmp::Ordering::Greater)
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

    #[test]
    fn test_parse_signature_finds_named_asset_signature() {
        let signature = vec![42_u8; 64];
        let encoded = BASE64.encode(&signature);
        let content = format!("{encoded}  {WINDOWS_ASSET_NAME}\n");

        assert_eq!(
            parse_signature(&content, WINDOWS_ASSET_NAME),
            Some(signature.clone())
        );

        assert_eq!(
            parse_signature(&encoded, WINDOWS_ASSET_NAME),
            Some(signature)
        );
        assert_eq!(parse_signature("not-signature", WINDOWS_ASSET_NAME), None);
    }

    #[test]
    fn test_verify_signature_with_public_key() {
        use ed25519_dalek::{Signer, SigningKey};

        let seed = [7_u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let public_key_b64 = BASE64.encode(signing_key.verifying_key().as_bytes());
        let message = b"release bytes";
        let signature = signing_key.sign(message).to_bytes();

        assert!(verify_signature_with_public_key(message, &signature, &public_key_b64).is_ok());
        assert!(
            verify_signature_with_public_key(
                b"tampered release bytes",
                &signature,
                &public_key_b64
            )
            .is_err()
        );
    }

    #[test]
    fn update_script_logs_retry_attempts_and_final_failure() {
        assert!(UPDATE_SCRIPT.contains("attempt {0}/10 copying"));
        assert!(UPDATE_SCRIPT.contains("attempt {0}/10 failed"));
        assert!(UPDATE_SCRIPT.contains("final failure after 10 attempts"));
    }

    #[tokio::test]
    async fn test_autoupdate_integration() {
        use ed25519_dalek::{Signer, SigningKey};
        use std::time::Duration;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        // 1. Generate Ed25519 signing keypair for tests
        let seed = [42_u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let public_key_b64 = BASE64.encode(verifying_key.as_bytes());

        // 2. Prepare mock executable asset and its signatures/checksums
        let mock_exe_data = b"mock executable file contents for update test";

        let mut hasher = Sha256::new();
        hasher.update(mock_exe_data);
        let mock_exe_hash = format!("{:x}", hasher.finalize());

        let signature_bytes = signing_key.sign(mock_exe_data).to_bytes();
        let signature_b64 = BASE64.encode(signature_bytes);

        // 3. Bind TCP listener on a dynamic port for our mock HTTP server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let addr_str = addr.to_string();

        // 4. Spawn the mock HTTP server
        let mock_exe_data_clone = mock_exe_data.to_vec();
        let mock_exe_hash_clone = mock_exe_hash.clone();
        let signature_b64_clone = signature_b64.clone();
        let addr_str_server = addr_str.clone();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mock_exe_data_local = mock_exe_data_clone.clone();
                let mock_exe_hash_local = mock_exe_hash_clone.clone();
                let signature_b64_local = signature_b64_clone.clone();
                let addr_str_local = addr_str_server.clone();

                tokio::spawn(async move {
                    let mut buf = vec![0; 4096];
                    let n = match socket.read(&mut buf).await {
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    let req = String::from_utf8_lossy(&buf[..n]);

                    if req.contains("GET /releases/latest") {
                        let body = format!(
                            r#"{{
                                "draft": false,
                                "prerelease": false,
                                "tag_name": "v999.0.0",
                                "html_url": "https://github.com/Lucashamburguru/RL-Platform-Overlay/releases/tag/v999.0.0",
                                "assets": [
                                    {{
                                        "name": "rl-platform-overlay.exe",
                                        "browser_download_url": "http://{}/download/rl-platform-overlay.exe"
                                    }},
                                    {{
                                        "name": "rl-platform-overlay.exe.sha256",
                                        "browser_download_url": "http://{}/download/rl-platform-overlay.exe.sha256"
                                    }},
                                    {{
                                        "name": "rl-platform-overlay.exe.sig",
                                        "browser_download_url": "http://{}/download/rl-platform-overlay.exe.sig"
                                    }}
                                ]
                            }}"#,
                            addr_str_local, addr_str_local, addr_str_local
                        );
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                    } else if req.contains("GET /download/rl-platform-overlay.exe.sha256") {
                        let body = format!("{}  rl-platform-overlay.exe\n", mock_exe_hash_local);
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                    } else if req.contains("GET /download/rl-platform-overlay.exe.sig") {
                        let body = format!("{}  rl-platform-overlay.exe\n", signature_b64_local);
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                    } else if req.contains("GET /download/rl-platform-overlay.exe") {
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            mock_exe_data_local.len()
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                        let _ = socket.write_all(&mock_exe_data_local).await;
                    }
                });
            }
        });

        // 5. Initialize AppState
        let state = AppState::new();
        state
            .system
            .release_url
            .store(Arc::new(format!("http://{}/releases/latest", addr_str)));
        state
            .system
            .release_public_key
            .store(Arc::new(public_key_b64));

        // 6. Start the version check and wait for it to run
        start_version_check(state.clone());

        let start_time = std::time::Instant::now();
        let timeout = Duration::from_secs(5);
        let mut check_succeeded = false;

        while start_time.elapsed() < timeout {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let check = state.system.version_check.load();
            if check.checked {
                assert!(check.update_available, "Expected update to be available");
                assert_eq!(check.latest_tag, "v999.0.0");
                assert!(
                    check.error.is_empty(),
                    "Expected no check error, got: {}",
                    check.error
                );
                check_succeeded = true;
                break;
            }
        }
        assert!(check_succeeded, "Version check timed out");

        // 7. Start the auto update download and verification flow
        start_auto_update(state.clone());

        let mut update_succeeded = false;
        let start_time = std::time::Instant::now();

        while start_time.elapsed() < timeout {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let status = state.system.auto_update_status.load();

            if !status.running {
                assert!(
                    status.error.is_empty(),
                    "Expected no update error, got: {}",
                    status.error
                );
                update_succeeded = true;
                break;
            }

            // Check if should_exit is already set to true (which is set at the end of download_and_apply_update)
            if state
                .flags
                .should_exit
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                update_succeeded = true;
                break;
            }
        }
        assert!(update_succeeded, "Auto update timed out or failed");

        // 8. Verify the updated file is written properly in the staged path
        let update_dir = state.paths.config_dir.join("update");
        let staged_exe = update_dir.join("rl-platform-overlay-v999.0.0.exe");

        assert!(
            staged_exe.exists(),
            "Staged executable does not exist at {:?}",
            staged_exe
        );
        let read_bytes = tokio::fs::read(&staged_exe).await.unwrap();
        assert_eq!(read_bytes, mock_exe_data);

        // Cleanup temp directory
        let _ = tokio::fs::remove_dir_all(&state.paths.config_dir).await;
    }
}
