//! Stage 6: сглаживание воксельной геометрии.
//!
//! Каркас готов под несколько алгоритмов (Laplacian / Median / Gaussian / Taubin).
//! В этой версии реально реализован Median — самый простой и предсказуемый.
//! Остальные показываются как «скоро» и не применяются, но интерфейс уже на месте.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use bevy_egui::egui::RichText;

use crate::core::mawt_engine::mawt_utils::embedded_3d_view;
use crate::grid::grid::VoxelGrid;
use crate::model::ProjectDoc;
use crate::app::state::{
    ActiveFrame, FullscreenState, SmoothAlgorithm, SmoothUiState, ViewType,
};
use crate::grid::smoothing::apply;
use super::theme::palette as p;
use super::widgets as w;

#[allow(clippy::too_many_arguments)]
pub fn draw_stage_smooth(
    ui: &mut egui::Ui,
    egui_contexts: &mut EguiContexts,
    interaction_state: &mut embedded_3d_view::Embedded3dInteractionState,
    camera_entity: Entity,
    cam_query: &mut Query<(
        &mut embedded_3d_view::Embedded3dCamera,
        &mut Transform,
        &Camera,
        &GlobalTransform,
        &Projection,
    )>,
    grid: &mut VoxelGrid,
    doc: &mut ProjectDoc,
    active: &ActiveFrame,
    ui_state: &mut SmoothUiState,
    fullscreen: &mut FullscreenState,
) {
    let frame_name = doc
        .frame(active.id)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "?".to_string());

    w::toolbar(ui, |ui| {
        ui.label(
            RichText::new(format!("Сглаживание · {}", frame_name))
                .strong()
                .size(14.0),
        );
        ui.separator();

        w::badge(ui, format!("{}³", grid.size.x), p::INFO);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if fullscreen.active == Some(ViewType::Perspective)
                && w::ghost_button_icon(ui, "x", "Выйти (Esc)").clicked()
            {
                fullscreen.active = None;
            }
        });
    });

    ui.add_space(6.0);

    let available = ui.available_size();
    let panel_width = 380.0_f32.min(available.x * 0.42).max(300.0);
    let gap = ui.spacing().item_spacing.x;
    let view_width = (available.x - panel_width - gap).max(160.0);

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(view_width);
            draw_smooth_preview(
                ui,
                egui_contexts,
                interaction_state,
                camera_entity,
                cam_query,
                egui::vec2(view_width, available.y - 20.0),
                fullscreen,
            );
        });

        ui.vertical(|ui| {
            ui.set_width(panel_width);
            egui::ScrollArea::vertical()
                .id_source("stage6_smooth_panel")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(panel_width);
                    draw_smooth_panel(ui, grid, ui_state);
                });
        });
    });
}

// ============================================================================
// Панель настроек
// ============================================================================

fn draw_smooth_panel(ui: &mut egui::Ui, grid: &mut VoxelGrid, ui_state: &mut SmoothUiState) {
    w::card(ui, |ui| {
        w::section(ui, "Алгоритм");
        ui.add_space(6.0);

        for algo in [
            SmoothAlgorithm::Laplacian,
            SmoothAlgorithm::Median,
            SmoothAlgorithm::Gaussian,
            SmoothAlgorithm::Taubin,
        ] {
            let selected = ui_state.algorithm == algo;
            let label = if algo.implemented() {
                algo.label().to_string()
            } else {
                format!("{}  ·  скоро", algo.label())
            };

            ui.horizontal(|ui| {
                let resp = ui.radio(selected, label);
                if resp.clicked() {
                    ui_state.algorithm = algo;
                }
                if selected {
                    w::icon(ui, "check", 12.0, p::SUCCESS);
                }
            });
            ui.add_space(2.0);
        }

        ui.add_space(4.0);
        w::hint(ui, ui_state.algorithm.hint());
    });

    ui.add_space(8.0);

    w::card(ui, |ui| {
        w::section(ui, "Параметры");
        ui.add_space(6.0);

        egui::Grid::new("smooth_params")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Итераций:").color(p::TEXT_DIM));
                ui.add(egui::Slider::new(&mut ui_state.iterations, 1..=10));
                ui.end_row();

                ui.label(RichText::new("Сила:").color(p::TEXT_DIM));
                ui.add(
                    egui::Slider::new(&mut ui_state.strength, 0.1..=1.0)
                        .step_by(0.05)
                        .fixed_decimals(2),
                );
                ui.end_row();
            });

        ui.add_space(4.0);
        w::hint(
            ui,
            "Чем больше итераций и сила — тем сильнее сглаживание, но дольше вычисления.",
        );
    });

    ui.add_space(8.0);

    w::card(ui, |ui| {
        w::section(ui, "Действие");
        ui.add_space(6.0);

        if w::primary_button_icon_full(ui, "sparkle", "Применить сглаживание").clicked() {
            let changed = apply(
                grid,
                ui_state.algorithm,
                ui_state.iterations,
                ui_state.strength,
            );
            ui_state.status = Some(format!(
                "Готово: {} вокселей изменено ({} итераций, сила {:.2})",
                changed, ui_state.iterations, ui_state.strength
            ));
        }

        if let Some(status) = &ui_state.status {
            ui.add_space(6.0);
            ui.label(RichText::new(status).size(12.0).color(p::SUCCESS));
        }
    });

    ui.add_space(8.0);

    w::card(ui, |ui| {
        w::section(ui, "Как это работает");
        ui.add_space(6.0);
        w::hint(
            ui,
            "Сглаживание убирает шум и артефакты — «рваные» границы, одиночные воксели \
             и мелкие дырки. Работает по окрестности 3×3×3. \
             Каждый проход оценивает: если у вокселя слишком мало или слишком много \
             соседей — его состояние меняется. Так граница становится ровнее.",
        );
        ui.add_space(4.0);
        w::hint(
            ui,
            "Median-фильтр работает прямо по воксельной сетке и не меняет объём сильно. \
             Laplacian / Gaussian / Taubin появятся вместе с surface-extraction (marching cubes) \
             и будут сглаживать уже извлечённую поверхность.",
        );
    });
}

// ============================================================================
// 3D-превью (единый шаблон с Stage3/Stage4)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn draw_smooth_preview(
    ui: &mut egui::Ui,
    egui_contexts: &mut EguiContexts,
    interaction_state: &mut embedded_3d_view::Embedded3dInteractionState,
    camera_entity: Entity,
    cam_query: &mut Query<(
        &mut embedded_3d_view::Embedded3dCamera,
        &mut Transform,
        &Camera,
        &GlobalTransform,
        &Projection,
    )>,
    view_size: egui::Vec2,
    fullscreen: &mut FullscreenState,
) {
    ui.group(|ui| {
        ui.set_width(view_size.x);

        w::viewport_header(
            ui,
            "globe",
            "Perspective — сглаживание",
            ViewType::Perspective,
            fullscreen,
            |_| {},
        );

        let Ok((mut cam_data, mut transform, _camera_comp, _global_transform, _projection)) =
            cam_query.get_mut(camera_entity)
        else {
            ui.label("❌ Камера не найдена");
            return;
        };

        let header_h = 32.0;
        let pad = 8.0;
        let img_w = (view_size.x - pad * 2.0).max(64.0);
        let img_h = (view_size.y - header_h - pad * 2.0).max(64.0);
        let image_size = egui::vec2(img_w, img_h);

        let response = embedded_3d_view::draw_embedded_3d_view(
            ui,
            egui_contexts,
            &mut cam_data,
            interaction_state,
            image_size,
        );

        w::navigate_3d_camera(ui, &response, &mut transform);
    });
}