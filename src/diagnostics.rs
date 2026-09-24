use std::path::PathBuf;
use std::sync::atomic::Ordering;
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
const BYTES_PER_MB: u64 = 1_048_576;
#[cfg(target_os = "windows")]
const SYSTEM_DIAGNOSTICS_CACHE_TTL: Duration = Duration::from_secs(5);

#[cfg(target_os = "windows")]
pub fn system_diagnostics() -> Vec<(&'static str, String)> {
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<SystemDiagnosticsCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(SystemDiagnosticsCache::default()));

    if let Ok(mut cached) = cache.lock() {
        if cached.is_fresh() {
            return cached.diagnostics.clone();
        }

        if !cached.refreshing {
            cached.refreshing = true;
            std::thread::spawn(move || {
                let diagnostics = collect_system_diagnostics();
                if let Some(cache) = CACHE.get()
                    && let Ok(mut cached) = cache.lock()
                {
                    cached.recorded_at = Some(Instant::now());
                    cached.diagnostics = diagnostics;
                    cached.refreshing = false;
                }
            });
        }

        let mut diagnostics = cached.diagnostics.clone();
        diagnostics.push(("System Diagnostics", "Refreshing...".to_string()));
        return diagnostics;
    }

    vec![("System Diagnostics", "Unavailable".to_string())]
}

#[cfg(target_os = "windows")]
#[derive(Default)]
struct SystemDiagnosticsCache {
    recorded_at: Option<Instant>,
    diagnostics: Vec<(&'static str, String)>,
    refreshing: bool,
}

#[cfg(target_os = "windows")]
impl SystemDiagnosticsCache {
    fn is_fresh(&self) -> bool {
        self.recorded_at
            .is_some_and(|recorded_at| recorded_at.elapsed() < SYSTEM_DIAGNOSTICS_CACHE_TTL)
    }
}

#[cfg(target_os = "windows")]
fn collect_system_diagnostics() -> Vec<(&'static str, String)> {
    use std::process::Command;

    let mut diag = Vec::new();

    if let Ok(output) = Command::new("powershell")
        .args(["-NoProfile", "-Command", TARGET_PROCESS_DIAGNOSTICS_PS])
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let mut found_process = false;
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            found_process = true;
            diag.push(("Target Process", line.to_string()));
        }
        if !found_process {
            diag.push(("Target Processes", "None detected".to_string()));
        }
    }

    if let Ok(output) = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-Process dwm | Select-Object -ExpandProperty WorkingSet64",
        ])
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if let Ok(bytes) = text.parse::<u64>() {
            diag.push(("DWM Memory", format!("{} MB", bytes / BYTES_PER_MB)));
        }
    }

    diag
}

#[cfg(target_os = "windows")]
const TARGET_PROCESS_DIAGNOSTICS_PS: &str = r#"
$names = @(
    'RocketLeague',
    'EasyAntiCheat',
    'EasyAntiCheat_EOS',
    'EOSOverlayRenderer-Win64-Shipping',
    'EOSOverlayRenderer-Win32-Shipping'
)
Get-Process |
    Where-Object {
        $names -contains $_.ProcessName -or
        $_.ProcessName -like '*EasyAntiCheat*' -or
        $_.ProcessName -like '*EOS*'
    } |
    Sort-Object ProcessName, Id |
    ForEach-Object {
        $priority = 'Unavailable'
        $affinity = 'Unavailable'
        try { $priority = [string]$_.PriorityClass } catch {}
        try { $affinity = [string]$_.ProcessorAffinity } catch {}
        "$($_.ProcessName).exe pid=$($_.Id) priority=$priority affinity=$affinity"
    }
"#;

#[cfg(not(target_os = "windows"))]
pub fn system_diagnostics() -> Vec<(&'static str, String)> {
    vec![]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SupportBundlePrivacy {
    #[default]
    Redacted,
    Identifiable,
}

impl SupportBundlePrivacy {
    fn includes_identifiable_details(self) -> bool {
        self == Self::Identifiable
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Redacted => "redacted",
            Self::Identifiable => "identifiable",
        }
    }
}

