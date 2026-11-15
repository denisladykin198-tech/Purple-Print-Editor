//! Модальное окно подтверждения смены размера сетки.
//! Управление: Enter — подтвердить, Esc или клик по фону — отмена.

use bevy_egui::egui::{self, Color32, RichText};

use crate::app::state::PendingGridResize;

use super::theme::palette as p;
use super::widgets as w;

const DIALOG_WIDTH: f32 = 360.0;

pub fn draw_modal_dialog(ctx: &egui::Context, pending: &mut PendingGridResize) {
    if !pending.confirm_dialog_open {
        return;
    }

    let mut confirm = false;
    let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        confirm = true;
    }

    let screen = ctx.screen_rect();

    // Dimmer: перехватывает клик по фону как отмену.
    egui::Area::new(egui::Id::new("modal_dimmer"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let resp = ui.allocate_response(screen.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(resp.rect, 0.0, Color32::from_black_alpha(160));
            if resp.clicked() {
                cancel = true;
            }
        });

    // Само окно.
    egui::Area::new(egui::Id::new("resize_confirm_dialog"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(p::PANEL)
                .rounding(egui::Rounding::same(12.0))
                .stroke(egui::Stroke::new(1.0, p::BORDER))
                .inner_margin(egui::Margin::same(20.0))
                .show(ui, |ui| {
                    ui.set_width(DIALOG_WIDTH);

                    ui.horizontal(|ui| {
                        w::icon(ui, "warning", 16.0, p::WARN);
                        ui.label(RichText::new("Пересоздать сетку?").strong().size(16.0));
                    });

                    ui.add_space(10.0);

                    let size_txt = pending
                        .new_size
                        .map(|s| format!("{}³", s))
                        .unwrap_or_else(|| "—".into());
                    ui.label(
                        RichText::new(format!("Новый размер: {}", size_txt)).color(p::TEXT),
                    );

                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "2D-холсты и 3D-сетка активного фрейма будут пересозданы.\n\
                             Текущие рисунки будут потеряны.",
                        )
                        .size(12.5)
                        .color(p::TEXT_DIM),
                    );

                    ui.add_space(16.0);

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if w::danger_button_icon(ui, "trash", "Пересоздать").clicked() {
                                confirm = true;
                            }
                            if w::ghost_button(ui, "Отмена").clicked() {
                                cancel = true;
                            }
                        },
                    );
                });
        });

    if confirm {
        pending.confirm_dialog_open = false;
    } else if cancel {
        pending.confirm_dialog_open = false;
        pending.new_size = None;
    }
}