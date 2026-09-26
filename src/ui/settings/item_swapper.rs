use crate::item_swapper::{CatalogPackage, ItemSlot, SoundChoice, SwapHealth};
use crate::state::AppState;
use eframe::egui;
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct ItemSwapperUiState {
    pub slot: Option<ItemSlot>,
    pub donor_search: String,
    pub target_search: String,
    pub donor: Option<String>,
    pub target: Option<String>,
    pub sound: SoundChoice,
    pub sound_search: String,
    pub sound_bank: Option<String>,
    pub choose_sound: bool,
}

pub(crate) enum ItemSwapperAction {
    Apply {
        donor: String,
        target: String,
        sound: SoundChoice,
    },
}

pub(crate) fn render_item_swapper_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    edit: &mut ItemSwapperUiState,
    rocket_league_path: &str,
    is_rl_running: bool,
) -> Option<ItemSwapperAction> {
    crate::item_swapper::request_catalog(state, rocket_league_path.to_owned(), false);
    let snapshot = state.item_swapper.snapshot.load();
    let mut action = None;

    ui.heading("Item Swapper");
    ui.label("Make one installed cosmetic package appear in place of another. Changes are local to this Rocket League installation.");
    ui.colored_label(
        egui::Color32::from_rgb(255, 188, 72),
        "Editing game files can violate Rocket League's Terms of Service and may carry account risk.",
    );
    ui.add_space(8.0);

    ui.horizontal_wrapped(|ui| {
        ui.label("Item type:");
        for slot in ItemSlot::ALL {
            ui.selectable_value(&mut edit.slot, Some(slot), slot.label());
        }
    });
    if edit.slot.is_none() {
        edit.slot = Some(ItemSlot::RocketBoost);
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!snapshot.refreshing, egui::Button::new("Refresh catalog"))
            .clicked()
        {
            crate::item_swapper::request_catalog(state, rocket_league_path.to_owned(), true);
        }
        if snapshot.refreshing {
            ui.add(egui::Spinner::new());
        }
        ui.label(&snapshot.message);
    });

    let packages = snapshot
        .packages
        .iter()
        .filter(|p| Some(p.slot) == edit.slot)
        .collect::<Vec<_>>();
    if edit
        .donor
        .as_ref()
        .is_some_and(|selected| !packages.iter().any(|p| p.package == *selected))
    {
        edit.donor = None;
    }
    if edit
        .target
        .as_ref()
        .is_some_and(|selected| !packages.iter().any(|p| p.package == *selected))
    {
        edit.target = None;
    }
    ui.add_space(6.0);
    ui.columns(2, |columns| {
        render_picker(
            &mut columns[0],
            "1. Appearance to copy (source)",
            "item_swap_donor",
            &packages,
            &mut edit.donor_search,
            &mut edit.donor,
        );
        render_picker(
            &mut columns[1],
            "2. Item being replaced (target)",
            "item_swap_target",
            &packages,
            &mut edit.target_search,
            &mut edit.target,
        );
    });

    let is_boost = edit.slot == Some(ItemSlot::RocketBoost);
    let mut sound_problem = None;
    if is_boost {
        ui.add_space(8.0);
        ui.strong("3. Boost sound");
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(
                    !edit.choose_sound && edit.sound == SoundChoice::Original,
                    "Keep original target sound",
                )
                .clicked()
            {
                edit.choose_sound = false;
                edit.sound = SoundChoice::Original;
            }
            if ui
                .selectable_label(
                    !edit.choose_sound && edit.sound == SoundChoice::MatchAppearance,
                    "Match appearance",
                )
                .clicked()
            {
                edit.choose_sound = false;
                edit.sound = SoundChoice::MatchAppearance;
            }
            if ui
                .selectable_label(edit.choose_sound, "Choose another sound")
                .clicked()
            {
                edit.choose_sound = true;
            }
        });
        if edit.choose_sound {
            ui.add(
                egui::TextEdit::singleline(&mut edit.sound_search).hint_text("Search boost sounds"),
            );
            let needle = edit.sound_search.to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("boost_sound_picker")
                .max_height(130.0)
                .show(ui, |ui| {
                    for bank in snapshot.sounds.iter().filter(|s| {
                        s.label.to_lowercase().contains(&needle)
                            || s.file.to_lowercase().contains(&needle)
                    }) {
                        let response = ui.add_enabled(
                            bank.unavailable.is_none(),
                            egui::SelectableLabel::new(
                                edit.sound_bank.as_deref() == Some(bank.file.as_str()),
                                &bank.label,
                            ),
                        );
                        if response.clicked() {
                            edit.sound_bank = Some(bank.file.clone());
                        }
                        if let Some(reason) = &bank.unavailable {
                            response.on_disabled_hover_text(reason);
                        }
                    }
                });
            if let Some(bank) = &edit.sound_bank {
                edit.sound = SoundChoice::Bank(bank.clone());
            } else {
                sound_problem = Some("Choose a sound to copy.".to_owned());
            }
        }
        if edit.sound != SoundChoice::Original {
            let source_bank = match &edit.sound {
                SoundChoice::Bank(bank) => Some(bank),
                SoundChoice::MatchAppearance => edit
                    .donor
                    .as_ref()
                    .and_then(|p| snapshot.boost_banks.get(p)),
                SoundChoice::Original => None,
            };
            if let Some(bank) = source_bank {
                ui.label(format!("Sound: {}", crate::boost_audio::display_name(bank)));
                if let Some(info) = snapshot.sounds.iter().find(|s| s.file == *bank) {
                    if let Some(reason) = &info.unavailable {
                        sound_problem = Some(reason.clone());
                    }
                } else {
                    sound_problem =
                        Some("This sound is no longer installed. Refresh the catalog.".into());
                }
            } else if edit.donor.is_some() {
                sound_problem = Some("This appearance's sound could not be identified. Choose another sound or keep the original.".into());
            }
            if let Some(target) = &edit.target {
                if let Some(bank) = snapshot.boost_banks.get(target) {
                    if let Some(reason) = snapshot
                        .sounds
                        .iter()
                        .find(|s| &s.file == bank)
                        .and_then(|s| s.unavailable.as_ref())
                    {
                        sound_problem = Some(format!("Target sound unavailable: {reason}"));
                    }
                    let shared = snapshot.boost_banks.values().filter(|b| *b == bank).count();
                    if shared > 1 {
                        ui.weak(format!("This sound is shared by {shared} boost packages. They will all use the replacement sound."));
                    }
                } else {
                    sound_problem = Some("This target's sound could not be identified. Its appearance can still be swapped with the original sound.".into());
                }
            }
        }
        ui.weak("To hear a boost's own sound, turn off Rocket League's setting that uses Standard Boost audio for every boost.");
        if let Some(problem) = &sound_problem {
            ui.colored_label(egui::Color32::from_rgb(255, 188, 72), problem);
        }
    }

    match (&edit.donor, &edit.target) {
        (Some(donor), Some(target)) => {
            let donor_name = display_name(&packages, donor);
            let target_name = display_name(&packages, target);
            ui.group(|ui| {
                ui.strong("What this swap does");
                ui.label(format!(
                    "When Rocket League loads {target_name}, it will show {donor_name}'s appearance."
                ));
                ui.monospace(format!("Replace {target} with {donor} visuals"));
            });
        }
        _ => {
            ui.weak("Choose the appearance to copy, then choose the item that should display it.");
        }
    }
    ui.add_space(6.0);

    let can_apply = !snapshot.running
        && !is_rl_running
        && edit.donor.is_some()
        && edit.target.is_some()
        && sound_problem.is_none()
        && (edit.donor != edit.target || (is_boost && edit.sound != SoundChoice::Original));
    if ui
        .add_enabled(
            can_apply,
            egui::Button::new(if is_boost {
                "Apply appearance and sound"
            } else {
                "Replace target with source appearance"
            }),
        )
        .clicked()
    {
        action = Some(ItemSwapperAction::Apply {
            donor: edit.donor.clone().unwrap(),
            target: edit.target.clone().unwrap(),
            sound: if is_boost {
                edit.sound.clone()
            } else {
                SoundChoice::Original
            },
        });
    }
    if is_rl_running {
        ui.colored_label(
            egui::Color32::from_rgb(230, 120, 80),
            "Close Rocket League before applying or restoring swaps.",
        );
    }
    if snapshot.running {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
            ui.label("Changing game files…");
        });
    }

    ui.add_space(12.0);
    ui.separator();
    ui.heading("Active swaps");
    if snapshot.active.is_empty() {
        ui.weak("No swaps are recorded.");
    }
    for swap in &snapshot.active {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "{} now displays {}'s appearance",
                swap.target_package, swap.donor_package
            ));
            if swap.sound_choice() != SoundChoice::Original {
                let label = match swap.sound_choice() {
                    SoundChoice::MatchAppearance => "matches appearance".to_owned(),
                    SoundChoice::Bank(bank) => crate::boost_audio::display_name(&bank),
                    SoundChoice::Original => unreachable!(),
                };
                ui.label(format!("Sound: {label}"));
            }
            let color = match swap.health {
                SwapHealth::Active => egui::Color32::from_rgb(80, 200, 120),
                SwapHealth::NeedsReapply => egui::Color32::from_rgb(255, 188, 72),
                SwapHealth::Conflict => egui::Color32::from_rgb(230, 100, 80),
            };
            ui.colored_label(color, swap.health.label());
            if swap.health == SwapHealth::NeedsReapply
                && ui
                    .add_enabled(
                        !snapshot.running && !is_rl_running,
                        egui::Button::new("Reapply"),
                    )
                    .clicked()
            {
                crate::item_swapper::start_reapply(
                    state.clone(),
                    rocket_league_path.to_owned(),
                    swap.target_package.clone(),
                );
            }
            if ui
                .add_enabled(
                    !snapshot.running && !is_rl_running,
                    egui::Button::new("Restore"),
                )
                .clicked()
            {
                crate::item_swapper::start_restore(
                    state.clone(),
                    rocket_league_path.to_owned(),
                    swap.target_package.clone(),
                );
            }
        });
    }
    action
}

