//! Stage 1: 2D-холсты (front/top/side), редактирование кистью/прямоугольником/линией,
//! генерация 3D из силуэта, perspective-превью, импорт фото.
//!
//! ## Симметрия
//!
//! `ToolSettings::{symmetry_x, symmetry_y}` — **live-флаги**, а не
//! пост-обработка. Когда флаг включён, любой инструмент, который пишет
//! пиксели (`apply_brush`, `apply_noise_brush`, `apply_smooth_brush`,
//! `flood_fill`, `draw_line_on_canvas`, `draw_rect_filled`,
//! `draw_ellipse_filled`), проходит через [`paint_with_symmetry`] и
//! автоматически отражает каждую точку относительно центра холста.
//!
//! Прежний `mirror_canvases` (переворот всего холста по кнопке) удалён —
//! это была неверная механика.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use bevy_egui::egui::RichText;
use crate::format::photo::Prepared;
use crate::format::photo::mask_stats;
use crate::format::photo::gray_is_valid;
use crate::core::mawt_engine::mawt_utils::embedded_3d_view;
use crate::mawt_utils::image_stub::load_from_memory;
use crate::canvases::OrthoCanvases;
use crate::constants::{ALLOWED_GRID_SIZES, ROOT_FRAME_ID};
use crate::grid::grid::{
    generate_3d_from_canvases, project_grid_to_canvases, VoxelGrid,
};
use crate::model::ProjectDoc;
use crate::format::photo::{
    apply_mask_opt, otsu_threshold, prepare_canvas, rgba_to_gray_on_white, threshold_mask,
    transform_gray, ApplyMode, FitMode,
};

use crate::app::state::{
    ActiveFrame, EditorCameras, EditorStage, EditorTool, FullscreenState,
    PendingFrameSwitch, PendingGridResize, PhotoImportState, RectSelectionState,
    ToolSettings, ViewType,
};
use crate::ui::stage2::draw_stage_modules;
use crate::ui::ModuleEditorState;
use crate::ui::PerspectiveCamState;
use crate::app::state::PlacementVisualState;
use super::theme::palette as p;
use super::widgets as w;

#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;

// ============================================================================
// Точка входа стадии
// ============================================================================

#[allow(clippy::too_many_arguments)]
pub fn draw_stage_voxels(
    ui: &mut egui::Ui,
    egui_contexts: &mut EguiContexts,
    interaction_state: &mut embedded_3d_view::Embedded3dInteractionState,
    grid: &mut VoxelGrid,
    cameras: &EditorCameras,
    cam_query: &mut Query<(
        &mut embedded_3d_view::Embedded3dCamera,
        &mut Transform,
        &Camera,
        &GlobalTransform,
        &Projection,
    )>,
    tool: &mut EditorTool,
    selection_state: &mut RectSelectionState,
    canvases: &mut OrthoCanvases,
    fullscreen: &mut FullscreenState,
    pending_resize: &mut PendingGridResize,
    active: &ActiveFrame,
    doc: &mut ProjectDoc,
    pending_switch: &mut PendingFrameSwitch,
    photo: &mut PhotoImportState,
    stage: &mut EditorStage,
    persp_cam: &mut PerspectiveCamState,
    placement_state: &mut PlacementVisualState,
    module_ui: &mut ModuleEditorState,
    tool_settings: &mut ToolSettings,
) {
    let frame_name = doc
        .frame(active.id)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "?".to_string());

    // ─── Тулбар ───────────────────────────────────────────────────────
    w::toolbar(ui, |ui| {
        if active.id != ROOT_FRAME_ID {
            if w::ghost_button_icon(ui, "arrow-left", "Корпус").clicked() {
                pending_switch.id = Some(ROOT_FRAME_ID);
            }
            ui.separator();
        }

        w::badge(ui, format!("Фрейм: {}", frame_name), p::ACCENT);
        ui.separator();

        if w::primary_button_icon(ui, "sparkle", "Сгенерировать 3D")
            .on_hover_text("Silhouette intersection из трёх 2D-видов")
            .clicked()
        {
            generate_3d_from_canvases(canvases, grid);
            project_grid_to_canvases(grid, canvases);
        }

        ui.separator();

        ui.label(RichText::new("Размер:").size(12.5).color(p::TEXT_DIM));
        let current_size = canvases.size;
        egui::ComboBox::from_id_source("grid_size_combo")
            .selected_text(format!("{}³", current_size))
            .show_ui(ui, |ui| {
                for &sz in ALLOWED_GRID_SIZES {
                    if ui.selectable_label(sz == current_size, format!("{}³", sz)).clicked()
                        && sz != current_size
                    {
                        pending_resize.new_size = Some(sz);
                        pending_resize.confirm_dialog_open = true;
                    }
                }
            });

        if fullscreen.active.is_some() {
            ui.separator();
            if w::ghost_button_icon(ui, "x", "Выйти (Esc)").clicked() {
                fullscreen.active = None;
            }
        }

        ui.separator();

        // ── «Модули вкл/выкл» — только вне 2D-fullscreen ────────────────
        // На общей перспективе 3D и в обычном режиме кнопка остаётся,
        // т.к. контролирует видимость призрачных модулей в этом же 3D-вьюпорте.
        let is_2d_fullscreen = matches!(
            fullscreen.active,
            Some(ViewType::Front | ViewType::Top | ViewType::Side)
        );
        if !is_2d_fullscreen {
            let label = if placement_state.show_placements { "Модули: вкл" } else { "Модули: выкл" };
            if w::tool_toggle_icon(ui, "eye", placement_state.show_placements, label)
                .on_hover_text("Показывать размещённые модули в 3D-вьюпорте")
                .clicked()
            {
                placement_state.show_placements = !placement_state.show_placements;
            }
        }
    });

    ui.add_space(8.0);

    // ─── Fullscreen ───────────────────────────────────────────────────
