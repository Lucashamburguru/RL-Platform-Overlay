use crate::state::AppState;
use eframe::egui;
use std::sync::Arc;

use super::common::debug_status_row;

pub(super) fn render_local_mmr_panel(ui: &mut egui::Ui, state: &Arc<AppState>) {
    let identity = state.local_player_identity.load();
    let local_mmr = state.local_mmr.load();

    ui.heading("Local MMR");
    if identity.is_known() {
        debug_status_row(ui, "Player", identity.name.as_str());
        debug_status_row(ui, "Platform", identity.platform.as_str());
    } else {
        ui.colored_label(
            egui::Color32::from_rgb(220, 200, 100),
            "Waiting for local player identity.",
        );
    }

    ui.add_space(6.0);
    let can_refresh = identity.is_known() && !local_mmr.fetching;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(can_refresh, egui::Button::new("Refresh"))
            .clicked()
        {
            crate::mmr::start_local_mmr_refresh(state.clone());
        }
        if local_mmr.fetching {
            ui.add(egui::Spinner::new());
            ui.label("Fetching...");
        }
    });

    if local_mmr.last_updated_unix_ms > 0 {
        debug_status_row(
            ui,
            "Updated",
            &format_age(crate::stats_api::now_ms(), local_mmr.last_updated_unix_ms),
        );
    }
    if !local_mmr.error.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(230, 120, 80),
            local_mmr.error.as_str(),
        );
    }

    ui.add_space(8.0);
    let Some(current) = &local_mmr.current else {
        ui.label(egui::RichText::new("No local MMR snapshot yet.").color(egui::Color32::GRAY));
        return;
    };

    let mut rows: Vec<_> = current.playlists.iter().collect();
    rows.sort_by_key(|(playlist_id, playlist)| {
        (
            ranked_playlist_sort_priority(**playlist_id, playlist.name.as_str()),
            **playlist_id,
        )
    });

    if rows.is_empty() {
        ui.label(egui::RichText::new("No ranked playlist data found.").color(egui::Color32::GRAY));
        return;
    }

    egui::Grid::new("local_mmr_grid")
        .num_columns(3)
        .spacing(egui::vec2(8.0, 4.0))
        .striped(true)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Mode").strong());
            ui.label(egui::RichText::new("MMR").strong());
            ui.label(egui::RichText::new("Delta").strong());
            ui.end_row();

            for (playlist_id, playlist) in rows {
                let previous_rating = local_mmr
                    .previous
                    .as_ref()
                    .and_then(|snapshot| snapshot.playlists.get(playlist_id))
                    .map(|playlist| playlist.rating);
                ui.label(compact_playlist_name(playlist.name.as_str()));
                ui.label(playlist.rating.to_string());
                render_mmr_delta(ui, previous_rating.map(|rating| playlist.rating - rating));
                ui.end_row();
            }
        });
}

fn ranked_playlist_sort_priority(playlist_id: i32, playlist_name: &str) -> i32 {
    let name = playlist_name.to_lowercase();
    if playlist_id == 10 || name.contains("duel") || name.contains("1v1") {
        0
    } else if playlist_id == 11 || name.contains("doubles") || name.contains("2v2") {
        1
    } else if playlist_id == 13 || name.contains("standard") || name.contains("3v3") {
        2
    } else {
        10
    }
}

fn compact_playlist_name(playlist_name: &str) -> String {
    let name = playlist_name.to_lowercase();
    if name.contains("duel") || name.contains("1v1") {
        "1v1".to_string()
    } else if name.contains("doubles") || name.contains("2v2") {
        "2v2".to_string()
    } else if name.contains("standard") || name.contains("3v3") {
        "3v3".to_string()
    } else {
        playlist_name
            .trim_start_matches("Ranked ")
            .trim()
            .to_string()
    }
}

fn render_mmr_delta(ui: &mut egui::Ui, delta: Option<i32>) {
    let Some(delta) = delta else {
        ui.label(egui::RichText::new("-").color(egui::Color32::GRAY));
        return;
    };

    let (text, color) = if delta > 0 {
        (format!("+{delta}"), egui::Color32::from_rgb(100, 220, 140))
    } else if delta < 0 {
        (delta.to_string(), egui::Color32::from_rgb(230, 120, 120))
    } else {
        ("0".to_string(), egui::Color32::from_gray(180))
    };
    ui.label(egui::RichText::new(text).color(color));
}

fn format_age(now_unix_ms: u128, then_unix_ms: u128) -> String {
    let elapsed_seconds = now_unix_ms.saturating_sub(then_unix_ms) / 1000;
    if elapsed_seconds < 60 {
        format!("{elapsed_seconds}s ago")
    } else if elapsed_seconds < 60 * 60 {
        format!("{}m ago", elapsed_seconds / 60)
    } else {
        format!("{}h ago", elapsed_seconds / (60 * 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_playlist_name_formats_common_ranked_modes() {
        assert_eq!(compact_playlist_name("Ranked Duel 1v1"), "1v1");
        assert_eq!(compact_playlist_name("Ranked Doubles 2v2"), "2v2");
        assert_eq!(compact_playlist_name("Ranked Standard 3v3"), "3v3");
        assert_eq!(compact_playlist_name("Ranked Hoops"), "Hoops");
    }

    #[test]
    fn ranked_playlist_sort_priority_orders_core_modes_first() {
        assert_eq!(ranked_playlist_sort_priority(10, "Ranked Duel"), 0);
        assert_eq!(ranked_playlist_sort_priority(11, "Ranked Doubles"), 1);
        assert_eq!(ranked_playlist_sort_priority(13, "Ranked Standard"), 2);
        assert_eq!(ranked_playlist_sort_priority(27, "Ranked Hoops"), 10);
    }

    #[test]
    fn format_age_handles_seconds_minutes_and_hours() {
        assert_eq!(format_age(10_000, 5_000), "5s ago");
        assert_eq!(format_age(120_000, 0), "2m ago");
        assert_eq!(format_age(7_200_000, 0), "2h ago");
    }
}
