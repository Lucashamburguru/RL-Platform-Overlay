use std::fs;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

const STATS_API_SECTION: &str = "TAGame.MatchStatsExporter_TA";
pub const PACKET_SEND_RATE_OPTIONS: [u16; 4] = [0, 5, 15, 30];
const DEFAULT_PORT: u16 = 49123;

pub fn ensure_stats_api_enabled_on_startup(
    rocket_league_root: &str,
    packet_send_rate: u16,
) -> Result<StatsApiSetupResult, String> {
    if packet_send_rate == 0 {
        return Ok(StatsApiSetupResult {
            message: "Automatic Stats API enablement is turned off.".to_string(),
            ..Default::default()
        });
    }
    if packet_send_rate > 120 {
        return Err("PacketSendRate must be between 0 and 120.".to_string());
    }
    if rocket_league_root.trim().is_empty() {
        return Err("Rocket League path is not configured.".to_string());
    }

    let root = Path::new(rocket_league_root);
    if !root.join("TAGame").is_dir() {
        return Err("Configured Rocket League folder does not contain TAGame.".to_string());
    }

    let status = inspect_stats_api_setup(rocket_league_root);
    if status.configured {
        return Ok(StatsApiSetupResult {
            message: format!(
                "Stats API config is already enabled at {} Hz.",
                status.packet_send_rate.unwrap_or(packet_send_rate)
            ),
            ..Default::default()
        });
    }

    ensure_stats_api_setup_with_rate(rocket_league_root, packet_send_rate)
}

#[derive(Clone, Debug, Default)]
pub struct StatsApiSetupStatus {
    pub ini_path: String,
    pub installation_found: bool,
    pub exists: bool,
    pub configured: bool,
    pub packet_send_rate: Option<u16>,
    pub port: Option<u16>,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct StatsApiSetupResult {
    pub changed: bool,
    pub backup_path: Option<String>,
    pub restart_required: bool,
    pub message: String,
}

pub fn stats_ini_path(rocket_league_root: &str) -> PathBuf {
    Path::new(rocket_league_root)
        .join("TAGame")
        .join("Config")
        .join("DefaultStatsAPI.ini")
}

pub fn inspect_stats_api_setup(rocket_league_root: &str) -> StatsApiSetupStatus {
    let ini_path = stats_ini_path(rocket_league_root);
    if rocket_league_root.trim().is_empty() {
        return StatsApiSetupStatus {
            ini_path: ini_path.display().to_string(),
            message: "Rocket League path is not configured.".to_string(),
            ..Default::default()
        };
    }

    let installation_found = Path::new(rocket_league_root).join("TAGame").is_dir();
    if !installation_found {
        return StatsApiSetupStatus {
            ini_path: ini_path.display().to_string(),
            message: "The selected folder does not contain a Rocket League installation."
                .to_string(),
            ..Default::default()
        };
    }

    let Ok(content) = fs::read_to_string(&ini_path) else {
        return StatsApiSetupStatus {
            ini_path: ini_path.display().to_string(),
            installation_found,
            exists: false,
            message: "DefaultStatsAPI.ini was not found.".to_string(),
            ..Default::default()
        };
    };

    let packet_send_rate = read_u16_in_section(&content, "PacketSendRate", true);
    let port = read_u16_in_section(&content, "Port", false);
    let configured = packet_send_rate.is_some_and(|rate| rate > 0);
    let message = if configured {
        "Stats API appears enabled.".to_string()
    } else {
        "Stats API is disabled or missing PacketSendRate.".to_string()
    };

    StatsApiSetupStatus {
        ini_path: ini_path.display().to_string(),
        installation_found,
        exists: true,
        configured,
        packet_send_rate,
        port,
        message,
    }
}

pub fn ensure_stats_api_setup_with_rate(
    rocket_league_root: &str,
    packet_send_rate: u16,
) -> Result<StatsApiSetupResult, String> {
    if rocket_league_root.trim().is_empty() {
        return Err("Rocket League path is not configured.".to_string());
    }
    if packet_send_rate > 120 {
        return Err("PacketSendRate must be between 0 and 120.".to_string());
    }

    let ini_path = stats_ini_path(rocket_league_root);
    let parent = ini_path
        .parent()
        .ok_or_else(|| "Stats API config parent folder is invalid.".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("Could not create config folder: {e}"))?;

    let original = fs::read_to_string(&ini_path).unwrap_or_default();
    let existed = ini_path.exists();
    let selected_port = read_u16_in_section(&original, "Port", false).unwrap_or(DEFAULT_PORT);
    let updated = upsert_stats_api_ini_content(&original, selected_port, packet_send_rate);

    if existed && original.trim_end() == updated.trim_end() {
        let message = if packet_send_rate == 0 {
            "Stats API is already disabled.".to_string()
        } else {
            format!("Stats API config is already enabled at {packet_send_rate} Hz.")
        };
        return Ok(StatsApiSetupResult {
            changed: false,
            restart_required: false,
            message,
            ..Default::default()
        });
    }

    let backup_path = if existed {
        let backup_path = stats_api_backup_path()?;
        if let Some(parent) = backup_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Could not create backup folder: {e}"))?;
        }
        fs::copy(&ini_path, &backup_path).map_err(|e| format!("Could not create backup: {e}"))?;
        Some(backup_path.display().to_string())
    } else {
        None
    };

