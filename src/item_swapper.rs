use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/ShinyEmii/Toga-Files/refs/heads/master/products.csv";
const MANIFEST_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ItemSlot {
    Body,
    Wheels,
    RocketBoost,
    Trail,
    GoalExplosion,
    Topper,
    Antenna,
    PaintFinish,
}

impl ItemSlot {
    pub const ALL: [Self; 8] = [
        Self::Body,
        Self::Wheels,
        Self::RocketBoost,
        Self::Trail,
        Self::GoalExplosion,
        Self::Topper,
        Self::Antenna,
        Self::PaintFinish,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::Wheels => "Wheels",
            Self::RocketBoost => "Rocket Boost",
            Self::Trail => "Trail",
            Self::GoalExplosion => "Goal Explosion",
            Self::Topper => "Topper",
            Self::Antenna => "Antenna",
            Self::PaintFinish => "Paint Finish",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.label() == value)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CatalogPackage {
    pub package: String,
    pub labels: Vec<String>,
    pub slot: ItemSlot,
    pub key: String,
}

impl CatalogPackage {
    pub fn display_name(&self) -> String {
        if self.labels.is_empty() {
            return self.package.clone();
        }
        if self.labels.len() == 1 {
            self.labels[0].clone()
        } else {
            format!("{} ({} variants)", self.labels[0], self.labels.len())
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SwapHealth {
    Active,
    NeedsReapply,
    Conflict,
}

impl SwapHealth {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::NeedsReapply => "Needs reapply",
            Self::Conflict => "Conflict",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActiveSwap {
    pub donor_package: String,
    pub target_package: String,
    pub target_backup: String,
    pub original_sha256: String,
    pub applied_sha256: String,
    pub health: SwapHealth,
    #[serde(default)]
    pub audio_backup: Option<String>,
    #[serde(default)]
    pub audio_original_sha256: Option<String>,
    #[serde(default)]
    pub audio_applied_sha256: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ItemSwapperSnapshot {
    pub install_path: String,
    pub packages: Vec<CatalogPackage>,
    pub active: Vec<ActiveSwap>,
    pub catalog_loaded: bool,
    pub refreshing: bool,
    pub running: bool,
    pub message: String,
}

pub struct ItemSwapperState {
    pub snapshot: ArcSwap<ItemSwapperSnapshot>,
    catalog_started: AtomicBool,
    operation_running: AtomicBool,
}

impl Default for ItemSwapperState {
    fn default() -> Self {
        Self {
            snapshot: ArcSwap::from_pointee(ItemSwapperSnapshot::default()),
            catalog_started: AtomicBool::new(false),
            operation_running: AtomicBool::new(false),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Manifest {
    version: u32,
    active: Vec<ActiveSwap>,
}

pub fn request_catalog(state: &Arc<crate::state::AppState>, install: String, force: bool) {
    let changed_install = {
        let snapshot = state.item_swapper.snapshot.load();
        !snapshot.install_path.is_empty() && snapshot.install_path != install
    };
    if force || changed_install {
        state
            .item_swapper
            .catalog_started
            .store(false, Ordering::SeqCst);
    }
    if state
        .item_swapper
        .catalog_started
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        publish(&state, |s| {
            s.refreshing = true;
            s.install_path = install.clone();
            s.message = "Loading item catalog…".into();
        });
        let conf = match crate::state::config_dir() {
            Some(v) => v,
            None => {
                finish_refresh(&state, "Could not resolve the config directory.");
                return;
            }
        };
        let cache = catalog_cache_path(&conf);
        if let Ok(csv) = fs::read_to_string(&cache) {
            match build_catalog(&csv, &install) {
                Ok(packages) => {
                    publish_catalog(&state, packages, &install, "Loaded cached catalog.")
                }
                Err(e) => publish(&state, |s| {
                    s.message = format!("Cached catalog is invalid: {e}")
                }),
            }
        }
        let fetched = async {
            let response = state
                .system
                .http_client
                .get(CATALOG_URL)
                .send()
                .await
                .map_err(|e| format!("Catalog refresh failed: {e}"))?;
            if !response.status().is_success() {
                return Err(format!("Catalog refresh returned {}", response.status()));
            }
            let bytes = response
                .bytes()
                .await
                .map_err(|e| format!("Could not read catalog: {e}"))?;
            let csv =
                std::str::from_utf8(&bytes).map_err(|e| format!("Catalog is not UTF-8: {e}"))?;
            let packages = build_catalog(csv, &install)?;
            if let Some(parent) = cache.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Could not create cache: {e}"))?;
            }
            atomic_write(&cache, &bytes)?;
            Ok(packages)
        }
        .await;
        match fetched {
            Ok(packages) => publish_catalog(&state, packages, &install, "Catalog refreshed."),
            Err(error) => {
                let has_cache = state.item_swapper.snapshot.load().catalog_loaded;
                let message = if has_cache {
                    format!("Using cached catalog. {error}")
                } else {
                    error
                };
                finish_refresh(&state, &message);
            }
        }
    });
}

pub fn start_apply(
    state: Arc<crate::state::AppState>,
    install: String,
    donor: String,
    target: String,
) {
    if state
        .item_swapper
        .operation_running
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    let alpha_pair = donor == "Boost_AlphaReward" && target == "Boost_Standard";
    publish(&state, |s| {
        s.running = true;
        s.message = if alpha_pair {
            "Preparing Alpha Boost visual and audio…".into()
        } else {
            "Generating swap…".into()
        };
    });
    let packages = state.item_swapper.snapshot.load().packages.clone();
    tokio::spawn(async move {
        let audio = if alpha_pair {
            match crate::assets::prepare_alpha_audio_asset().await {
                Ok(path) => Some(path),
                Err(error) => {
                    finish_operation(&state, Err(error), &install);
                    return;
                }
            }
        } else {
            None
        };
        let operation_install = install.clone();
        let result = tokio::task::spawn_blocking(move || {
            if crate::assets::is_rocket_league_running() {
                return Err("Close Rocket League before changing game files.".into());
            }
            if alpha_pair {
                crate::assets::restore_legacy_bubble_swap(&operation_install)?;
            }
            let message = apply(&operation_install, &packages, &donor, &target)?;
            if let Some(audio) = audio
                && let Err(error) = apply_alpha_audio(&operation_install, &audio)
            {
                let _ = restore(&operation_install, "Boost_Standard");
                return Err(format!(
                    "Alpha audio failed; visual swap was rolled back: {error}"
                ));
            }
            Ok(message)
        })
        .await
        .unwrap_or_else(|e| Err(format!("Swap worker failed: {e}")));
        finish_operation(&state, result, &install);
    });
}

pub fn start_alpha_preset(state: Arc<crate::state::AppState>, install: String) {
    let snapshot = state.item_swapper.snapshot.load();
    let needs_catalog = !snapshot.catalog_loaded && !snapshot.refreshing;
    drop(snapshot);
    request_catalog(&state, install.clone(), needs_catalog);
    {
        let mut status = state
            .boost
            .boost_swap_status
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *status = "Loading the current item catalog for Alpha Boost…".into();
    }
    tokio::spawn(async move {
        for _ in 0..300 {
            let snapshot = state.item_swapper.snapshot.load();
            if snapshot.catalog_loaded && snapshot.install_path == install {
                drop(snapshot);
                start_apply(
                    state,
                    install,
                    "Boost_AlphaReward".into(),
                    "Boost_Standard".into(),
                );
                return;
            }
            if !snapshot.refreshing && !snapshot.message.is_empty() {
                let message = format!("Error: {}", snapshot.message);
                drop(snapshot);
                let mut status = state
                    .boost
                    .boost_swap_status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                *status = message;
                return;
            }
            drop(snapshot);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let mut status = state
            .boost
            .boost_swap_status
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *status = "Error: Timed out loading the item catalog.".into();
    });
}

pub fn has_alpha_preset() -> bool {
    let Some(conf) = crate::state::config_dir() else {
        return false;
    };
    load_manifest(&conf).is_ok_and(|manifest| {
        manifest.active.iter().any(|swap| {
            swap.donor_package == "Boost_AlphaReward" && swap.target_package == "Boost_Standard"
        })
    })
}

pub fn alpha_preset_file_state(install: &str) -> Option<crate::assets::BoostGameFileState> {
    let conf = crate::state::config_dir()?;
    let manifest = load_manifest(&conf).ok()?;
    let record = manifest.active.iter().find(|swap| {
        swap.donor_package == "Boost_AlphaReward" && swap.target_package == "Boost_Standard"
    })?;
    let cooked = cooked_dir(install).ok()?;
    let visual = hash_file(&package_path(&cooked, "Boost_Standard")).ok()?;
    let audio = hash_file(&cooked.join("SFX_Boost_Standard.bnk")).ok()?;
    if visual == record.applied_sha256
        && record.audio_applied_sha256.as_deref() == Some(audio.as_str())
    {
        Some(crate::assets::BoostGameFileState::Alpha)
    } else if visual == record.original_sha256
        && record.audio_original_sha256.as_deref() == Some(audio.as_str())
    {
        Some(crate::assets::BoostGameFileState::Original)
    } else {
        Some(crate::assets::BoostGameFileState::Unknown)
    }
}

pub fn start_restore(state: Arc<crate::state::AppState>, install: String, target: String) {
    start_operation(state, "Restoring original package…", move |_| {
        restore(&install, &target)
    });
}

pub fn start_reapply(state: Arc<crate::state::AppState>, install: String, target: String) {
    let snapshot = state.item_swapper.snapshot.load();
    let Some(record) = snapshot.active.iter().find(|v| v.target_package == target) else {
        publish(&state, |s| {
            s.message = "Swap record no longer exists.".into()
        });
        return;
    };
    start_apply(state, install, record.donor_package.clone(), target);
}

fn start_operation(
    state: Arc<crate::state::AppState>,
    message: &str,
    operation: impl FnOnce(&[CatalogPackage]) -> Result<String, String> + Send + 'static,
) {
    if state
        .item_swapper
        .operation_running
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    publish(&state, |s| {
        s.running = true;
        s.message = message.into();
    });
    let packages = state.item_swapper.snapshot.load().packages.clone();
    tokio::task::spawn_blocking(move || {
        let result = if crate::assets::is_rocket_league_running() {
            Err("Close Rocket League before changing game files.".into())
        } else {
            operation(&packages)
        };
        let install = state.system.config.load().rocket_league_path.clone();
        finish_operation(&state, result, &install);
    });
}

fn finish_operation(
    state: &Arc<crate::state::AppState>,
    result: Result<String, String>,
    install: &str,
) {
    state
        .item_swapper
        .operation_running
        .store(false, Ordering::SeqCst);
    let message = match result {
        Ok(message) => message,
        Err(error) => format!("Error: {error}"),
    };
    publish(state, |s| {
        s.running = false;
        s.message = message.clone();
    });
    refresh_active(state, install);
    let alpha_managed = has_alpha_preset();
    let mut boost_status = state
        .boost
        .boost_swap_status
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let alpha_context = alpha_managed
        || boost_status.starts_with("Loading the current item catalog")
        || boost_status.starts_with("Restoring the Alpha Boost preset");
    if alpha_context {
        *boost_status = message;
    }
    drop(boost_status);
    if alpha_managed {
        state.update_config(|config| config.alpha_boost_enabled = true);
    } else if alpha_context {
        state.update_config(|config| config.alpha_boost_enabled = false);
    }
    crate::assets::request_boost_swap_inspection(state, install.to_owned(), true);
}

fn apply(
    install: &str,
    packages: &[CatalogPackage],
    donor_id: &str,
    target_id: &str,
) -> Result<String, String> {
    if donor_id == target_id {
        return Err("Choose two different packages.".into());
    }
    let donor = packages
        .iter()
        .find(|p| p.package == donor_id)
        .ok_or("Donor is not in the current catalog")?;
    let target = packages
        .iter()
        .find(|p| p.package == target_id)
        .ok_or("Target is not in the current catalog")?;
    if donor.slot != target.slot {
        return Err("Donor and target must use the same item slot.".into());
    }
    let cooked = cooked_dir(install)?;
    let conf = crate::state::config_dir().ok_or("Could not resolve config directory")?;
    let mut manifest = load_manifest(&conf)?;
    let target_path = package_path(&cooked, target_id);
    let donor_path = pristine_path(&cooked, &manifest, donor_id)?;
    let target_current = fs::read(&target_path)
        .map_err(|e| format!("Could not read {}: {e}", target_path.display()))?;
    let current_hash = hash_bytes(&target_current);

    let existing = manifest
        .active
        .iter()
        .find(|v| v.target_package == target_id)
        .cloned();
    let (target_original, backup_path, original_hash) = if let Some(record) = &existing {
        if current_hash != record.applied_sha256 && current_hash != record.original_sha256 {
            let target_key = crate::upk_swap::key_from_base64(&target.key)?;
            crate::upk_swap::validate_identity(&target_current, &target_key, target_id).map_err(
                |_| {
                    "Target is not a valid current package. Verify the game files before applying."
                        .to_string()
                },
            )?;
            let prior = fs::read(&record.target_backup)
                .map_err(|e| format!("Could not read prior target backup: {e}"))?;
            if crate::upk_swap::package_guid(&target_current)?
                == crate::upk_swap::package_guid(&prior)?
            {
                return Err("Target changed without a new package GUID. Verify the game files before applying.".into());
            }
            let backup = backup_path(&conf, target_id, &current_hash);
            if !backup.exists() {
                if let Some(parent) = backup.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Could not create backup directory: {e}"))?;
                }
                atomic_write(&backup, &target_current)?;
            }
            if hash_file(&backup)? != current_hash {
                return Err("Updated target backup failed hash verification.".into());
            }
            (target_current, backup, current_hash)
        } else {
            let bytes = fs::read(&record.target_backup)
                .map_err(|e| format!("Could not read target backup: {e}"))?;
            if hash_bytes(&bytes) != record.original_sha256 {
                return Err("Target backup failed hash verification.".into());
            }
            (
                bytes,
                PathBuf::from(&record.target_backup),
                record.original_sha256.clone(),
            )
        }
    } else {
        let backup = backup_path(&conf, target_id, &current_hash);
        if !backup.exists() {
            if let Some(parent) = backup.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Could not create backup directory: {e}"))?;
            }
            atomic_write(&backup, &target_current)?;
        }
        if hash_file(&backup)? != current_hash {
            return Err("New target backup failed hash verification.".into());
        }
        (target_current, backup, current_hash)
    };
    let donor_bytes = fs::read(&donor_path)
        .map_err(|e| format!("Could not read donor {}: {e}", donor_path.display()))?;
    let donor_key = crate::upk_swap::key_from_base64(&donor.key)?;
    let target_key = crate::upk_swap::key_from_base64(&target.key)?;
    let generated = crate::upk_swap::masquerade(
        &donor_bytes,
        &target_original,
        &donor_key,
        &target_key,
        donor_id,
        target_id,
    )?;
    crate::upk_swap::validate_identity(&generated, &target_key, target_id)?;
    let applied_hash = hash_bytes(&generated);
    install_transaction(&target_path, &generated, &target_original)?;
    if hash_file(&target_path)? != applied_hash {
        return Err("Installed package failed verification; the original was restored.".into());
    }
    manifest.active.retain(|v| v.target_package != target_id);
    manifest.active.push(ActiveSwap {
        donor_package: donor_id.into(),
        target_package: target_id.into(),
        target_backup: backup_path.display().to_string(),
        original_sha256: original_hash,
        applied_sha256: applied_hash,
        health: SwapHealth::Active,
        audio_backup: existing.as_ref().and_then(|v| v.audio_backup.clone()),
        audio_original_sha256: existing
            .as_ref()
            .and_then(|v| v.audio_original_sha256.clone()),
        audio_applied_sha256: existing
            .as_ref()
            .and_then(|v| v.audio_applied_sha256.clone()),
    });
    if let Err(error) = save_manifest(&conf, &manifest) {
        install_transaction(&target_path, &target_original, &target_original)?;
        return Err(format!(
            "Could not save swap manifest; restored original: {error}"
        ));
    }
    Ok(format!(
        "Applied: {} now displays {}'s appearance.",
        target.display_name(),
        donor.display_name()
    ))
}

fn restore(install: &str, target_id: &str) -> Result<String, String> {
    let cooked = cooked_dir(install)?;
    let conf = crate::state::config_dir().ok_or("Could not resolve config directory")?;
    let mut manifest = load_manifest(&conf)?;
    let index = manifest
        .active
        .iter()
        .position(|v| v.target_package == target_id)
        .ok_or("No active swap exists for this target")?;
    let record = manifest.active[index].clone();
    let backup =
        fs::read(&record.target_backup).map_err(|e| format!("Could not read backup: {e}"))?;
    if hash_bytes(&backup) != record.original_sha256 {
        return Err("Backup failed hash verification.".into());
    }
    let target = package_path(&cooked, target_id);
    let visual_current = fs::read(&target)
        .map_err(|e| format!("Could not read current target before restore: {e}"))?;
    let current_hash = hash_file(&target)?;
    if current_hash != record.applied_sha256 && current_hash != record.original_sha256 {
        return Err("Target changed outside the swapper. Refusing to overwrite it.".into());
    }
    let prepared_audio = if let (Some(audio_backup), Some(original_hash)) =
        (&record.audio_backup, &record.audio_original_sha256)
    {
        let audio_backup =
            fs::read(audio_backup).map_err(|e| format!("Could not read audio backup: {e}"))?;
        if hash_bytes(&audio_backup) != *original_hash {
            return Err("Audio backup failed hash verification.".into());
        }
        let audio_target = cooked.join("SFX_Boost_Standard.bnk");
        let current = hash_file(&audio_target)?;
        if current != *original_hash
            && record.audio_applied_sha256.as_deref() != Some(current.as_str())
        {
            return Err("Audio target changed outside the swapper. Nothing was restored.".into());
        }
        Some((audio_target, audio_backup))
    } else {
        None
    };
    install_transaction(&target, &backup, &backup)?;
    if let Some((audio_target, audio_backup)) = prepared_audio {
        if let Err(error) = install_transaction(&audio_target, &audio_backup, &audio_backup) {
            let _ = install_transaction(&target, &visual_current, &visual_current);
            return Err(format!(
                "Audio restore failed and the visual swap was put back: {error}"
            ));
        }
    }
    manifest.active.remove(index);
    save_manifest(&conf, &manifest)?;
    Ok(format!("Restored {target_id}."))
}

fn apply_alpha_audio(install: &str, audio_asset: &Path) -> Result<(), String> {
    let cooked = cooked_dir(install)?;
    let conf = crate::state::config_dir().ok_or("Could not resolve config directory")?;
    let target = cooked.join("SFX_Boost_Standard.bnk");
    let asset = fs::read(audio_asset).map_err(|e| format!("Could not read Alpha audio: {e}"))?;
    let applied_hash = hash_bytes(&asset);
    let current =
        fs::read(&target).map_err(|e| format!("Could not read Standard Boost audio: {e}"))?;
    let current_hash = hash_bytes(&current);
    let mut manifest = load_manifest(&conf)?;
    let record = manifest
        .active
        .iter_mut()
        .find(|v| v.target_package == "Boost_Standard")
        .ok_or("Alpha visual swap record is missing")?;
    let (original, original_hash, backup) =
        if let (Some(path), Some(hash)) = (&record.audio_backup, &record.audio_original_sha256) {
            let bytes = fs::read(path).map_err(|e| format!("Could not read audio backup: {e}"))?;
            if hash_bytes(&bytes) != *hash {
                return Err("Audio backup failed hash verification.".into());
            }
            (bytes, hash.clone(), PathBuf::from(path))
        } else {
            let (bytes, hash) = if current_hash == applied_hash {
                legacy_alpha_audio_backup(&conf).unwrap_or((current, current_hash))
            } else {
                (current, current_hash)
            };
            let path = conf
                .join("backups/ItemSwapper/audio/Boost_Standard")
                .join(format!("{hash}.bnk"));
            if !path.exists() {
                atomic_write(&path, &bytes)?;
            }
            if hash_file(&path)? != hash {
                return Err("Audio backup failed verification.".into());
            }
            (bytes, hash, path)
        };
    install_transaction(&target, &asset, &original)?;
    record.audio_backup = Some(backup.display().to_string());
    record.audio_original_sha256 = Some(original_hash);
    record.audio_applied_sha256 = Some(applied_hash);
    if let Err(error) = save_manifest(&conf, &manifest) {
        install_transaction(&target, &original, &original)?;
        return Err(format!("Could not save audio swap record: {error}"));
    }
    Ok(())
}

fn legacy_alpha_audio_backup(conf: &Path) -> Option<(Vec<u8>, String)> {
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(conf.join("backups/Boost/backup_metadata.json")).ok()?)
            .ok()?;
    let file = value.get("files")?.as_array()?.iter().find(|file| {
        file.get("file_name").and_then(|v| v.as_str()) == Some("SFX_Boost_Standard.bnk")
    })?;
    let path = file.get("backup_path")?.as_str()?;
    let expected = file.get("original_sha256")?.as_str()?.to_owned();
    let bytes = fs::read(path).ok()?;
    (hash_bytes(&bytes) == expected).then_some((bytes, expected))
}

fn install_transaction(target: &Path, bytes: &[u8], rollback: &[u8]) -> Result<(), String> {
    let staged = target.with_extension("upk.swap-staged");
    let previous = target.with_extension("upk.swap-previous");
    atomic_write(&staged, bytes)?;
    if previous.exists() {
        fs::remove_file(&previous)
            .map_err(|e| format!("Could not clear previous transaction file: {e}"))?;
    }
    fs::rename(target, &previous)
        .map_err(|e| format!("Could not stage the current target: {e}"))?;
    if let Err(error) = fs::rename(&staged, target) {
        let _ = fs::remove_file(&staged);
        let _ = fs::rename(&previous, target);
        return Err(format!("Could not replace target package: {error}"));
    }
    if hash_file(target)? != hash_bytes(bytes) {
        let _ = fs::remove_file(target);
        if fs::rename(&previous, target).is_err() {
            atomic_write(target, rollback)?;
        }
        return Err("Installed package hash mismatch".into());
    }
    fs::remove_file(&previous)
        .map_err(|e| format!("Swap succeeded but transaction cleanup failed: {e}"))?;
    Ok(())
}

fn pristine_path(cooked: &Path, manifest: &Manifest, package: &str) -> Result<PathBuf, String> {
    if let Some(active) = manifest.active.iter().find(|v| v.target_package == package) {
        let backup = PathBuf::from(&active.target_backup);
        if hash_file(&backup)? != active.original_sha256 {
            return Err(format!("Backup for donor {package} is corrupt"));
        }
        Ok(backup)
    } else {
        Ok(package_path(cooked, package))
    }
}

fn refresh_active(state: &Arc<crate::state::AppState>, install: &str) {
    let Some(conf) = crate::state::config_dir() else {
        return;
    };
    let Ok(mut manifest) = load_manifest(&conf) else {
        return;
    };
    let Ok(cooked) = cooked_dir(install) else {
        return;
    };
    let packages = state.item_swapper.snapshot.load().packages.clone();
    for record in &mut manifest.active {
        let target_path = package_path(&cooked, &record.target_package);
        record.health = match hash_file(&target_path) {
            Ok(hash) if hash == record.applied_sha256 => SwapHealth::Active,
            Ok(hash) if hash == record.original_sha256 => SwapHealth::NeedsReapply,
            Ok(_) => {
                let key = packages
                    .iter()
                    .find(|p| p.package == record.target_package)
                    .and_then(|p| crate::upk_swap::key_from_base64(&p.key).ok());
                match (fs::read(&target_path), fs::read(&record.target_backup), key) {
                    (Ok(current), Ok(prior), Some(key))
                        if crate::upk_swap::validate_identity(
                            &current,
                            &key,
                            &record.target_package,
                        )
                        .is_ok()
                            && crate::upk_swap::package_guid(&current).ok()
                                != crate::upk_swap::package_guid(&prior).ok() =>
                    {
                        SwapHealth::NeedsReapply
                    }
                    _ => SwapHealth::Conflict,
                }
            }
            Err(_) => SwapHealth::Conflict,
        };
    }
    let _ = save_manifest(&conf, &manifest);
    publish(state, |s| s.active = manifest.active);
}

fn publish_catalog(
    state: &Arc<crate::state::AppState>,
    packages: Vec<CatalogPackage>,
    install: &str,
    message: &str,
) {
    publish(state, |s| {
        s.packages = packages;
        s.install_path = install.into();
        s.catalog_loaded = true;
        s.refreshing = false;
        s.message = message.into();
    });
    refresh_active(state, install);
}
fn finish_refresh(state: &Arc<crate::state::AppState>, message: &str) {
    publish(state, |s| {
        s.refreshing = false;
        s.message = message.into();
    });
}
fn publish(state: &Arc<crate::state::AppState>, change: impl FnOnce(&mut ItemSwapperSnapshot)) {
    let mut next = (**state.item_swapper.snapshot.load()).clone();
    change(&mut next);
    state.item_swapper.snapshot.store(Arc::new(next));
}

fn build_catalog(csv: &str, install: &str) -> Result<Vec<CatalogPackage>, String> {
    let rows = parse_csv(csv)?;
    let header = rows.first().ok_or("Catalog is empty")?;
    let column = |name: &str| {
        header
            .iter()
            .position(|v| v == name)
            .ok_or_else(|| format!("Catalog column {name} is missing"))
    };
    let label = column("Label")?;
    let slot = column("Slot")?;
    let package = column("Package")?;
    let aes = column("AES")?;
    let cooked = cooked_dir(install)?;
    let mut grouped: BTreeMap<(ItemSlot, String), CatalogPackage> = BTreeMap::new();
    for row in rows.into_iter().skip(1) {
        let Some(slot_value) = row.get(slot).and_then(|v| ItemSlot::parse(v)) else {
            continue;
        };
        let (Some(package_value), Some(label_value), Some(key)) =
            (row.get(package), row.get(label), row.get(aes))
        else {
            continue;
        };
        if package_value.is_empty()
            || key.is_empty()
            || crate::upk_swap::key_from_base64(key).is_err()
            || !package_path(&cooked, package_value).exists()
        {
            continue;
        }
        let entry = grouped
            .entry((slot_value, package_value.clone()))
            .or_insert_with(|| CatalogPackage {
                package: package_value.clone(),
                labels: Vec::new(),
                slot: slot_value,
                key: key.clone(),
            });
        let label_value = if label_value.is_empty() {
            package_value
        } else {
            label_value
        };
        if !entry.labels.contains(label_value) {
            entry.labels.push(label_value.clone());
        }
    }
    for package in grouped.values_mut() {
        package.labels.sort();
    }
    Ok(grouped.into_values().collect())
}

fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut field)),
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            '\r' if !quoted => {}
            _ => field.push(ch),
        }
    }
    if quoted {
        return Err("Unterminated quoted CSV field".into());
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

fn cooked_dir(install: &str) -> Result<PathBuf, String> {
    let path = Path::new(install).join("TAGame/CookedPCConsole");
    if !path.is_dir() {
        return Err("Rocket League TAGame/CookedPCConsole folder was not found".into());
    }
    Ok(path)
}
fn package_path(cooked: &Path, package: &str) -> PathBuf {
    cooked.join(format!("{package}_SF.upk"))
}
fn catalog_cache_path(conf: &Path) -> PathBuf {
    conf.join("cache/ItemSwapper/products.csv")
}
fn manifest_path(conf: &Path) -> PathBuf {
    conf.join("backups/ItemSwapper/manifest.json")
}
fn backup_path(conf: &Path, package: &str, hash: &str) -> PathBuf {
    conf.join("backups/ItemSwapper/packages")
        .join(package)
        .join(format!("{hash}.upk"))
}
fn load_manifest(conf: &Path) -> Result<Manifest, String> {
    let path = manifest_path(conf);
    if !path.exists() {
        return Ok(Manifest {
            version: MANIFEST_VERSION,
            active: Vec::new(),
        });
    }
    let value: Manifest = serde_json::from_slice(
        &fs::read(&path).map_err(|e| format!("Could not read swap manifest: {e}"))?,
    )
    .map_err(|e| format!("Invalid swap manifest: {e}"))?;
    if value.version != MANIFEST_VERSION {
        return Err(format!(
            "Unsupported swap manifest version {}",
            value.version
        ));
    }
    Ok(value)
}
fn save_manifest(conf: &Path, manifest: &Manifest) -> Result<(), String> {
    atomic_write(
        &manifest_path(conf),
        &serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?,
    )
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
    }
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes).map_err(|e| format!("Could not write {}: {e}", temp.display()))?;
    fs::rename(&temp, path).map_err(|e| format!("Could not replace {}: {e}", path.display()))
}
fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_file(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|e| format!("Could not open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_parser_handles_commas_and_quotes() {
        let rows = parse_csv("A,B\n\"x, y\",\"a\"\"b\"\n").unwrap();
        assert_eq!(rows[1], ["x, y", "a\"b"]);
    }

    #[test]
    fn file_install_transaction_replaces_and_verifies_content() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("Package_SF.upk");
        fs::write(&target, b"original").unwrap();
        install_transaction(&target, b"replacement", b"original").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"replacement");
        assert!(!target.with_extension("upk.swap-staged").exists());
        assert!(!target.with_extension("upk.swap-previous").exists());
    }

    #[test]
    #[ignore = "requires a Rocket League install and downloaded catalog"]
    fn current_catalog_and_cross_version_packages_validate() {
        let install = std::env::var("RL_INSTALL").unwrap();
        let csv = fs::read_to_string(std::env::var("RL_KEY_INDEX").unwrap()).unwrap();
        let catalog = build_catalog(&csv, &install).unwrap();
        assert!(
            catalog.len() > 3_500,
            "only {} installed packages",
            catalog.len()
        );
        let donor = catalog
            .iter()
            .find(|p| p.package == "Antenna_8Ball")
            .unwrap();
        let target = catalog
            .iter()
            .find(|p| p.package == "Antenna_SoccerBall")
            .unwrap();
        let cooked = cooked_dir(&install).unwrap();
        let donor_bytes = fs::read(package_path(&cooked, &donor.package)).unwrap();
        let target_bytes = fs::read(package_path(&cooked, &target.package)).unwrap();
        assert_eq!(
            crate::upk_swap::package_version(&donor_bytes).unwrap().1,
            32
        );
        assert!(crate::upk_swap::package_version(&target_bytes).unwrap().1 >= 33);
        let target_key = crate::upk_swap::key_from_base64(&target.key).unwrap();
        let generated = crate::upk_swap::masquerade(
            &donor_bytes,
            &target_bytes,
            &crate::upk_swap::key_from_base64(&donor.key).unwrap(),
            &target_key,
            &donor.package,
            &target.package,
        )
        .unwrap();
        crate::upk_swap::validate_identity(&generated, &target_key, &target.package).unwrap();
    }
}