if let Some(active_view) = fullscreen.active {
    // Размер фиксируем ДО раскладки, один раз.
    let total = ui.available_size();
    let tool_w = if tool_settings.show_panel { 230.0 } else { 40.0 };
    let gap = ui.spacing().item_spacing.x * 2.0 + 1.0; // spacing + separator
    let canvas_size = egui::vec2(
        (total.x - tool_w - gap).max(64.0),
        total.y.max(64.0),
    );

    ui.horizontal_top(|ui| {
        if tool_settings.show_panel {
            ui.vertical(|ui| {
                ui.set_width(230.0);
                ui.set_min_width(230.0);
                ui.set_max_width(230.0);
                egui::ScrollArea::vertical()
                    .id_source("stage1_tool_drawer") // тот же id — скролл не сбрасывается
                    .auto_shrink([false, false])
                    .show(ui, |ui| draw_tool_panel(ui, tool, tool_settings, canvases));
            });
        } else {
            ui.vertical(|ui| {
                ui.add_space(4.0);
                if w::icon_button(ui, "chevron-right", 16.0, "Показать инструменты").clicked() {
                    tool_settings.show_panel = true;
                }
            });
        }
        ui.separator();

        // ⬇️ КЛЮЧЕВОЕ: вертикальная колонка, иначе group унаследует
        //    horizontal-layout и header+image встанут в ряд и обрежутся.
        ui.vertical(|ui| {
            ui.set_width(canvas_size.x);
            ui.set_max_width(canvas_size.x);
            ui.set_max_height(canvas_size.y);
            match active_view {
                ViewType::Front | ViewType::Top | ViewType::Side => draw_2d_canvas(
                    ui, active_view, canvases, canvas_size,
                    *tool, tool_settings, selection_state, fullscreen, photo,
                ),
                ViewType::Perspective => draw_perspective_view(
                    ui, egui_contexts, interaction_state, cameras.perspective,
                    cam_query, canvas_size, fullscreen, persp_cam,
                ),
            }
        });
    });
    return;
}

    // ─── Раскладка: tool drawer + 2×2 вьюпорты + модули ───────────────
    let available_size = ui.available_size();

    ui.horizontal_top(|ui| {
        // 1. ВЫДВИЖНАЯ ПАНЕЛЬ ИНСТРУМЕНТОВ (fixed-width column)
        if tool_settings.show_panel {
            ui.vertical(|ui| {
                ui.set_width(230.0);
                ui.set_min_width(230.0);
                ui.set_max_width(230.0);
                egui::ScrollArea::vertical()
                    .id_source("stage1_tool_drawer")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        draw_tool_panel(ui, tool, tool_settings, canvases);
                    });
            });
            ui.separator();
        } else {
            // Панель скрыта — небольшая кнопка-развёртка слева от вьюпортов.
            ui.vertical(|ui| {
                ui.add_space(4.0);
                if w::icon_button(ui, "chevron-right", 16.0, "Показать инструменты").clicked() {
                    tool_settings.show_panel = true;
                }
            });
            ui.separator();
        }

        // 2. Площадь под вьюпорты
        let tool_w = if tool_settings.show_panel { 240.0 } else { 40.0 };
        let panel_width = (available_size.x * 0.24).clamp(240.0, 340.0);
        let canvas_area_x =
            (available_size.x - panel_width - 25.0 - tool_w).max(400.0);
        let cell_width = (canvas_area_x / 2.0) - 15.0;
        let cell_height = (available_size.y / 2.0) - 15.0;
        let square_size = cell_width.min(cell_height).max(160.0);
        let final_size = egui::vec2(square_size, square_size);

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
            draw_2d_canvas(ui, ViewType::Front, canvases, final_size, *tool,
                           tool_settings, selection_state, fullscreen, photo);
            draw_2d_canvas(ui, ViewType::Top,   canvases, final_size, *tool,
                           tool_settings, selection_state, fullscreen, photo);
        });

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
            draw_2d_canvas(ui, ViewType::Side, canvases, final_size, *tool,
                           tool_settings, selection_state, fullscreen, photo);
            draw_perspective_view(
                ui, egui_contexts, interaction_state, cameras.perspective,
                cam_query, final_size, fullscreen, persp_cam,
            );
        });

        ui.separator();

        // 3. Модули
        ui.vertical(|ui| {
            ui.set_width(panel_width);
            ui.set_max_height(available_size.y);
            egui::ScrollArea::vertical()
                .id_source("stage1_modules_panel")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(panel_width);
                    draw_stage_modules(ui, doc, active, module_ui, pending_switch, stage);
                });
        });
    });
}

// ============================================================================
// 2D-холст
// ============================================================================