pub fn support_diagnostics_bundle(
    state: &crate::state::AppState,
    is_launched: bool,
    is_rl_running: bool,
    rl_process_detection_detail: &str,
) -> String {
    support_diagnostics_bundle_with_privacy(
        state,
        is_launched,
        is_rl_running,
        rl_process_detection_detail,
        SupportBundlePrivacy::Redacted,
    )
}

pub fn support_diagnostics_bundle_with_privacy(
    state: &crate::state::AppState,
    is_launched: bool,
    is_rl_running: bool,
    rl_process_detection_detail: &str,
    privacy: SupportBundlePrivacy,
) -> String {
    let config = state.system.config.load();
    let config_status = state.system.config_status.load();
    let diagnostics = state.system.network_diagnostics.load();
    let version = state.system.version_check.load();
    let local_identity = state.game.local_player_identity.load();
    let local_name = state.game.local_player_name.load();
    let local_team = state.game.local_team.load(Ordering::SeqCst);
    let players = state.game.players.load();
    let session = state.game.session.load();
    let capture = state.diagnostics.debug_capture_status.load();

    let mut lines = Vec::new();
    lines.push("RL Platform Overlay Support Diagnostics".to_string());
    lines.push(format!("generated_unix_ms={}", crate::stats_api::now_ms()));
    lines.push(format!("app_version={}", crate::app_version()));
    lines.push(format!("os={}", std::env::consts::OS));
    lines.push(format!("arch={}", std::env::consts::ARCH));
    lines.push(format!("privacy={}", privacy.label()));
    lines.push(format!(
        "identifiable_details_included={}",
        privacy.includes_identifiable_details()
    ));
    lines.push(format!(
        "debug_tab_enabled={}",
        if state.debug_enabled { "true" } else { "false" }
    ));
    lines.push(format!(
        "debug_logging_enabled={}",
        if state.debug_logging_enabled.load(Ordering::SeqCst) {
            "true"
        } else {
            "false"
        }
    ));

    lines.push(String::new());
    lines.push("[config]".to_string());
    lines.push(format!(
        "config_path={}",
        support_private_value(&config_status.path, privacy)
    ));
    lines.push(format!(
        "config_status={}",
        if config_status.last_error.is_empty() {
            "OK"
        } else {
            if privacy.includes_identifiable_details() {
                config_status.last_error.as_str()
            } else {
                "Error (details redacted)"
            }
        }
    ));
    lines.push(format!(
        "rocket_league_path={}",
        support_private_value(&config.rocket_league_path, privacy)
    ));
    lines.push(format!(
        "replays_folder={}",
        support_private_value(&config.replays_folder, privacy)
    ));
    lines.push(format!(
        "ballchasing_enabled={}",
        config.ballchasing_enabled
    ));
    lines.push(format!(
        "ballchasing_api_key_present={}",
        !config.ballchasing_api_key.trim().is_empty()
    ));
    lines.push(format!("layout_mode={}", config.layout_mode));
    lines.push(format!("show_stats={}", config.show_stats));
    lines.push(format!("show_lobby_ranks={}", config.show_lobby_ranks));
    lines.push(format!(
        "show_teammate_boost={}",
        config.show_teammate_boost
    ));
    lines.push(format!(
        "session_overlay_enabled={}",
        config.session_overlay_enabled
    ));

    lines.push(String::new());
    lines.push("[runtime]".to_string());
    lines.push(format!("overlay_launched={is_launched}"));
    lines.push(format!(
        "hud_visible={}",
        state.flags.is_visible.load(Ordering::SeqCst)
    ));
    lines.push(format!(
        "settings_visible={}",
        state.flags.is_settings_visible.load(Ordering::SeqCst)
    ));
    lines.push(format!(
        "stats_api_connected={}",
        state.flags.is_connected.load(Ordering::SeqCst)
    ));
    lines.push(format!("rocket_league_running={is_rl_running}"));
    lines.push(format!(
        "rocket_league_detection_detail={}",
        support_private_value(rl_process_detection_detail, privacy)
    ));

    lines.push(String::new());
    lines.push("[stats_api]".to_string());
    lines.push(format!("transport={}", diagnostics.transport.label()));
    lines.push(format!(
        "last_event={}",
        empty_label(&diagnostics.last_event)
    ));
    lines.push(format!(
        "last_event_unix_ms={}",
        diagnostics.last_event_unix_ms
    ));
    lines.push(format!(
        "last_event_rate_estimate={}",
        empty_label(&diagnostics.last_event_rate_estimate)
    ));
    lines.push(format!(
        "last_roster_signature_change_unix_ms={}",
        diagnostics.last_roster_signature_change_unix_ms
    ));
    lines.push(format!(
        "last_match_guid={}",
        support_private_value(&diagnostics.last_match_guid, privacy)
    ));
    lines.push(format!(
        "last_result_signature={}",
        empty_label(&diagnostics.last_result_signature)
    ));
    lines.push(format!(
        "last_duplicate_result_suppression_reason={}",
        empty_label(&diagnostics.last_duplicate_result_suppression_reason)
    ));
    lines.push(format!(
        "last_parse_error={}",
        support_private_value(&diagnostics.last_parse_error, privacy)
    ));
    lines.push(format!(
        "last_connection_error={}",
        support_private_value(&diagnostics.last_connection_error, privacy)
    ));

    lines.push(String::new());
    lines.push("[local_player]".to_string());
    lines.push(format!(
        "local_name={}",
        support_private_value(local_name.as_str(), privacy)
    ));
    lines.push(format!(
        "identity_name={}",
        support_private_value(&local_identity.name, privacy)
    ));
    lines.push(format!(
        "identity_platform={}",
        empty_label(&local_identity.platform)
    ));
    lines.push(format!(
        "identity_primary_id={}",
        support_private_value(&local_identity.primary_id, privacy)
    ));
    lines.push(format!(
        "local_team={}",
        crate::state::standard_team(local_team)
            .map(|team| team.to_string())
            .unwrap_or_else(|| "Unknown".to_string())
    ));

    lines.push(String::new());
    lines.push("[session]".to_string());
    lines.push(format!(
        "active_match_id={}",
        support_private_value(&session.active_match_id, privacy)
    ));
    lines.push(format!("active_mode={}", session.active_mode.label()));
    lines.push(format!(
        "active_mode_source={}",
        session.active_mode_source.label()
    ));
    lines.push(format!("matches_played={}", session.matches_played));
    lines.push(format!("wins={}", session.wins));
    lines.push(format!("losses={}", session.losses));
    lines.push(format!("streak={}", session.streak));
    lines.push(format!("last_result={}", session.last_result.label()));
    lines.push(format!("blue_score={}", session.blue_score));
    lines.push(format!("orange_score={}", session.orange_score));
    lines.push(format!("round_started={}", session.round_started));
    if session.mode_records.is_empty() {
        lines.push("mode_records=none".to_string());
    } else {
        for (mode, record) in &session.mode_records {
            lines.push(format!(
                "mode_record={} wins={} losses={} matches={}",
                mode.label(),
                record.wins,
                record.losses,
                record.matches_played()
            ));
        }
    }

    lines.push(String::new());
    lines.push("[players]".to_string());
    lines.push(format!("count={}", players.len()));
    if players.is_empty() {
        lines.push("no players parsed".to_string());
    } else {
        let mut sorted_players = players.values().collect::<Vec<_>>();
        sorted_players.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
                .then_with(|| a.primary_id.cmp(&b.primary_id))
        });
        let player_lines = sorted_players
            .into_iter()
            .enumerate()
            .map(|(index, player)| {
                let display_name = if privacy.includes_identifiable_details() {
                    player.name.clone()
                } else {
                    format!("Player {}", index + 1)
                };
                format!(
                    "name={} platform={} team={} local={} bot={} boost={} score={} goals={} saves={} touches={} demos={} mmr_loaded={}",
                    display_name,
                    player.platform,
                    player.team,
                    player.is_local,
                    player.is_bot,
                    player.boost,
                    player.score,
                    player.goals,
                    player.saves,
                    player.touches,
                    player.demos,
                    player.mmr.is_some()
                )
            })
            .collect::<Vec<_>>();
        lines.extend(player_lines);
    }

    lines.push(String::new());
    lines.push("[version_check]".to_string());
    lines.push(format!("checked={}", version.checked));
    lines.push(format!("update_available={}", version.update_available));
    lines.push(format!("latest_tag={}", empty_label(&version.latest_tag)));
    lines.push(format!(
        "error={}",
        support_private_value(&version.error, privacy)
    ));

    lines.push(String::new());
    lines.push("[diagnostics]".to_string());
    lines.push(format!(
        "hotkey_log_path={}",
        support_private_value(
            &crate::input::hotkey_debug_log_path().display().to_string(),
            privacy
        )
    ));
    lines.push(format!("stats_api_capture_running={}", capture.running));
    lines.push(format!(
        "last_capture_output={}",
        support_private_value(&capture.last_output_path, privacy)
    ));
    lines.push(format!(
        "last_capture_error={}",
        support_private_value(&capture.error, privacy)
    ));

    let upload_progress = state.replays.upload_progress.load();
    lines.push(String::new());
    lines.push("[replay_upload]".to_string());
    lines.push(format!("running={}", upload_progress.running));
    lines.push(format!("paused={}", upload_progress.paused));
    lines.push(format!("stop_requested={}", upload_progress.stop_requested));
    lines.push(format!("total={}", upload_progress.total));
    lines.push(format!("processed={}", upload_progress.processed));
    lines.push(format!("uploaded={}", upload_progress.uploaded));
    lines.push(format!("skipped={}", upload_progress.skipped));
    lines.push(format!("failed={}", upload_progress.failed));
    lines.push(format!(
        "current_file={}",
        support_private_value(&upload_progress.current_file, privacy)
    ));
    lines.push(format!(
        "last_error={}",
        support_private_value(&upload_progress.last_error, privacy)
    ));
    if upload_progress.recent_events.is_empty() {
        lines.push("recent_events=none".to_string());
    } else if !privacy.includes_identifiable_details() {
        lines.push(format!(
            "recent_events={} entries redacted",
            upload_progress.recent_events.len()
        ));
    } else {
        for event in &upload_progress.recent_events {
            lines.push(format!("event={event}"));
        }
    }

    let system = system_diagnostics();
    lines.push(String::new());
    lines.push("[system]".to_string());
    if system.is_empty() {
        lines.push("no system diagnostics available".to_string());
    } else {
        for (label, value) in system {
            lines.push(format!("{label}={value}"));
        }
    }

    lines.push(String::new());
    lines.push("[recent_hotkey_log]".to_string());
    if privacy.includes_identifiable_details() {
        lines.extend(read_recent_lines(
            &crate::input::hotkey_debug_log_path(),
            40,
        ));
    } else {
        lines.push("omitted (identifiable details disabled)".to_string());
    }

    lines.join("\n")
}

