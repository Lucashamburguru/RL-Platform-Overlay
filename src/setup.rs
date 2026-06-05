use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const STATS_API_SECTION: &str = "TAGame.MatchStatsExporter_TA";
const REQUIRED_PACKET_SEND_RATE: u16 = 30;
const DEFAULT_PORT: u16 = 49123;

#[derive(Clone, Debug, Default)]
pub struct StatsApiSetupStatus {
    pub ini_path: String,
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

    let Ok(content) = fs::read_to_string(&ini_path) else {
        return StatsApiSetupStatus {
            ini_path: ini_path.display().to_string(),
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
        exists: true,
        configured,
        packet_send_rate,
        port,
        message,
    }
}

pub fn ensure_stats_api_setup(rocket_league_root: &str) -> Result<StatsApiSetupResult, String> {
    if rocket_league_root.trim().is_empty() {
        return Err("Rocket League path is not configured.".to_string());
    }

    let ini_path = stats_ini_path(rocket_league_root);
    let parent = ini_path
        .parent()
        .ok_or_else(|| "Stats API config parent folder is invalid.".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("Could not create config folder: {e}"))?;

    let original = fs::read_to_string(&ini_path).unwrap_or_default();
    let existed = ini_path.exists();
    let selected_port = read_u16_in_section(&original, "Port", false).unwrap_or(DEFAULT_PORT);
    let updated = upsert_stats_api_ini_content(&original, selected_port);

    if existed && original.trim_end() == updated.trim_end() {
        return Ok(StatsApiSetupResult {
            changed: false,
            restart_required: false,
            message: "Stats API config is already enabled.".to_string(),
            ..Default::default()
        });
    }

    let backup_path = if existed {
        let backup_path = ini_path.with_file_name(format!(
            "DefaultStatsAPI.ini.bak_{}",
            backup_timestamp(SystemTime::now())
        ));
        fs::copy(&ini_path, &backup_path).map_err(|e| format!("Could not create backup: {e}"))?;
        Some(backup_path.display().to_string())
    } else {
        None
    };

    fs::write(&ini_path, updated).map_err(|e| format!("Could not write Stats API config: {e}"))?;

    Ok(StatsApiSetupResult {
        changed: true,
        backup_path,
        restart_required: true,
        message: "Stats API config updated. Restart Rocket League once.".to_string(),
    })
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

fn upsert_stats_api_ini_content(content: &str, selected_port: u16) -> String {
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
                    updated_lines.push(format!("PacketSendRate={REQUIRED_PACKET_SEND_RATE}"));
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
        updated_lines.push(format!("PacketSendRate={REQUIRED_PACKET_SEND_RATE}"));
        updated_lines.push(format!("Port={selected_port}"));
    } else if inside_section {
        push_missing_keys(
            &mut updated_lines,
            packet_written,
            port_written,
            selected_port,
        );
    }

    updated_lines.join(line_ending)
}

fn push_missing_keys(
    updated_lines: &mut Vec<String>,
    packet_written: bool,
    port_written: bool,
    selected_port: u16,
) {
    if !packet_written {
        updated_lines.push(format!("PacketSendRate={REQUIRED_PACKET_SEND_RATE}"));
    }
    if !port_written {
        updated_lines.push(format!("Port={selected_port}"));
    }
}

fn backup_timestamp(now: SystemTime) -> String {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let result = ensure_stats_api_setup(&root.to_string_lossy()).unwrap();
        assert!(result.changed);
        let content = fs::read_to_string(stats_ini_path(&root.to_string_lossy())).unwrap();
        assert!(content.contains("[TAGame.MatchStatsExporter_TA]"));
        assert!(content.contains("PacketSendRate=30"));
        assert!(content.contains("Port=49123"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn updates_packet_send_rate_and_preserves_port() {
        let root = temp_root("update");
        let ini = stats_ini_path(&root.to_string_lossy());
        fs::write(
            &ini,
            "[TAGame.MatchStatsExporter_TA]\nPacketSendRate=0\nPort=12345\n",
        )
        .unwrap();
        let result = ensure_stats_api_setup(&root.to_string_lossy()).unwrap();
        assert!(result.backup_path.is_some());
        let content = fs::read_to_string(ini).unwrap();
        assert!(content.contains("PacketSendRate=30"));
        assert!(content.contains("Port=12345"));
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
        let result = ensure_stats_api_setup(&root.to_string_lossy()).unwrap();
        assert!(!result.changed);
        assert!(result.backup_path.is_none());
        let _ = fs::remove_dir_all(root);
    }
}