fn draw_2d_canvas(
    ui: &mut egui::Ui,
    view_type: ViewType,
    canvases: &mut OrthoCanvases,
    view_size: egui::Vec2,
    current_tool: EditorTool,
    settings: &ToolSettings,
    selection_state: &mut RectSelectionState,
    fullscreen: &mut FullscreenState,
    photo: &mut PhotoImportState,
) {
    // Защита от inf: если horizontal_top дал бесконечность,
    // ограничиваем размер тем, что реально доступно в ui.
    let max_avail = ui.available_size();
    let safe_size = egui::vec2(
        view_size.x.min(max_avail.x).max(64.0),
        view_size.y.min(max_avail.y).max(64.0),
    );

    ui.group(|ui| {
        ui.set_width(view_size.x);

        let label = match view_type {
            ViewType::Front => "Front (XY)",
            ViewType::Top   => "Top (XZ)",
            ViewType::Side  => "Side (YZ)",
            _ => "",
        };

        w::viewport_header(ui, "eye", label, view_type, fullscreen, |ui| {
            if w::icon_button(ui, "camera", 16.0, "Импорт фото в этот вид").clicked() {
                photo.pending_dialog_request = Some(view_type);
            }
        });

        let size = canvases.size;
        let side = (safe_size.x - 12.0).min(safe_size.y - 52.0).max(64.0);
        let image_size = egui::vec2(side, side);

        let (data, tex_opt, dirty_flag) = match view_type {
            ViewType::Front => (&canvases.front, &mut canvases.front_tex, &mut canvases.dirty_front),
            ViewType::Top => (&canvases.top, &mut canvases.top_tex, &mut canvases.dirty_top),
            ViewType::Side => (&canvases.side, &mut canvases.side_tex, &mut canvases.dirty_side),
            _ => return,
        };

        // ─── Ленивая пересборка egui-текстуры ─────────────────────────
        if *dirty_flag || tex_opt.is_none() {
            let mut pixels = Vec::with_capacity((size * size) as usize);
            let max_row = size.saturating_sub(1).max(1) as f32;

            for (idx, &v) in data.iter().enumerate() {
                if v == 1 {
                    let row = (idx as u32 / size) as f32;
                    let t = (1.0 - (row / max_row)).clamp(0.0, 1.0);
                    let c = 0.68;
                    let x_val = c * (1.0 - ((t * 6.0) % 2.0 - 1.0).abs());
                    let m = 0.6 - c / 2.0;

                    let (r, g, b) = if t < 1.0 / 6.0 {
                        (c, x_val, 0.0)
                    } else if t < 2.0 / 6.0 {
                        (x_val, c, 0.0)
                    } else if t < 3.0 / 6.0 {
                        (0.0, c, x_val)
                    } else if t < 4.0 / 6.0 {
                        (0.0, x_val, c)
                    } else if t < 5.0 / 6.0 {
                        (x_val, 0.0, c)
                    } else {
                        (c, 0.0, x_val)
                    };

                    pixels.push(egui::Color32::from_rgb(
                        ((r + m) * 255.0) as u8,
                        ((g + m) * 255.0) as u8,
                        ((b + m) * 255.0) as u8,
                    ));
                } else {
                    pixels.push(egui::Color32::from_rgb(28, 28, 34));
                }
            }

            let color_image = egui::ColorImage {
                size: [size as usize, size as usize],
                pixels,
            };

            if let Some(tex) = tex_opt.as_mut() {
                tex.set(color_image, egui::TextureOptions::NEAREST);
            } else {
                *tex_opt = Some(ui.ctx().load_texture(
                    format!("ortho_canvas_{:?}", view_type),
                    color_image,
                    egui::TextureOptions::NEAREST,
                ));
            }

            *dirty_flag = false;
        }

        // ─── Теперь текстура точно существует ─────────────────────────
        let texture = tex_opt.as_ref().expect("canvas texture must exist");
        let response = ui.add(
            egui::Image::new(texture)
                .fit_to_exact_size(image_size)
                .sense(egui::Sense::click_and_drag()),
        );
        let rect = response.rect;

        // ─── Осевые линии симметрии (индикация режима) ────────────────
        if settings.symmetry_x {
            let cx = rect.center().x;
            ui.painter().line_segment(
                [egui::pos2(cx, rect.min.y), egui::pos2(cx, rect.max.y)],
                egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_premultiplied(120, 170, 255, 140),
                ),
            );
        }
        if settings.symmetry_y {
            let cy = rect.center().y;
            ui.painter().line_segment(
                [egui::pos2(rect.min.x, cy), egui::pos2(rect.max.x, cy)],
                egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_premultiplied(120, 170, 255, 140),
                ),
            );
        }

        let to_pixel_clamped = |pos: egui::Pos2, rect: egui::Rect| -> (u32, u32) {
            let u = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
            let v = ((pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);
            let px = (u * (size as f32 - 1.0)).round() as u32;
            let py = (v * (size as f32 - 1.0)).round() as u32;
            (px.clamp(0, size - 1), py.clamp(0, size - 1))
        };
        let to_pixel = |pos: egui::Pos2, rect: egui::Rect| -> Option<(u32, u32)> {
            if !rect.contains(pos) { return None; }
            Some(to_pixel_clamped(pos, rect))
        };

        // ─── Диспетчеризация по инструменту ─────────────────────────────
        let pointer = ui.input(|i| i.pointer.clone());
        let any_button_down = pointer.primary_down() || pointer.secondary_down();
        let brush_active = response.is_pointer_button_down_on() && any_button_down;

        // ПКМ (клик или удержание) → стирание
        let erase = pointer.secondary_down() || response.secondary_clicked();

        match current_tool {
            EditorTool::Brush => {
                if brush_active {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if let Some((px, py)) = to_pixel(pos, rect) {
                            apply_brush(canvases, view_type, px, py, !erase, settings);
                        }
                    }
                }
            }
            EditorTool::NoiseBrush => {
                if brush_active {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if let Some((px, py)) = to_pixel(pos, rect) {
                            apply_noise_brush(canvases, view_type, px, py, !erase, settings);
                        }
                    }
                }
            }
            EditorTool::SmoothBrush => {
                if brush_active {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if let Some((px, py)) = to_pixel(pos, rect) {
                            if erase {
                                apply_brush(canvases, view_type, px, py, false, settings);
                            } else {
                                apply_smooth_brush(
                                    canvases, view_type, px, py,
                                    settings.brush_size as i32, settings,
                                );
                            }
                        }
                    }
                }
            }
            EditorTool::FloodFill => {
                if response.clicked() || response.secondary_clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        if let Some((px, py)) = to_pixel(pos, rect) {
                            flood_fill(canvases, view_type, px, py, !erase, settings);
                        }
                    }
                }
            }
            EditorTool::Line | EditorTool::RectFill | EditorTool::Circle => {
                let kind = match current_tool {
                    EditorTool::Line => DragKind::Line,
                    EditorTool::RectFill => DragKind::RectFill,
                    EditorTool::Circle => DragKind::Circle,
                    _ => unreachable!(),
                };
                handle_drag_tool(
                    ui, &response, rect, view_type, canvases, selection_state, settings,
                    to_pixel, to_pixel_clamped, kind,
                );
            }
        }
    });
}

#[derive(Clone, Copy)]
enum DragKind {
    Line,
    RectFill,
    /// Эллипс; при Shift — идеальный круг.
    Circle,
}

/// Ограничение драга по Shift: превращает bounding-box в квадрат,
/// сохраняя начальную точку и текущее направление.
fn apply_shift_constraint(
    x1: u32, y1: u32, x2: u32, y2: u32, shift: bool, size: u32,
) -> (u32, u32) {
    if !shift { return (x2, y2); }
    let dx = (x2 as i32 - x1 as i32).abs();
    let dy = (y2 as i32 - y1 as i32).abs();
    let s = dx.max(dy);
    let sx = if x2 >= x1 { 1 } else { -1 };
    let sy = if y2 >= y1 { 1 } else { -1 };
    let nx = (x1 as i32 + sx * s).clamp(0, size as i32 - 1) as u32;
    let ny = (y1 as i32 + sy * s).clamp(0, size as i32 - 1) as u32;
    (nx, ny)
}

/// То же для экранных координат (превью).
fn snap_pos_to_square(start: egui::Pos2, current: egui::Pos2) -> egui::Pos2 {
    let dx = current.x - start.x;
    let dy = current.y - start.y;
    let s = dx.abs().max(dy.abs());
    let sx = if dx >= 0.0 { 1.0 } else { -1.0 };
    let sy = if dy >= 0.0 { 1.0 } else { -1.0 };
    egui::pos2(start.x + sx * s, start.y + sy * s)
}

