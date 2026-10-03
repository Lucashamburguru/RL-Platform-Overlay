//! Shared, virtualized tables with a full-width detail row and a fixed header.
use eframe::egui;

pub(super) struct Column {
    pub label: &'static str,
    pub sort: usize,
    pub width: f32,
    pub numeric: bool,
}

pub(super) fn remaining_height(ui: &egui::Ui) -> f32 {
    (ui.clip_rect().bottom() - ui.cursor().top() - 8.0).max(40.0)
}

pub(super) fn cell_text(ui: &mut egui::Ui, text: impl Into<String>) {
    let text = text.into();
    ui.add(
        egui::Label::new(egui::RichText::new(&text).color(egui::Color32::from_gray(218)))
            .truncate(),
    )
    .on_hover_text(text);
}

pub(super) fn toggle_selection(selection: &mut Option<String>, key: &str) {
    if selection.as_deref() == Some(key) {
        *selection = None;
    } else {
        *selection = Some(key.to_owned());
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ui: &mut egui::Ui,
    salt: &str,
    columns: &[Column],
    keys: &[String],
    sort: &mut (usize, bool),
    row_height: f32,
    mut cell: impl FnMut(&mut egui::Ui, usize, usize, bool),
    mut details: impl FnMut(&mut egui::Ui, usize),
) {
    let id = ui.make_persistent_id(salt);
    let mut selected = ui.data(|d| d.get_temp::<String>(id.with("selected")));
    let width = (ui.available_width() - ui.spacing().scroll.bar_width - 6.0).max(1.0);
    let column_total: f32 = columns.iter().map(|c| c.width).sum();
    let scale = (width / column_total).min(1.0);
    let widths: Vec<f32> = columns.iter().map(|c| c.width * scale).collect();
    let header_height = ui.text_style_height(&egui::TextStyle::Body).max(18.0) + 14.0;
    let (header, _) =
        ui.allocate_exact_size(egui::vec2(width, header_height), egui::Sense::hover());
    ui.painter()
        .rect_filled(header, 3.0, egui::Color32::from_gray(37));
    let mut x = header.left();
    for (index, column) in columns.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            egui::pos2(x, header.top()),
            egui::vec2(widths[index], header.height()),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(egui::vec2(5.0, 3.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let arrow = if sort.0 == column.sort {
            if sort.1 { " ↓" } else { " ↑" }
        } else {
            ""
        };
        if child
            .add(
                egui::Button::new(egui::RichText::new(format!("{}{arrow}", column.label)).strong())
                    .frame(false)
                    .truncate(),
            )
            .clicked()
        {
            if sort.0 == column.sort {
                sort.1 = !sort.1;
            } else {
                *sort = (column.sort, column.numeric);
            }
        }
        x += widths[index];
    }
    let height = remaining_height(ui);
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .max_height(height)
        .show_viewport(ui, |ui, viewport| {
            let top = ui.cursor().top();
            let left = ui.cursor().left();
            let mut offset = 0.0;
            for (index, key) in keys.iter().enumerate() {
                let expanded = selected.as_deref() == Some(key.as_str());
                let detail_id = id.with(("height", key, width.to_bits()));
                let old_detail_height = if expanded {
                    ui.data(|d| d.get_temp::<f32>(detail_id)).unwrap_or(220.0)
                } else {
                    0.0
                };
                let visible = offset + row_height + old_detail_height >= viewport.top()
                    && offset <= viewport.bottom();
                let mut detail_height = old_detail_height;
                if visible {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(left, top + offset),
                        egui::vec2(width, row_height),
                    );
                    let response = ui.interact(rect, id.with(("row", key)), egui::Sense::click());
                    let color = if expanded {
                        egui::Color32::from_rgb(29, 55, 64)
                    } else if response.hovered() {
                        egui::Color32::from_gray(43)
                    } else if index % 2 == 0 {
                        egui::Color32::from_gray(29)
                    } else {
                        egui::Color32::from_gray(24)
                    };
                    ui.painter().rect_filled(rect, 0.0, color);
                    if response.clicked() {
                        toggle_selection(&mut selected, key);
                        ui.ctx().request_repaint();
                    }
                    let mut x = left;
                    for (column, spec) in columns.iter().enumerate() {
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(x, rect.top()),
                            egui::vec2(widths[column], row_height),
                        );
                        let layout = if spec.numeric {
                            egui::Layout::right_to_left(egui::Align::Center)
                        } else {
                            egui::Layout::left_to_right(egui::Align::Center)
                        };
                        let mut child = ui.new_child(
                            egui::UiBuilder::new()
                                .id_salt((id, key, column))
                                .max_rect(rect.shrink2(egui::vec2(7.0, 3.0)))
                                .layout(layout),
                        );
                        child.set_clip_rect(rect.intersect(ui.clip_rect()));
                        cell(&mut child, index, column, expanded);
                        x += widths[column];
                    }
                    if expanded {
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(left + 12.0, top + offset + row_height + 6.0),
                            egui::vec2((width - 24.0).max(1.0), f32::INFINITY),
                        );
                        let mut child = ui.new_child(
                            egui::UiBuilder::new()
                                .id_salt((id, key, "detail"))
                                .max_rect(rect)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                        );
                        child.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        details(&mut child, index);
                        detail_height = child.min_size().y + 18.0;
                        ui.data_mut(|d| d.insert_temp(detail_id, detail_height));
                        if (detail_height - old_detail_height).abs() > 1.0 {
                            ui.ctx().request_repaint();
                        }
                    }
                }
                offset += row_height + detail_height;
            }
            ui.set_min_size(egui::vec2(width, offset));
        });
    ui.data_mut(|d| {
        if let Some(key) = selected {
            d.insert_temp(id.with("selected"), key);
        } else {
            d.remove::<String>(id.with("selected"));
        }
    });
}