    fs::write(&ini_path, updated).map_err(|e| format!("Could not write Stats API config: {e}"))?;

    let message = if packet_send_rate == 0 {
        "Stats API has been disabled. Restart Rocket League once.".to_string()
    } else {
        format!("Stats API config updated to {packet_send_rate} Hz. Restart Rocket League once.")
    };

    Ok(StatsApiSetupResult {
        changed: true,
        backup_path,
        restart_required: true,
        message,
    })
}

fn stats_api_backup_path() -> Result<PathBuf, String> {
    crate::state::config_dir()
        .map(|dir| dir.join("backups").join("DefaultStatsAPI.ini.backup"))
        .ok_or_else(|| "Could not resolve app config directory for Stats API backup.".to_string())
}

fn read_u16_in_section(content: &str, key: &str, allow_zero: bool) -> Option<u16> {
    let mut inside_section = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            inside_section = trimmed.eq_ignore_ascii_case(&format!("[{STATS_API_SECTION}]"));
            continue;
        }
        if !inside_section {
            continue;
        }
        let Some((raw_key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        if !raw_key.trim().eq_ignore_ascii_case(key) {
            continue;
        }
        let parsed = raw_value.trim().parse::<u16>().ok()?;
        if allow_zero || parsed > 0 {
            return Some(parsed);
        }
    }
    None
}

fn upsert_stats_api_ini_content(
    content: &str,
    selected_port: u16,
    packet_send_rate: u16,
) -> String {
    let line_ending = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut updated_lines = Vec::new();
    let mut inside_section = false;
    let mut section_found = false;
    let mut packet_written = false;
    let mut port_written = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if inside_section {
                push_missing_keys(
                    &mut updated_lines,
                    packet_written,
                    port_written,
                    selected_port,
                    packet_send_rate,
                );
            }
            inside_section = trimmed.eq_ignore_ascii_case(&format!("[{STATS_API_SECTION}]"));
            if inside_section {
                section_found = true;
                packet_written = false;
                port_written = false;
            }
            updated_lines.push(line.to_string());
            continue;
        }

        if inside_section && let Some((raw_key, _)) = trimmed.split_once('=') {
            if raw_key.trim().eq_ignore_ascii_case("PacketSendRate") {
                if !packet_written {
                    updated_lines.push(format!("PacketSendRate={packet_send_rate}"));
                    packet_written = true;
                }
                continue;
            }
            if raw_key.trim().eq_ignore_ascii_case("Port") {
                if !port_written {
                    updated_lines.push(format!("Port={selected_port}"));
                    port_written = true;
                }
                continue;
            }
        }

        updated_lines.push(line.to_string());
    }

    if !section_found {
        if !updated_lines.is_empty() && !updated_lines.last().is_some_and(|line| line.is_empty()) {
            updated_lines.push(String::new());
        }
        updated_lines.push(format!("[{STATS_API_SECTION}]"));
        updated_lines.push(format!("PacketSendRate={packet_send_rate}"));
        updated_lines.push(format!("Port={selected_port}"));
    } else if inside_section {
        push_missing_keys(
            &mut updated_lines,
            packet_written,
            port_written,
            selected_port,
            packet_send_rate,
        );
    }

    updated_lines.join(line_ending)
}

fn push_missing_keys(
    updated_lines: &mut Vec<String>,
    packet_written: bool,
    port_written: bool,
    selected_port: u16,
    packet_send_rate: u16,
) {
    if !packet_written {
        updated_lines.push(format!("PacketSendRate={packet_send_rate}"));
    }
    if !port_written {
        updated_lines.push(format!("Port={selected_port}"));
    }
}