/// Отрисовка «призрачного» превью драга (без записи в холст).
fn paint_drag_preview(
    ui: &mut egui::Ui,
    kind: DragKind,
    start: egui::Pos2,
    current: egui::Pos2,
    erase: bool,
) {
    let (stroke_color, fill_color) = if erase {
        (
            egui::Color32::from_rgb(240, 80, 80),
            egui::Color32::from_rgba_premultiplied(240, 80, 80, 50),
        )
    } else {
        (
            p::ACCENT,
            egui::Color32::from_rgba_premultiplied(180, 70, 255, 50),
        )
    };

    match kind {
        DragKind::Line => {
            ui.painter()
                .line_segment([start, current], egui::Stroke::new(2.0, stroke_color));
        }
        _ => {
            let r = egui::Rect::from_two_pos(start, current);
            ui.painter().rect_filled(r, 3.0, fill_color);
            ui.painter()
                .rect_stroke(r, 3.0, egui::Stroke::new(2.5, stroke_color));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_drag_tool(
    ui: &mut egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
    view_type: ViewType,
    canvases: &mut OrthoCanvases,
    selection_state: &mut RectSelectionState,
    settings: &ToolSettings,
    to_pixel: impl Fn(egui::Pos2, egui::Rect) -> Option<(u32, u32)>,
    to_pixel_clamped: impl Fn(egui::Pos2, egui::Rect) -> (u32, u32),
    kind: DragKind,
) {
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some(pixel) = to_pixel(pos, rect) {
                selection_state.is_active = true;
                selection_state.view_type = Some(view_type);
                selection_state.start_pos = Some(pos);
                selection_state.start_pixel = Some(pixel);
                selection_state.current_pos = Some(pos);
                selection_state.current_pixel = Some(pixel);
                // Фиксируем режим на старте — на release кнопка уже может быть отпущена.
                selection_state.erase_mode = ui.input(|i| i.pointer.secondary_down());
            }
        }
    }

    let shift = ui.input(|i| i.modifiers.shift);

    if response.dragged()
        && selection_state.is_active
        && selection_state.view_type == Some(view_type)
    {
        if let Some(pos) = response.interact_pointer_pos() {
            selection_state.current_pos = Some(pos);
            selection_state.current_pixel = to_pixel(pos, rect);
        }
    }

    if response.drag_stopped()
        && selection_state.is_active
        && selection_state.view_type == Some(view_type)
    {
        let final_pixel = if let Some(pos) = response.interact_pointer_pos() {
            Some(to_pixel_clamped(pos, rect))
        } else {
            selection_state.current_pixel.or(selection_state.start_pixel)
        };

        if let (Some((x1, y1)), Some((x2, y2))) = (selection_state.start_pixel, final_pixel) {
            let val = !selection_state.erase_mode;

            let (x2, y2) = if matches!(kind, DragKind::RectFill | DragKind::Circle) {
                apply_shift_constraint(x1, y1, x2, y2, shift, canvases.size)
            } else {
                (x2, y2)
            };

            match kind {
                DragKind::Line => {
                    draw_line_on_canvas(canvases, view_type, x1, y1, x2, y2, val, settings);
                }
                DragKind::RectFill => {
                    draw_rect_filled(canvases, view_type, x1, y1, x2, y2, val, settings);
                }
                DragKind::Circle => {
                    draw_ellipse_filled(canvases, view_type, x1, y1, x2, y2, val, settings);
                }
            }
        }

        selection_state.is_active = false;
        selection_state.start_pos = None;
        selection_state.current_pos = None;
        selection_state.start_pixel = None;
        selection_state.current_pixel = None;
        selection_state.view_type = None;
        selection_state.erase_mode = false;
    }

    // ─── Превью с учётом симметрии и режима ───────────────────────────
    if let (Some(start), Some(current)) =
        (selection_state.start_pos, selection_state.current_pos)
    {
        if selection_state.view_type == Some(view_type) {
            let current = if shift && matches!(kind, DragKind::RectFill | DragKind::Circle) {
                snap_pos_to_square(start, current)
            } else {
                current
            };

            let mx = |p: egui::Pos2| egui::pos2(rect.min.x + rect.max.x - p.x, p.y);
            let my = |p: egui::Pos2| egui::pos2(p.x, rect.min.y + rect.max.y - p.y);
            let mxy = |p: egui::Pos2| {
                egui::pos2(rect.min.x + rect.max.x - p.x, rect.min.y + rect.max.y - p.y)
            };

            let erase = selection_state.erase_mode;

            paint_drag_preview(ui, kind, start, current, erase);

            if settings.symmetry_x {
                paint_drag_preview(ui, kind, mx(start), mx(current), erase);
            }
            if settings.symmetry_y {
                paint_drag_preview(ui, kind, my(start), my(current), erase);
            }
            if settings.symmetry_x && settings.symmetry_y {
                paint_drag_preview(ui, kind, mxy(start), mxy(current), erase);
            }
        }
    }
}

pub fn draw_line_on_canvas(
    canvases: &mut OrthoCanvases,
    view: ViewType,
    x0: u32, y0: u32, x1: u32, y1: u32,
    val: bool,
    settings: &ToolSettings,
) {
    let mut x0 = x0 as i32;
    let mut y0 = y0 as i32;
    let x1 = x1 as i32;
    let y1 = y1 as i32;

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        paint_with_symmetry(canvases, view, x0 as u32, y0 as u32, val, settings);
        if x0 == x1 && y0 == y1 { break; }

        let e2 = 2 * err;
        if e2 >= dy { err += dy; x0 += sx; }
        if e2 <= dx { err += dx; y0 += sy; }
    }
}

// ============================================================================
// Perspective-превью
// ============================================================================

fn draw_perspective_view(
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
    persp_cam: &mut PerspectiveCamState,
) {
    let max_avail = ui.available_size();
    let safe_size = egui::vec2(
        view_size.x.min(max_avail.x).max(64.0),
        view_size.y.min(max_avail.y).max(64.0),
    );

    ui.group(|ui| {
        ui.set_width(view_size.x);

        w::viewport_header(ui, "globe", "Perspective", ViewType::Perspective, fullscreen, |_| {});

        let Ok((mut cam_data, mut transform, _camera, _global_transform, _projection)) =
            cam_query.get_mut(camera_entity)
        else {
            ui.label("❌ Камера не найдена");
            return;
        };

        let image_size = egui::vec2(
            (safe_size.x - 8.0).max(32.0),
            (safe_size.y - 36.0).max(32.0),
        );
        let response = embedded_3d_view::draw_embedded_3d_view(
            ui, egui_contexts, &mut cam_data, interaction_state, image_size,
        );

        w::navigate_3d_camera(ui, &response, &mut transform);
        persp_cam.translation = transform.translation;
    });
}

// ============================================================================
// Импорт фото
// ============================================================================

fn reset_photo_state(photo: &mut PhotoImportState) {
    photo.source_gray.clear();
    photo.src_w = 0;
    photo.src_h = 0;
    photo.preview_tex = None;
    photo.rotate = 0;
    photo.flip_h = false;
    photo.flip_v = false;
    photo.invert = false;
    photo.threshold = 128;
    photo.auto_threshold = true;
    photo.fit = FitMode::Fit;
    photo.apply_mode = ApplyMode::Replace;
    photo.status = None;
}