pub(super) fn local_timestamp(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "—".into())
}

pub(super) fn local_timestamp_detail(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S %:z")
                .to_string()
        })
        .unwrap_or_else(|| "—".into())
}

#[cfg(test)]
pub(super) fn assert_page_layout(name: &str, mut render: impl FnMut(&mut egui::Ui, bool)) {
    for size in [[640.0, 600.0], [760.0, 820.0], [1100.0, 820.0]] {
        for zoom in [1.0, 1.5] {
            for expanded in [false, true] {
                let ctx = egui::Context::default();
                ctx.set_visuals(egui::Visuals::dark());
                crate::ui::fonts::install_fallbacks(&ctx);
                egui_extras::install_image_loaders(&ctx);
                ctx.set_zoom_factor(zoom);
                let mut renderer = crate::ui::review_renderer::ReviewRenderer::default();
                for frame in 0..3 {
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(size[0], size[1]),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
                                ui.label("Launch Overlay    Arrange HUD    Quit");
                            });
                            egui::CentralPanel::default().show(ctx, |ui| {
                                crate::ui::common::settings_style(ui);
                                let bounds = ui.max_rect();
                                let page = egui::ScrollArea::vertical()
                                    .id_salt("page")
                                    .auto_shrink([false, false])
                                    .max_height(ui.available_height())
                                    .show(ui, |ui| render(ui, expanded));
                                assert!(
                                    page.content_size.y <= page.inner_rect.height() + 1.0,
                                    "{name} nested page overflow at {size:?}/{zoom}"
                                );
                                assert!(
                                    ui.min_rect().right() <= bounds.right() + 1.0,
                                    "{name} width overflow at {size:?}/{zoom}: {:?}",
                                    ui.min_rect()
                                );
                                assert!(
                                    ui.min_rect().bottom() <= bounds.bottom() + 1.0,
                                    "{name} height overflow at {size:?}/{zoom}: {:?}",
                                    ui.min_rect()
                                );
                            });
                        },
                    );
                    let path = std::env::var_os("RL_LIBRARY_REVIEW_DIR")
                        .filter(|_| frame == 2)
                        .map(|dir| {
                            let dir = std::path::PathBuf::from(dir);
                            std::fs::create_dir_all(&dir).unwrap();
                            dir.join(format!(
                                "{name}-{}-{zoom}-{}.png",
                                size[0],
                                if expanded { "expanded" } else { "collapsed" }
                            ))
                        });
                    if std::env::var_os("RL_LIBRARY_REVIEW_DIR").is_some() {
                        renderer.capture(&ctx, &output, size, path.as_deref());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanded_key_survives_reordering_and_background_refresh() {
        let ctx = egui::Context::default();
        let columns = [Column {
            label: "Name",
            sort: 0,
            width: 300.0,
            numeric: false,
        }];
        let mut sort = (0, false);
        let mut detail_keys = Vec::new();
        for frame in 0..3 {
            let keys = if frame == 0 {
                vec!["a".to_owned(), "b".to_owned()]
            } else {
                vec!["b".to_owned(), "c".to_owned(), "a".to_owned()]
            };
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if frame == 0 {
                            let id = ui.make_persistent_id("stable").with("selected");
                            ui.data_mut(|d| d.insert_temp(id, "a".to_owned()));
                        }
                        show(
                            ui,
                            "stable",
                            &columns,
                            &keys,
                            &mut sort,
                            34.0,
                            |ui, i, _, _| cell_text(ui, &keys[i]),
                            |ui, i| {
                                detail_keys.push(keys[i].clone());
                                ui.label("Details");
                            },
                        );
                    });
                },
            );
        }
        assert_eq!(detail_keys, vec!["a", "a", "a"]);
    }

    #[test]
    fn table_virtualizes_large_lists_and_expands_under_clicked_row() {
        let ctx = egui::Context::default();
        let keys: Vec<_> = (0..10000).map(|i| format!("row-{i}")).collect();
        let columns = [Column {
            label: "Name",
            sort: 0,
            width: 300.0,
            numeric: false,
        }];
        let mut sort = (0, false);
        let mut cells = 0;
        let mut detail_draws = 0;
        for frame in 0..4 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500.0, 400.0),
                )),
                ..Default::default()
            };
            if frame == 1 || frame == 2 {
                input
                    .events
                    .push(egui::Event::PointerMoved(egui::pos2(60.0, 60.0)));
                input.events.push(egui::Event::PointerButton {
                    pos: egui::pos2(60.0, 60.0),
                    button: egui::PointerButton::Primary,
                    pressed: frame == 1,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(
                        ui,
                        "test_table",
                        &columns,
                        &keys,
                        &mut sort,
                        34.0,
                        |ui, _, _, _| {
                            cells += 1;
                            cell_text(ui, "Player");
                        },
                        |ui, _| {
                            detail_draws += 1;
                            ui.label("Expanded details");
                        },
                    );
                });
            });
        }
        assert!(cells < 100, "only visible rows should be rendered: {cells}");
        assert!(detail_draws > 0, "clicking the row should reveal details");
    }
}
