//! Portable presentation settings. Credentials, paths and game-file actions are excluded.
use crate::session::SessionOverlayDisplay;
use crate::state::{Config, LobbyDisplayMode, LobbyTheme, TeammateBoostDisplay};
use serde::{Deserialize, Serialize};

pub const MAX_PROFILES: usize = 16;
pub const MAX_IMPORT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudProfile {
    pub version: u32,
    pub name: String,
    pub layout: HudLayout,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudLayout {
    pub transparency: u8,
    pub ui_scale: f32,
    pub lobby_theme: LobbyTheme,
    pub lobby_display_mode: LobbyDisplayMode,
    pub show_stats: bool,
    pub show_lobby_matches: bool,
    pub show_lobby_ranks: bool,
    pub show_peak_rank: bool,
    pub show_bots: bool,
    pub show_teammate_boost: bool,
    pub teammate_hud_scale: f32,
    pub teammate_boost_display: TeammateBoostDisplay,
    pub session_overlay_enabled: bool,
    pub session_overlay_scale: f32,
    pub session_overlay_opacity: u8,
    pub session_overlay_display: SessionOverlayDisplay,
    pub session_expanded_show_streaks: bool,
    pub session_expanded_show_breakdown: bool,
    pub session_expanded_show_mmr_delta: bool,
    pub lobby_manual_position: Option<[f32; 2]>,
    pub teammate_boost_manual_position: Option<[f32; 2]>,
    pub session_manual_position: Option<[f32; 2]>,
}

impl HudProfile {
    pub fn capture(name: &str, config: &Config) -> Result<Self, String> {
        let profile = Self {
            version: 1,
            name: name.trim().to_string(),
            layout: HudLayout {
                transparency: config.transparency,
                ui_scale: config.ui_scale,
                lobby_theme: config.lobby_theme,
                lobby_display_mode: config.lobby_display_mode,
                show_stats: config.show_stats,
                show_lobby_matches: config.show_lobby_matches,
                show_lobby_ranks: config.show_lobby_ranks,
                show_peak_rank: config.show_peak_rank,
                show_bots: config.show_bots,
                show_teammate_boost: config.show_teammate_boost,
                teammate_hud_scale: config.teammate_hud_scale,
                teammate_boost_display: config.teammate_boost_display,
                session_overlay_enabled: config.session_overlay_enabled,
                session_overlay_scale: config.session_overlay_scale,
                session_overlay_opacity: config.session_overlay_opacity,
                session_overlay_display: config.session_overlay_display,
                session_expanded_show_streaks: config.session_expanded_show_streaks,
                session_expanded_show_breakdown: config.session_expanded_show_breakdown,
                session_expanded_show_mmr_delta: config.session_expanded_show_mmr_delta,
                lobby_manual_position: config.lobby_manual_position,
                teammate_boost_manual_position: config.teammate_boost_manual_position,
                session_manual_position: config.session_manual_position,
            },
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn import(text: &str) -> Result<Self, String> {
        if text.len() > MAX_IMPORT_BYTES {
            return Err("Profile is too large (maximum 16 KiB).".into());
        }
        let profile: Self =
            serde_json::from_str(text).map_err(|_| "Invalid HUD profile format.".to_string())?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn export(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported HUD profile version.".into());
        }
        if self.name.trim().is_empty()
            || self.name.len() > 64
            || self.name.chars().any(char::is_control)
        {
            return Err("Choose a profile name of 1–64 bytes without control characters.".into());
        }
        for scale in [
            self.layout.ui_scale,
            self.layout.teammate_hud_scale,
            self.layout.session_overlay_scale,
        ] {
            if !scale.is_finite() || !(0.5..=2.5).contains(&scale) {
                return Err("HUD scales must be between 0.5 and 2.5.".into());
            }
        }
        for position in [
            self.layout.lobby_manual_position,
            self.layout.teammate_boost_manual_position,
            self.layout.session_manual_position,
        ]
        .into_iter()
        .flatten()
        {
            if position
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 100_000.0)
            {
                return Err("Profile contains an invalid HUD position.".into());
            }
        }
        Ok(())
    }

    pub fn apply(&self, config: &mut Config) -> Result<(), String> {
        self.validate()?;
        config.transparency = self.layout.transparency;
        config.ui_scale = self.layout.ui_scale;
        config.lobby_theme = self.layout.lobby_theme;
        config.lobby_display_mode = self.layout.lobby_display_mode;
        config.show_stats = self.layout.show_stats;
        config.show_lobby_matches = self.layout.show_lobby_matches;
        config.show_lobby_ranks = self.layout.show_lobby_ranks;
        config.show_peak_rank = self.layout.show_peak_rank;
        config.show_bots = self.layout.show_bots;
        config.show_teammate_boost = self.layout.show_teammate_boost;
        config.teammate_hud_scale = self.layout.teammate_hud_scale;
        config.teammate_boost_display = self.layout.teammate_boost_display;
        config.session_overlay_enabled = self.layout.session_overlay_enabled;
        config.session_overlay_scale = self.layout.session_overlay_scale;
        config.session_overlay_opacity = self.layout.session_overlay_opacity;
        config.session_overlay_display = self.layout.session_overlay_display;
        config.session_expanded_show_streaks = self.layout.session_expanded_show_streaks;
        config.session_expanded_show_breakdown = self.layout.session_expanded_show_breakdown;
        config.session_expanded_show_mmr_delta = self.layout.session_expanded_show_mmr_delta;
        config.lobby_manual_position = self.layout.lobby_manual_position;
        config.teammate_boost_manual_position = self.layout.teammate_boost_manual_position;
        config.session_manual_position = self.layout.session_manual_position;
        Ok(())
    }

    pub fn save(self, config: &mut Config) -> Result<(), String> {
        self.validate()?;
        if let Some(existing) = config
            .hud_profiles
            .iter_mut()
            .find(|item| item.name.eq_ignore_ascii_case(&self.name))
        {
            *existing = self;
        } else if config.hud_profiles.len() < MAX_PROFILES {
            config.hud_profiles.push(self);
        } else {
            return Err("A maximum of 16 HUD profiles can be saved.".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_profile_round_trip_changes_presentation_without_sharing_private_settings() {
        let source = Config {
            ballchasing_api_key: "private-token".into(),
            rocket_league_path: "/private/install".into(),
            ui_scale: 1.3,
            lobby_manual_position: Some([10.0, 20.0]),
            ..Default::default()
        };
        let encoded = HudProfile::capture("Match HUD", &source)
            .unwrap()
            .export()
            .unwrap();
        assert!(!encoded.contains("private"));
        assert!(!encoded.contains("hotkey"));
        let profile = HudProfile::import(&encoded).unwrap();
        let mut target = Config {
            ballchasing_api_key: "my-token".into(),
            auto_gg: true,
            launch_log_enabled: false,
            ..Default::default()
        };
        profile.apply(&mut target).unwrap();
        assert_eq!(target.ui_scale, 1.3);
        assert_eq!(target.lobby_manual_position, Some([10.0, 20.0]));
        assert_eq!(target.ballchasing_api_key, "my-token");
        assert!(target.auto_gg);
        assert!(!target.launch_log_enabled);
    }

    #[test]
    fn invalid_imports_and_apply_are_rejected_before_mutation() {
        let mut profile = HudProfile::capture("Example", &Config::default()).unwrap();
        profile.layout.ui_scale = f32::INFINITY;
        let mut config = Config::default();
        let previous = config.ui_scale;
        assert!(profile.apply(&mut config).is_err());
        assert_eq!(config.ui_scale, previous);
        let mut value =
            serde_json::to_value(HudProfile::capture("Example", &config).unwrap()).unwrap();
        value["ballchasing_api_key"] = serde_json::json!("not-allowed");
        assert!(HudProfile::import(&value.to_string()).is_err());
        assert!(HudProfile::import(&" ".repeat(MAX_IMPORT_BYTES + 1)).is_err());
    }
}
