//! Stage 7: цветовые схемы («текстуры») для воксельной модели.
//!
//! Раскраска — vertex-color: `render::textures::sample_voxel_color` даёт
//! цвет для каждого вокселя, `meshing` кладёт его в `ATTRIBUTE_COLOR`.
//! Превью в этой панели использует ту же функцию (`rainbow_from_height`,
//! `lerp3`, `hash_noise`), поэтому 2D-превью совпадает с 3D-моделью.
//!
//! Кнопка «Применить» переключает `TextureUiState::applied_mode`;
//! `update_voxel_mesh_system` замечает разницу и пересобирает чанки.

use bevy::prelude::*;
use bevy_egui::egui::RichText;
use bevy_egui::{egui, EguiContexts};

use crate::app::state::{
    ActiveFrame, FullscreenState, TextureMode, TextureUiState, ViewType,
};
use crate::core::mawt_engine::mawt_utils::embedded_3d_view;
use crate::grid::grid::VoxelGrid;
use crate::model::ProjectDoc;
use crate::render::textures::{hash_noise, lerp3, rainbow_from_height};

use super::theme::palette as p;
use super::widgets as w;

const PREVIEW_SIZE: usize = 128;
const CHECKER_CELL: usize = 8;

#[allow(clippy::too_many_arguments)]
pub fn draw_stage_textures(
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
    grid: &VoxelGrid,
    doc: &mut ProjectDoc,
    active: &ActiveFrame,
    ui_state: &mut TextureUiState,
    fullscreen: &mut FullscreenState,
) {
    let frame_name = doc
        .frame(active.id)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "?".to_string());

    w::toolbar(ui, |ui| {
        ui.label(
            RichText::new(format!("Текстуры · {}", frame_name))
                .strong()
                .size(14.0),
        );
        ui.separator();
        w::badge(ui, ui_state.mode.label(), p::ACCENT);

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
            draw_texture_preview_3d(
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
                .id_source("stage7_textures_panel")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(panel_width);
                    draw_texture_panel(ui, grid, ui_state);
                });
        });
    });
}

// ============================================================================
// Панель настроек
// ============================================================================