fn support_private_value(value: &str, privacy: SupportBundlePrivacy) -> String {
    if value.trim().is_empty() {
        "(empty)".to_string()
    } else if privacy.includes_identifiable_details() {
        value.to_string()
    } else {
        "[redacted]".to_string()
    }
}

fn empty_label(value: &str) -> &str {
    if value.trim().is_empty() {
        "(empty)"
    } else {
        value
    }
}

fn read_recent_lines(path: &PathBuf, max_lines: usize) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let mut lines = content
                .lines()
                .rev()
                .take(max_lines)
                .map(str::to_string)
                .collect::<Vec<_>>();
            lines.reverse();
            if lines.is_empty() {
                vec!["hotkey log is empty".to_string()]
            } else {
                lines
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            vec!["hotkey log file not found".to_string()]
        }
        Err(error) => vec![format!("could not read hotkey log: {error}")],
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    #[test]
    fn support_bundle_includes_mode_provenance() {
        let state = crate::state::AppState::new();
        let mut session = crate::session::SessionState::default();
        session.active_match_id = "mode-source-test".to_string();
        session.active_mode = crate::session::SessionMode::Twos;
        session.active_mode_source = crate::session::SessionModeSource::PlaylistMetadata;
        state.game.session.store(std::sync::Arc::new(session));

        let bundle = super::support_diagnostics_bundle(&state, false, false, "not running");

        assert!(bundle.contains("active_mode=2v2"));
        assert!(bundle.contains("active_mode_source=playlist_metadata"));
    }

    #[test]
    fn support_bundle_is_redacted_by_default_and_identifiable_only_on_request() {
        let state = crate::state::AppState::new();
        let mut config = (**state.system.config.load()).clone();
        config.rocket_league_path = "/Users/Secret/Game".to_string();
        config.replays_folder = "/Users/Secret/Replays".to_string();
        config.ballchasing_api_key = "never-copy-this-token".to_string();
        state.system.config.store(Arc::new(config));
        state
            .system
            .config_status
            .store(Arc::new(crate::state::ConfigStatus {
                path: "/Users/Secret/config.toml".to_string(),
                last_error: String::new(),
            }));
        state
            .game
            .local_player_name
            .store(Arc::new("PrivatePlayer".to_string()));
        state
            .game
            .local_player_identity
            .store(Arc::new(crate::state::LocalPlayerIdentity {
                name: "PrivatePlayer".to_string(),
                primary_id: "Steam|private-account|0".to_string(),
                platform: "Steam".to_string(),
            }));
        let player = crate::state::PlayerInfo {
            name: "PrivateOpponent".to_string(),
            primary_id: "Epic|private-opponent|0".to_string(),
            platform: "Epic".to_string(),
            team: 1,
            ..Default::default()
        };
        let key = crate::state::PlayerKey::from_account(&player).unwrap();
        state
            .game
            .players
            .store(Arc::new(crate::state::PlayerMap::from([(key, player)])));
        let mut session = crate::session::SessionState::default();
        session.active_match_id = "private-match-guid".to_string();
        state.game.session.store(Arc::new(session));
        state
            .replays
            .upload_progress
            .store(Arc::new(crate::state::ReplayUploadProgress {
                current_file: "private-replay.replay".to_string(),
                last_error: "Could not upload private-replay.replay".to_string(),
                recent_events: vec!["Failed private-replay.replay".to_string()],
                ..Default::default()
            }));

        let redacted = super::support_diagnostics_bundle(
            &state,
            false,
            false,
            "/Users/Secret/Game/RocketLeague",
        );
        let identifiable = super::support_diagnostics_bundle_with_privacy(
            &state,
            false,
            false,
            "/Users/Secret/Game/RocketLeague",
            super::SupportBundlePrivacy::Identifiable,
        );

        for secret in [
            "/Users/Secret",
            "PrivatePlayer",
            "PrivateOpponent",
            "private-account",
            "private-match-guid",
            "private-replay.replay",
            "never-copy-this-token",
        ] {
            assert!(
                !redacted.contains(secret),
                "redacted bundle leaked {secret}"
            );
        }
        assert!(redacted.contains("privacy=redacted"));
        assert!(redacted.contains("name=Player 1"));
        assert!(redacted.contains("ballchasing_api_key_present=true"));
        assert!(identifiable.contains("privacy=identifiable"));
        assert!(identifiable.contains("PrivatePlayer"));
        assert!(identifiable.contains("PrivateOpponent"));
        assert!(identifiable.contains("private-match-guid"));
        assert!(identifiable.contains("private-replay.replay"));
        assert!(!identifiable.contains("never-copy-this-token"));
    }
}
