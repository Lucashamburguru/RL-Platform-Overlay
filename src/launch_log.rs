//! Read-only, incremental Launch.log metadata. Live telemetry remains Stats API owned.
use crate::state::{AppState, LocalPlayerIdentity};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const READ_BUDGET: u64 = 256 * 1024;
const MAX_LINE: usize = 32 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapContext {
    pub generation: u64,
    pub arena: String,
    pub playlist: Option<i64>,
    pub offline: bool,
    pub replay: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LogSnapshot {
    pub status: String,
    pub path: Option<PathBuf>,
    pub identity: Option<LocalPlayerIdentity>,
    pub map: Option<MapContext>,
    pub checked_at: Option<Instant>,
}

impl LogSnapshot {
    pub fn is_current(&self) -> bool {
        self.checked_at
            .is_some_and(|at| at.elapsed() < Duration::from_secs(10))
    }
}

#[derive(Default)]
struct Parser {
    epic: Option<String>,
    steam: Option<String>,
    name: Option<String>,
    pending_playlist: Option<i64>,
    map: Option<MapContext>,
    generation: u64,
}

fn quoted<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.split_once(marker)?
        .1
        .split_once('\'')
        .map(|(value, _)| value)
}

fn argument<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    // Only command-line records are callers; match argument boundaries so an
    // unrelated field cannot contain a fake -epicuserid suffix.
    for (position, _) in line.to_ascii_lowercase().match_indices(key) {
        if position > 0 && !line.as_bytes()[position - 1].is_ascii_whitespace() {
            continue;
        }
        let rest = &line[position + key.len()..];
        return if let Some(rest) = rest.strip_prefix('"') {
            rest.split_once('"').map(|(value, _)| value)
        } else {
            rest.split_whitespace().next()
        };
    }
    None
}

