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
// Older versions restore a bank as soon as one swap is removed. Shared ownership
// needs a new version so those builds cannot silently break the remaining swaps.
const MANIFEST_VERSION: u32 = 3;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum SoundChoice {
    #[default]
    Original,
    MatchAppearance,
    Bank(String),
}

#[derive(Clone, Debug)]
pub struct SoundBankInfo {
    pub file: String,
    pub label: String,
    pub unavailable: Option<String>,
}

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
    EngineAudio,
}

impl ItemSlot {
    pub const ALL: [Self; 9] = [
        Self::Body,
        Self::Wheels,
        Self::RocketBoost,
        Self::Trail,
        Self::GoalExplosion,
        Self::Topper,
        Self::Antenna,
        Self::PaintFinish,
        Self::EngineAudio,
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
            Self::EngineAudio => "Engine Audio",
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
    #[serde(default)]
    pub unavailable: Option<String>,
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
    #[serde(default)]
    pub audio_target: Option<String>,
    #[serde(default)]
    pub sound: Option<SoundChoice>,
}

impl ActiveSwap {
    pub(crate) fn audio_file(&self) -> Option<&str> {
        self.audio_backup.as_ref().map(|_| {
            self.audio_target
                .as_deref()
                .unwrap_or("SFX_Boost_Standard.bnk")
        })
    }
    pub fn sound_choice(&self) -> SoundChoice {
        self.sound.clone().unwrap_or_else(|| {
            if self.audio_backup.is_some() {
                SoundChoice::Bank("SFX_Boost_Alpha.bnk".into())
            } else {
                SoundChoice::Original
            }
        })
    }