#[cfg(not(target_arch = "wasm32"))]
pub fn pick_photo_for_view(view: ViewType, photo: &mut PhotoImportState) {
    let Some(path) = FileDialog::new()
        .add_filter("Изображения", &["png", "jpg", "jpeg", "bmp", "webp"])
        .pick_file()
    else { return; };

    reset_photo_state(photo);
    photo.target = Some(view);
    photo.open = true;

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => { photo.status = Some(format!("Ошибка чтения файла: {e}")); return; }
    };

    let img = match load_from_memory(&bytes) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[IMPORT] decode failed for {:?}: {e}", path.file_name().unwrap_or_default());
            photo.status = Some(format!("Ошибка декодирования: {e}"));
            return;
        }
    };
    let rgba = img.to_rgba8();
    let (w0, h0) = rgba.dimensions();

    println!("[IMPORT] {:?} · {} байт · decoded {}×{}",
             path.file_name().unwrap_or_default(), bytes.len(), w0, h0);
    let (w0, h0) = (w0 as usize, h0 as usize);
    let raw = rgba.as_raw();
    if raw.len() != w0 * h0 * 4 {
        photo.status = Some(format!("Битый RGBA: {} != {} ({}×{})", raw.len(), w0 * h0 * 4, w0, h0));
        return;
    }

    let exif = read_exif_orientation(&bytes);
    let (raw_rot, w, h) = apply_exif_to_rgba(raw, w0, h0, exif);

    let gray = rgba_to_gray_on_white(&raw_rot);
    if !gray_is_valid(&gray, w, h) {
        photo.status = Some(format!("Битый gray: {} при {}×{}", gray.len(), w, h));
        return;
    }

    photo.source_gray = gray;
    photo.src_w = w;
    photo.src_h = h;
    photo.status = Some(format!("Загружено {}×{} (EXIF {})", w, h, exif));
}

fn read_exif_orientation(bytes: &[u8]) -> u8 {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 { return 1; }

    let mut p = 2usize;
    while p + 4 <= bytes.len() {
        if bytes[p] != 0xFF { break; }
        let marker = bytes[p + 1];
        let seg_len = u16::from_be_bytes([bytes[p + 2], bytes[p + 3]]) as usize;
        if seg_len < 2 || p + 2 + seg_len > bytes.len() { break; }
        let seg_start = p + 4;
        let seg_end = p + 2 + seg_len;

        if marker == 0xE1 && seg_end - seg_start >= 6 {
            let hdr = &bytes[seg_start..seg_end];
            if hdr.starts_with(b"Exif\0\0") {
                if let Some(o) = parse_tiff_orientation(&hdr[6..]) { return o; }
            }
        }

        if marker == 0xDA { break; }
        p += 2 + seg_len;
    }
    1
}

fn parse_tiff_orientation(tiff: &[u8]) -> Option<u8> {
    if tiff.len() < 8 { return None; }
    let le = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let rd_u16 = |b: &[u8], o: usize| -> Option<u16> {
        let x = [*b.get(o)?, *b.get(o + 1)?];
        Some(if le { u16::from_le_bytes(x) } else { u16::from_be_bytes(x) })
    };
    let rd_u32 = |b: &[u8], o: usize| -> Option<u32> {
        let x = [*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?];
        Some(if le { u32::from_le_bytes(x) } else { u32::from_be_bytes(x) })
    };

    if rd_u16(tiff, 2)? != 42 { return None; }
    let ifd0 = rd_u32(tiff, 4)? as usize;
    let count = rd_u16(tiff, ifd0)? as usize;
    let entries = ifd0 + 2;

    for i in 0..count {
        let off = entries + i * 12;
        let tag = rd_u16(tiff, off)?;
        if tag == 0x0112 {
            let v = rd_u16(tiff, off + 8)?;
            if (1..=8).contains(&v) { return Some(v as u8); }
            return None;
        }
    }
    None
}

fn apply_exif_to_rgba(raw: &[u8], w: usize, h: usize, orientation: u8) -> (Vec<u8>, usize, usize) {
    if w == 0 || h == 0 || raw.len() != w * h * 4 || orientation <= 1 || orientation > 8 {
        return (raw.to_vec(), w, h);
    }

    let (new_w, new_h) = match orientation {
        2 | 3 | 4 => (w, h),
        _ => (h, w),
    };

    let mut out = vec![0u8; raw.len()];
    for dy in 0..new_h {
        for dx in 0..new_w {
            let (sx, sy) = match orientation {
                2 => (w - 1 - dx, dy),
                3 => (w - 1 - dx, h - 1 - dy),
                4 => (dx, h - 1 - dy),
                5 => (dy, dx),
                6 => (dy, h - 1 - dx),
                7 => (w - 1 - dy, h - 1 - dx),
                8 => (w - 1 - dy, dx),
                _ => (dx, dy),
            };
            let src = (sy * w + sx) * 4;
            let dst = (dy * new_w + dx) * 4;
            out[dst..dst + 4].copy_from_slice(&raw[src..src + 4]);
        }
    }
    (out, new_w, new_h)
}

#[cfg(target_arch = "wasm32")]
fn pick_photo_for_view(_view: ViewType, photo: &mut PhotoImportState) {
    photo.status = Some("Импорт файлов недоступен в WASM-режиме".to_string());
    photo.open = true;
}

// ============================================================================
// Импорт фото — превью (кэш + валидация + диагностика)
// ============================================================================

const PREVIEW_MAX_SIDE: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq)]
struct PreviewKey {
    src_w: usize, src_h: usize, src_len: usize,
    canvas: usize, rotate: u8,
    flip_h: bool, flip_v: bool, invert: bool,
    auto: bool, threshold: u8, fit: FitMode,
}

#[derive(Clone)]
struct PreviewData {
    key: PreviewKey,
    prepared: Prepared,
    mask: Vec<u8>,
    threshold: u8,
}

fn preview_key(photo: &PhotoImportState, canvas: usize) -> PreviewKey {
    PreviewKey {
        src_w: photo.src_w, src_h: photo.src_h, src_len: photo.source_gray.len(),
        canvas, rotate: photo.rotate,
        flip_h: photo.flip_h, flip_v: photo.flip_v, invert: photo.invert,
        auto: photo.auto_threshold,
        threshold: if photo.auto_threshold { 0 } else { photo.threshold },
        fit: photo.fit,
    }
}

fn build_preview(photo: &PhotoImportState, key: PreviewKey) -> PreviewData {
    let (tg, tw, th) = transform_gray(
        &photo.source_gray, photo.src_w, photo.src_h,
        photo.rotate, photo.flip_h, photo.flip_v,
    );
    let prepared = prepare_canvas(&tg, tw, th, key.canvas, photo.fit);
    let threshold = if photo.auto_threshold { otsu_threshold(&prepared) } else { photo.threshold };
    let mask = threshold_mask(&prepared, threshold, photo.invert);
    PreviewData { key, prepared, mask, threshold }
}

fn preview_image(p: &Prepared, mask: &[u8], mask_only: bool) -> egui::ColorImage {
    let size = p.size;
    let mut pixels = Vec::with_capacity(size * size);
    let (ar, ag, ab) = (p::ACCENT.r() as f32, p::ACCENT.g() as f32, p::ACCENT.b() as f32);
    for i in 0..size * size {
        if p.valid[i] == 0 {
            pixels.push(egui::Color32::from_rgb(18, 18, 22));
            continue;
        }
        let g = p.gray[i] as f32;
        if mask[i] == 1 {
            let a = if mask_only { 1.0 } else { 0.55 };
            let mix = |b: f32, acc: f32| (acc * a + b * (1.0 - a)).clamp(0.0, 255.0) as u8;
            pixels.push(egui::Color32::from_rgb(mix(g, ar), mix(g, ag), mix(g, ab)));
        } else {
            pixels.push(egui::Color32::from_gray(p.gray[i]));
        }
    }
    egui::ColorImage {
        size: [size, size],
        pixels,
    }
}

