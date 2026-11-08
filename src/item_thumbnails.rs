//! Selected-item inventory previews. File I/O and decoding stay on one bounded worker.
use crate::item_swapper::{CatalogPackage, ItemSlot};
use crate::upk_swap::thumbnail::{self, MAX_PACKAGE_BYTES};
use eframe::egui;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::UNIX_EPOCH;

const DECODER_VERSION: u32 = 1;
const MEMORY_ENTRIES: usize = 64;
const DISK_ENTRIES: usize = 128;
const MAX_PNG_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub(crate) enum PreviewStatus {
    Queued,
    Ready(Arc<egui::ColorImage>),
    Unavailable(String),
}

pub(crate) struct Preview {
    pub id: String,
    pub status: PreviewStatus,
}
struct Entry {
    status: PreviewStatus,
    used: u64,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<String, Entry>,
    clock: u64,
}

#[derive(Default)]
pub(crate) struct ThumbnailState {
    cache: Arc<Mutex<Cache>>,
    worker: Mutex<Option<mpsc::SyncSender<Job>>>,
    generation: AtomicU64,
}
struct Job {
    id: String,
    install: PathBuf,
    package: CatalogPackage,
    cache_dir: PathBuf,
    context: egui::Context,
}

impl ThumbnailState {
    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .clear();
    }

    pub fn request(
        &self,
        install: &str,
        package: &CatalogPackage,
        cache_dir: &Path,
        context: &egui::Context,
    ) -> Preview {
        let mut digest = Sha256::new();
        for value in [install, &package.package, &package.key] {
            digest.update(value.as_bytes());
            digest.update([0]);
        }
        digest.update(self.generation.load(Ordering::Relaxed).to_le_bytes());
        let id = format!("{:x}", digest.finalize());
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.clock = cache.clock.wrapping_add(1);
            let used = cache.clock;
            if let Some(entry) = cache.entries.get_mut(&id) {
                entry.used = used;
                return Preview {
                    id,
                    status: entry.status.clone(),
                };
            }
            while cache.entries.len() >= MEMORY_ENTRIES {
                let oldest = cache
                    .entries
                    .iter()
                    .filter(|(_, entry)| !matches!(entry.status, PreviewStatus::Queued))
                    .min_by_key(|(_, entry)| entry.used)
                    .map(|(key, _)| key.clone());
                if let Some(oldest) = oldest {
                    cache.entries.remove(&oldest);
                } else {
                    break;
                }
            }
            cache.entries.insert(
                id.clone(),
                Entry {
                    status: PreviewStatus::Queued,
                    used,
                },
            );
        }
        let mut worker = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        if worker.is_none() {
            let (sender, receiver) = mpsc::sync_channel::<Job>(4);
            let cache = self.cache.clone();
            let spawned = std::thread::Builder::new()
                .name("item-thumbnail-reader".into())
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        if !cache
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .entries
                            .contains_key(&job.id)
                        {
                            continue;
                        }
                        let status = match load(&job.install, &job.package, &job.cache_dir) {
                            Ok(image) => PreviewStatus::Ready(Arc::new(
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [image.width() as usize, image.height() as usize],
                                    image.as_raw(),
                                ),
                            )),
                            Err(error) => PreviewStatus::Unavailable(error),
                        };
                        if let Some(entry) = cache
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .entries
                            .get_mut(&job.id)
                        {
                            entry.status = status;
                        }
                        job.context.request_repaint();
                    }
                });
            match spawned {
                Ok(_) => *worker = Some(sender),
                Err(_) => {
                    let status =
                        PreviewStatus::Unavailable("Could not start preview reader".into());
                    if let Some(entry) = self
                        .cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .entries
                        .get_mut(&id)
                    {
                        entry.status = status.clone();
                    }
                    return Preview { id, status };
                }
            }
        }
        let job = Job {
            id: id.clone(),
            install: PathBuf::from(install),
            package: package.clone(),
            cache_dir: cache_dir.to_path_buf(),
            context: context.clone(),
        };
        if let Err(error) = worker.as_ref().unwrap().try_send(job) {
            self.cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entries
                .remove(&id);
            if matches!(error, mpsc::TrySendError::Disconnected(_)) {
                *worker = None;
            }
        }
        Preview {
            id,
            status: PreviewStatus::Queued,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Dependency {
    name: String,
    size: u64,
    modified_ns: u128,
}
impl Dependency {
    fn read(path: &Path) -> Option<Self> {
        let metadata = path.metadata().ok()?;
        Some(Self {
            name: path.file_name()?.to_str()?.into(),
            size: metadata.len(),
            modified_ns: metadata
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_nanos(),
        })
    }
    fn valid(&self, directory: &Path) -> bool {
        if !self.name.ends_with(".tfc")
            || self.name.contains(['/', '\\', ':'])
            || self.name.contains("..")
        {
            return false;
        }
        Self::read(&directory.join(&self.name)).is_some_and(|current| {
            current.size == self.size && current.modified_ns == self.modified_ns
        })
    }
}
#[derive(Deserialize, Serialize)]
struct DiskMetadata {
    version: u32,
    dependencies: Vec<Dependency>,
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path)
        .map_err(|_| "No inventory thumbnail is installed for this item".to_string())?;
    if file
        .metadata()
        .map_err(|_| "Could not inspect inventory thumbnail")?
        .len()
        > limit as u64
    {
        return Err("Inventory preview file is too large".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read inventory thumbnail")?;
    if bytes.len() > limit {
        return Err("Inventory preview file grew beyond the size limit".into());
    }
    Ok(bytes)
}
fn cached_image(bytes: &[u8]) -> Option<image::RgbaImage> {
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(256);
    limits.max_image_height = Some(256);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().ok().map(|image| image.to_rgba8())
}
fn load(
    install: &Path,
    package: &CatalogPackage,
    cache_dir: &Path,
) -> Result<image::RgbaImage, String> {
    if package.slot == ItemSlot::EngineAudio {
        return Err("Engine sounds do not have appearance previews".into());
    }
    if package.package.is_empty()
        || package.package.len() > 200
        || !package
            .package
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err("Invalid inventory package name".into());
    }
    let directory = install.join("TAGame/CookedPCConsole");
    let asset = format!("{}_T", package.package);
    let bytes = read_bounded(
        &directory.join(format!("{asset}_SF.upk")),
        MAX_PACKAGE_BYTES,
    )?;
    let key = crate::upk_swap::key_from_base64(&package.key)?;
    let mut digest = Sha256::new();
    digest.update(DECODER_VERSION.to_le_bytes());
    digest.update(&bytes);
    digest.update(key);
    digest.update(asset.as_bytes());
    let identity = format!("{:x}", digest.finalize());
    let png = cache_dir.join(format!("{identity}.png"));
    let metadata_path = cache_dir.join(format!("{identity}.json"));
    if let Ok(metadata) = read_bounded(&metadata_path, 16 * 1024)
        && let Ok(metadata) = serde_json::from_slice::<DiskMetadata>(&metadata)
        && metadata.version == DECODER_VERSION
        && metadata.dependencies.len() <= 16
        && metadata
            .dependencies
            .iter()
            .all(|entry| entry.valid(&directory))
        && let Ok(bytes) = read_bounded(&png, MAX_PNG_BYTES)
        && let Some(image) = cached_image(&bytes)
    {
        return Ok(image);
    }
    let decoded = thumbnail::decode(&bytes, &key, &directory, &asset)?;
    let dependencies = decoded
        .dependencies
        .iter()
        .map(|path| Dependency::read(path))
        .collect::<Option<Vec<_>>>();
    // Cache failures never make an otherwise valid preview unavailable.
    if let Some(dependencies) = dependencies {
        let metadata = DiskMetadata {
            version: DECODER_VERSION,
            dependencies,
        };
        let mut encoded = Cursor::new(Vec::new());
        if image::DynamicImage::ImageRgba8(decoded.image.clone())
            .write_to(&mut encoded, image::ImageFormat::Png)
            .is_ok()
            && let Ok(metadata) = serde_json::to_vec(&metadata)
            && std::fs::create_dir_all(cache_dir).is_ok()
            && write_cache(&png, encoded.get_ref())
            && write_cache(&metadata_path, &metadata)
        {
            prune(cache_dir);
        }
    }
    Ok(decoded.image)
}
fn write_cache(path: &Path, bytes: &[u8]) -> bool {
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    if std::fs::write(&temporary, bytes).is_ok() && std::fs::rename(&temporary, path).is_ok() {
        return true;
    }
    let _ = std::fs::remove_file(temporary);
    false
}
fn prune(directory: &Path) {
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "png")
        })
        .filter(|entry| {
            entry.path().file_stem().is_some_and(|name| {
                name.len() == 64
                    && name
                        .to_string_lossy()
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit())
            })
        })
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    files.sort_by_key(|(modified, _)| *modified);
    let remove = files.len().saturating_sub(DISK_ENTRIES);
    for (_, path) in files.into_iter().take(remove) {
        let _ = std::fs::remove_file(path.with_extension("json"));
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires RL_INSTALL and RL_KEY_INDEX; modifies only temporary copies"]
    fn installed_preview_cache_recovers_from_bad_png_and_rejects_changed_source() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("TAGame/CookedPCConsole");
        std::fs::create_dir_all(&directory).unwrap();
        let name = "Boost_Standard_T_SF.upk";
        let real =
            PathBuf::from(std::env::var("RL_INSTALL").unwrap()).join("TAGame/CookedPCConsole");
        std::fs::copy(real.join(name), directory.join(name)).unwrap();
        let csv = std::fs::read_to_string(std::env::var("RL_KEY_INDEX").unwrap()).unwrap();
        let key = csv
            .lines()
            .skip(1)
            .find_map(|line| {
                let columns: Vec<_> = line.split(',').collect();
                (columns.len() >= 9 && columns[7].trim_matches('"') == "Boost_Standard")
                    .then(|| columns[8].trim_matches('"').to_string())
            })
            .unwrap();
        let package = CatalogPackage {
            package: "Boost_Standard".into(),
            labels: vec!["Standard".into()],
            slot: ItemSlot::RocketBoost,
            key,
            unavailable: None,
        };
        let cache = root.path().join("preview-cache");
        let first = load(root.path(), &package, &cache).unwrap();
        let png = std::fs::read_dir(&cache)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|ext| ext == "png"))
            .unwrap();
        let cached_at = png.metadata().unwrap().modified().unwrap();
        assert_eq!(load(root.path(), &package, &cache).unwrap(), first);
        assert_eq!(
            png.metadata().unwrap().modified().unwrap(),
            cached_at,
            "cache hit should not rewrite the image"
        );
        std::fs::write(&png, b"corrupt cache").unwrap();
        assert_eq!(load(root.path(), &package, &cache).unwrap(), first);
        assert_eq!(cached_image(&std::fs::read(&png).unwrap()).unwrap(), first);
        let mut changed = std::fs::read(directory.join(name)).unwrap();
        changed[0] ^= 1;
        std::fs::write(directory.join(name), changed).unwrap();
        assert!(
            load(root.path(), &package, &cache).is_err(),
            "a stale PNG must not hide invalid new source data"
        );
    }
    #[test]
    fn cached_images_are_bounded_and_external_files_are_revalidated() {
        let root = tempfile::tempdir().unwrap();
        let tfc = root.path().join("Textures.tfc");
        std::fs::write(&tfc, b"first").unwrap();
        let dependency = Dependency::read(&tfc).unwrap();
        assert!(dependency.valid(root.path()));
        std::fs::write(&tfc, b"updated pixels").unwrap();
        assert!(!dependency.valid(root.path()));
        let image = image::RgbaImage::new(257, 1);
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        assert!(cached_image(bytes.get_ref()).is_none());
        assert!(cached_image(b"not an image").is_none());
    }
    #[test]
    fn worker_caches_missing_previews_and_refresh_allows_retry() {
        let root = tempfile::tempdir().unwrap();
        let state = ThumbnailState::default();
        let context = egui::Context::default();
        let package = CatalogPackage {
            package: "MissingItem".into(),
            labels: vec!["Missing".into()],
            slot: ItemSlot::RocketBoost,
            key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
            unavailable: None,
        };
        let first = state.request(
            root.path().to_str().unwrap(),
            &package,
            root.path(),
            &context,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let result = state.request(
                root.path().to_str().unwrap(),
                &package,
                root.path(),
                &context,
            );
            assert_eq!(first.id, result.id);
            if matches!(result.status, PreviewStatus::Unavailable(_)) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "preview worker did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(state.cache.lock().unwrap().entries.len(), 1);
        state.invalidate();
        let retry = state.request(
            root.path().to_str().unwrap(),
            &package,
            root.path(),
            &context,
        );
        assert_ne!(first.id, retry.id);
    }
    #[test]
    fn pruning_removes_only_our_old_cache_entries() {
        let root = tempfile::tempdir().unwrap();
        for i in 0..DISK_ENTRIES + 2 {
            let name = format!("{i:064x}");
            std::fs::write(root.path().join(format!("{name}.png")), []).unwrap();
            std::fs::write(root.path().join(format!("{name}.json")), []).unwrap();
        }
        let other = root.path().join("user-image.png");
        std::fs::write(&other, b"keep").unwrap();
        prune(root.path());
        assert!(other.exists());
        assert_eq!(
            std::fs::read_dir(root.path()).unwrap().count(),
            DISK_ENTRIES * 2 + 1
        );
    }
}