fn display_name(packages: &[&CatalogPackage], package: &str) -> String {
    packages
        .iter()
        .find(|candidate| candidate.package == package)
        .map(|candidate| candidate.display_name())
        .unwrap_or_else(|| package.to_owned())
}

fn render_picker(
    ui: &mut egui::Ui,
    title: &str,
    id: &'static str,
    packages: &[&CatalogPackage],
    search: &mut String,
    selected: &mut Option<String>,
) {
    ui.strong(title);
    ui.add(egui::TextEdit::singleline(search).hint_text("Search items or packages"));
    let needle = search.trim().to_lowercase();
    let filtered = packages
        .iter()
        .copied()
        .filter(|p| {
            needle.is_empty()
                || p.package.to_lowercase().contains(&needle)
                || p.labels
                    .iter()
                    .any(|label| label.to_lowercase().contains(&needle))
        })
        .collect::<Vec<_>>();
    egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(220.0)
        .auto_shrink([false, false])
        .show_rows(ui, 22.0, filtered.len(), |ui, rows| {
            for package in &filtered[rows] {
                let response = ui.selectable_label(
                    selected.as_deref() == Some(&package.package),
                    package.display_name(),
                );
                if response.clicked() {
                    *selected = Some(package.package.clone());
                }
                response.on_hover_text(format!(
                    "{}\n{}",
                    package.package,
                    package.labels.join(", ")
                ));
            }
        });
    if let Some(value) = selected.as_deref() {
        ui.monospace(value);
    }
}