fn draw_texture_panel(ui: &mut egui::Ui, grid: &VoxelGrid, ui_state: &mut TextureUiState) {
    // Режим.
    w::card(ui, |ui| {
        w::section(ui, "Режим текстуры");
        ui.add_space(6.0);

        for mode in [
            TextureMode::RainbowHeight,
            TextureMode::Solid,
            TextureMode::HeightGradient,
            TextureMode::Checker,
            TextureMode::Noise,
        ] {
            let selected = ui_state.mode == mode;
            if ui.radio(selected, mode.label()).clicked() {
                if ui_state.mode != mode {
                    ui_state.mode = mode;
                    ui_state.preview_dirty = true;
                }
            }
            ui.add_space(2.0);
        }

        ui.add_space(4.0);
        w::hint(ui, ui_state.mode.hint());
    });

    ui.add_space(8.0);

    // Цвета.
    w::card(ui, |ui| {
        w::section(ui, "Цвета");
        ui.add_space(6.0);

        egui::Grid::new("texture_colors")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Основной:").color(p::TEXT_DIM));
                if ui.color_edit_button_rgb(&mut ui_state.primary_color).changed() {
                    ui_state.preview_dirty = true;
                }
                ui.end_row();

                ui.label(RichText::new("Второй:").color(p::TEXT_DIM));
                if ui.color_edit_button_rgb(&mut ui_state.secondary_color).changed() {
                    ui_state.preview_dirty = true;
                }
                ui.end_row();
            });
    });

    ui.add_space(8.0);

    // Дополнительные параметры.
    match ui_state.mode {
        TextureMode::Noise => {
            w::card(ui, |ui| {
                w::section(ui, "Параметры шума");
                ui.add_space(6.0);

                egui::Grid::new("noise_params")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Масштаб:").color(p::TEXT_DIM));
                        if ui
                            .add(
                                egui::Slider::new(&mut ui_state.noise_scale, 0.5..=16.0)
                                    .step_by(0.5),
                            )
                            .changed()
                        {
                            ui_state.preview_dirty = true;
                        }
                        ui.end_row();

                        ui.label(RichText::new("Seed:").color(p::TEXT_DIM));
                        let mut seed = ui_state.noise_seed as i64;
                        if ui
                            .add(
                                egui::DragValue::new(&mut seed)
                                    .speed(1.0)
                                    .clamp_range(0..=1_000_000),
                            )
                            .changed()
                        {
                            ui_state.noise_seed = seed.max(0) as u64;
                            ui_state.preview_dirty = true;
                        }
                        ui.end_row();
                    });
            });
            ui.add_space(8.0);
        }
        TextureMode::Checker => {
            w::card(ui, |ui| {
                w::section(ui, "Параметры");
                ui.add_space(4.0);
                w::hint(ui, "Размер клетки фиксирован — 8 вокселей.");
            });
            ui.add_space(8.0);
        }
        TextureMode::RainbowHeight | TextureMode::Solid | TextureMode::HeightGradient => {}
    }

    // Превью схемы.
    w::card(ui, |ui| {
        w::section(ui, "Превью текстуры");
        ui.add_space(6.0);

        if ui_state.preview_tex.is_none() || ui_state.preview_dirty {
            let image = generate_preview_image(ui_state);
            if let Some(tex) = ui_state.preview_tex.as_mut() {
                tex.set(image, egui::TextureOptions::NEAREST);
            } else {
                ui_state.preview_tex = Some(ui.ctx().load_texture(
                    "mpp_texture_preview",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }
            ui_state.preview_dirty = false;
        }

        if let Some(tex) = ui_state.preview_tex.as_ref() {
            let side = ui.available_width().min(240.0).max(64.0);
            ui.add(
                egui::Image::new(tex)
                    .fit_to_exact_size(egui::vec2(side, side))
                    .rounding(6.0),
            );
        }

        ui.add_space(4.0);
        w::hint(
            ui,
            format!(
                "Палитра {}×{} · {}",
                PREVIEW_SIZE,
                PREVIEW_SIZE,
                match ui_state.mode {
                    TextureMode::RainbowHeight => "спектр по Y",
                    TextureMode::Solid => "один цвет",
                    TextureMode::HeightGradient => "градиент по Y",
                    TextureMode::Checker => "шахматка XY",
                    TextureMode::Noise => "процедурный шум",
                }
            ),
        );
    });

    ui.add_space(8.0);

    // Применить.
    w::card(ui, |ui| {
        w::section(ui, "Действие");
        ui.add_space(6.0);

        if w::primary_button_icon_full(ui, "sparkle", "Применить к модели").clicked() {
            ui_state.applied_mode = Some(ui_state.mode);
            ui_state.status = Some(format!(
                "Схема «{}» применена к воксельному мешу",
                ui_state.mode.label()
            ));
        }

        if let Some(status) = &ui_state.status {
            ui.add_space(6.0);
            ui.label(RichText::new(status).size(11.5).color(p::SUCCESS));
        }
    });

    ui.add_space(8.0);

    // Пояснение.
    w::card(ui, |ui| {
        w::section(ui, "Как это работает в играх");
        ui.add_space(6.0);
        w::hint(
            ui,
            "Для воксельных моделей самый практичный путь — vertex-color / palette: \
             у каждого вокселя есть цвет, и шейдер просто умножает его на материал. \
             Это дёшево, легко сериализуется в файл и не требует UV-развёртки.",
        );
        ui.add_space(4.0);
        w::hint(
            ui,
            "Альтернативы: атлас с UV (общий PNG-файл), triplanar-шейдер \
             (цвет из мировой позиции). В Mawt Purple Print движок принимает \
             vertex-color напрямую, так что этот режим — приоритетный.",
        );
        ui.add_space(4.0);
        w::hint(
            ui,
            format!(
                "Плотность модели: {}³ = {} вокселей. \
                 Цвета применятся к каждой видимой грани.",
                grid.size.x,
                (grid.size.x as u64) * (grid.size.y as u64) * (grid.size.z as u64),
            ),
        );
    });
}

// ============================================================================
// Генерация превью
// ============================================================================

fn generate_preview_image(state: &TextureUiState) -> egui::ColorImage {
    let n = PREVIEW_SIZE;
    let mut pixels = Vec::with_capacity(n * n);

    for y in 0..n {
        for x in 0..n {
            let fx = x as f32 / n as f32;
            let fy = y as f32 / n as f32;
            let t = 1.0 - fy;

            let c = match state.mode {
                TextureMode::RainbowHeight => rainbow_from_height(t),
                TextureMode::Solid => state.primary_color,
                TextureMode::HeightGradient => {
                    lerp3(state.primary_color, state.secondary_color, t)
                }
                TextureMode::Checker => {
                    let on = ((x / CHECKER_CELL) + (y / CHECKER_CELL)) % 2 == 0;
                    if on {
                        state.primary_color
                    } else {
                        state.secondary_color
                    }
                }
                TextureMode::Noise => {
                    let nv = hash_noise(
                        (fx * n as f32 * state.noise_scale) as u32,
                        (fy * n as f32 * state.noise_scale) as u32,
                        state.noise_seed,
                    );
                    lerp3(state.primary_color, state.secondary_color, nv)
                }
            };

            pixels.push(egui::Color32::from_rgb(
                (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                (c[2].clamp(0.0, 1.0) * 255.0) as u8,
            ));
        }
    }

    egui::ColorImage {
        size: [n, n],
        pixels,
    }
}

// ============================================================================
// 3D-превью
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn draw_texture_preview_3d(
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
            "Perspective — текстуры",
            ViewType::Perspective,
            fullscreen,
            |_| {},
        );

        let Ok((mut cam_data, mut transform, _camera_comp, _global_transform, _projection)) =
            cam_query.get_mut(camera_entity)
        else {
            ui.label("Камера не найдена");
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