//! Bounded discovery of game installations and user data. No game process access.
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GamePaths {
    pub installations: Vec<PathBuf>,
    pub data_dirs: Vec<PathBuf>,
    pub replay_dirs: Vec<PathBuf>,
}

fn push_existing(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_dir() {
        let path = path.canonicalize().unwrap_or(path);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
}

fn children(path: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<_> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .take(256)
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    entries.sort();
    entries
}

fn read_small(path: &Path) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > 1024 * 1024 {
        return None;
    }
    let mut text = String::new();
    file.take(1024 * 1024).read_to_string(&mut text).ok()?;
    Some(text)
}

// libraryfolders.vdf uses quoted strings, including escaped Windows separators.
fn library_paths(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("\"path\"")?.trim();
            let value = rest.strip_prefix('"')?.split('"').next()?;
            let path = PathBuf::from(value.replace("\\\\", "\\"));
            path.is_absolute().then_some(path)
        })
        .collect()
}

impl GamePaths {
    pub fn discover() -> Self {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .unwrap_or_default();
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let custom = std::env::var_os("RL_OVERLAY_WINE_PREFIX").map(PathBuf::from);
        Self::discover_at(&home, &config, custom.as_deref())
    }

    fn discover_at(home: &Path, config: &Path, custom_prefix: Option<&Path>) -> Self {
        let mut found = Self::default();
        let mut steamapps = Vec::new();
        for steam in [
            home.join(".local/share/Steam"),
            home.join(".steam/steam"),
            home.join(".steam/root"),
            home.join(".var/app/com.valvesoftware.Steam/data/Steam"),
            PathBuf::from("C:/Program Files (x86)/Steam"),
        ] {
            push_existing(&mut steamapps, steam.join("steamapps"));
            if let Some(vdf) = read_small(&steam.join("steamapps/libraryfolders.vdf")) {
                for library in library_paths(&vdf) {
                    push_existing(&mut steamapps, library.join("steamapps"));
                }
            }
        }
        for library in &steamapps {
            found.add_install(library.join("common/rocketleague"));
        }
        found.add_install(PathBuf::from("C:/Program Files/Epic Games/rocketleague"));
        found.add_install(home.join("Games/Heroic/rocketleague"));
        found.add_documents(home.join("Documents"));
        found.add_documents(home.join("OneDrive/Documents"));
        for key in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
            if cfg!(windows)
                && let Some(root) = std::env::var_os(key)
            {
                found.add_documents(PathBuf::from(root).join("Documents"));
            }
        }
        let mut prefixes = Vec::new();
        if let Some(prefix) = custom_prefix {
            push_existing(&mut prefixes, prefix.to_path_buf());
        }
        for library in &steamapps {
            for prefix in children(&library.join("compatdata")) {
                push_existing(&mut prefixes, prefix.join("pfx"));
            }
        }
        for root in [
            home.join("Games/Heroic/Prefixes"),
            home.join("Games/Heroic/Prefixes/default"),
        ] {
            for prefix in children(&root) {
                push_existing(&mut prefixes, prefix.clone());
                push_existing(&mut prefixes, prefix.join("pfx"));
            }
        }
        for heroic in [
            config.join("heroic/GamesConfig"),
            home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic/GamesConfig"),
        ] {
            for entry in std::fs::read_dir(heroic)
                .into_iter()
                .flatten()
                .take(256)
                .flatten()
            {
                if entry.path().extension().is_some_and(|ext| ext == "json")
                    && let Some(text) = read_small(&entry.path())
                    && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
                {
                    // Heroic stores per-game launch settings under the app ID.
                    for settings in value.as_object().into_iter().flat_map(|map| map.values()) {
                        if let Some(prefix) = settings.get("winePrefix").and_then(|v| v.as_str()) {
                            let prefix = PathBuf::from(prefix);
                            if prefix.is_absolute() {
                                push_existing(&mut prefixes, prefix);
                            }
                        }
                    }
                }
            }
        }
        for prefix in prefixes {
            for user in children(&prefix.join("drive_c/users")) {
                found.add_documents(user.join("Documents"));
                found.add_documents(user.join("My Documents"));
            }
            found.add_install(prefix.join("drive_c/Program Files/Epic Games/rocketleague"));
        }
        found.installations.sort();
        found.data_dirs.sort();
        found.replay_dirs.sort();
        found
    }

    fn add_install(&mut self, path: PathBuf) {
        if path.join("TAGame/CookedPCConsole").is_dir() {
            push_existing(&mut self.installations, path);
        }
    }

    fn add_documents(&mut self, documents: PathBuf) {
        let data = documents.join("My Games/Rocket League/TAGame");
        push_existing(&mut self.data_dirs, data.clone());
        for name in ["Demos", "DemosEpic"] {
            push_existing(&mut self.replay_dirs, data.join(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_secondary_library_arbitrary_proton_id_and_custom_prefix() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let steam = home.join(".local/share/Steam/steamapps");
        std::fs::create_dir_all(&steam).unwrap();
        let library = root.path().join("other drive");
        let data = library.join("steamapps/compatdata/39283712/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame");
        std::fs::create_dir_all(data.join("DemosEpic")).unwrap();
        std::fs::create_dir_all(
            library.join("steamapps/common/rocketleague/TAGame/CookedPCConsole"),
        )
        .unwrap();
        std::fs::write(
            steam.join("libraryfolders.vdf"),
            format!("\"path\" \"{}\"", library.display()),
        )
        .unwrap();
        let custom = root.path().join("bottles");
        let extra = custom.join("drive_c/users/other/Documents/My Games/Rocket League/TAGame");
        std::fs::create_dir_all(extra.join("Demos")).unwrap();
        let found = GamePaths::discover_at(&home, &home.join(".config"), Some(&custom));
        assert_eq!(found.installations.len(), 1);
        assert!(found.data_dirs.contains(&data.canonicalize().unwrap()));
        assert!(found.data_dirs.contains(&extra.canonicalize().unwrap()));
        assert_eq!(found.replay_dirs.len(), 2);
    }

    #[test]
    fn discovers_heroic_config_and_deduplicates_document_aliases() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let config = home.join(".config");
        let heroic = config.join("heroic/GamesConfig");
        std::fs::create_dir_all(&heroic).unwrap();
        let prefix = home.join("my-prefix");
        let data = prefix.join("drive_c/users/me/Documents/My Games/Rocket League/TAGame");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            heroic.join("Sugar.json"),
            serde_json::json!({"Sugar": {"winePrefix": prefix}}).to_string(),
        )
        .unwrap();
        let found = GamePaths::discover_at(home, &config, Some(&prefix));
        assert_eq!(found.data_dirs, vec![data.canonicalize().unwrap()]);
    }
}
