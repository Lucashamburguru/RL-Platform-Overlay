use crate::state::config_dir;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::{ProcessesToUpdate, System};

pub fn is_rocket_league_running() -> bool {
    fn is_rocket_league_name(name: &OsStr) -> bool {
        let normalized = name.to_string_lossy().to_lowercase();
        normalized == "rocketleague.exe"
            || normalized == "rocketleague"
            || normalized == "rocketleague-linux-shipping"
    }

    let mut system = System::new_all();
    system.refresh_processes(ProcessesToUpdate::All, true);

    for process in system.processes().values() {
        if is_rocket_league_name(process.name()) {
            return true;
        }

        if let Some(executable_path) = process.exe()
            && let Some(file_name) = executable_path.file_name()
            && is_rocket_league_name(file_name)
        {
            return true;
        }
    }

    false
}

fn cooked_pc_console_path(rocket_league_path: &str) -> Result<PathBuf, String> {
    if rocket_league_path.trim().is_empty() {
        return Err("Rocket League folder path is empty.".to_string());
    }

    let root = Path::new(rocket_league_path);
    if !root.exists() || !root.is_dir() {
        return Err(format!(
            "Rocket League path does not exist or is not a directory: {}",
            rocket_league_path
        ));
    }

    let cooked_path = root.join("TAGame").join("CookedPCConsole");
    if !cooked_path.exists() || !cooked_path.is_dir() {
        return Err(format!(
            "Invalid Rocket League path. Could not find TAGame/CookedPCConsole in: {}",
            rocket_league_path
        ));
    }

    Ok(cooked_path)
}

fn boost_backup_dir() -> Result<PathBuf, String> {
    let conf_dir =
        config_dir().ok_or_else(|| "Could not resolve user config directory.".to_string())?;
    let backup_path = conf_dir.join("backups").join("Boost");
    if !backup_path.exists() {
        fs::create_dir_all(&backup_path)
            .map_err(|e| format!("Failed to create backup directory: {}", e))?;
    }
    Ok(backup_path)
}

async fn download_file(url: &str, dest_path: &Path) -> Result<(), String> {
    let client = wreq::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("HTTP client init failed: {}", e))?;

    let response = client
        .get(url)
        .header("User-Agent", "RL-Platform-Overlay")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Server returned HTTP status {}", status));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response bytes: {}", e))?;

    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create cache directories: {}", e))?;
    }

    let temp_path = dest_path.with_extension("download");
    fs::write(&temp_path, bytes).map_err(|e| format!("Failed to write temporary file: {}", e))?;
    fs::rename(&temp_path, dest_path)
        .map_err(|e| format!("Failed to replace cached file: {}", e))?;

    Ok(())
}

