#![cfg_attr(feature = "microsoft-store", allow(dead_code))]

use crate::state::{AppState, config_dir};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use sysinfo::{ProcessesToUpdate, System};

const BOOST_RELEASE_TAG: &str = "alpha-boost-assets-v1";
const BACKUP_METADATA_FILE: &str = "backup_metadata.json";
const METADATA_VERSION: u32 = 3;
#[allow(dead_code)]
const ALPHA_DONOR_ID: &str = "Boost_AlphaReward";
#[allow(dead_code)]
const VISUAL_TARGET_ID: &str = "Boost_Standard";
#[allow(dead_code)]
const KEY_INDEX_URL: &str =
    "https://raw.githubusercontent.com/ShinyEmii/Toga-Files/refs/heads/master/products.csv";

const ALPHA_VISUAL_SHA256: &str = "";
const ALPHA_AUDIO_SHA256: &str = "cca81ccfdd4bfb63464211cbd8354f86ec884a43afbe890c2998593242231211";

#[derive(Clone, Copy, Debug)]
struct BoostAssetSpec {
    file_name: &'static str,
    url: &'static str,
    expected_sha256: &'static str,
}

const ALPHA_VISUAL_SPEC: BoostAssetSpec = BoostAssetSpec {
    file_name: "Boost_Standard_SF.upk",
    url: "",
    expected_sha256: ALPHA_VISUAL_SHA256,
};