fn epic_id(value: &str) -> Option<String> {
    (value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

impl Parser {
    fn line(&mut self, line: &str) {
        let lower = line.to_ascii_lowercase();
        if lower.contains("command line:") || lower.contains("commandline:") {
            if let Some(id) = argument(line, "-epicuserid=").and_then(epic_id) {
                self.epic = Some(id);
            }
            if let Some(name) = argument(line, "-epicusername=") {
                self.name = Some(name.to_string());
            }
        }
        if let Some(name) = quoted(line, "DevOnline: Logged in as '") {
            self.name = Some(name.to_string());
        }
        if let Some((_, rest)) = line.split_once("DevOnline: Steam ID: ") {
            let id = rest.split_whitespace().next().unwrap_or_default();
            if id.len() == 17 && id.bytes().all(|byte| byte.is_ascii_digit()) {
                self.steam = Some(id.to_string());
            }
        }
        if let Some(id) =
            quoted(line, "Connect login result 'EOS_Success' for player '").and_then(epic_id)
        {
            self.epic = Some(id);
        }
        if let Some((_, rest)) = line.split_once("LoadMap: ") {
            self.generation = self.generation.wrapping_add(1);
            let mut parts = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .split(['?', '&']);
            let arena = normalize_arena(parts.next().unwrap_or_default());
            let options: Vec<_> = parts.collect();
            let offline = options
                .iter()
                .any(|part| part.eq_ignore_ascii_case("Offline"));
            let replay = options
                .iter()
                .any(|part| part.to_ascii_lowercase().contains("gameinfo_replay"));
            let menu = arena.to_ascii_lowercase().starts_with("menu_");
            let playlist = if offline || replay || menu {
                None
            } else {
                self.pending_playlist
            };
            self.pending_playlist = None;
            self.map = (!menu && !arena.is_empty()).then_some(MapContext {
                generation: self.generation,
                arena,
                playlist,
                offline,
                replay,
            });
        } else {
            for key in ["PlaylistId=", "Playlist="] {
                if let Some((_, rest)) = line.split_once(key) {
                    let digits: String = rest
                        .trim_start_matches('(')
                        .chars()
                        .take_while(char::is_ascii_digit)
                        .collect();
                    if let Ok(id) = digits.parse::<i64>() {
                        self.pending_playlist = Some(id);
                    }
                }
            }
        }
    }

    fn identity(&self) -> Option<LocalPlayerIdentity> {
        // Steam logins can also initialize an EOS account. Prefer their Steam
        // identity, which is the platform identifier emitted by Stats API.
        let (platform, id) = self
            .steam
            .as_ref()
            .map(|id| ("Steam", id))
            .or_else(|| self.epic.as_ref().map(|id| ("Epic", id)))?;
        let name = self
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty() && !name.eq_ignore_ascii_case("player"))
            .map(str::to_string)
            .unwrap_or_else(|| format!("{platform} account {}", &id[..8]));
        Some(LocalPlayerIdentity {
            name,
            platform: platform.into(),
            primary_id: format!("{platform}|{id}|0"),
        })
    }
}

pub(crate) fn normalize_arena(value: &str) -> String {
    value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .trim_end_matches(".upk")
        .trim_end_matches(".UPK")
        .to_ascii_lowercase()
}

#[derive(Default)]
struct LogReader {
    parser: Parser,
    path: Option<PathBuf>,
    process: Option<(u32, u64)>,
    offset: u64,
    prefix: Vec<u8>,
    partial: Vec<u8>,
    dropping_line: bool,
    modified: Option<SystemTime>,
}

impl LogReader {
    fn reset(&mut self) {
        let generation = self.parser.generation.wrapping_add(1);
        *self = Self::default();
        self.parser.generation = generation;
    }

    fn read(&mut self, path: &Path, process: (u32, u64)) -> std::io::Result<bool> {
        let mut file = std::fs::File::open(path)?;
        let metadata = file.metadata()?;
        let modified = metadata.modified()?;
        let mut prefix = vec![0; metadata.len().min(128) as usize];
        file.read_exact(&mut prefix)?;
        let replaced = !self.prefix.is_empty() && !prefix.starts_with(&self.prefix);
        if self.path.as_deref() != Some(path)
            || self.process != Some(process)
            || metadata.len() < self.offset
            || replaced
            || (metadata.len() == self.offset && self.modified.is_some_and(|old| old != modified))
        {
            self.reset();
            self.path = Some(path.to_path_buf());
            self.process = Some(process);
        }
        self.prefix = prefix;
        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::new();
        file.take(READ_BUDGET).read_to_end(&mut bytes)?;
        self.offset += bytes.len() as u64;
        self.modified = Some(modified);
        for byte in bytes {
            if byte == b'\n' {
                if !self.dropping_line {
                    self.parser
                        .line(String::from_utf8_lossy(&self.partial).trim_end_matches('\r'));
                }
                self.partial.clear();
                self.dropping_line = false;
            } else if !self.dropping_line {
                if self.partial.len() < MAX_LINE {
                    self.partial.push(byte);
                } else {
                    self.partial.clear();
                    self.dropping_line = true;
                }
            }
        }
        // Do not publish intermediate history while catching up to a large log.
        Ok(self.offset >= metadata.len())
    }
}

fn fresh_log(path: &Path, process_start: u64) -> bool {
    std::fs::metadata(path)
        .ok()
        .filter(|m| m.is_file())
        .and_then(|m| m.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .is_some_and(|time| time.as_secs() >= process_start)
}

fn candidates(
    paths: &crate::game_paths::GamePaths,
    replay_folder: &str,
    explicit: &str,
    process_start: u64,
) -> Vec<PathBuf> {
    let mut logs = Vec::new();
    let mut add = |path: PathBuf| {
        if fresh_log(&path, process_start) {
            let path = path.canonicalize().unwrap_or(path);
            if !logs.contains(&path) {
                logs.push(path);
            }
        }
    };
    if !explicit.trim().is_empty() {
        add(PathBuf::from(explicit));
        return logs;
    }
    if !replay_folder.trim().is_empty()
        && let Some(parent) = Path::new(replay_folder).parent()
    {
        add(parent.join("Logs/Launch.log"));
    }
    for data in &paths.data_dirs {
        add(data.join("Logs/Launch.log"));
    }
    logs
}

pub fn start(state: &Arc<AppState>) {
    let weak = Arc::downgrade(state);
    let runtime = tokio::runtime::Handle::current();
    std::thread::Builder::new().name("launch-log-metadata".into()).spawn(move || {
        let _runtime = runtime.enter();
        let mut system = sysinfo::System::new();
        let mut reader = LogReader::default();
        let mut paths = crate::game_paths::GamePaths::default();
        let mut discovery_at = None::<Instant>;
        loop {
            let Some(state) = weak.upgrade() else { break };
            if state.flags.should_exit.load(Ordering::SeqCst) { break; }
            if discovery_at.is_none_or(|at| at.elapsed() >= Duration::from_secs(30)) {
                paths = crate::game_paths::GamePaths::discover();
                state.system.game_paths.store(Arc::new(paths.clone()));
                discovery_at = Some(Instant::now());
            }
            let config = state.system.config.load_full();
            let mut snapshot = LogSnapshot::default();
            if !config.launch_log_enabled {
                reader.reset();
                snapshot.status = "Log account and playlist detection is off.".into();
            } else {
                system.refresh_processes_specifics(
                    sysinfo::ProcessesToUpdate::All,
                    true,
                    sysinfo::ProcessRefreshKind::nothing().without_tasks()
                        .with_exe(sysinfo::UpdateKind::OnlyIfNotSet),
                );
                let games: Vec<_> = system.processes().iter()
                    .filter(|(_, process)| crate::assets::is_rocket_league_name(process.name())
                        || process.exe().and_then(|exe| exe.file_name()).is_some_and(crate::assets::is_rocket_league_name))
                    .map(|(pid, process)| (pid.as_u32(), process.start_time()))
                    .filter(|(_, start)| *start > 0)
                    .collect();
                if let [process] = games.as_slice() {
                    let logs = candidates(&paths, &config.replays_folder, &config.launch_log_path, process.1);
                    if let [path] = logs.as_slice() {
                        snapshot.path = Some(path.clone());
                        match reader.read(path, *process) {
                            Ok(true) => {
                                snapshot.identity = reader.parser.identity();
                                snapshot.map = reader.parser.map.clone();
                                snapshot.checked_at = Some(Instant::now());
                                snapshot.status = if snapshot.identity.is_some() {
                                    "Current game account detected from Launch.log."
                                } else { "Waiting for account login information in Launch.log." }.into();
                            }
                            Ok(false) => snapshot.status = "Reading current Launch.log…".into(),
                            Err(_) => { reader.reset(); snapshot.status = "Launch.log is unavailable; using Stats API detection.".into(); }
                        }
                    } else {
                        reader.reset();
                        snapshot.status = if logs.len() > 1 {
                            "Several current logs found. Select Launch.log below to enable detection."
                        } else { "Waiting for a Launch.log from the current game launch." }.into();
                    }
                } else {
                    reader.reset();
                    snapshot.status = if games.is_empty() { "Waiting for Rocket League to start." }
                        else { "Several game processes found; using Stats API detection." }.into();
                }
            }
            publish(&state, snapshot);
            drop(state);
            std::thread::sleep(Duration::from_secs(1));
        }
    }).expect("could not start Launch.log reader");
}

fn publish(state: &Arc<AppState>, snapshot: LogSnapshot) {
    if state.system.config.load().launch_log_enabled
        && snapshot.is_current()
        && !state.system.config.load().lock_local_player
        && let Some(identity) = &snapshot.identity
    {
        // An identified live Stats API player takes priority over the log.
        let live_local = state
            .game
            .players
            .load()
            .values()
            .find(|player| player.is_local)
            .map(|player| LocalPlayerIdentity {
                name: player.name.clone(),
                primary_id: player.primary_id.clone(),
                platform: player.platform.clone(),
            });
        let agrees = live_local
            .as_ref()
            .is_none_or(|live| live.same_account(identity));
        if agrees {
            let mut identity = identity.clone();
            if let Some(live) = live_local {
                identity.name = live.name;
            } else {
                let cached = state.game.local_player_identity.load();
                if cached.same_account(&identity)
                    && identity
                        .name
                        .starts_with(&format!("{} account ", identity.platform))
                {
                    identity.name = cached.name.clone();
                }
            }
            let changed = state.update_local_player_identity(identity);
            if changed {
                state.game.local_player_name.store(Arc::new(String::new()));
                state
                    .game
                    .local_team
                    .store(crate::state::NO_TEAM, Ordering::SeqCst);
                crate::mmr::start_local_mmr_refresh(state.clone());
            }
        }
    }
    let previous = state.system.launch_log.load();
    let changed = previous.status != snapshot.status
        || previous.path != snapshot.path
        || previous.identity != snapshot.identity
        || previous.map != snapshot.map;
    state.system.launch_log.store(Arc::new(snapshot));
    if changed {
        state.request_ui_repaint();
    }
}

/// One log map-load generation may corroborate only one Stats API match GUID.
#[derive(Default)]
pub struct MatchBinding {
    bound: Option<(u64, String)>,
}

impl MatchBinding {
    pub fn mode(
        &mut self,
        snapshot: &LogSnapshot,
        match_id: &str,
        arena: &str,
    ) -> Option<crate::session::SessionMode> {
        if !snapshot.is_current() || match_id.is_empty() || arena.is_empty() {
            return None;
        }
        let map = snapshot.map.as_ref()?;
        if map.offline || map.replay || map.arena != normalize_arena(arena) {
            return None;
        }
        if self.bound.as_ref().is_some_and(|(generation, id)| {
            (*generation == map.generation && id != match_id)
                || (*generation != map.generation && id == match_id)
        }) {
            return None;
        }
        let mode = crate::session::SessionMode::from_playlist_id(map.playlist?)?;
        self.bound = Some((map.generation, match_id.to_string()));
        Some(mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_account_switch_persists_identity_and_clears_previous_rank() {
        let state = AppState::new();
        state
            .game
            .local_player_identity
            .store(Arc::new(LocalPlayerIdentity {
                name: "Previous".into(),
                platform: "Steam".into(),
                primary_id: "Steam|76561198000000000|0".into(),
            }));
        state
            .mmr
            .local_mmr
            .store(Arc::new(crate::state::LocalMmrState {
                current: Some(Default::default()),
                previous: Some(Default::default()),
                ..Default::default()
            }));
        let mut parser = Parser::default();
        parser.line(
            "Log: Command line: -EpicUserId=0123456789abcdef0123456789abcdef -EpicUsername=Current",
        );
        publish(
            &state,
            LogSnapshot {
                checked_at: Some(Instant::now()),
                identity: parser.identity(),
                ..Default::default()
            },
        );
        assert_eq!(state.game.local_player_identity.load().name, "Current");
        assert_eq!(
            state.system.config.load().cached_local_player_identity.name,
            "Current"
        );
        assert!(state.mmr.local_mmr.load().current.is_none());
        assert!(state.mmr.local_mmr.load().previous.is_none());
        state.flush_config().unwrap();
        let (saved, _) = crate::state::Config::load_from_path(state.paths.config_path.clone());
        assert_eq!(saved.cached_local_player_identity.name, "Current");
    }

    #[test]
    fn candidates_ignore_old_launches_and_require_disambiguation() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        for data in [&first, &second] {
            std::fs::create_dir_all(data.join("Logs")).unwrap();
            let path = data.join("Logs/Launch.log");
            std::fs::write(&path, "test\n").unwrap();
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(
                    std::fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(30)),
                )
                .unwrap();
        }
        let paths = crate::game_paths::GamePaths {
            data_dirs: vec![first.clone(), second],
            ..Default::default()
        };
        assert!(candidates(&paths, "", "", 40).is_empty());
        assert_eq!(candidates(&paths, "", "", 20).len(), 2);
        let explicit = first.join("Logs/Launch.log");
        assert_eq!(
            candidates(&paths, "", explicit.to_str().unwrap(), 20),
            vec![explicit.canonicalize().unwrap()]
        );
        assert!(candidates(&paths, "", "/missing/Launch.log", 20).is_empty());
    }

    #[test]
    fn log_publication_respects_disabled_detection_and_account_lock() {
        let state = AppState::new();
        let saved = LocalPlayerIdentity {
            name: "Saved".into(),
            platform: "Steam".into(),
            primary_id: "Steam|76561198000000000|0".into(),
        };
        state
            .game
            .local_player_identity
            .store(Arc::new(saved.clone()));
        let snapshot = LogSnapshot {
            checked_at: Some(Instant::now()),
            identity: Some(LocalPlayerIdentity {
                name: "Other".into(),
                platform: "Epic".into(),
                primary_id: "Epic|0123456789abcdef0123456789abcdef|0".into(),
            }),
            ..Default::default()
        };
        state.update_config(|config| config.lock_local_player = true);
        publish(&state, snapshot.clone());
        assert_eq!(**state.game.local_player_identity.load(), saved);
        state.update_config(|config| {
            config.lock_local_player = false;
            config.launch_log_enabled = false;
        });
        publish(&state, snapshot);
        assert_eq!(**state.game.local_player_identity.load(), saved);
    }

    #[test]
    fn epic_and_steam_identity_preserve_platform_and_unicode_name() {
        let mut parser = Parser::default();
        parser.line("Log: Command line: -epicuserid=0123456789abcdef0123456789abcdef -epicusername=\"Player 日本\" -AUTH_PASSWORD=ignored");
        let identity = parser.identity().unwrap();
        assert_eq!(identity.name, "Player 日本");
        assert_eq!(
            identity.primary_id,
            "Epic|0123456789abcdef0123456789abcdef|0"
        );
        parser.line("DevOnline: Steam ID: 76561198000000000");
        assert_eq!(parser.identity().unwrap().platform, "Steam");
        let mut invalid = Parser::default();
        invalid.line("Chat: -epicuserid=0123456789abcdef0123456789abcdef");
        invalid.line("Log: Command line: -epicuserid=not-an-account");
        assert!(invalid.identity().is_none());
    }

    #[test]
    fn map_load_consumes_playlist_and_clears_menu_offline_and_replay_context() {
        let mut parser = Parser::default();
        parser.line("Log: JoinSettings PlaylistId=(11)");
        parser.line("Log: LoadMap: Stadium_P?Game=TAGame.GameInfo_Soccar_TA");
        assert_eq!(parser.map.as_ref().unwrap().playlist, Some(11));
        parser.line("Log: LoadMap: Stadium_P?Game=TAGame.GameInfo_Soccar_TA");
        assert_eq!(parser.map.as_ref().unwrap().playlist, None);
        parser.line("Log: PlaylistId=13");
        parser.line("Log: LoadMap: Stadium_P?Offline?Game=TAGame.GameInfo_Soccar_TA");
        assert!(parser.map.as_ref().unwrap().offline);
        assert_eq!(parser.map.as_ref().unwrap().playlist, None);
        parser.line("Log: LoadMap: MENU_Main_p");
        assert!(parser.map.is_none());
        parser.line("Log: PlaylistId=13");
        parser.line("Log: LoadMap: Stadium_P?Game=TAGame.GameInfo_Replay_TA");
        assert!(parser.map.as_ref().unwrap().replay);
        assert_eq!(parser.map.as_ref().unwrap().playlist, None);
    }

    #[test]
    fn incremental_reader_handles_fragments_truncation_and_same_size_replacement() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Launch.log");
        std::fs::write(&path, "DevOnline: Logged in as 'Pla").unwrap();
        let mut reader = LogReader::default();
        assert!(reader.read(&path, (1, 1)).unwrap());
        assert!(reader.parser.name.is_none());
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"yer'\nDevOnline: Steam ID: 76561198000000000\n")
            .unwrap();
        reader.read(&path, (1, 1)).unwrap();
        assert_eq!(
            reader.parser.identity().unwrap().name,
            "Steam account 76561198"
        );
        std::fs::write(&path, "DevOnline: Logged in as 'Other'\n").unwrap();
        reader.read(&path, (1, 1)).unwrap();
        assert!(reader.parser.identity().is_none());
        assert_eq!(reader.parser.name.as_deref(), Some("Other"));
        std::fs::write(&path, "DevOnline: Logged in as 'Third'\n").unwrap();
        reader.read(&path, (1, 1)).unwrap();
        assert_eq!(reader.parser.name.as_deref(), Some("Third"));
    }

    #[test]
    fn binding_rejects_stale_mismatched_and_reused_map_generations() {
        let mut snapshot = LogSnapshot {
            checked_at: Some(Instant::now()),
            map: Some(MapContext {
                generation: 5,
                arena: "stadium_p".into(),
                playlist: Some(11),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut binding = MatchBinding::default();
        assert!(binding.mode(&snapshot, "one", "Other_P").is_none());
        assert_eq!(
            binding.mode(&snapshot, "one", "Stadium_P"),
            Some(crate::session::SessionMode::Twos)
        );
        assert!(binding.mode(&snapshot, "two", "Stadium_P").is_none());
        snapshot.map.as_mut().unwrap().generation += 1;
        assert!(binding.mode(&snapshot, "one", "Stadium_P").is_none());
        assert!(binding.mode(&snapshot, "two", "Stadium_P").is_some());
        snapshot.checked_at = Some(Instant::now() - Duration::from_secs(20));
        assert!(binding.mode(&snapshot, "two", "Stadium_P").is_none());
    }
}