    pub fn is_engine_audio(&self) -> bool {
        self.target_package
            .to_ascii_lowercase()
            .starts_with("engineaudio_")
    }
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
    pub sounds: Vec<SoundBankInfo>,
    pub boost_banks: BTreeMap<String, String>,
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
    sound: SoundChoice,
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
        s.message = "Generating appearance and sound swap…".into();
    });
    let packages = state.item_swapper.snapshot.load().packages.clone();
    tokio::spawn(async move {
        let operation_install = install.clone();
        let result = tokio::task::spawn_blocking(move || {
            if crate::assets::is_rocket_league_running() {
                return Err("Close Rocket League before changing game files.".into());
            }
            if donor == "Boost_AlphaReward" && target == "Boost_Standard" {
                crate::assets::restore_legacy_bubble_swap(&operation_install)?;
            }
            apply(&operation_install, &packages, &donor, &target, &sound)
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
                    SoundChoice::MatchAppearance,
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

pub fn has_standard_swap() -> bool {
    let Some(conf) = crate::state::config_dir() else {
        return false;
    };
    load_manifest(&conf).is_ok_and(|manifest| {
        manifest
            .active
            .iter()
            .any(|swap| swap.target_package == "Boost_Standard")
    })
}

pub(crate) fn check_legacy_standard_restore(conf: &Path) -> Result<(), String> {
    let manifest = load_manifest(conf)?;
    if let Some(owner) = manifest.active.iter().find(|record| {
        record.target_package == "Boost_Standard"
            || record.audio_file() == Some("SFX_Boost_Standard.bnk")
    }) {
        return Err(format!(
            "Standard's appearance or shared sound is managed by {}. Restore that swap in Item Swapper first.",
            owner.target_package
        ));
    }
    Ok(())
}

pub fn alpha_preset_file_state(install: &str) -> Option<crate::assets::BoostGameFileState> {
    let conf = crate::state::config_dir()?;
    let manifest = load_manifest(&conf).ok()?;
    let record = manifest
        .active
        .iter()
        .find(|swap| swap.target_package == "Boost_Standard")?;
    let cooked = cooked_dir(install).ok()?;
    let visual = hash_file(&package_path(&cooked, "Boost_Standard")).ok()?;
    let audio = hash_file(&cooked.join("SFX_Boost_Standard.bnk")).ok()?;
    Some(standard_swap_state(record, &visual, &audio))
}

fn standard_swap_state(
    record: &ActiveSwap,
    visual: &str,
    audio: &str,
) -> crate::assets::BoostGameFileState {
    use crate::assets::BoostGameFileState;
    let sound = record.sound_choice();
    let untouched_audio = sound == SoundChoice::Original && record.audio_file().is_none();
    let audio_original = untouched_audio || record.audio_original_sha256.as_deref() == Some(audio);
    let audio_applied = record.audio_applied_sha256.as_deref() == Some(audio);
    if visual == record.original_sha256 && audio_original {
        BoostGameFileState::Original
    } else if visual == record.applied_sha256 && (audio_applied || untouched_audio) {
        let alpha_sound = sound == SoundChoice::MatchAppearance
            || sound == SoundChoice::Bank("SFX_Boost_Alpha.bnk".into());
        if record.donor_package == "Boost_AlphaReward" && alpha_sound && audio_applied {
            BoostGameFileState::Alpha
        } else {
            BoostGameFileState::Custom
        }
    } else {
        BoostGameFileState::Unknown
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
    start_apply(
        state,
        install,
        record.donor_package.clone(),
        target,
        record.sound_choice(),
    );
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
    let was_standard_managed = state
        .item_swapper
        .snapshot
        .load()
        .active
        .iter()
        .any(|record| record.target_package == "Boost_Standard");
    refresh_active(state, install);
    let standard_managed = has_standard_swap();
    let alpha_enabled =
        alpha_preset_file_state(install) == Some(crate::assets::BoostGameFileState::Alpha);
    let mut boost_status = state
        .boost
        .boost_swap_status
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let alpha_context = standard_managed
        || was_standard_managed
        || boost_status.starts_with("Loading the current item catalog")
        || boost_status.starts_with("Restoring the Alpha Boost preset");
    if alpha_context {
        *boost_status = message;
    }
    drop(boost_status);
    if alpha_context {
        state.update_config(|config| config.alpha_boost_enabled = alpha_enabled);
    }
    crate::assets::request_boost_swap_inspection(state, install.to_owned(), true);
}

fn apply(
    install: &str,
    packages: &[CatalogPackage],
    donor_id: &str,
    target_id: &str,
    sound: &SoundChoice,
) -> Result<String, String> {
    let cooked = cooked_dir(install)?;
    let conf = crate::state::config_dir().ok_or("Could not resolve config directory")?;
    apply_at(&cooked, &conf, packages, donor_id, target_id, sound)
}

fn apply_at(
    cooked: &Path,
    conf: &Path,
    packages: &[CatalogPackage],
    donor_id: &str,
    target_id: &str,
    sound: &SoundChoice,
) -> Result<String, String> {
    if donor_id == target_id && *sound == SoundChoice::Original {
        return Err("Choose a different appearance or a replacement sound.".into());
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
    for package in [donor, target] {
        if let Some(reason) = &package.unavailable {
            return Err(format!(
                "{} is unavailable: {reason}",
                package.display_name()
            ));
        }
    }
    let mut manifest = load_manifest(conf)?;
    let target_path = package_path(cooked, target_id);
    let donor_path = pristine_path(cooked, &manifest, donor_id)?;
    let donor_bytes = fs::read(&donor_path)
        .map_err(|e| format!("Could not read donor {}: {e}", donor_path.display()))?;
    let donor_key = crate::upk_swap::key_from_base64(&donor.key)?;
    crate::upk_swap::validate_identity(&donor_bytes, &donor_key, donor_id).map_err(|error| {
        format!(
            "Could not validate {donor_id} with its catalog key: {error}. Refresh the catalog and confirm the package is from the current game install. No game files were changed."
        )
    })?;
    let target_current = fs::read(&target_path)
        .map_err(|e| format!("Could not read {}: {e}", target_path.display()))?;
    let target_key = crate::upk_swap::key_from_base64(&target.key)?;
    crate::upk_swap::validate_identity(&target_current, &target_key, target_id).map_err(|error| {
        format!(
            "Could not validate {target_id} with its catalog key: {error}. Refresh the catalog and confirm the package is from the current game install. No game files were changed."
        )
    })?;
    let visual_before = target_current.clone();
    let current_hash = hash_bytes(&target_current);

    let existing = manifest
        .active
        .iter()
        .find(|v| v.target_package == target_id)
        .cloned();
    let (target_original, backup_path, original_hash) = if let Some(record) = &existing {
        if current_hash != record.applied_sha256 && current_hash != record.original_sha256 {
            let prior = fs::read(&record.target_backup)
                .map_err(|e| format!("Could not read prior target backup: {e}"))?;
            if crate::upk_swap::package_guid(&target_current)?
                == crate::upk_swap::package_guid(&prior)?
            {
                return Err("Target changed without a new package GUID. Verify the game files before applying.".into());
            }
            let backup = backup_path(conf, target_id, &current_hash);
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
        let backup = backup_path(conf, target_id, &current_hash);
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
    let generated = if donor_id == target_id {
        target_original.clone()
    } else {
        crate::upk_swap::masquerade(
            &donor_bytes,
            &target_original,
            &donor_key,
            &target_key,
            donor_id,
            target_id,
        )?
    };
    crate::upk_swap::validate_identity(&generated, &target_key, target_id)?;
    let audio = prepare_audio(
        conf,
        cooked,
        &manifest,
        packages,
        existing.as_ref(),
        sound,
        donor.slot,
        &donor_bytes,
        &donor_key,
        &target_original,
        &target_key,
        target_id,
    )?;
    let mut changes = vec![FileChange {
        path: target_path,
        before: visual_before,
        after: generated.clone(),
    }];
    changes.extend(audio.changes);
    let applied_hash = hash_bytes(&generated);
    if let Some(file) = audio.target.as_deref() {
        for record in manifest
            .active
            .iter_mut()
            .filter(|r| r.audio_file() == Some(file))
        {
            record.audio_backup = audio.backup.clone();
            record.audio_original_sha256 = audio.original_hash.clone();
            record.audio_applied_sha256 = audio.applied_hash.clone();
        }
    }
    manifest.active.retain(|v| v.target_package != target_id);
    manifest.active.push(ActiveSwap {
        donor_package: donor_id.into(),
        target_package: target_id.into(),
        target_backup: backup_path.display().to_string(),
        original_sha256: original_hash,
        applied_sha256: applied_hash,
        health: SwapHealth::Active,
        audio_backup: audio.backup,
        audio_original_sha256: audio.original_hash,
        audio_applied_sha256: audio.applied_hash,
        audio_target: audio.target,
        sound: Some(sound.clone()),
    });
    commit_changes(conf, &manifest, &changes)?;
    if target.slot == ItemSlot::EngineAudio {
        Ok(format!(
            "Applied: {} now uses {}'s engine sound. Equip {} in Rocket League's Engine Audio selection.",
            target.display_name(),
            donor.display_name(),
            target.display_name()
        ))
    } else {
        Ok(format!(
            "Applied: {} now displays {}'s appearance. {}",
            target.display_name(),
            donor.display_name(),
            audio.description
        ))
    }
}

fn restore(install: &str, target_id: &str) -> Result<String, String> {
    let cooked = cooked_dir(install)?;
    let conf = crate::state::config_dir().ok_or("Could not resolve config directory")?;
    restore_at(&cooked, &conf, target_id)
}

fn restore_at(cooked: &Path, conf: &Path, target_id: &str) -> Result<String, String> {
    let mut manifest = load_manifest(conf)?;
    let index = manifest
        .active
        .iter()
        .position(|v| v.target_package == target_id)
        .ok_or("No active swap exists for this target")?;
    let record = &manifest.active[index];
    let is_engine_audio = record.is_engine_audio();
    let backup = verified_backup(&record.target_backup, &record.original_sha256)?;
    let target = package_path(cooked, target_id);
    let visual_current =
        fs::read(&target).map_err(|e| format!("Could not read current target: {e}"))?;
    check_known(
        &visual_current,
        &record.original_sha256,
        &record.applied_sha256,
    )?;
    let mut changes = vec![FileChange {
        path: target,
        before: visual_current,
        after: backup,
    }];
    let mut keep_shared_sound = false;
    if let Some(file) = record.audio_file() {
        let (before, original) = recorded_audio(cooked, record)?;
        for other in manifest
            .active
            .iter()
            .filter(|r| r.target_package != target_id && r.audio_file() == Some(file))
        {
            let (other_current, other_original) = recorded_audio(cooked, other)?;
            if other_current != before || other_original != original {
                return Err("Shared sound records disagree. Nothing was restored.".into());
            }
            keep_shared_sound = true;
        }
        changes.push(FileChange {
            path: cooked.join(file),
            after: if keep_shared_sound {
                before.clone()
            } else {
                original
            },
            before,
        });
    }
    manifest.active.remove(index);
    commit_changes(conf, &manifest, &changes)?;
    if is_engine_audio {
        Ok(format!("Restored {target_id}'s original engine sound."))
    } else if keep_shared_sound {
        Ok(format!(
            "Restored {target_id}'s appearance. Its shared sound remains in use by another active swap."
        ))
    } else {
        Ok(format!("Restored {target_id}'s appearance and sound."))
    }
}

struct FileChange {
    path: PathBuf,
    before: Vec<u8>,
    after: Vec<u8>,
}

fn commit_changes(conf: &Path, manifest: &Manifest, changes: &[FileChange]) -> Result<(), String> {
    // Preflight the entire set before the first write, including changes made
    // outside this process while the replacements were being generated.
    for change in changes {
        if hash_file(&change.path)? != hash_bytes(&change.before) {
            return Err(format!(
                "{} changed while preparing the swap. Nothing was applied.",
                change.path.display()
            ));
        }
    }
    for (i, change) in changes.iter().enumerate() {
        if change.before == change.after {
            continue;
        }
        if let Err(error) = install_transaction(&change.path, &change.after, &change.before) {
            return Err(rollback_changes(&changes[..=i], error));
        }
    }
    if let Err(error) = save_manifest(conf, manifest) {
        return Err(rollback_changes(
            changes,
            format!("Could not save swap record: {error}"),
        ));
    }
    Ok(())
}

fn rollback_changes(changes: &[FileChange], error: String) -> String {
    let mut failures = Vec::new();
    for change in changes.iter().rev() {
        if change.before == change.after {
            continue;
        }
        if let Err(e) = atomic_write(&change.path, &change.before) {
            failures.push(e);
        }
    }
    if failures.is_empty() {
        format!("{error}. The previous files were restored.")
    } else {
        format!(
            "{error}. Could not fully roll back: {}. Backups were retained.",
            failures.join("; ")
        )
    }
}

fn verified_backup(path: &str, expected: &str) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|e| format!("Could not read backup: {e}"))?;
    if hash_bytes(&bytes) != expected {
        return Err("Backup failed hash verification".into());
    }
    Ok(bytes)
}

fn check_known(bytes: &[u8], original: &str, applied: &str) -> Result<(), String> {
    let hash = hash_bytes(bytes);
    if hash != original && hash != applied {
        return Err("A game file changed outside the swapper. Restore or verify game files before applying; nothing was changed.".into());
    }
    Ok(())
}

fn recorded_audio(cooked: &Path, record: &ActiveSwap) -> Result<(Vec<u8>, Vec<u8>), String> {
    let name = record.audio_file().ok_or("Missing recorded sound bank")?;
    if !crate::boost_audio::valid_bank_name(name) {
        return Err("Invalid recorded sound bank name".into());
    }
    let hash = record
        .audio_original_sha256
        .as_deref()
        .ok_or("Missing original sound hash")?;
    let original = verified_backup(
        record
            .audio_backup
            .as_deref()
            .ok_or("Missing sound backup")?,
        hash,
    )?;
    let current =
        fs::read(cooked.join(name)).map_err(|e| format!("Could not read sound bank: {e}"))?;
    let known = check_known(
        &current,
        hash,
        record
            .audio_applied_sha256
            .as_deref()
            .ok_or("Missing applied sound hash")?,
    );
    if let Err(error) = known {
        if package_was_updated(cooked, record)? {
            crate::boost_audio::validate(&current, name)?;
            return Ok((current.clone(), current));
        }
        return Err(error);
    }
    Ok((current, original))
}

fn package_was_updated(cooked: &Path, record: &ActiveSwap) -> Result<bool, String> {
    let current =
        fs::read(package_path(cooked, &record.target_package)).map_err(|e| e.to_string())?;
    let original = verified_backup(&record.target_backup, &record.original_sha256)?;
    Ok(crate::upk_swap::package_guid(&current)? != crate::upk_swap::package_guid(&original)?)
}

fn original_bank(
    cooked: &Path,
    conf: &Path,
    manifest: &Manifest,
    file: &str,
) -> Result<Vec<u8>, String> {
    if !crate::boost_audio::valid_bank_name(file) {
        return Err("Invalid boost sound bank name".into());
    }
    if let Some(record) = manifest
        .active
        .iter()
        .find(|r| r.audio_file() == Some(file))
    {
        return recorded_audio(cooked, record).map(|(_, original)| original);
    }
    let bytes = fs::read(cooked.join(file)).map_err(|e| format!("Could not read {file}: {e}"))?;
    if file == "SFX_Boost_Standard.bnk" && hash_bytes(&bytes) == crate::assets::alpha_audio_sha256()
    {
        return legacy_alpha_audio_backup(conf)
            .map(|(original, _)| original)
            .ok_or("Missing pristine Standard audio backup for the legacy Alpha swap".into());
    }
    Ok(bytes)
}

fn package_bank(bytes: &[u8], key: &[u8; 32], cooked: &Path) -> Result<String, String> {
    let banks = crate::upk_swap::boost_sound_banks(bytes, key)?
        .into_iter()
        .filter(|bank| crate::boost_audio::valid_bank_name(bank) && cooked.join(bank).is_file())
        .collect::<Vec<_>>();
    if banks.len() != 1 {
        return Err("Could not identify a single installed sound bank for this boost".into());
    }
    Ok(banks[0].clone())
}

#[derive(Default)]
struct PreparedAudio {
    changes: Vec<FileChange>,
    backup: Option<String>,
    original_hash: Option<String>,
    applied_hash: Option<String>,
    target: Option<String>,
    description: String,
}

fn recorded_sound_source(
    cooked: &Path,
    manifest: &Manifest,
    packages: &[CatalogPackage],
    record: &ActiveSwap,
) -> Result<String, String> {
    match record.sound_choice() {
        SoundChoice::Bank(file) => Ok(file),
        SoundChoice::MatchAppearance => {
            let donor = packages.iter().find(|p| p.package == record.donor_package)
                .ok_or("The existing sound swap's source is missing from the catalog. Refresh the catalog first.")?;
            let bytes = fs::read(pristine_path(cooked, manifest, &donor.package)?)
                .map_err(|e| e.to_string())?;
            package_bank(
                &bytes,
                &crate::upk_swap::key_from_base64(&donor.key)?,
                cooked,
            )
        }
        SoundChoice::Original => {
            Err("An existing sound swap has inconsistent original-sound metadata".into())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_audio(
    conf: &Path,
    cooked: &Path,
    manifest: &Manifest,
    packages: &[CatalogPackage],
    existing: Option<&ActiveSwap>,
    choice: &SoundChoice,
    slot: ItemSlot,
    donor: &[u8],
    donor_key: &[u8; 32],
    target: &[u8],
    target_key: &[u8; 32],
    target_id: &str,
) -> Result<PreparedAudio, String> {
    let mut result = PreparedAudio {
        description: "Original sound.".into(),
        ..Default::default()
    };
    let previous = existing.filter(|r| r.audio_file().is_some());
    if *choice == SoundChoice::Original {
        if slot == ItemSlot::RocketBoost
            && let Ok(file) = package_bank(target, target_key, cooked)
            && let Some(owner) = manifest
                .active
                .iter()
                .find(|r| r.target_package != target_id && r.audio_file() == Some(file.as_str()))
        {
            return Err(format!(
                "The original sound is shared with {} and currently replaced by its swap. Restore that swap first.",
                owner.target_package
            ));
        }
        if let Some(record) = previous {
            let (before, after) = recorded_audio(cooked, record)?;
            result.changes.push(FileChange {
                path: cooked.join(record.audio_file().unwrap()),
                before,
                after,
            });
        } else if slot == ItemSlot::RocketBoost
            && package_bank(target, target_key, cooked).as_deref() == Ok("SFX_Boost_Standard.bnk")
        {
            let path = cooked.join("SFX_Boost_Standard.bnk");
            let before = fs::read(&path).map_err(|e| e.to_string())?;
            if hash_bytes(&before) == crate::assets::alpha_audio_sha256() {
                let after = legacy_alpha_audio_backup(conf)
                    .ok_or("Missing original sound backup for the legacy Alpha swap")?
                    .0;
                result.changes.push(FileChange {
                    path,
                    before,
                    after,
                });
            }
        }
        return Ok(result);
    }
    if slot != ItemSlot::RocketBoost {
        return Err("Sound choices are available for Rocket Boosts only".into());
    }
    let target_file = package_bank(target, target_key, cooked)?;
    let owners = manifest
        .active
        .iter()
        .filter(|r| r.target_package != target_id && r.audio_file() == Some(target_file.as_str()))
        .collect::<Vec<_>>();
    if previous.is_some_and(|r| r.audio_file() != Some(target_file.as_str())) {
        return Err(
            "This boost's sound bank changed. Restore the previous swap before applying.".into(),
        );
    }
    let source_file = match choice {
        SoundChoice::MatchAppearance => package_bank(donor, donor_key, cooked)?,
        SoundChoice::Bank(file) => file.clone(),
        SoundChoice::Original => unreachable!(),
    };
    for owner in &owners {
        if recorded_sound_source(cooked, manifest, packages, owner)? != source_file {
            return Err(format!(
                "This boost shares its sound with {}, which uses a different sound. Choose the same sound or restore that swap first.",
                owner.target_package
            ));
        }
    }
    let current = fs::read(cooked.join(&target_file))
        .map_err(|e| format!("Could not read target sound: {e}"))?;
    let original = if let Some(record) = previous.or_else(|| owners.first().copied()) {
        recorded_audio(cooked, record)?.1
    } else if target_file == "SFX_Boost_Standard.bnk"
        && hash_bytes(&current) == crate::assets::alpha_audio_sha256()
    {
        legacy_alpha_audio_backup(conf)
            .ok_or("The existing Alpha sound needs its original backup before continuing")?
            .0
    } else {
        current.clone()
    };
    for owner in &owners {
        let (other_current, other_original) = recorded_audio(cooked, owner)?;
        if other_current != current || other_original != original {
            return Err("Shared sound records disagree. Nothing was applied.".into());
        }
    }
    let source = if source_file == target_file {
        original.clone()
    } else {
        original_bank(cooked, conf, manifest, &source_file)?
    };
    // Owners agree on the source filename, but its contents may have changed.
    // Regenerate from the current pristine source, including migrated records.
    let generated = crate::boost_audio::generate(&source, &original, &source_file, &target_file)?;
    let original_hash = hash_bytes(&original);
    let backup = conf
        .join("backups/ItemSwapper/audio")
        .join(&target_file)
        .join(format!("{original_hash}.bnk"));
    if !backup.exists() {
        atomic_write(&backup, &original)?;
    }
    if hash_file(&backup)? != original_hash {
        return Err("Sound backup failed hash verification".into());
    }
    result.applied_hash = Some(hash_bytes(&generated));
    result.original_hash = Some(original_hash);
    result.backup = Some(backup.display().to_string());
    result.target = Some(target_file.clone());
    result.description = format!("Sound: {}.", crate::boost_audio::display_name(&source_file));
    if !owners.is_empty() {
        result.description.push_str(&format!(
            " Shared with {} other active swap(s).",
            owners.len()
        ));
    }
    result.changes.push(FileChange {
        path: cooked.join(target_file),
        before: current,
        after: generated,
    });
    Ok(result)
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
        let current_path = package_path(cooked, package);
        let current = hash_file(&current_path)?;
        if current != active.original_sha256 && current != active.applied_sha256 {
            if package_was_updated(cooked, active)? {
                return Ok(current_path);
            }
            return Err(format!(
                "Donor {package} changed outside the swapper; restore or verify it first"
            ));
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
        if let Some(file) = record.audio_file() {
            let audio_health = match hash_file(&cooked.join(file)) {
                Ok(hash) if record.audio_applied_sha256.as_deref() == Some(&hash) => {
                    SwapHealth::Active
                }
                Ok(hash) if record.audio_original_sha256.as_deref() == Some(&hash) => {
                    SwapHealth::NeedsReapply
                }
                Ok(_)
                    if package_was_updated(&cooked, record).unwrap_or(false)
                        && fs::read(cooked.join(file)).is_ok_and(|bytes| {
                            crate::boost_audio::validate(&bytes, file).is_ok()
                        }) =>
                {
                    SwapHealth::NeedsReapply
                }
                _ => SwapHealth::Conflict,
            };
            if audio_health == SwapHealth::Conflict || record.health == SwapHealth::Conflict {
                record.health = SwapHealth::Conflict;
            } else if audio_health == SwapHealth::NeedsReapply {
                record.health = SwapHealth::NeedsReapply;
            }
        }
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
    let (sounds, boost_banks) = sound_catalog(&packages, install);
    publish(state, |s| {
        s.packages = packages;
        s.sounds = sounds;
        s.boost_banks = boost_banks;
        s.install_path = install.into();
        s.catalog_loaded = true;
        s.refreshing = false;
        s.message = message.into();
    });
    refresh_active(state, install);
}

fn sound_catalog(
    packages: &[CatalogPackage],
    install: &str,
) -> (Vec<SoundBankInfo>, BTreeMap<String, String>) {
    let mut sounds = Vec::new();
    let mut mappings = BTreeMap::new();
    let Ok(cooked) = cooked_dir(install) else {
        return (sounds, mappings);
    };
    let Some(conf) = crate::state::config_dir() else {
        return (sounds, mappings);
    };
    let manifest = load_manifest(&conf).unwrap_or_default();
    if let Ok(entries) = fs::read_dir(&cooked) {
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if !crate::boost_audio::valid_bank_name(&file) {
                continue;
            }
            let unavailable = original_bank(&cooked, &conf, &manifest, &file)
                .and_then(|bytes| crate::boost_audio::validate(&bytes, &file))
                .err();
            sounds.push(SoundBankInfo {
                label: crate::boost_audio::display_name(&file),
                file,
                unavailable,
            });
        }
    }
    sounds.sort_by_key(|s| s.label.to_lowercase());
    for package in packages.iter().filter(|p| p.slot == ItemSlot::RocketBoost) {
        let resolve = || -> Result<String, String> {
            let bytes = fs::read(pristine_path(&cooked, &manifest, &package.package)?)
                .map_err(|e| e.to_string())?;
            let key = crate::upk_swap::key_from_base64(&package.key)?;
            package_bank(&bytes, &key, &cooked)
        };
        if let Ok(bank) = resolve() {
            mappings.insert(package.package.clone(), bank);
        }
    }
    for sound in &mut sounds {
        if sound.file == "SFX_Boost_Alpha.bnk" {
            continue;
        }
        let mut labels = packages
            .iter()
            .filter(|p| mappings.get(&p.package) == Some(&sound.file))
            .flat_map(|p| p.labels.iter().cloned())
            .collect::<Vec<_>>();
        labels.sort();
        labels.dedup();
        if !labels.is_empty() {
            sound.label = labels
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(" / ");
            if labels.len() > 3 {
                sound
                    .label
                    .push_str(&format!(" (+{} variants)", labels.len() - 3));
            }
        }
    }
    sounds.sort_by_key(|s| s.label.to_lowercase());
    (sounds, mappings)
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
                unavailable: None,
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
        if package.slot == ItemSlot::EngineAudio {
            package.unavailable = fs::read(package_path(&cooked, &package.package))
                .map_err(|e| format!("Could not read engine sound package: {e}"))
                .and_then(|bytes| {
                    let key = crate::upk_swap::key_from_base64(&package.key)?;
                    crate::upk_swap::validate_identity(&bytes, &key, &package.package)
                })
                .err();
        }
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
    let mut value: Manifest = serde_json::from_slice(
        &fs::read(&path).map_err(|e| format!("Could not read swap manifest: {e}"))?,
    )
    .map_err(|e| format!("Invalid swap manifest: {e}"))?;
    if !(1..=MANIFEST_VERSION).contains(&value.version) {
        return Err(format!(
            "Unsupported swap manifest version {}",
            value.version
        ));
    }
    value.version = MANIFEST_VERSION;
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
    fn shared_sound_reapply_uses_updated_source_and_preserves_restore() {
        use base64::Engine;
        let root = tempfile::tempdir().unwrap();
        let cooked = root.path().join("cooked");
        let conf = root.path().join("conf");
        fs::create_dir_all(&cooked).unwrap();
        let key = [7; 32];
        let source_file = "SFX_Boost_Alpha.bnk";
        let target_file = "SFX_Boost_Standard.bnk";
        let original = crate::boost_audio::tests::fixture(target_file, b"original");
        fs::write(cooked.join(target_file), &original).unwrap();
        fs::write(
            cooked.join(source_file),
            crate::boost_audio::tests::fixture(source_file, b"old source"),
        )
        .unwrap();
        let mut packages = Vec::new();
        let mut originals = Vec::new();
        for (id, bank) in [
            ("Boost_Source", "SFX_Boost_Alpha"),
            ("Boost_Standard", "SFX_Boost_Standard"),
            ("Boost_Blue", "SFX_Boost_Standard"),
        ] {
            let bytes = crate::upk_swap::tests::fixture_with_names(id, &key, 1, 32, &[bank]);
            fs::write(package_path(&cooked, id), &bytes).unwrap();
            originals.push((id, bytes));
            packages.push(CatalogPackage {
                package: id.into(),
                labels: vec![],
                slot: ItemSlot::RocketBoost,
                key: base64::engine::general_purpose::STANDARD.encode(key),
                unavailable: None,
            });
        }
        for target in ["Boost_Standard", "Boost_Blue"] {
            apply_at(
                &cooked,
                &conf,
                &packages,
                "Boost_Source",
                target,
                &SoundChoice::MatchAppearance,
            )
            .unwrap();
        }
        let old_applied = fs::read(cooked.join(target_file)).unwrap();
        // Legacy manifests may describe a different representation of the same
        // sound. Normalize it on reapply without adopting it as the backup.
        let mut legacy_audio = old_applied.clone();
        legacy_audio.extend_from_slice(b"JUNK\0\0\0\0");
        fs::write(cooked.join(target_file), &legacy_audio).unwrap();
        let mut legacy = load_manifest(&conf).unwrap();
        legacy.version = 1;
        for owner in &mut legacy.active {
            owner.sound = None;
            owner.audio_target = None;
            owner.audio_applied_sha256 = Some(hash_bytes(&legacy_audio));
        }
        save_manifest(&conf, &legacy).unwrap();
        apply_at(
            &cooked,
            &conf,
            &packages,
            "Boost_Source",
            "Boost_Blue",
            &SoundChoice::MatchAppearance,
        )
        .unwrap();
        assert_eq!(fs::read(cooked.join(target_file)).unwrap(), old_applied);
        let updated = crate::boost_audio::tests::fixture(source_file, b"updated source");
        fs::write(cooked.join(source_file), &updated).unwrap();
        apply_at(
            &cooked,
            &conf,
            &packages,
            "Boost_Source",
            "Boost_Blue",
            &SoundChoice::MatchAppearance,
        )
        .unwrap();
        let expected =
            crate::boost_audio::generate(&updated, &original, source_file, target_file).unwrap();
        assert_ne!(old_applied, expected);
        assert_eq!(fs::read(cooked.join(target_file)).unwrap(), expected);
        let manifest = load_manifest(&conf).unwrap();
        assert_eq!(manifest.active.len(), 2);
        for owner in &manifest.active {
            assert_eq!(
                owner.audio_applied_sha256.as_deref(),
                Some(hash_bytes(&expected).as_str())
            );
            assert_eq!(
                owner.audio_original_sha256.as_deref(),
                Some(hash_bytes(&original).as_str())
            );
        }
        restore_at(&cooked, &conf, "Boost_Standard").unwrap();
        assert_eq!(fs::read(cooked.join(target_file)).unwrap(), expected);
        restore_at(&cooked, &conf, "Boost_Blue").unwrap();
        assert_eq!(fs::read(cooked.join(target_file)).unwrap(), original);
        for (id, bytes) in originals {
            assert_eq!(fs::read(package_path(&cooked, id)).unwrap(), bytes);
        }
    }

    #[test]
    fn manifest_failure_rolls_back_visual_and_audio_together() {
        let root = tempfile::tempdir().unwrap();
        let conf = root.path().join("conf");
        // A directory where the manifest file belongs forces the final save to fail.
        fs::create_dir_all(manifest_path(&conf)).unwrap();
        let visual = root.path().join("target.upk");
        let audio = root.path().join("target.bnk");
        fs::write(&visual, b"old visual").unwrap();
        fs::write(&audio, b"old audio").unwrap();
        let changes = [
            FileChange {
                path: visual.clone(),
                before: b"old visual".to_vec(),
                after: b"new visual".to_vec(),
            },
            FileChange {
                path: audio.clone(),
                before: b"old audio".to_vec(),
                after: b"new audio".to_vec(),
            },
        ];
        assert!(
            commit_changes(&conf, &Manifest::default(), &changes)
                .unwrap_err()
                .contains("previous files were restored")
        );
        assert_eq!(fs::read(visual).unwrap(), b"old visual");
        assert_eq!(fs::read(audio).unwrap(), b"old audio");
    }

    #[test]
    fn changed_audio_aborts_before_visual_write() {
        let root = tempfile::tempdir().unwrap();
        let visual = root.path().join("target.upk");
        let audio = root.path().join("target.bnk");
        fs::write(&visual, b"old visual").unwrap();
        fs::write(&audio, b"outside edit").unwrap();
        let changes = [
            FileChange {
                path: visual.clone(),
                before: b"old visual".to_vec(),
                after: b"new visual".to_vec(),
            },
            FileChange {
                path: audio.clone(),
                before: b"old audio".to_vec(),
                after: b"new audio".to_vec(),
            },
        ];
        assert!(commit_changes(root.path(), &Manifest::default(), &changes).is_err());
        assert_eq!(fs::read(visual).unwrap(), b"old visual");
        assert_eq!(fs::read(audio).unwrap(), b"outside edit");
    }

    #[test]
    fn audio_write_failure_rolls_back_the_visual_and_preserves_manifest() {
        let root = tempfile::tempdir().unwrap();
        let visual = root.path().join("target.upk");
        let audio = root.path().join("sound.bnk");
        fs::write(&visual, b"old visual").unwrap();
        fs::write(&audio, b"old audio").unwrap();
        let conf = root.path().join("conf");
        let manifest = Manifest {
            version: MANIFEST_VERSION,
            active: vec![],
        };
        save_manifest(&conf, &manifest).unwrap();
        let before_manifest = fs::read(manifest_path(&conf)).unwrap();
        fs::create_dir(audio.with_extension("upk.swap-previous")).unwrap();
        let changes = [
            FileChange {
                path: visual.clone(),
                before: b"old visual".to_vec(),
                after: b"new visual".to_vec(),
            },
            FileChange {
                path: audio.clone(),
                before: b"old audio".to_vec(),
                after: b"new audio".to_vec(),
            },
        ];
        assert!(commit_changes(&conf, &manifest, &changes).is_err());
        assert_eq!(fs::read(visual).unwrap(), b"old visual");
        assert_eq!(fs::read(audio).unwrap(), b"old audio");
        assert_eq!(fs::read(manifest_path(&conf)).unwrap(), before_manifest);
    }

    #[test]
    fn shared_sound_is_restored_only_after_the_last_owner() {
        for reverse in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let cooked = root.path().join("cooked");
            let conf = root.path().join("conf");
            fs::create_dir_all(&cooked).unwrap();
            let visual_backup = root.path().join("original.upk");
            let audio_backup = root.path().join("original.bnk");
            fs::write(&visual_backup, b"original visual").unwrap();
            fs::write(&audio_backup, b"original audio").unwrap();
            let audio = cooked.join("SFX_Boost_Standard.bnk");
            fs::write(&audio, b"shared audio").unwrap();
            let mut manifest = Manifest {
                version: MANIFEST_VERSION,
                active: vec![],
            };
            let mut targets = ["Boost_Standard", "Boost_Standard_Blue"];
            for target in targets {
                fs::write(package_path(&cooked, target), b"applied visual").unwrap();
                manifest.active.push(ActiveSwap {
                    donor_package: "Boost_AlphaReward".into(),
                    target_package: target.into(),
                    target_backup: visual_backup.display().to_string(),
                    original_sha256: hash_bytes(b"original visual"),
                    applied_sha256: hash_bytes(b"applied visual"),
                    health: SwapHealth::Active,
                    audio_backup: Some(audio_backup.display().to_string()),
                    audio_original_sha256: Some(hash_bytes(b"original audio")),
                    audio_applied_sha256: Some(hash_bytes(b"shared audio")),
                    audio_target: Some("SFX_Boost_Standard.bnk".into()),
                    sound: Some(SoundChoice::Bank("SFX_Boost_Alpha.bnk".into())),
                });
            }
            save_manifest(&conf, &manifest).unwrap();
            if reverse {
                targets.reverse();
            }
            restore_at(&cooked, &conf, targets[0]).unwrap();
            assert_eq!(fs::read(&audio).unwrap(), b"shared audio");
            assert_eq!(
                fs::read(package_path(&cooked, targets[0])).unwrap(),
                b"original visual"
            );
            assert_eq!(
                fs::read(package_path(&cooked, targets[1])).unwrap(),
                b"applied visual"
            );
            assert_eq!(load_manifest(&conf).unwrap().active.len(), 1);
            assert!(check_legacy_standard_restore(&conf).is_err());
            restore_at(&cooked, &conf, targets[1]).unwrap();
            assert_eq!(fs::read(&audio).unwrap(), b"original audio");
            assert_eq!(
                fs::read(package_path(&cooked, targets[1])).unwrap(),
                b"original visual"
            );
            assert!(load_manifest(&conf).unwrap().active.is_empty());
            assert!(check_legacy_standard_restore(&conf).is_ok());
        }
    }

    #[test]
    fn reads_legacy_alpha_sound_record() {
        let record: ActiveSwap = serde_json::from_value(serde_json::json!({
            "donor_package": "Boost_AlphaReward", "target_package": "Boost_Standard",
            "target_backup": "backup.upk", "original_sha256": "a", "applied_sha256": "b",
            "health": "Active", "audio_backup": "backup.bnk", "audio_original_sha256": "c", "audio_applied_sha256": "d"
        })).unwrap();
        assert_eq!(record.audio_file(), Some("SFX_Boost_Standard.bnk"));
        assert_eq!(
            record.sound_choice(),
            SoundChoice::Bank("SFX_Boost_Alpha.bnk".into())
        );
    }

    #[test]
    fn alpha_indicator_requires_alpha_appearance_and_verified_alpha_sound() {
        use crate::assets::BoostGameFileState as State;
        let mut record: ActiveSwap = serde_json::from_value(serde_json::json!({
            "donor_package": "Boost_AlphaReward", "target_package": "Boost_Standard",
            "target_backup": "backup.upk", "original_sha256": "visual-original", "applied_sha256": "visual-applied",
            "health": "Active", "audio_backup": "backup.bnk", "audio_original_sha256": "audio-original", "audio_applied_sha256": "audio-applied"
        })).unwrap();
        // Legacy Alpha records and both ways of selecting Alpha sound agree.
        for sound in [
            None,
            Some(SoundChoice::MatchAppearance),
            Some(SoundChoice::Bank("SFX_Boost_Alpha.bnk".into())),
        ] {
            record.sound = sound;
            assert_eq!(
                standard_swap_state(&record, "visual-applied", "audio-applied"),
                State::Alpha
            );
            assert_eq!(
                standard_swap_state(&record, "visual-original", "audio-original"),
                State::Original
            );
            assert_eq!(
                standard_swap_state(&record, "visual-applied", "unexpected-audio"),
                State::Unknown
            );
            assert_eq!(
                standard_swap_state(&record, "unexpected-visual", "audio-applied"),
                State::Unknown
            );
        }
        record.sound = Some(SoundChoice::Bank("SFX_Boost_Bubbles.bnk".into()));
        assert_eq!(
            standard_swap_state(&record, "visual-applied", "audio-applied"),
            State::Custom
        );
        record.sound = Some(SoundChoice::MatchAppearance);
        record.donor_package = "boost_alphadevreward".into();
        assert_eq!(
            standard_swap_state(&record, "visual-applied", "audio-applied"),
            State::Custom
        );
        record.donor_package = "Boost_AlphaReward".into();
        record.sound = Some(SoundChoice::Original);
        record.audio_backup = None;
        record.audio_original_sha256 = None;
        record.audio_applied_sha256 = None;
        assert_eq!(
            standard_swap_state(&record, "visual-applied", "audio-original"),
            State::Custom
        );
        assert_eq!(
            standard_swap_state(&record, "visual-original", "audio-original"),
            State::Original
        );
    }

    #[test]
    #[ignore = "requires RL_AUDIO_FIXTURES with pristine UPKs/banks and RL_KEY_INDEX; writes only temporary copies"]
    fn boost_appearance_and_sound_round_trip_on_copies() {
        let inputs = PathBuf::from(std::env::var("RL_AUDIO_FIXTURES").unwrap());
        let root = tempfile::tempdir().unwrap();
        let install = root.path().join("game");
        let cooked = install.join("TAGame/CookedPCConsole");
        fs::create_dir_all(&cooked).unwrap();
        for name in [
            "boost_alphadevreward_SF.upk",
            "Boost_AlphaReward_SF.upk",
            "Boost_Standard_SF.upk",
            "Boost_Standard_Blue_SF.upk",
            "SFX_Boost_Alpha.bnk",
            "SFX_Boost_Standard.bnk",
        ] {
            fs::copy(inputs.join(name), cooked.join(name)).unwrap();
        }
        let conf = root.path().join("conf");
        let csv = fs::read_to_string(std::env::var("RL_KEY_INDEX").unwrap()).unwrap();
        let packages = build_catalog(&csv, install.to_str().unwrap()).unwrap();
        let original_visual = fs::read(cooked.join("Boost_Standard_SF.upk")).unwrap();
        let original_audio = fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap();
        let source_audio = fs::read(cooked.join("SFX_Boost_Alpha.bnk")).unwrap();
        let expected_audio = crate::boost_audio::generate(
            &source_audio,
            &original_audio,
            "SFX_Boost_Alpha.bnk",
            "SFX_Boost_Standard.bnk",
        )
        .unwrap();
        apply_at(
            &cooked,
            &conf,
            &packages,
            "boost_alphadevreward",
            "Boost_Standard",
            &SoundChoice::MatchAppearance,
        )
        .unwrap();
        assert_eq!(
            fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
            expected_audio
        );
        restore_at(&cooked, &conf, "Boost_Standard").unwrap();
        let blue_before = fs::read(cooked.join("Boost_Standard_Blue_SF.upk")).unwrap();
        // Alpha and Dev have different appearances but the same source sound.
        // Exercise both restore orders, including a legacy Alpha swap record.
        for legacy in [false, true] {
            apply_at(
                &cooked,
                &conf,
                &packages,
                "Boost_AlphaReward",
                "Boost_Standard",
                &SoundChoice::MatchAppearance,
            )
            .unwrap();
            let shared_audio = expected_audio.clone();
            if legacy {
                let mut legacy_audio = shared_audio.clone();
                // An older generator can produce different bytes for the same sound.
                legacy_audio.extend_from_slice(b"JUNK\0\0\0\0");
                fs::write(cooked.join("SFX_Boost_Standard.bnk"), &legacy_audio).unwrap();
                let mut manifest = load_manifest(&conf).unwrap();
                manifest.version = 1;
                manifest.active[0].sound = None;
                manifest.active[0].audio_target = None;
                manifest.active[0].audio_applied_sha256 = Some(hash_bytes(&legacy_audio));
                save_manifest(&conf, &manifest).unwrap();
            }
            apply_at(
                &cooked,
                &conf,
                &packages,
                "boost_alphadevreward",
                "Boost_Standard_Blue",
                &SoundChoice::MatchAppearance,
            )
            .unwrap();
            assert_eq!(
                fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
                shared_audio
            );
            let manifest = load_manifest(&conf).unwrap();
            assert_eq!(manifest.version, MANIFEST_VERSION);
            assert_eq!(manifest.active.len(), 2);
            assert_eq!(
                manifest.active[0].audio_backup,
                manifest.active[1].audio_backup
            );
            assert_eq!(
                manifest.active[0].audio_original_sha256.as_deref(),
                Some(hash_bytes(&original_audio).as_str())
            );
            // Conflicting choices must leave both files and records untouched.
            let saved_manifest = fs::read(manifest_path(&conf)).unwrap();
            let blue_applied = fs::read(cooked.join("Boost_Standard_Blue_SF.upk")).unwrap();
            for choice in [
                SoundChoice::Original,
                SoundChoice::Bank("SFX_Boost_Standard.bnk".into()),
            ] {
                assert!(
                    apply_at(
                        &cooked,
                        &conf,
                        &packages,
                        "boost_alphadevreward",
                        "Boost_Standard_Blue",
                        &choice
                    )
                    .unwrap_err()
                    .to_lowercase()
                    .contains("restore that swap first")
                );
                assert_eq!(fs::read(manifest_path(&conf)).unwrap(), saved_manifest);
                assert_eq!(
                    fs::read(cooked.join("Boost_Standard_Blue_SF.upk")).unwrap(),
                    blue_applied
                );
                assert_eq!(
                    fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
                    shared_audio
                );
            }
            if !legacy {
                // After verification resets the bank, reapply updates every owner.
                fs::write(cooked.join("SFX_Boost_Standard.bnk"), &original_audio).unwrap();
                apply_at(
                    &cooked,
                    &conf,
                    &packages,
                    "boost_alphadevreward",
                    "Boost_Standard_Blue",
                    &SoundChoice::MatchAppearance,
                )
                .unwrap();
                let manifest = load_manifest(&conf).unwrap();
                for record in &manifest.active {
                    assert_eq!(
                        record.audio_applied_sha256.as_deref(),
                        Some(hash_bytes(&expected_audio).as_str())
                    );
                }
            }
            let (first, last) = if legacy {
                ("Boost_Standard_Blue", "Boost_Standard")
            } else {
                ("Boost_Standard", "Boost_Standard_Blue")
            };
            assert!(
                restore_at(&cooked, &conf, first)
                    .unwrap()
                    .contains("shared sound remains")
            );
            assert_eq!(
                fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
                shared_audio
            );
            assert_eq!(load_manifest(&conf).unwrap().active.len(), 1);
            restore_at(&cooked, &conf, last).unwrap();
            assert_eq!(
                fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
                original_audio
            );
            assert_eq!(
                fs::read(cooked.join("Boost_Standard_SF.upk")).unwrap(),
                original_visual
            );
            assert_eq!(
                fs::read(cooked.join("Boost_Standard_Blue_SF.upk")).unwrap(),
                blue_before
            );
            assert!(load_manifest(&conf).unwrap().active.is_empty());
        }
        // Selecting the original bank as a sound source reads its pristine backup.
        apply_at(
            &cooked,
            &conf,
            &packages,
            "boost_alphadevreward",
            "Boost_Standard",
            &SoundChoice::Bank("SFX_Boost_Standard.bnk".into()),
        )
        .unwrap();
        assert_eq!(
            fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
            original_audio
        );
        // Reapply uses the stored selection; switch to Alpha and then Original.
        apply_at(
            &cooked,
            &conf,
            &packages,
            "boost_alphadevreward",
            "Boost_Standard",
            &SoundChoice::Bank("SFX_Boost_Alpha.bnk".into()),
        )
        .unwrap();
        let record = load_manifest(&conf).unwrap().active.remove(0);
        apply_at(
            &cooked,
            &conf,
            &packages,
            &record.donor_package,
            &record.target_package,
            &record.sound_choice(),
        )
        .unwrap();
        assert_eq!(
            fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
            expected_audio
        );
        apply_at(
            &cooked,
            &conf,
            &packages,
            "boost_alphadevreward",
            "Boost_Standard",
            &SoundChoice::Original,
        )
        .unwrap();
        assert_eq!(
            fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
            original_audio
        );
        assert!(
            load_manifest(&conf).unwrap().active[0]
                .audio_file()
                .is_none()
        );
        restore_at(&cooked, &conf, "Boost_Standard").unwrap();
        assert_eq!(
            fs::read(cooked.join("Boost_Standard_SF.upk")).unwrap(),
            original_visual
        );
        assert_eq!(
            fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
            original_audio
        );
        assert!(load_manifest(&conf).unwrap().active.is_empty());
        // Sound-only swaps also restore without changing the original appearance.
        apply_at(
            &cooked,
            &conf,
            &packages,
            "Boost_Standard",
            "Boost_Standard",
            &SoundChoice::Bank("SFX_Boost_Alpha.bnk".into()),
        )
        .unwrap();
        restore_at(&cooked, &conf, "Boost_Standard").unwrap();
        assert_eq!(
            fs::read(cooked.join("Boost_Standard_SF.upk")).unwrap(),
            original_visual
        );
        assert_eq!(
            fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
            original_audio
        );
        // A game update must replace the old pristine backups, never restore
        // pre-update sound or use a pre-update package as the next donor.
        apply_at(
            &cooked,
            &conf,
            &packages,
            "boost_alphadevreward",
            "Boost_Standard",
            &SoundChoice::MatchAppearance,
        )
        .unwrap();
        let mut updated_visual = original_visual.clone();
        let guid = crate::upk_swap::package_guid(&original_visual).unwrap();
        let guid_offset = updated_visual.windows(16).position(|v| v == guid).unwrap();
        updated_visual[guid_offset] ^= 0x80;
        let mut updated_audio = original_audio.clone();
        updated_audio.extend_from_slice(b"JUNK\0\0\0\0");
        fs::write(cooked.join("Boost_Standard_SF.upk"), &updated_visual).unwrap();
        fs::write(cooked.join("SFX_Boost_Standard.bnk"), &updated_audio).unwrap();
        assert_eq!(
            pristine_path(&cooked, &load_manifest(&conf).unwrap(), "Boost_Standard").unwrap(),
            cooked.join("Boost_Standard_SF.upk")
        );
        apply_at(
            &cooked,
            &conf,
            &packages,
            "boost_alphadevreward",
            "Boost_Standard",
            &SoundChoice::MatchAppearance,
        )
        .unwrap();
        restore_at(&cooked, &conf, "Boost_Standard").unwrap();
        assert_eq!(
            fs::read(cooked.join("Boost_Standard_SF.upk")).unwrap(),
            updated_visual
        );
        assert_eq!(
            fs::read(cooked.join("SFX_Boost_Standard.bnk")).unwrap(),
            updated_audio
        );
    }
    #[test]
    #[ignore = "requires RL_INSTALL and RL_KEY_INDEX; writes only temporary copies"]
    fn engine_audio_round_trip_on_copies() {
        let root = tempfile::tempdir().unwrap();
        let install = root.path().join("game");
        let cooked = install.join("TAGame/CookedPCConsole");
        fs::create_dir_all(&cooked).unwrap();
        let real_cooked = cooked_dir(&std::env::var("RL_INSTALL").unwrap()).unwrap();
        let source = "EngineAudio_HumanVoice";
        let target = "EngineAudio_Car01_OE";
        let unsupported = "engineaudio_twinzer";
        for id in [source, target, unsupported] {
            fs::copy(package_path(&real_cooked, id), package_path(&cooked, id)).unwrap();
        }
        let source_original = fs::read(package_path(&cooked, source)).unwrap();
        let target_original = fs::read(package_path(&cooked, target)).unwrap();
        let csv = fs::read_to_string(std::env::var("RL_KEY_INDEX").unwrap()).unwrap();
        let packages = build_catalog(&csv, install.to_str().unwrap()).unwrap();
        assert_eq!(packages.len(), 3);
        assert!(packages.iter().all(|p| p.slot == ItemSlot::EngineAudio));
        assert!(
            packages
                .iter()
                .find(|p| p.package == unsupported)
                .unwrap()
                .unavailable
                .is_some()
        );
        let conf = root.path().join("conf");
        assert!(
            apply_at(
                &cooked,
                &conf,
                &packages,
                unsupported,
                target,
                &SoundChoice::Original
            )
            .unwrap_err()
            .contains("unavailable")
        );
        assert_eq!(
            fs::read(package_path(&cooked, target)).unwrap(),
            target_original
        );
        let message = apply_at(
            &cooked,
            &conf,
            &packages,
            source,
            target,
            &SoundChoice::Original,
        )
        .unwrap();
        assert!(message.contains("engine sound"));
        let target_key = crate::upk_swap::key_from_base64(
            &packages.iter().find(|p| p.package == target).unwrap().key,
        )
        .unwrap();
        let source_key = crate::upk_swap::key_from_base64(
            &packages.iter().find(|p| p.package == source).unwrap().key,
        )
        .unwrap();
        let applied = fs::read(package_path(&cooked, target)).unwrap();
        assert_ne!(applied, target_original);
        crate::upk_swap::validate_identity(&applied, &target_key, target).unwrap();
        assert_eq!(
            fs::read(package_path(&cooked, source)).unwrap(),
            source_original
        );
        let record = load_manifest(&conf).unwrap().active.remove(0);
        assert!(record.is_engine_audio());
        assert!(record.audio_file().is_none());
        apply_at(
            &cooked,
            &conf,
            &packages,
            &record.donor_package,
            &record.target_package,
            &record.sound_choice(),
        )
        .unwrap();
        assert_eq!(fs::read(package_path(&cooked, target)).unwrap(), applied);
        // An engine already used as a target must still donate its pristine profile.
        apply_at(
            &cooked,
            &conf,
            &packages,
            target,
            source,
            &SoundChoice::Original,
        )
        .unwrap();
        let expected = crate::upk_swap::masquerade(
            &target_original,
            &source_original,
            &target_key,
            &source_key,
            target,
            source,
        )
        .unwrap();
        assert_eq!(fs::read(package_path(&cooked, source)).unwrap(), expected);
        for id in [target, source] {
            assert!(
                restore_at(&cooked, &conf, id)
                    .unwrap()
                    .contains("original engine sound")
            );
        }
        assert_eq!(
            fs::read(package_path(&cooked, target)).unwrap(),
            target_original
        );
        assert_eq!(
            fs::read(package_path(&cooked, source)).unwrap(),
            source_original
        );
        assert!(load_manifest(&conf).unwrap().active.is_empty());
        // Engine profile swaps need no bank rewrites or additional game files.
        assert_eq!(fs::read_dir(&cooked).unwrap().count(), 3);
    }

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