fn raw_gray_image(gray: &[u8], w: usize, h: usize) -> egui::ColorImage {
    let k = (PREVIEW_MAX_SIDE as f32 / w.max(h) as f32).min(1.0);
    let dw = ((w as f32 * k).round() as usize).max(1);
    let dh = ((h as f32 * k).round() as usize).max(1);
    let mut pixels = Vec::with_capacity(dw * dh);
    for y in 0..dh {
        let sy = (y * h / dh).min(h - 1);
        for x in 0..dw {
            let sx = (x * w / dw).min(w - 1);
            pixels.push(egui::Color32::from_gray(gray[sy * w + sx]));
        }
    }
    egui::ColorImage {
        size: [dw, dh],
        pixels,
    }
}

#[derive(Clone)]
struct RawTex {
    w: usize,
    h: usize,
    sig: u64,
    tex: egui::TextureHandle,
}

fn src_sig(gray: &[u8]) -> u64 {
    let mut s = gray.len() as u64;
    let step = (gray.len() / 257).max(1);
    for (i, &b) in gray.iter().enumerate().step_by(step) {
        s = s.wrapping_mul(31).wrapping_add(b as u64).wrapping_add(i as u64);
    }
    s
}

pub fn draw_photo_import(
    ui: &mut egui::Ui,
    photo: &mut PhotoImportState,
    canvases: &mut OrthoCanvases,
) {
    if photo.source_gray.is_empty() {
        match &photo.status {
            Some(s) => { ui.colored_label(egui::Color32::from_rgb(255, 120, 120), s); }
            None    => { ui.label("Нет загруженного изображения."); }
        }
        ui.add_space(6.0);
        if w::ghost_button(ui, "Закрыть").clicked() { photo.open = false; }
        return;
    }

    let (sw, sh) = (photo.src_w, photo.src_h);
    if !gray_is_valid(&photo.source_gray, sw, sh) {
        ui.colored_label(
            egui::Color32::from_rgb(255, 90, 90),
            format!("Битый буфер: {} байт при заявленных {}×{} (ожидалось {}).\n\
                     Размеры не совпадают с данными — превью было бы «диагональным мусором».",
                    photo.source_gray.len(), sw, sh, sw.saturating_mul(sh)),
        );
        if w::ghost_button(ui, "Сбросить импорт").clicked() {
            photo.source_gray.clear();
            photo.src_w = 0;
            photo.src_h = 0;
            photo.preview_tex = None;
            photo.open = false;
        }
        return;
    }

    if let Some(status) = &photo.status {
        ui.label(RichText::new(status).size(12.0).color(p::TEXT_DIM));
    }
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.label(RichText::new("Поворот:").color(p::TEXT_DIM));
        if w::icon_button(ui, "rotate-left",  16.0, "Повернуть влево").clicked() {
            photo.rotate = (photo.rotate + 3) % 4;
        }
        if w::icon_button(ui, "rotate-right", 16.0, "Повернуть вправо").clicked() {
            photo.rotate = (photo.rotate + 1) % 4;
        }
        ui.separator();
        ui.checkbox(&mut photo.flip_h, "Гориз");
        ui.checkbox(&mut photo.flip_v, "Верт");
        ui.checkbox(&mut photo.invert, "Инверсия");
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Подгонка:").color(p::TEXT_DIM));
        ui.selectable_value(&mut photo.fit, FitMode::Fit, "Вписать");
        ui.selectable_value(&mut photo.fit, FitMode::Fill, "Заполнить");
        ui.selectable_value(&mut photo.fit, FitMode::Stretch, "Растянуть");
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut photo.auto_threshold, "Автопорог (Оцу)");
        if !photo.auto_threshold {
            ui.add(egui::Slider::new(&mut photo.threshold, 0..=255).text("порог"));
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Наложение:").color(p::TEXT_DIM));
        ui.selectable_value(&mut photo.apply_mode, ApplyMode::Replace, "Заменить");
        ui.selectable_value(&mut photo.apply_mode, ApplyMode::Union, "Объединить");
        ui.selectable_value(&mut photo.apply_mode, ApplyMode::Subtract, "Вычесть");
        ui.selectable_value(&mut photo.apply_mode, ApplyMode::Intersect, "Пересечь");
    });
    ui.add_space(6.0);

    let cache_id = egui::Id::new("mpp_photo_preview_cache");
    let key = preview_key(photo, canvases.size as usize);

    let cached = ui.data_mut(|d| d.get_temp::<PreviewData>(cache_id))
        .filter(|c| c.key == key);
    let cache_hit = cached.is_some();
    let data = cached.unwrap_or_else(|| build_preview(photo, key));

    if photo.auto_threshold { photo.threshold = data.threshold; }

    let mask_only_id = egui::Id::new("mpp_photo_mask_only");
    let prev_mask_only = ui.data_mut(|d| d.get_temp::<bool>(mask_only_id)).unwrap_or(false);
    let mut mask_only = prev_mask_only;
    ui.checkbox(&mut mask_only, "Только маска (без полутона)");
    ui.data_mut(|d| d.insert_temp(mask_only_id, mask_only));

    let size = data.prepared.size;
    let need_recreate = photo.preview_tex.as_ref().map_or(true, |t| t.size() != [size, size]);
    let need_upload = need_recreate || !cache_hit || mask_only != prev_mask_only;

    if need_upload {
        let image = preview_image(&data.prepared, &data.mask, mask_only);
        if need_recreate {
            photo.preview_tex = Some(ui.ctx().load_texture(
                "mpp_photo_preview", image, egui::TextureOptions::NEAREST,
            ));
        } else if let Some(tex) = photo.preview_tex.as_mut() {
            tex.set(image, egui::TextureOptions::NEAREST);
        }
    }
    ui.data_mut(|d| d.insert_temp(cache_id, data.clone()));

    if let Some(tex) = photo.preview_tex.as_ref() {
        let s = ui.available_width().min(320.0).max(64.0);
        ui.add(egui::Image::new(tex).fit_to_exact_size(egui::vec2(s, s)));
    }

    let (valid, filled) = mask_stats(&data.prepared, &data.mask);
    ui.label(RichText::new(format!(
        "Холст {}² · порог {} · маска {}/{} ({:.0}%)",
        size, data.threshold, filled, valid,
        100.0 * filled as f32 / valid.max(1) as f32
    )).size(11.5).color(p::TEXT_DIM));

    ui.collapsing("Исходник (диагностика)", |ui| {
        ui.label(RichText::new(format!(
            "{}×{}, {} байт", sw, sh, photo.source_gray.len()
        )).size(11.5).color(p::TEXT_DIM));

        let raw_id = egui::Id::new("mpp_photo_raw_tex");
        let sig = src_sig(&photo.source_gray);

        let raw = ui.data_mut(|d| d.get_temp::<RawTex>(raw_id))
            .filter(|c| c.w == sw && c.h == sh && c.sig == sig)
            .unwrap_or_else(|| {
                let img = raw_gray_image(&photo.source_gray, sw, sh);
                RawTex {
                    w: sw, h: sh, sig,
                    tex: ui.ctx().load_texture("mpp_photo_raw", img, egui::TextureOptions::LINEAR),
                }
            });

        let [iw, ih] = raw.tex.size();
        let s = ui.available_width().min(320.0).max(64.0);
        ui.add(egui::Image::new(&raw.tex)
            .fit_to_exact_size(egui::vec2(s, s * ih as f32 / iw as f32)));

        ui.data_mut(|d| d.insert_temp(raw_id, raw));
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if w::primary_button_icon(ui, "check", "Применить").clicked() {
            if let Some(target) = photo.target {
                let dst = match target {
                    ViewType::Front => Some((&mut canvases.front, &mut canvases.dirty_front)),
                    ViewType::Top   => Some((&mut canvases.top,   &mut canvases.dirty_top)),
                    ViewType::Side  => Some((&mut canvases.side,  &mut canvases.dirty_side)),
                    _ => None,
                };
                if let Some((dst, dirty)) = dst {
                    apply_mask_opt(dst, &data.mask, &data.prepared.valid, photo.apply_mode);
                    *dirty = true;
                }
                photo.open = false;
            }
        }
        if w::ghost_button(ui, "Отмена").clicked() { photo.open = false; }
    });
}

