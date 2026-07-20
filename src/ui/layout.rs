use crate::state::AppState;
use eframe::egui;
use std::sync::Arc;

pub(super) fn render_drag_position_handle(
    ui: &mut egui::Ui,
    enabled: bool,
    scale: f32,
) -> Option<egui::Response> {
    if !enabled {
        return None;
    }

    ui.add_space(2.0 * scale);
    let size = egui::vec2(78.0 * scale, 15.0 * scale);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
    let rounding = 3.0 * scale;
    ui.painter().rect_filled(
        rect,
        rounding,
        egui::Color32::from_rgba_unmultiplied(80, 68, 24, 190),
    );
    ui.painter().rect_stroke(
        rect,
        rounding,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 190, 90)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Drag to move",
        egui::FontId::proportional(8.0 * scale),
        egui::Color32::from_rgb(245, 220, 130),
    );
    Some(response)
}

pub(super) fn normalized_to_pos(ctx: &egui::Context, position: [f32; 2]) -> egui::Pos2 {
    let rect = ctx.input(|i| i.screen_rect());
    egui::pos2(
        rect.left() + rect.width() * position[0].clamp(0.0, 1.0),
        rect.top() + rect.height() * position[1].clamp(0.0, 1.0),
    )
}

fn pos_to_normalized(ctx: &egui::Context, pos: egui::Pos2) -> [f32; 2] {
    let rect = ctx.input(|i| i.screen_rect());
    [
        ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0),
        ((pos.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0),
    ]
}

fn layout_drag_position_id(target: &str) -> egui::Id {
    egui::Id::new(("layout_drag_position", target))
}

fn layout_drag_start_id(target: &str) -> egui::Id {
    egui::Id::new(("layout_drag_pointer_offset", target))
}

pub(super) fn active_layout_drag_position(ctx: &egui::Context, target: &str) -> Option<egui::Pos2> {
    ctx.data(|data| data.get_temp::<egui::Pos2>(layout_drag_position_id(target)))
}

pub(super) fn persist_dragged_position(
    ctx: &egui::Context,
    state: &Arc<AppState>,
    panel_pos: egui::Pos2,
    target: &str,
    drag_response: &egui::Response,
) {
    if !drag_response.drag_started() && !drag_response.dragged() && !drag_response.drag_stopped() {
        return;
    }

    let drag_offset_id = layout_drag_start_id(target);
    let drag_position_id = layout_drag_position_id(target);

    if drag_response.drag_started()
        && let Some(pointer_pos) = drag_response.interact_pointer_pos()
    {
        ctx.data_mut(|data| data.insert_temp(drag_offset_id, pointer_pos - panel_pos));
    }

    let pointer_pos = drag_response
        .interact_pointer_pos()
        .unwrap_or(panel_pos + drag_response.drag_delta());
    let pointer_offset = ctx
        .data(|data| data.get_temp::<egui::Vec2>(drag_offset_id))
        .unwrap_or_default();
    let new_panel_pos = pointer_pos - pointer_offset;
    ctx.data_mut(|data| data.insert_temp(drag_position_id, new_panel_pos));

    let new_position = pos_to_normalized(ctx, new_panel_pos);
    if drag_response.drag_stopped() {
        state.update_config(|config| match target {
            "lobby" => config.lobby_manual_position = Some(new_position),
            "boost" => config.teammate_boost_manual_position = Some(new_position),
            "session" => config.session_manual_position = Some(new_position),
            _ => {}
        });
    }
    ctx.request_repaint();

    if drag_response.drag_stopped() {
        ctx.data_mut(|data| {
            data.remove_temp::<egui::Vec2>(drag_offset_id);
            data.remove_temp::<egui::Pos2>(drag_position_id);
        });
    }
}