#[cfg(test)]
fn backup_timestamp(now: SystemTime) -> String {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn backup_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rl_overlay_setup_{name}_{}",
            backup_timestamp(SystemTime::now())
        ));
        fs::create_dir_all(root.join("TAGame").join("Config")).unwrap();
        root
    }

    #[test]
    fn creates_missing_ini() {
        let root = temp_root("create");
        let result = ensure_stats_api_setup_with_rate(&root.to_string_lossy(), 30).unwrap();
        assert!(result.changed);
        let content = fs::read_to_string(stats_ini_path(&root.to_string_lossy())).unwrap();
        assert!(content.contains("[TAGame.MatchStatsExporter_TA]"));
        assert!(content.contains("PacketSendRate=30"));
        assert!(content.contains("Port=49123"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn updates_packet_send_rate_and_preserves_port() {
        let _guard = backup_lock();
        let root = temp_root("update");
        let ini = stats_ini_path(&root.to_string_lossy());
        fs::write(
            &ini,
            "[TAGame.MatchStatsExporter_TA]\nPacketSendRate=0\nPort=12345\n",
        )
        .unwrap();
        let result = ensure_stats_api_setup_with_rate(&root.to_string_lossy(), 30).unwrap();
        assert!(result.backup_path.is_some());
        let backup_path = PathBuf::from(result.backup_path.unwrap());
        assert!(backup_path.ends_with("backups/DefaultStatsAPI.ini.backup"));
        assert!(!backup_path.starts_with(root.join("TAGame").join("Config")));
        let content = fs::read_to_string(ini).unwrap();
        assert!(content.contains("PacketSendRate=30"));
        assert!(content.contains("Port=12345"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn overwrites_single_app_config_backup() {
        let _guard = backup_lock();
        let root = temp_root("single_backup");
        let ini = stats_ini_path(&root.to_string_lossy());
        fs::write(
            &ini,
            "[TAGame.MatchStatsExporter_TA]\nPacketSendRate=5\nPort=12345\n",
        )
        .unwrap();

        let first = ensure_stats_api_setup_with_rate(&root.to_string_lossy(), 10).unwrap();
        let backup_path = PathBuf::from(first.backup_path.unwrap());
        let first_backup = fs::read_to_string(&backup_path).unwrap();
        assert!(first_backup.contains("PacketSendRate=5"));

        let second = ensure_stats_api_setup_with_rate(&root.to_string_lossy(), 15).unwrap();
        assert_eq!(Some(backup_path.display().to_string()), second.backup_path);
        let second_backup = fs::read_to_string(&backup_path).unwrap();
        assert!(second_backup.contains("PacketSendRate=10"));
        assert!(!second_backup.contains("PacketSendRate=5"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn writes_selected_packet_send_rate() {
        let root = temp_root("selected");
        let result = ensure_stats_api_setup_with_rate(&root.to_string_lossy(), 10).unwrap();
        assert!(result.changed);
        let content = fs::read_to_string(stats_ini_path(&root.to_string_lossy())).unwrap();
        assert!(content.contains("PacketSendRate=10"));
        assert!(content.contains("Port=49123"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_invalid_packet_send_rate() {
        let root = temp_root("invalid");
        let result = ensure_stats_api_setup_with_rate(&root.to_string_lossy(), 121);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn turns_off_stats_api() {
        let root = temp_root("turn_off");
        let result = ensure_stats_api_setup_with_rate(&root.to_string_lossy(), 0).unwrap();
        assert!(result.changed);
        let content = fs::read_to_string(stats_ini_path(&root.to_string_lossy())).unwrap();
        assert!(content.contains("PacketSendRate=0"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn already_enabled_is_not_rewritten() {
        let root = temp_root("noop");
        let ini = stats_ini_path(&root.to_string_lossy());
        fs::write(
            &ini,
            "[TAGame.MatchStatsExporter_TA]\nPacketSendRate=30\nPort=49123\n",
        )
        .unwrap();
        let result = ensure_stats_api_setup_with_rate(&root.to_string_lossy(), 30).unwrap();
        assert!(!result.changed);
        assert!(result.backup_path.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_repairs_disabled_stats_api() {
        let _guard = backup_lock();
        let root = temp_root("startup_repair");
        let ini = stats_ini_path(&root.to_string_lossy());
        fs::write(
            &ini,
            "[TAGame.MatchStatsExporter_TA]\nPacketSendRate=0\nPort=49123\n",
        )
        .unwrap();

        let result = ensure_stats_api_enabled_on_startup(&root.to_string_lossy(), 15).unwrap();

        assert!(result.changed);
        assert!(
            fs::read_to_string(ini)
                .unwrap()
                .contains("PacketSendRate=15")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_preserves_an_existing_positive_rate() {
        let root = temp_root("startup_preserve");
        let ini = stats_ini_path(&root.to_string_lossy());
        fs::write(
            &ini,
            "[TAGame.MatchStatsExporter_TA]\nPacketSendRate=5\nPort=49123\n",
        )
        .unwrap();

        let result = ensure_stats_api_enabled_on_startup(&root.to_string_lossy(), 30).unwrap();

        assert!(!result.changed);
        assert!(
            fs::read_to_string(ini)
                .unwrap()
                .contains("PacketSendRate=5")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_respects_deliberate_turn_off() {
        let root = temp_root("startup_off");
        let ini = stats_ini_path(&root.to_string_lossy());
        fs::write(
            &ini,
            "[TAGame.MatchStatsExporter_TA]\nPacketSendRate=0\nPort=49123\n",
        )
        .unwrap();

        let result = ensure_stats_api_enabled_on_startup(&root.to_string_lossy(), 0).unwrap();

        assert!(!result.changed);
        assert!(
            fs::read_to_string(ini)
                .unwrap()
                .contains("PacketSendRate=0")
        );
        let _ = fs::remove_dir_all(root);
    }
}