// ============================================================================
// Выдвижная панель инструментов
// ============================================================================

fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(
        RichText::new(title)
            .size(11.0)
            .color(p::TEXT_DIM)
            .strong(),
    );
    ui.add_space(2.0);
}

/// Ряд квадратных иконок-переключателей (без текста, названия в tooltip).
fn tool_row(ui: &mut egui::Ui, current: &mut EditorTool, tools: &[EditorTool]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for &t in tools {
            let selected = *current == t;
            let icon_name = tool_icon_name(t);
            let hint = format!("{}\n{}", t.label(), t.hint());
            if w::icon_toggle_square(ui, icon_name, selected, &hint).clicked() {
                *current = t;
            }
        }
    });
}

fn draw_tool_panel(
    ui: &mut egui::Ui,
    tool: &mut EditorTool,
    settings: &mut ToolSettings,
    canvases: &mut OrthoCanvases,
) {
    // ─── Заголовок с кнопкой «свернуть» ────────────────────────────────
    ui.horizontal(|ui| {
        w::icon(ui, "wand", 20.0, p::ACCENT);
        ui.label(
            RichText::new("Инструменты")
                .strong()
                .size(14.0)
                .color(p::ACCENT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if w::icon_button(ui, "chevron-left", 14.0, "Свернуть панель").clicked() {
                settings.show_panel = false;
            }
        });
    });
    ui.separator();

    // ─── Кисти ─────────────────────────────────────────────────────────
    section_header(ui, "КИСТИ");
    tool_row(ui, tool, &[
        EditorTool::Brush,
        EditorTool::NoiseBrush,
        EditorTool::SmoothBrush,
    ]);

    // ─── Формы ─────────────────────────────────────────────────────────
    section_header(ui, "ФОРМЫ");
    tool_row(ui, tool, &[
        EditorTool::Line,
        EditorTool::RectFill,
        EditorTool::Circle,
    ]);

    // ─── Заливка ───────────────────────────────────────────────────────
    section_header(ui, "ЗАЛИВКА");
    tool_row(ui, tool, &[EditorTool::FloodFill]);

    // ─── Размер кисти ──────────────────────────────────────────────────
    section_header(ui, "РАЗМЕР КИСТИ");
    ui.add(
        egui::Slider::new(&mut settings.brush_size, 1..=32)
            .show_value(true)
            .suffix(" px"),
    );

    // ─── Симметрия ─────────────────────────────────────────────────────
    section_header(ui, "СИММЕТРИЯ");
    ui.label(
        RichText::new("Действует при нанесении")
            .size(10.5)
            .color(p::TEXT_MUTED)
            .italics(),
    );
    ui.horizontal(|ui| {
        if w::tool_toggle_icon(ui, "mirror", settings.symmetry_x, "Зеркало X")
            .on_hover_text("Отражать каждый мазок по вертикальной оси")
            .clicked()
        {
            settings.symmetry_x = !settings.symmetry_x;
        }
        if w::tool_toggle_icon(ui, "mirror-y", settings.symmetry_y, "Зеркало Y")
            .on_hover_text("Отражать каждый мазок по горизонтальной оси")
            .clicked()
        {
            settings.symmetry_y = !settings.symmetry_y;
        }
    });

    // ─── Плотность шума ────────────────────────────────────────────────
    if *tool == EditorTool::NoiseBrush {
        section_header(ui, "ПЛОТНОСТЬ ШУМА");
        ui.add(
            egui::Slider::new(&mut settings.noise_density, 0.05..=1.0)
                .show_value(true)
                .fixed_decimals(2),
        );
    }

    ui.separator();

    // ─── Быстрые действия ──────────────────────────────────────────────
    // ⛔ Кнопки «Зеркало по X / по Y» удалены: пост-обработки холста больше нет,
    //    симметрия — live-флаг выше.
    section_header(ui, "БЫСТРЫЕ ДЕЙСТВИЯ");

    if w::ghost_button_icon(ui, "rotate-left", "Инвертировать").clicked() {
        invert_canvases(canvases);
    }
    if w::ghost_button_icon(ui, "square", "Заполнить всё").clicked() {
        canvases.fill(ViewType::Front);
        canvases.fill(ViewType::Top);
        canvases.fill(ViewType::Side);
    }
    if w::ghost_button_icon(ui, "trash", "Очистить всё").clicked() {
        canvases.clear(ViewType::Front);
        canvases.clear(ViewType::Top);
        canvases.clear(ViewType::Side);
    }

    ui.separator();
    ui.add_space(4.0);

    ui.label(
        RichText::new(tool.hint())
            .size(11.0)
            .color(p::TEXT_MUTED)
            .italics(),
    );
}

// ============================================================================
// Реализации инструментов
// ============================================================================