const ALPHA_AUDIO_SPEC: BoostAssetSpec = BoostAssetSpec {
    file_name: "SFX_Boost_Standard.bnk",
    url: "https://github.com/Lucashamburguru/RL-Platform-Overlay/releases/download/alpha-boost-assets-v1/SFX_Boost_Standard.bnk",
    expected_sha256: ALPHA_AUDIO_SHA256,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BoostBackupMetadata {
    version: u32,
    created_unix_ms: u128,
    release_tag: String,
    files: Vec<BoostBackupFileMetadata>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BoostBackupFileMetadata {
    file_name: String,
    original_size: u64,
    original_sha256: String,
    backup_path: String,
    #[serde(default)]
    applied_sha256: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct BoostSwapInspection {
    pub metadata_exists: bool,
    pub cache_verified: bool,
    pub game_file_state: BoostGameFileState,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct BoostSwapInspectionSnapshot {
    pub rocket_league_path: String,
    pub inspection: BoostSwapInspection,
    pub loaded: bool,
    pub refreshing: bool,
    pub error: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoostGameFileState {
    #[default]
    Unavailable,
    Original,
    Alpha,
    Unbacked,
    Mixed,
    Unknown,
    Missing,
}

impl BoostGameFileState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Original => "original",
            Self::Alpha => "alpha",
            Self::Unbacked => "unbacked",
            Self::Mixed => "mixed",
            Self::Unknown => "unknown",
            Self::Missing => "missing",
        }
    }
}

pub struct RocketLeagueProcessWatcher {
    system: System,
}

#[derive(Clone, Debug, Default)]
pub struct ProcessDetection {
    pub running: bool,
    pub detail: String,
}

impl RocketLeagueProcessWatcher {
    pub fn new() -> Self {
        Self {
            system: System::new(),
        }
    }

    pub fn is_running(&mut self) -> bool {
        self.detect().running
    }

    pub fn detect(&mut self) -> ProcessDetection {
        self.system.refresh_processes(ProcessesToUpdate::All, true);

        for process in self.system.processes().values() {
            if is_rocket_league_name(process.name()) {
                return ProcessDetection {
                    running: true,
                    detail: format!("process name: {}", process.name().to_string_lossy()),
                };
            }

            if let Some(executable_path) = process.exe()
                && let Some(file_name) = executable_path.file_name()
                && is_rocket_league_name(file_name)
            {
                return ProcessDetection {
                    running: true,
                    detail: format!("executable: {}", executable_path.display()),
                };
            }

            if let Some(detail) = process
                .cmd()
                .iter()
                .find_map(|argument| rocket_league_argument_match(argument))
            {
                return ProcessDetection {
                    running: true,
                    detail,
                };
            }
        }

        ProcessDetection {
            running: false,
            detail: "not found".to_string(),
        }
    }
}

impl Default for RocketLeagueProcessWatcher {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn is_rocket_league_name(name: &OsStr) -> bool {
    let name_lossy = name.to_string_lossy();
    // Bolt: zero-allocation string comparison
    name_lossy.eq_ignore_ascii_case("rocketleague.exe")
        || name_lossy.eq_ignore_ascii_case("rocketleague.ex")
        || name_lossy.eq_ignore_ascii_case("rocketleague")
        || name_lossy.eq_ignore_ascii_case("rocketleague-linux-shipping")
}

fn rocket_league_argument_match(argument: &OsStr) -> Option<String> {
    let normalized = argument.to_string_lossy().to_lowercase().replace('\\', "/");
    if normalized.contains("rocketleague.exe")
        || normalized.contains("rocketleague_eac.exe")
        || normalized.contains("rocketleague-linux-shipping")
        || normalized.contains("rocketleague/binaries")
    {
        Some(format!("command: {}", argument.to_string_lossy()))
    } else {
        None
    }
}

#[allow(dead_code)]
pub fn is_rocket_league_running() -> bool {
    RocketLeagueProcessWatcher::new().is_running()
}

#[allow(dead_code)]
pub fn inspect_boost_swap(rocket_league_path: &str) -> BoostSwapInspection {
    let Ok(conf_dir) =
        config_dir().ok_or_else(|| "Could not resolve config directory.".to_string())
    else {
        return BoostSwapInspection {
            message: "Could not resolve config directory.".to_string(),
            ..Default::default()
        };
    };
    inspect_boost_swap_at(rocket_league_path, &conf_dir)
}

fn inspect_boost_swap_at(rocket_league_path: &str, conf_dir: &Path) -> BoostSwapInspection {
    #[cfg(not(feature = "microsoft-store"))]
    if let Some(game_file_state) = crate::item_swapper::alpha_preset_file_state(rocket_league_path)
    {
        return BoostSwapInspection {
            metadata_exists: true,
            cache_verified: cached_asset_verified(conf_dir, ALPHA_AUDIO_SPEC),
            game_file_state,
            message: "Alpha Boost is managed by the Item Swapper catalog.".into(),
        };
    }
    let metadata_exists = backup_metadata_path(conf_dir).exists();
    let game_file_state = inspect_game_file_state(rocket_league_path, conf_dir)
        .unwrap_or(BoostGameFileState::Unavailable);
    let cache_verified = cached_asset_verified(conf_dir, ALPHA_AUDIO_SPEC);

    let message = if cache_verified {
        "Visual UPK will be generated from this Rocket League install; cached audio verified."
            .to_string()
    } else {
        format!(
            "Visual UPK will be generated from this Rocket League install; audio will download from {BOOST_RELEASE_TAG}."
        )
    };

    BoostSwapInspection {
        metadata_exists,
        cache_verified,
        game_file_state,
        message,
    }
}

pub fn request_boost_swap_inspection(
    state: &std::sync::Arc<AppState>,
    rocket_league_path: String,
    force: bool,
) {
    let current = state.boost.boost_swap_inspection.load();
    if !force
        && current.loaded
        && current.rocket_league_path == rocket_league_path
        && !current.refreshing
    {
        return;
    }
    if state
        .boost
        .inspection_running
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }

    state
        .boost
        .boost_swap_inspection
        .store(std::sync::Arc::new(BoostSwapInspectionSnapshot {
            rocket_league_path: rocket_league_path.clone(),
            refreshing: true,
            ..(**current).clone()
        }));

    let state_clone = state.clone();
    let config_dir = state.paths.config_dir.clone();
    let run = move || {
        let snapshot = BoostSwapInspectionSnapshot {
            inspection: inspect_boost_swap_at(&rocket_league_path, &config_dir),
            rocket_league_path,
            loaded: true,
            refreshing: false,
            error: String::new(),
        };
        state_clone
            .boost
            .boost_swap_inspection
            .store(std::sync::Arc::new(snapshot));
        state_clone
            .boost
            .inspection_running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn_blocking(run);
    } else {
        run();
    }
}

fn inspect_game_file_state(
    rocket_league_path: &str,
    conf_dir: &Path,
) -> Result<BoostGameFileState, String> {
    if rocket_league_path.trim().is_empty() {
        return Ok(BoostGameFileState::Unavailable);
    }

    let game_dir = cooked_pc_console_path(rocket_league_path)?;
    let targets = boost_targets(&game_dir);
    inspect_game_file_state_for_targets(&targets, conf_dir)
}

fn inspect_game_file_state_for_targets(
    targets: &[(&str, PathBuf)],
    conf_dir: &Path,
) -> Result<BoostGameFileState, String> {
    if targets
        .iter()
        .any(|(_, path)| !path.exists() || !path.is_file())
    {
        return Ok(BoostGameFileState::Missing);
    }

    let metadata = load_backup_metadata(conf_dir)?;
    if metadata
        .as_ref()
        .is_some_and(|metadata| metadata.version != METADATA_VERSION)
    {
        return Ok(BoostGameFileState::Unbacked);
    }
    let mut states = Vec::with_capacity(targets.len());
    for (file_name, target) in targets.iter() {
        let actual_hash = file_sha256(target)?;
        let alpha_hash = metadata
            .as_ref()
            .and_then(|metadata| metadata_file(metadata, file_name).ok())
            .and_then(|file| file.applied_sha256.as_deref())
            .or_else(|| alpha_hash_for_file(file_name))
            .unwrap_or("");
        let is_alpha =
            expected_hash_configured(alpha_hash) && actual_hash.eq_ignore_ascii_case(alpha_hash);
        let is_original = metadata
            .as_ref()
            .and_then(|metadata| metadata_file(metadata, file_name).ok())
            .is_some_and(|file| actual_hash.eq_ignore_ascii_case(&file.original_sha256));

        states.push(if is_alpha {
            BoostGameFileState::Alpha
        } else if is_original {
            BoostGameFileState::Original
        } else if metadata.is_none() {
            BoostGameFileState::Unbacked
        } else {
            BoostGameFileState::Unknown
        });
    }

    if states
        .iter()
        .all(|state| *state == BoostGameFileState::Alpha)
    {
        Ok(BoostGameFileState::Alpha)
    } else if states
        .iter()
        .all(|state| *state == BoostGameFileState::Original)
    {
        Ok(BoostGameFileState::Original)
    } else if states
        .iter()
        .all(|state| *state == BoostGameFileState::Unbacked)
    {
        Ok(BoostGameFileState::Unbacked)
    } else if states.contains(&BoostGameFileState::Unknown) {
        Ok(BoostGameFileState::Unknown)
    } else {
        Ok(BoostGameFileState::Mixed)
    }
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

#[allow(dead_code)]
fn boost_backup_dir(conf_dir: &Path) -> Result<PathBuf, String> {
    let backup_path = conf_dir.join("backups").join("Boost");
    fs::create_dir_all(&backup_path)
        .map_err(|e| format!("Failed to create backup directory: {e}"))?;
    Ok(backup_path)
}

fn boost_cache_dir(conf_dir: &Path) -> Result<PathBuf, String> {
    let cache_path = conf_dir.join("cache").join("Boost");
    fs::create_dir_all(&cache_path)
        .map_err(|e| format!("Failed to create cache directory: {e}"))?;
    Ok(cache_path)
}

fn backup_metadata_path(conf_dir: &Path) -> PathBuf {
    conf_dir
        .join("backups")
        .join("Boost")
        .join(BACKUP_METADATA_FILE)
}

fn expected_hash_configured(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn cached_asset_verified(conf_dir: &Path, spec: BoostAssetSpec) -> bool {
    if !expected_hash_configured(spec.expected_sha256) {
        return false;
    }
    let path = conf_dir.join("cache").join("Boost").join(spec.file_name);
    file_sha256(&path)
        .map(|actual| actual.eq_ignore_ascii_case(spec.expected_sha256))
        .unwrap_or(false)
}

async fn ensure_verified_cached_asset(
    conf_dir: &Path,
    spec: BoostAssetSpec,
) -> Result<PathBuf, String> {
    if !expected_hash_configured(spec.expected_sha256) {
        return Err(format!(
            "Error: SHA-256 is not configured for {}. Upload the GitHub Release asset and fill the expected hash constant.",
            spec.file_name
        ));
    }

    let cache_dir = boost_cache_dir(conf_dir)?;
    let cache_path = cache_dir.join(spec.file_name);
    if cache_path.exists() {
        match file_sha256(&cache_path) {
            Ok(hash) if hash.eq_ignore_ascii_case(spec.expected_sha256) => {
                return Ok(cache_path);
            }
            Ok(_) | Err(_) => {
                let _ = fs::remove_file(&cache_path);
            }
        }
    }

    download_file(spec.url, &cache_path).await?;
    let actual = file_sha256(&cache_path)?;
    if !actual.eq_ignore_ascii_case(spec.expected_sha256) {
        let _ = fs::remove_file(&cache_path);
        return Err(format!(
            "Failed: hash verification failed for {}.",
            spec.file_name
        ));
    }

    Ok(cache_path)
}

pub(crate) async fn prepare_alpha_audio_asset() -> Result<PathBuf, String> {
    let conf_dir =
        config_dir().ok_or_else(|| "Error: Could not resolve config directory.".to_string())?;
    ensure_verified_cached_asset(&conf_dir, ALPHA_AUDIO_SPEC).await
}

async fn download_file(url: &str, dest_path: &Path) -> Result<(), String> {
    let client = wreq::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .redirect(wreq::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;

    let response = client
        .get(url)
        .header("User-Agent", "RL-Platform-Overlay")
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let location = response
            .headers()
            .get(wreq::header::LOCATION)
            .and_then(|header| header.to_str().ok())
            .unwrap_or("");
        if location.is_empty() {
            return Err(format!("Server returned HTTP status {status} for {url}"));
        }
        return Err(format!(
            "Server returned HTTP status {status} for {url}; redirect location: {location}"
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response bytes: {e}"))?;

    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create cache directories: {e}"))?;
    }

    let temp_path = dest_path.with_extension("download");
    fs::write(&temp_path, bytes).map_err(|e| format!("Failed to write temporary file: {e}"))?;
    fs::rename(&temp_path, dest_path).map_err(|e| format!("Failed to replace cached file: {e}"))?;

    Ok(())
}

pub fn start_apply_alpha_boost(
    state: std::sync::Arc<crate::state::AppState>,
    rocket_league_path: String,
) {
    #[cfg(not(feature = "microsoft-store"))]
    {
        crate::item_swapper::start_alpha_preset(state, rocket_league_path);
        return;
    }
    #[cfg(feature = "microsoft-store")]
    let state_clone = state.clone();
    #[cfg(feature = "microsoft-store")]
    tokio::spawn(async move {
        set_boost_status(&state_clone, "Initializing Alpha Boost swap...");
        match apply_alpha_boost(&rocket_league_path).await {
            Ok(()) => {
                state_clone.update_config(|config| config.alpha_boost_enabled = true);
                set_boost_status(&state_clone, "Success: Alpha Boost applied!");
                request_boost_swap_inspection(&state_clone, rocket_league_path.clone(), true);
            }
            Err(error) => set_boost_status(&state_clone, &error),
        }
    });
}

/// Builds a visual swap from the installed donor and target packages, then applies audio separately.
#[allow(dead_code)]
async fn apply_alpha_boost(rocket_league_path: &str) -> Result<(), String> {
    let game_dir = cooked_pc_console_path(rocket_league_path)?;
    let conf_dir =
        config_dir().ok_or_else(|| "Error: Could not resolve config directory.".to_string())?;

    let targets = boost_targets(&game_dir);
    verify_targets_exist(&targets)?;
    let donor_path = game_dir.join(format!("{ALPHA_DONOR_ID}_SF.upk"));
    let donor = fs::read(&donor_path).map_err(|e| {
        format!(
            "Cannot read installed Alpha Boost package {}: {e}",
            donor_path.display()
        )
    })?;
    let (donor_key, target_key) = fetch_upk_keys().await?;
    if load_backup_metadata(&conf_dir)?
        .as_ref()
        .is_none_or(|metadata| metadata.version != METADATA_VERSION)
    {
        let current_target = fs::read(&targets[0].1)
            .map_err(|e| format!("Cannot read installed target UPK: {e}"))?;
        crate::upk_swap::masquerade(
            &donor,
            &current_target,
            &donor_key,
            &target_key,
            ALPHA_DONOR_ID,
            VISUAL_TARGET_ID,
        )?;
    }
    let mut metadata = ensure_backup_metadata(&conf_dir, &targets)?;
    refresh_updated_backups(
        &conf_dir,
        &targets,
        &donor,
        &donor_key,
        &target_key,
        &mut metadata,
    )?;
    verify_targets_known(&targets, &metadata, &[ALPHA_VISUAL_SPEC, ALPHA_AUDIO_SPEC])?;
    let original_visual =
        PathBuf::from(&metadata_file(&metadata, ALPHA_VISUAL_SPEC.file_name)?.backup_path);
    let original_hash = &metadata_file(&metadata, ALPHA_VISUAL_SPEC.file_name)?.original_sha256;
    if !file_sha256(&original_visual)?.eq_ignore_ascii_case(original_hash) {
        return Err("Visual backup failed verification".into());
    }
    let target =
        fs::read(&original_visual).map_err(|e| format!("Cannot read visual backup: {e}"))?;
    let visual = crate::upk_swap::masquerade(
        &donor,
        &target,
        &donor_key,
        &target_key,
        ALPHA_DONOR_ID,
        VISUAL_TARGET_ID,
    )?;
    let visual_hash = hex_lower(&Sha256::digest(&visual));
    let audio_cache = ensure_verified_cached_asset(&conf_dir, ALPHA_AUDIO_SPEC).await?;
    let audio_hash = file_sha256(&audio_cache)?;
    metadata_file_mut(&mut metadata, ALPHA_VISUAL_SPEC.file_name)?.applied_sha256 =
        Some(visual_hash);
    metadata_file_mut(&mut metadata, ALPHA_AUDIO_SPEC.file_name)?.applied_sha256 = Some(audio_hash);
    save_backup_metadata(&conf_dir, &metadata)?;
    let staged = targets[0].1.with_extension("upk.swap-staged");
    fs::write(&staged, visual).map_err(|e| format!("Could not stage generated UPK: {e}"))?;
    fs::copy(&staged, &targets[0].1)
        .map_err(|e| format!("Could not install generated UPK: {e}"))?;
    let _ = fs::remove_file(&staged);
    if file_sha256(&targets[0].1)?
        != metadata_file(&metadata, ALPHA_VISUAL_SPEC.file_name)?
            .applied_sha256
            .as_deref()
            .unwrap_or("")
    {
        return Err("Installed visual UPK failed verification; restore backup".into());
    }
    fs::copy(&audio_cache, &targets[1].1).map_err(|e| format!("Audio swap failed: {e}"))?;

    Ok(())
}

#[allow(dead_code)]
fn refresh_updated_backups(
    conf_dir: &Path,
    targets: &[(&str, PathBuf)],
    donor: &[u8],
    donor_key: &[u8; 32],
    target_key: &[u8; 32],
    metadata: &mut BoostBackupMetadata,
) -> Result<(), String> {
    let visual_meta = metadata_file(metadata, ALPHA_VISUAL_SPEC.file_name)?;
    let current_hash = file_sha256(&targets[0].1)?;
    let visual_known = current_hash.eq_ignore_ascii_case(&visual_meta.original_sha256)
        || visual_meta
            .applied_sha256
            .as_deref()
            .is_some_and(|h| current_hash.eq_ignore_ascii_case(h));
    let backup_dir = boost_backup_dir(conf_dir)?;
    if !visual_known {
        let current = fs::read(&targets[0].1)
            .map_err(|e| format!("Could not read updated target UPK: {e}"))?;
        let previous = fs::read(&visual_meta.backup_path)
            .map_err(|e| format!("Could not read previous backup: {e}"))?;
        if file_sha256(Path::new(&visual_meta.backup_path))? != visual_meta.original_sha256 {
            return Err("Previous visual backup is corrupt".into());
        }
        if crate::upk_swap::package_guid(&current)? == crate::upk_swap::package_guid(&previous)? {
            return Err(
                "Target UPK changed without a new package GUID; refusing to replace backup".into(),
            );
        }
        crate::upk_swap::masquerade(
            donor,
            &current,
            donor_key,
            target_key,
            ALPHA_DONOR_ID,
            VISUAL_TARGET_ID,
        )?;
        let visual_backup = backup_dir.join(format!("{}-{current_hash}.upk", VISUAL_TARGET_ID));
        fs::copy(&targets[0].1, &visual_backup)
            .map_err(|e| format!("Could not back up updated UPK: {e}"))?;
        if file_sha256(&visual_backup)? != current_hash {
            return Err("Updated UPK backup verification failed".into());
        }
        let visual_meta = metadata_file_mut(metadata, ALPHA_VISUAL_SPEC.file_name)?;
        visual_meta.backup_path = visual_backup.display().to_string();
        visual_meta.original_sha256 = current_hash;
        visual_meta.original_size = fs::metadata(&visual_backup)
            .map_err(|e| e.to_string())?
            .len();
        visual_meta.applied_sha256 = None;
    }

    let audio_meta = metadata_file(metadata, ALPHA_AUDIO_SPEC.file_name)?;
    let audio_hash = file_sha256(&targets[1].1)?;
    if !audio_hash.eq_ignore_ascii_case(&audio_meta.original_sha256) {
        let audio_is_applied = audio_hash.eq_ignore_ascii_case(ALPHA_AUDIO_SHA256)
            || audio_meta
                .applied_sha256
                .as_deref()
                .is_some_and(|h| audio_hash.eq_ignore_ascii_case(h));
        if !audio_is_applied {
            let audio_backup = backup_dir.join(format!("SFX_Boost_Standard-{audio_hash}.bnk"));
            fs::copy(&targets[1].1, &audio_backup)
                .map_err(|e| format!("Could not back up updated audio: {e}"))?;
            if file_sha256(&audio_backup)? != audio_hash {
                return Err("Updated audio backup verification failed".into());
            }
            let audio_meta = metadata_file_mut(metadata, ALPHA_AUDIO_SPEC.file_name)?;
            audio_meta.backup_path = audio_backup.display().to_string();
            audio_meta.original_sha256 = audio_hash;
            audio_meta.original_size = fs::metadata(&audio_backup)
                .map_err(|e| e.to_string())?
                .len();
            audio_meta.applied_sha256 = None;
        }
    }
    save_backup_metadata(conf_dir, metadata)
}

pub fn start_restore_standard_boost(
    state: std::sync::Arc<crate::state::AppState>,
    rocket_league_path: String,
) {
    #[cfg(not(feature = "microsoft-store"))]
    if crate::item_swapper::has_alpha_preset() {
        {
            let mut status = state
                .boost
                .boost_swap_status
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *status = "Restoring the Alpha Boost preset…".into();
        }
        crate::item_swapper::start_restore(state, rocket_league_path, "Boost_Standard".into());
        return;
    }
    let state_clone = state.clone();
    tokio::spawn(async move {
        set_boost_status(&state_clone, "Restoring original boost files...");
        match restore_standard_boost(&rocket_league_path) {
            Ok(()) => {
                state_clone.update_config(|config| config.alpha_boost_enabled = false);
                set_boost_status(&state_clone, "Success: original boost files restored!");
                request_boost_swap_inspection(&state_clone, rocket_league_path.clone(), true);
            }
            Err(error) => set_boost_status(&state_clone, &error),
        }
    });
}

/// Restores the original Standard visual and audio files from backup.
///
/// This function:
/// 1. Reads the backup metadata file saved during the initial swap.
/// 2. Verifies the backup files exist on disk and match the recorded original SHA-256 hashes.
/// 3. Overwrites the game files in `TAGame/CookedPCConsole` with the clean backups, reversing the swap.
fn restore_standard_boost(rocket_league_path: &str) -> Result<(), String> {
    let game_dir = cooked_pc_console_path(rocket_league_path)?;
    let conf_dir =
        config_dir().ok_or_else(|| "Error: Could not resolve config directory.".to_string())?;
    let metadata = load_backup_metadata(&conf_dir)?
        .ok_or_else(|| "Error: Backup metadata not found. Cannot restore safely.".to_string())?;
    let targets = boost_targets(&game_dir);

    for (file_name, target) in targets {
        let file_metadata = metadata_file(&metadata, file_name)?;
        let backup_path = PathBuf::from(&file_metadata.backup_path);
        if !backup_path.exists() {
            return Err(format!("Restore failed: backup missing for {file_name}."));
        }
        let backup_hash = file_sha256(&backup_path)?;
        if !backup_hash.eq_ignore_ascii_case(&file_metadata.original_sha256) {
            return Err(format!(
                "Restore failed: backup hash does not match metadata for {file_name}."
            ));
        }
        fs::copy(&backup_path, &target)
            .map_err(|e| format!("Restore failed (check permissions): {e}"))?;
    }

    Ok(())
}

fn set_boost_status(state: &std::sync::Arc<crate::state::AppState>, message: &str) {
    let mut status = state
        .boost
        .boost_swap_status
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    *status = message.to_string();
}

fn boost_targets(game_dir: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![
        (
            ALPHA_VISUAL_SPEC.file_name,
            game_dir.join(ALPHA_VISUAL_SPEC.file_name),
        ),
        (
            ALPHA_AUDIO_SPEC.file_name,
            game_dir.join(ALPHA_AUDIO_SPEC.file_name),
        ),
    ]
}

#[allow(dead_code)]
fn verify_targets_exist(targets: &[(&str, PathBuf)]) -> Result<(), String> {
    for (file_name, target) in targets {
        if !target.exists() || !target.is_file() {
            return Err(format!("Error: {file_name} not found in game directory."));
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn ensure_backup_metadata(
    conf_dir: &Path,
    targets: &[(&str, PathBuf)],
) -> Result<BoostBackupMetadata, String> {
    let previous = load_backup_metadata(conf_dir)?;
    if let Some(metadata) = &previous
        && metadata.version == METADATA_VERSION
    {
        return Ok(metadata.clone());
    }
    if let Some(metadata) = &previous {
        restore_retired_visual_target(metadata, targets)?;
    }

    let backup_dir = boost_backup_dir(conf_dir)?;
    let mut files = Vec::new();
    for (file_name, target) in targets {
        if let Some(old_file) = previous.as_ref().and_then(|metadata| {
            metadata
                .files
                .iter()
                .find(|file| file.file_name == *file_name)
        }) {
            let old_backup = PathBuf::from(&old_file.backup_path);
            if old_backup.is_file()
                && file_sha256(&old_backup)?.eq_ignore_ascii_case(&old_file.original_sha256)
            {
                let mut migrated = old_file.clone();
                migrated.applied_sha256 = None;
                files.push(migrated);
                continue;
            }
            return Err(format!(
                "Existing backup metadata for {file_name} is invalid; restore the game file before applying"
            ));
        }
        let mut backup_path = backup_dir.join(file_name);
        if backup_path.exists() {
            let target_hash = file_sha256(target)?;
            if file_sha256(&backup_path)? != target_hash && previous.is_some() {
                let extension = target
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("bak");
                let stem = target
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("backup");
                backup_path = backup_dir.join(format!("{stem}-{target_hash}.{extension}"));
                fs::copy(target, &backup_path).map_err(|e| format!("Backup failed: {e}"))?;
            } else if file_sha256(&backup_path)? != target_hash {
                return Err(format!(
                    "Existing backup for {file_name} differs from installed file; refusing to use it without metadata"
                ));
            }
        } else {
            fs::copy(target, &backup_path).map_err(|e| format!("Backup failed: {e}"))?;
        }
        let metadata = fs::metadata(&backup_path)
            .map_err(|e| format!("Backup metadata failed for {file_name}: {e}"))?;
        files.push(BoostBackupFileMetadata {
            file_name: (*file_name).to_string(),
            original_size: metadata.len(),
            original_sha256: file_sha256(&backup_path)?,
            backup_path: backup_path.display().to_string(),
            applied_sha256: None,
        });
    }

    let metadata = BoostBackupMetadata {
        version: METADATA_VERSION,
        created_unix_ms: crate::stats_api::now_ms(),
        release_tag: BOOST_RELEASE_TAG.to_string(),
        files,
    };
    save_backup_metadata(conf_dir, &metadata)?;
    Ok(metadata)
}

fn restore_retired_visual_target(
    metadata: &BoostBackupMetadata,
    targets: &[(&str, PathBuf)],
) -> Result<(), String> {
    let Some(old_visual) = metadata_file(metadata, "Boost_Bubble_SF.upk").ok() else {
        return Ok(());
    };
    let game_dir = targets
        .first()
        .and_then(|(_, target)| target.parent())
        .ok_or("Could not resolve game directory while migrating boost backups")?;
    let retired_target = game_dir.join("Boost_Bubble_SF.upk");
    let old_backup = PathBuf::from(&old_visual.backup_path);
    if !old_backup.is_file()
        || !file_sha256(&old_backup)?.eq_ignore_ascii_case(&old_visual.original_sha256)
    {
        return Err("Cannot migrate from Bubble Boost: its pristine backup is invalid".into());
    }
    let current_hash = file_sha256(&retired_target)?;
    if current_hash.eq_ignore_ascii_case(&old_visual.original_sha256) {
        return Ok(());
    }
    if !old_visual
        .applied_sha256
        .as_deref()
        .is_some_and(|hash| current_hash.eq_ignore_ascii_case(hash))
    {
        return Err(
            "Cannot migrate from Bubble Boost: the installed package is neither the recorded original nor generated swap"
                .into(),
        );
    }
    fs::copy(&old_backup, &retired_target)
        .map_err(|e| format!("Could not restore Bubble Boost during migration: {e}"))?;
    if file_sha256(&retired_target)? != old_visual.original_sha256 {
        return Err("Bubble Boost restoration failed verification".into());
    }
    Ok(())
}

pub(crate) fn restore_legacy_bubble_swap(rocket_league_path: &str) -> Result<(), String> {
    let conf_dir = config_dir().ok_or("Could not resolve config directory")?;
    let Some(metadata) = load_backup_metadata(&conf_dir)? else {
        return Ok(());
    };
    let game_dir = cooked_pc_console_path(rocket_league_path)?;
    restore_retired_visual_target(&metadata, &boost_targets(&game_dir))
}

fn load_backup_metadata(conf_dir: &Path) -> Result<Option<BoostBackupMetadata>, String> {
    let path = backup_metadata_path(conf_dir);
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Could not read backup metadata: {e}"))?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|e| format!("Could not parse backup metadata at {}: {e}", path.display()))
}

#[allow(dead_code)]
fn save_backup_metadata(conf_dir: &Path, metadata: &BoostBackupMetadata) -> Result<(), String> {
    let path = backup_metadata_path(conf_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create backup metadata folder: {e}"))?;
    }
    let content = serde_json::to_string_pretty(metadata)
        .map_err(|e| format!("Could not serialize backup metadata: {e}"))?;
    fs::write(&path, content).map_err(|e| format!("Could not write backup metadata: {e}"))
}

#[allow(dead_code)]
fn verify_targets_known(
    targets: &[(&str, PathBuf)],
    metadata: &BoostBackupMetadata,
    specs: &[BoostAssetSpec],
) -> Result<(), String> {
    for (file_name, target) in targets {
        let actual_hash = file_sha256(target)?;
        let original_hash = &metadata_file(metadata, file_name)?.original_sha256;
        let alpha_hash = metadata_file(metadata, file_name)?
            .applied_sha256
            .as_deref()
            .unwrap_or_else(|| {
                specs
                    .iter()
                    .find(|spec| spec.file_name == *file_name)
                    .map(|spec| spec.expected_sha256)
                    .unwrap_or("")
            });
        let is_original = actual_hash.eq_ignore_ascii_case(original_hash);
        let is_alpha =
            expected_hash_configured(alpha_hash) && actual_hash.eq_ignore_ascii_case(alpha_hash);
        if !is_original && !is_alpha {
            return Err(format!(
                "Blocked: current {file_name} does not match original backup metadata or verified Alpha Boost asset. Restore/recover originals before applying."
            ));
        }
    }
    Ok(())
}

fn alpha_hash_for_file(file_name: &str) -> Option<&'static str> {
    [ALPHA_VISUAL_SPEC, ALPHA_AUDIO_SPEC]
        .iter()
        .find(|spec| spec.file_name == file_name)
        .map(|spec| spec.expected_sha256)
}

fn metadata_file<'a>(
    metadata: &'a BoostBackupMetadata,
    file_name: &str,
) -> Result<&'a BoostBackupFileMetadata, String> {
    metadata
        .files
        .iter()
        .find(|file| file.file_name == file_name)
        .ok_or_else(|| format!("Backup metadata missing entry for {file_name}."))
}

#[allow(dead_code)]
fn metadata_file_mut<'a>(
    metadata: &'a mut BoostBackupMetadata,
    file_name: &str,
) -> Result<&'a mut BoostBackupFileMetadata, String> {
    metadata
        .files
        .iter_mut()
        .find(|file| file.file_name == file_name)
        .ok_or_else(|| format!("Backup metadata missing entry for {file_name}."))
}

#[allow(dead_code)]
async fn fetch_upk_keys() -> Result<([u8; 32], [u8; 32]), String> {
    let response = wreq::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("UPK key client failed: {e}"))?
        .get(KEY_INDEX_URL)
        .send()
        .await
        .map_err(|e| format!("Could not fetch current UPK keys: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("UPK key index returned {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Could not read UPK keys: {e}"))?;
    let csv = std::str::from_utf8(&bytes).map_err(|e| format!("Invalid UPK key index: {e}"))?;
    let mut donor = None;
    let mut target = None;
    for line in csv.lines().skip(1) {
        let fields: Vec<_> = line.split(',').collect();
        if fields.len() < 9 {
            continue;
        }
        let package = fields[7].trim_matches('"');
        let key = fields[8].trim_matches('"');
        if package == ALPHA_DONOR_ID {
            donor = Some(crate::upk_swap::key_from_base64(key)?);
        }
        if package == VISUAL_TARGET_ID {
            target = Some(crate::upk_swap::key_from_base64(key)?);
        }
    }
    Ok((
        donor.ok_or("Alpha Boost UPK key missing from index")?,
        target.ok_or("Standard Boost UPK key missing from index")?,
    ))
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|e| format!("Could not open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rl_overlay_assets_{name}_{}",
            crate::stats_api::now_ms()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn fake_spec(file_name: &'static str, hash: &'static str) -> BoostAssetSpec {
        BoostAssetSpec {
            file_name,
            url: "https://example.invalid/file",
            expected_sha256: hash,
        }
    }

    #[test]
    fn rocket_league_process_name_matches_known_executables() {
        assert!(is_rocket_league_name(OsStr::new("RocketLeague.exe")));
        assert!(is_rocket_league_name(OsStr::new("RocketLeague.ex")));
        assert!(is_rocket_league_name(OsStr::new("rocketleague")));
        assert!(is_rocket_league_name(OsStr::new(
            "RocketLeague-Linux-Shipping"
        )));
        assert!(!is_rocket_league_name(OsStr::new("rocketleague-helper")));
    }

    #[test]
    fn rocket_league_process_command_matches_proton_paths() {
        assert!(
            rocket_league_argument_match(OsStr::new(
                "/home/user/.steam/steamapps/common/rocketleague/Binaries/Win64/RocketLeague.exe"
            ))
            .is_some()
        );
        assert!(
            rocket_league_argument_match(OsStr::new(
                "Z:\\home\\user\\.steam\\steamapps\\common\\rocketleague\\Binaries\\Win64\\RocketLeague.exe"
            ))
            .is_some()
        );
        assert!(
            rocket_league_argument_match(OsStr::new(
                "/home/user/.steam/steamapps/common/rocketleague/Binaries/Win64/RocketLeague_EAC.exe"
            ))
            .is_some()
        );
        assert!(
            rocket_league_argument_match(OsStr::new(
                "S:\\common\\rocketleague\\Binaries\\Win64\\RocketLeague.exe"
            ))
            .is_some()
        );
        assert!(rocket_league_argument_match(OsStr::new("steamwebhelper")).is_none());
    }

    #[test]
    fn test_cooked_pc_console_path_validation() {
        let temp_dir = temp_dir("path_validation");

        assert!(cooked_pc_console_path("").is_err());
        assert!(cooked_pc_console_path(&temp_dir.to_string_lossy()).is_err());

        let ta_game = temp_dir.join("TAGame");
        let cooked = ta_game.join("CookedPCConsole");
        fs::create_dir_all(&cooked).unwrap();

        let resolved = cooked_pc_console_path(&temp_dir.to_string_lossy()).unwrap();
        assert_eq!(resolved, cooked);

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn hash_matches_known_sha256() {
        let root = temp_dir("hash");
        let path = root.join("sample.bin");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            file_sha256(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn backup_metadata_round_trips() {
        let root = temp_dir("metadata");
        let game_dir = root.join("game");
        let conf_dir = root.join("config");
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(game_dir.join("Boost_Standard_SF.upk"), b"visual-original").unwrap();
        fs::write(game_dir.join("SFX_Boost_Standard.bnk"), b"audio-original").unwrap();

        let targets = boost_targets(&game_dir);
        let metadata = ensure_backup_metadata(&conf_dir, &targets).unwrap();
        assert_eq!(metadata.files.len(), 2);
        assert!(backup_metadata_path(&conf_dir).exists());

        let loaded = load_backup_metadata(&conf_dir).unwrap().unwrap();
        assert_eq!(loaded.files.len(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_metadata_keeps_verified_audio_backup_and_adds_standard_backup() {
        let root = temp_dir("metadata_migration");
        let game_dir = root.join("game");
        let conf_dir = root.join("config");
        let backup_dir = boost_backup_dir(&conf_dir).unwrap();
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(game_dir.join("Boost_Standard_SF.upk"), b"standard-current").unwrap();
        fs::write(game_dir.join("Boost_Bubble_SF.upk"), b"bubble-applied").unwrap();
        fs::write(game_dir.join("SFX_Boost_Standard.bnk"), b"audio-swapped").unwrap();
        fs::write(backup_dir.join("Boost_Standard_SF.upk"), b"standard-stale").unwrap();
        let bubble_backup = backup_dir.join("Boost_Bubble_SF.upk");
        fs::write(&bubble_backup, b"bubble-original").unwrap();
        let audio_backup = backup_dir.join("SFX_Boost_Standard.bnk");
        fs::write(&audio_backup, b"audio-original").unwrap();
        let legacy = BoostBackupMetadata {
            version: 2,
            created_unix_ms: 1,
            release_tag: "legacy".into(),
            files: vec![
                BoostBackupFileMetadata {
                    file_name: "Boost_Bubble_SF.upk".into(),
                    original_size: 15,
                    original_sha256: file_sha256(&bubble_backup).unwrap(),
                    backup_path: bubble_backup.display().to_string(),
                    applied_sha256: Some(
                        file_sha256(&game_dir.join("Boost_Bubble_SF.upk")).unwrap(),
                    ),
                },
                BoostBackupFileMetadata {
                    file_name: "SFX_Boost_Standard.bnk".into(),
                    original_size: 14,
                    original_sha256: file_sha256(&audio_backup).unwrap(),
                    backup_path: audio_backup.display().to_string(),
                    applied_sha256: None,
                },
            ],
        };
        save_backup_metadata(&conf_dir, &legacy).unwrap();

        let migrated = ensure_backup_metadata(&conf_dir, &boost_targets(&game_dir)).unwrap();
        assert_eq!(migrated.version, METADATA_VERSION);
        assert_eq!(
            metadata_file(&migrated, ALPHA_AUDIO_SPEC.file_name)
                .unwrap()
                .original_sha256,
            file_sha256(&audio_backup).unwrap()
        );
        assert_eq!(
            metadata_file(&migrated, ALPHA_VISUAL_SPEC.file_name)
                .unwrap()
                .original_sha256,
            file_sha256(&game_dir.join("Boost_Standard_SF.upk")).unwrap()
        );
        assert_ne!(
            metadata_file(&migrated, ALPHA_VISUAL_SPEC.file_name)
                .unwrap()
                .backup_path,
            backup_dir
                .join("Boost_Standard_SF.upk")
                .display()
                .to_string()
        );
        assert_eq!(
            fs::read(game_dir.join("Boost_Bubble_SF.upk")).unwrap(),
            b"bubble-original"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verify_targets_known_blocks_unknown_target() {
        let root = temp_dir("unknown");
        let game_dir = root.join("game");
        let conf_dir = root.join("config");
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(game_dir.join("Boost_Standard_SF.upk"), b"visual-original").unwrap();
        fs::write(game_dir.join("SFX_Boost_Standard.bnk"), b"audio-original").unwrap();
        let targets = boost_targets(&game_dir);
        let metadata = ensure_backup_metadata(&conf_dir, &targets).unwrap();

        fs::write(game_dir.join("Boost_Standard_SF.upk"), b"unknown-change").unwrap();
        let specs = [
            fake_spec(
                "Boost_Standard_SF.upk",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
            fake_spec(
                "SFX_Boost_Standard.bnk",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
        ];
        let error = verify_targets_known(&targets, &metadata, &specs).unwrap_err();
        assert!(error.starts_with("Blocked:"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_game_file_state_detects_original_from_metadata() {
        let root = temp_dir("state_original");
        let game_dir = root.join("game");
        let conf_dir = root.join("config");
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(game_dir.join("Boost_Standard_SF.upk"), b"visual-original").unwrap();
        fs::write(game_dir.join("SFX_Boost_Standard.bnk"), b"audio-original").unwrap();
        let targets = boost_targets(&game_dir);
        ensure_backup_metadata(&conf_dir, &targets).unwrap();

        assert_eq!(
            inspect_game_file_state_for_targets(&targets, &conf_dir).unwrap(),
            BoostGameFileState::Original
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_game_file_state_for_targets_blocks_unknown() {
        let root = temp_dir("state_unknown");
        let game_dir = root.join("game");
        let conf_dir = root.join("config");
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(game_dir.join("Boost_Standard_SF.upk"), b"visual-original").unwrap();
        fs::write(game_dir.join("SFX_Boost_Standard.bnk"), b"audio-original").unwrap();
        let targets = boost_targets(&game_dir);
        ensure_backup_metadata(&conf_dir, &targets).unwrap();

        fs::write(game_dir.join("Boost_Standard_SF.upk"), b"unknown").unwrap();
        assert_eq!(
            inspect_game_file_state_for_targets(&targets, &conf_dir).unwrap(),
            BoostGameFileState::Unknown
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inspect_game_file_state_detects_unbacked_first_run() {
        let root = temp_dir("state_unbacked");
        let game_dir = root.join("game");
        let conf_dir = root.join("config");
        fs::create_dir_all(&game_dir).unwrap();
        fs::write(game_dir.join("Boost_Standard_SF.upk"), b"visual-current").unwrap();
        fs::write(game_dir.join("SFX_Boost_Standard.bnk"), b"audio-current").unwrap();
        let targets = boost_targets(&game_dir);

        assert_eq!(
            inspect_game_file_state_for_targets(&targets, &conf_dir).unwrap(),
            BoostGameFileState::Unbacked
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn boost_inspection_request_dedupes_and_stores_snapshot() {
        let state = crate::state::AppState::new();
        state
            .boost
            .inspection_running
            .store(true, std::sync::atomic::Ordering::SeqCst);
        request_boost_swap_inspection(&state, "ignored".to_string(), true);
        assert!(!state.boost.boost_swap_inspection.load().loaded);

        state
            .boost
            .inspection_running
            .store(false, std::sync::atomic::Ordering::SeqCst);
        request_boost_swap_inspection(&state, String::new(), true);

        let snapshot = state.boost.boost_swap_inspection.load();
        assert!(snapshot.loaded);
        assert!(!snapshot.refreshing);
        assert_eq!(
            snapshot.inspection.game_file_state,
            BoostGameFileState::Unavailable
        );
    }

    #[test]
    fn restore_refuses_corrupt_backup() {
        let root = temp_dir("restore_corrupt");
        let conf_dir = root.join("config");
        let backup_dir = boost_backup_dir(&conf_dir).unwrap();
        let backup_path = backup_dir.join("Boost_Standard_SF.upk");
        fs::write(&backup_path, b"original").unwrap();
        let metadata = BoostBackupMetadata {
            version: METADATA_VERSION,
            created_unix_ms: crate::stats_api::now_ms(),
            release_tag: BOOST_RELEASE_TAG.to_string(),
            files: vec![BoostBackupFileMetadata {
                file_name: "Boost_Standard_SF.upk".to_string(),
                original_size: 8,
                original_sha256: file_sha256(&backup_path).unwrap(),
                backup_path: backup_path.display().to_string(),
                applied_sha256: None,
            }],
        };
        save_backup_metadata(&conf_dir, &metadata).unwrap();
        fs::write(&backup_path, b"corrupt").unwrap();
        let loaded = load_backup_metadata(&conf_dir).unwrap().unwrap();
        let file_metadata = metadata_file(&loaded, "Boost_Standard_SF.upk").unwrap();
        let backup_hash = file_sha256(&PathBuf::from(&file_metadata.backup_path)).unwrap();
        assert!(!backup_hash.eq_ignore_ascii_case(&file_metadata.original_sha256));
        let _ = fs::remove_dir_all(root);
    }
}