pub fn start_apply_alpha_boost(
    state: std::sync::Arc<crate::state::AppState>,
    rocket_league_path: String,
) {
    let state_clone = state.clone();
    tokio::spawn(async move {
        {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = "Initializing Alpha Boost swap...".to_string();
        }

        // 1. Validate game paths
        let game_dir = match cooked_pc_console_path(&rocket_league_path) {
            Ok(path) => path,
            Err(e) => {
                let mut status = state_clone.boost_swap_status.lock().unwrap();
                *status = format!("Error: {}", e);
                return;
            }
        };

        let conf_dir =
            match config_dir().ok_or_else(|| "Could not resolve config directory.".to_string()) {
                Ok(path) => path,
                Err(e) => {
                    let mut status = state_clone.boost_swap_status.lock().unwrap();
                    *status = format!("Error: {}", e);
                    return;
                }
            };

        let cache_dir = conf_dir.join("cache").join("Boost");
        let visual_cache = cache_dir.join("Boost_Standard_SF.upk");
        let audio_cache = cache_dir.join("SFX_Boost_Standard.bnk");

        // 2. Download custom visuals if not cached
        if !visual_cache.exists() {
            {
                let mut status = state_clone.boost_swap_status.lock().unwrap();
                *status = "Downloading custom visuals (~370KB)...".to_string();
            }
            let url =
                "https://api.rlpeak.com/v1/files/Boost/Boost_AlphaReward/Boost_Standard_SF.upk";
            if let Err(e) = download_file(url, &visual_cache).await {
                let mut status = state_clone.boost_swap_status.lock().unwrap();
                *status = format!("Download failed: {}", e);
                return;
            }
        }

        // 3. Download custom audio if not cached
        if !audio_cache.exists() {
            {
                let mut status = state_clone.boost_swap_status.lock().unwrap();
                *status = "Downloading custom audio (~77KB)...".to_string();
            }
            let url =
                "https://api.rlpeak.com/v1/files/Boost/Boost_AlphaReward/SFX_Boost_Standard.bnk";
            if let Err(e) = download_file(url, &audio_cache).await {
                let mut status = state_clone.boost_swap_status.lock().unwrap();
                *status = format!("Download failed: {}", e);
                return;
            }
        }

        // 4. Perform backups and copy files
        {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = "Backing up original assets...".to_string();
        }

        let backup_dir = match boost_backup_dir() {
            Ok(dir) => dir,
            Err(e) => {
                let mut status = state_clone.boost_swap_status.lock().unwrap();
                *status = format!("Backup failed: {}", e);
                return;
            }
        };

        let visual_target = game_dir.join("Boost_Standard_SF.upk");
        let audio_target = game_dir.join("SFX_Boost_Standard.bnk");

        let visual_backup = backup_dir.join("Boost_Standard_SF.upk");
        let audio_backup = backup_dir.join("SFX_Boost_Standard.bnk");

        // Verify standard target assets exist in standard folder
        if !visual_target.exists() || !visual_target.is_file() {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = "Error: Standard Boost visuals not found in game directory.".to_string();
            return;
        }
        if !audio_target.exists() || !audio_target.is_file() {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = "Error: Standard Boost audio not found in game directory.".to_string();
            return;
        }

        // Backup originals once
        if !visual_backup.exists()
            && let Err(e) = fs::copy(&visual_target, &visual_backup)
        {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = format!("Backup failed: {}", e);
            return;
        }
        if !audio_backup.exists()
            && let Err(e) = fs::copy(&audio_target, &audio_backup)
        {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = format!("Backup failed: {}", e);
            return;
        }

        {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = "Applying Alpha Boost assets...".to_string();
        }

        // Overwrite
        if let Err(e) = fs::copy(&visual_cache, &visual_target) {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = format!("Swap failed (check write permissions): {}", e);
            return;
        }
        if let Err(e) = fs::copy(&audio_cache, &audio_target) {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = format!("Swap failed (check write permissions): {}", e);
            return;
        }

        // Update configuration state
        let mut config = (**state_clone.config.load()).clone();
        config.alpha_boost_enabled = true;
        state_clone.save_config(config);

        {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = "Success: Alpha Boost applied!".to_string();
        }
    });
}

pub fn start_restore_standard_boost(
    state: std::sync::Arc<crate::state::AppState>,
    rocket_league_path: String,
) {
    let state_clone = state.clone();
    tokio::spawn(async move {
        {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = "Restoring Standard Boost...".to_string();
        }

        // 1. Validate game paths
        let game_dir = match cooked_pc_console_path(&rocket_league_path) {
            Ok(path) => path,
            Err(e) => {
                let mut status = state_clone.boost_swap_status.lock().unwrap();
                *status = format!("Error: {}", e);
                return;
            }
        };

        let backup_dir = match boost_backup_dir() {
            Ok(dir) => dir,
            Err(e) => {
                let mut status = state_clone.boost_swap_status.lock().unwrap();
                *status = format!("Backup resolve failed: {}", e);
                return;
            }
        };

        let visual_target = game_dir.join("Boost_Standard_SF.upk");
        let audio_target = game_dir.join("SFX_Boost_Standard.bnk");

        let visual_backup = backup_dir.join("Boost_Standard_SF.upk");
        let audio_backup = backup_dir.join("SFX_Boost_Standard.bnk");

        // Verify backups exist
        if !visual_backup.exists() || !visual_backup.is_file() {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = "Error: Backup visuals file not found. Cannot restore.".to_string();
            return;
        }
        if !audio_backup.exists() || !audio_backup.is_file() {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = "Error: Backup audio file not found. Cannot restore.".to_string();
            return;
        }

        // Overwrite game targets with backed up original assets
        if let Err(e) = fs::copy(&visual_backup, &visual_target) {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = format!("Restore failed (check permissions): {}", e);
            return;
        }
        if let Err(e) = fs::copy(&audio_backup, &audio_target) {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = format!("Restore failed (check permissions): {}", e);
            return;
        }

        // Update configuration state
        let mut config = (**state_clone.config.load()).clone();
        config.alpha_boost_enabled = false;
        state_clone.save_config(config);

        {
            let mut status = state_clone.boost_swap_status.lock().unwrap();
            *status = "Success: Standard Boost restored!".to_string();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cooked_pc_console_path_validation() {
        let temp_dir = std::env::temp_dir().join("rl_overlay_test_assets");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        assert!(cooked_pc_console_path("").is_err());
        assert!(cooked_pc_console_path(&temp_dir.to_string_lossy()).is_err());

        let ta_game = temp_dir.join("TAGame");
        let cooked = ta_game.join("CookedPCConsole");
        fs::create_dir_all(&cooked).unwrap();

        let resolved = cooked_pc_console_path(&temp_dir.to_string_lossy()).unwrap();
        assert_eq!(resolved, cooked);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