/// Красит `(x, y)` и (при включённой симметрии) его зеркальные отражения.
fn paint_with_symmetry(
    canvases: &mut OrthoCanvases,
    view: ViewType,
    x: u32,
    y: u32,
    val: bool,
    s: &ToolSettings,
) {
    let size = canvases.size;
    let mx = size.saturating_sub(1).saturating_sub(x);
    let my = size.saturating_sub(1).saturating_sub(y);
    canvases.set(view, x, y, val);
    if s.symmetry_x { canvases.set(view, mx, y, val); }
    if s.symmetry_y { canvases.set(view, x, my, val); }
    if s.symmetry_x && s.symmetry_y { canvases.set(view, mx, my, val); }
}

fn apply_brush(
    canvases: &mut OrthoCanvases,
    view: ViewType,
    px: u32,
    py: u32,
    val: bool,
    s: &ToolSettings,
) {
    let size = canvases.size;
    let r = s.brush_size as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy > r * r { continue; }
            let nx = px as i32 + dx;
            let ny = py as i32 + dy;
            if nx >= 0 && ny >= 0 && (nx as u32) < size && (ny as u32) < size {
                paint_with_symmetry(canvases, view, nx as u32, ny as u32, val, s);
            }
        }
    }
}

fn apply_noise_brush(
    canvases: &mut OrthoCanvases,
    view: ViewType,
    px: u32,
    py: u32,
    val: bool,
    s: &ToolSettings,
) {
    let size = canvases.size;
    let r = s.brush_size as i32;
    let mut rng = (px as u64)
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(py as u64);
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy > r * r { continue; }
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let rand = ((rng >> 33) & 0xFFFFFF) as f32 / 0xFFFFFF as f32;
            if rand < s.noise_density {
                let nx = px as i32 + dx;
                let ny = py as i32 + dy;
                if nx >= 0 && ny >= 0 && (nx as u32) < size && (ny as u32) < size {
                    paint_with_symmetry(canvases, view, nx as u32, ny as u32, val, s);
                }
            }
        }
    }
}

fn apply_smooth_brush(
    canvases: &mut OrthoCanvases,
    view: ViewType,
    px: u32,
    py: u32,
    r: i32,
    settings: &ToolSettings,
) {
    let size = canvases.size as i32;
    let mut changes: Vec<(u32, u32, bool)> = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy > r * r { continue; }
            let cx = px as i32 + dx;
            let cy = py as i32 + dy;
            if cx < 0 || cy < 0 || cx >= size || cy >= size { continue; }
            let mut count = 0;
            for ndy in -1..=1i32 {
                for ndx in -1..=1i32 {
                    if ndx == 0 && ndy == 0 { continue; }
                    let nx = cx + ndx;
                    let ny = cy + ndy;
                    if nx >= 0 && ny >= 0 && nx < size && ny < size {
                        if canvases.get(view, nx as u32, ny as u32) { count += 1; }
                    }
                }
            }
            let new_val = count >= 5;
            if canvases.get(view, cx as u32, cy as u32) != new_val {
                changes.push((cx as u32, cy as u32, new_val));
            }
        }
    }
    for (cx, cy, v) in changes {
        paint_with_symmetry(canvases, view, cx, cy, v, settings);
    }
}

fn flood_fill(
    canvases: &mut OrthoCanvases,
    view: ViewType,
    sx: u32,
    sy: u32,
    new_val: bool,
    settings: &ToolSettings,
) {
    let size = canvases.size;
    if sx >= size || sy >= size { return; }
    let old_val = canvases.get(view, sx, sy);
    if old_val == new_val { return; }

    let mut stack = vec![(sx, sy)];
    let mut visited = vec![false; (size * size) as usize];
    let mut changed: Vec<(u32, u32)> = Vec::new();

    while let Some((x, y)) = stack.pop() {
        if x >= size || y >= size { continue; }
        let idx = (y * size + x) as usize;
        if visited[idx] { continue; }
        if canvases.get(view, x, y) != old_val { continue; }
        visited[idx] = true;
        changed.push((x, y));
        if x > 0 { stack.push((x - 1, y)); }
        if y > 0 { stack.push((x, y - 1)); }
        if x + 1 < size { stack.push((x + 1, y)); }
        if y + 1 < size { stack.push((x, y + 1)); }
    }

    for (x, y) in changed {
        paint_with_symmetry(canvases, view, x, y, new_val, settings);
    }
}

fn draw_rect_filled(
    c: &mut OrthoCanvases,
    v: ViewType,
    x1: u32, y1: u32, x2: u32, y2: u32,
    val: bool,
    s: &ToolSettings,
) {
    for y in y1.min(y2)..=y1.max(y2) {
        for x in x1.min(x2)..=x1.max(x2) {
            paint_with_symmetry(c, v, x, y, val, s);
        }
    }
}

/// Залитый эллипс по bounding-box. При Shift (из `apply_shift_constraint`)
/// bbox становится квадратом → получаем идеальный круг.
fn draw_ellipse_filled(
    c: &mut OrthoCanvases,
    v: ViewType,
    x1: u32, y1: u32, x2: u32, y2: u32,
    val: bool,
    s: &ToolSettings,
) {
    let cx = (x1 as f32 + x2 as f32) * 0.5;
    let cy = (y1 as f32 + y2 as f32) * 0.5;
    let rx = ((x2 as f32 - x1 as f32).abs() * 0.5).max(0.5);
    let ry = ((y2 as f32 - y1 as f32).abs() * 0.5).max(0.5);
    let xmin = x1.min(x2);
    let xmax = x1.max(x2);
    let ymin = y1.min(y2);
    let ymax = y1.max(y2);
    for y in ymin..=ymax {
        for x in xmin..=xmax {
            let nx = (x as f32 - cx) / rx;
            let ny = (y as f32 - cy) / ry;
            if nx * nx + ny * ny <= 1.0 {
                paint_with_symmetry(c, v, x, y, val, s);
            }
        }
    }
}

// ─── Глобальные действия панели ─────────────────────────────────────────

/// Инверсия 0↔1 на всех трёх холстах. Это **не** зеркало — просто
/// «негатив», не связан с `symmetry_x` / `symmetry_y`.
fn invert_canvases(c: &mut OrthoCanvases) {
    for data in [&mut c.front, &mut c.top, &mut c.side] {
        for v in data.iter_mut() {
            *v = 1 - *v;
        }
    }
    c.dirty_front = true;
    c.dirty_top = true;
    c.dirty_side = true;
}

// ============================================================================
// Иконки инструментов
// ============================================================================

fn tool_icon_name(tool: EditorTool) -> &'static str {
    match tool {
        EditorTool::Brush       => "brush",
        EditorTool::NoiseBrush  => "sparkle",
        EditorTool::SmoothBrush => "wand",
        EditorTool::Line        => "line",
        EditorTool::RectFill    => "square",
        EditorTool::Circle      => "circle",
        EditorTool::FloodFill   => "flood-fill",
    }
}