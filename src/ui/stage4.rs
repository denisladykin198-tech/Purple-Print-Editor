// stage4.rs
//! Stage 4: скрипты фрейма + зоны поведения в 3D.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use bevy_egui::egui::RichText;

use crate::core::mawt_engine::mawt_utils::embedded_3d_view;

use crate::constants::zone_color_egui;
use crate::grid::grid::{compute_ray_from_screen, raycast_editor_grid, VoxelGrid};
use crate::model::ProjectDoc;
use crate::rhai::rhai_edit;
use crate::app::state::{
    ActiveFrame, Axis, FullscreenState, ScriptUiState, ScriptZonesStore, ViewType,
    ZoneCreationState,
};

use super::theme::palette as p;
use super::widgets as w;

#[allow(clippy::too_many_arguments)]
pub fn draw_stage_scripts(
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
    zones_store: &mut ScriptZonesStore,
    creation_state: &mut ZoneCreationState,
    grid: &VoxelGrid,
    doc: &mut ProjectDoc,
    active: &ActiveFrame,
    fullscreen: &mut FullscreenState,
    script_ui: &mut ScriptUiState,
) {
    w::toolbar(ui, |ui| {
        if w::tool_toggle_icon(ui, "grid",   !script_ui.frame_tab, "Зоны").clicked()          { script_ui.frame_tab = false; }
        if w::tool_toggle_icon(ui, "scroll",  script_ui.frame_tab, "Скрипт фрейма").clicked()  { script_ui.frame_tab = true;  }
        ui.separator();

        if !script_ui.frame_tab {
            if w::tool_toggle_icon(ui, "ruler", creation_state.is_drawing, "Режим создания").clicked() {
                creation_state.is_drawing = !creation_state.is_drawing;
                if !creation_state.is_drawing {
                    creation_state.start_voxel = None;
                    creation_state.current_hover_voxel = None;
                }
            }

            let can_delete = zones_store.selected_zone_id.is_some();
            let del = ui
                .add_enabled_ui(can_delete, |ui| {
                    w::danger_button_icon(ui, "trash", "Удалить")
                })
                .inner
                .on_hover_text(if can_delete {
                    "Удалить выбранную зону"
                } else {
                    "Сначала выбери зону"
                });
            if del.clicked() {
                if let Some(id) = zones_store.selected_zone_id.take() {
                    zones_store.remove_zone(id);
                }
            }

            ui.separator();
            w::badge(ui, format!("Зон: {}", zones_store.zones.len()), p::ACCENT);
        }

        if fullscreen.active == Some(ViewType::Perspective) {
            ui.separator();
            if w::ghost_button_icon(ui, "x", "Выйти (Esc)").clicked() {
                fullscreen.active = None;
            }
        }
    });

    ui.add_space(6.0);

    let available = ui.available_size();
    let code_width = 430.0_f32.min(available.x * 0.4).max(320.0);
    let gap = ui.spacing().item_spacing.x;
    let view_width = (available.x - code_width - gap).max(160.0);

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(view_width);
            draw_perspective_view_zones(
                ui,
                egui_contexts,
                interaction_state,
                camera_entity,
                cam_query,
                egui::vec2(view_width, available.y - 20.0),
                zones_store,
                creation_state,
                grid,
                fullscreen,
                script_ui.frame_tab,
            );
        });

        ui.vertical(|ui| {
            ui.set_width(code_width);
            if script_ui.frame_tab {
                w::section(ui, "Скрипт активного фрейма");
                if let Some(f) = doc.frame_mut(active.id) {
                    ui.add_space(4.0);
                    rhai_edit(ui, &mut f.script, 24);
                }
            } else {
                draw_zones_tab(ui, zones_store);
            }
        });
    });
}

fn draw_zones_tab(ui: &mut egui::Ui, zones_store: &mut ScriptZonesStore) {
    w::section(ui, "Зоны поведения");
    w::card(ui, |ui| {
        egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
            if zones_store.zones.is_empty() {
                w::empty_state_icon(
                    ui,
                    "grid",
                    "Нет зон.\nВключи режим создания и выдели область на 3D-виде.",
                );
            }
            let mut to_select: Option<u64> = None;
            for zone in &zones_store.zones {
                let selected = zones_store.selected_zone_id == Some(zone.id);
                let color = zone_color_egui(zone.color_idx);
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 3.0, color);
                    if w::tool_toggle(ui, selected, &zone.name).clicked() {
                        to_select = Some(zone.id);
                    }
                });
            }
            if let Some(id) = to_select {
                zones_store.selected_zone_id = Some(id);
            }
        });
    });

    ui.add_space(8.0);

    if let Some(zone_id) = zones_store.selected_zone_id {
        if let Some(zone) = zones_store.get_zone_mut(zone_id) {
            w::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Имя:").color(p::TEXT_DIM));
                    ui.add(
                        egui::TextEdit::singleline(&mut zone.name)
                            .desired_width(200.0)
                            .hint_text("turret_mount"),
                    );
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "Bounds: {} → {}   Axis: {:?}",
                        zone.min, zone.max, zone.axis
                    ))
                    .monospace()
                    .size(11.0)
                    .color(p::TEXT_MUTED),
                );
                ui.add_space(8.0);
                w::section(ui, "Rhai-скрипт зоны");
                rhai_edit(ui, &mut zone.script, 18);
            });
        }
    } else {
        w::empty_state_icon(
            ui,
            "pointing-left",
            "Выбери зону из списка\nили нарисуй новую на 3D-виде",
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_perspective_view_zones(
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
    zones_store: &mut ScriptZonesStore,
    creation_state: &mut ZoneCreationState,
    grid: &VoxelGrid,
    fullscreen: &mut FullscreenState,
    frame_tab_active: bool,
) {
    ui.group(|ui| {
        ui.set_width(view_size.x);

        w::viewport_header(ui, "globe", "Perspective — зоны в 3D", ViewType::Perspective, fullscreen, |_| {});

        let Ok((mut cam_data, mut transform, _camera_comp, _global_transform, projection)) =
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

        let img_rect = response.rect;
        let pointer_pos = ui.input(|i| i.pointer.latest_pos());
        let primary_clicked = ui.input(|i| i.pointer.primary_clicked());
        let secondary_clicked = ui.input(|i| i.pointer.secondary_clicked());
        let is_hovering_viewport = pointer_pos.map_or(false, |p| img_rect.contains(p));

        if creation_state.is_drawing && !frame_tab_active {
            if secondary_clicked {
                creation_state.start_voxel = None;
                creation_state.current_hover_voxel = None;
            }

            if let Some(pos) = pointer_pos {
                if let Some(hit_voxel) =
                    screen_to_voxel(pos, img_rect, image_size, &*transform, projection, grid)
                {
                    if hit_voxel.x < grid.size.x
                        && hit_voxel.y < grid.size.y
                        && hit_voxel.z < grid.size.z
                    {
                        creation_state.current_hover_voxel = Some(hit_voxel);

                        if primary_clicked && is_hovering_viewport {
                            if let Some(start) = creation_state.start_voxel {
                                let end = hit_voxel;
                                let dx = (end.x as i32 - start.x as i32).abs();
                                let dy = (end.y as i32 - start.y as i32).abs();
                                let dz = (end.z as i32 - start.z as i32).abs();
                                let axis = if dx <= dy && dx <= dz {
                                    Axis::X
                                } else if dy <= dx && dy <= dz {
                                    Axis::Y
                                } else {
                                    Axis::Z
                                };
                                let min = UVec3::new(
                                    start.x.min(end.x),
                                    start.y.min(end.y),
                                    start.z.min(end.z),
                                );
                                let max = UVec3::new(
                                    start.x.max(end.x),
                                    start.y.max(end.y),
                                    start.z.max(end.z),
                                );
                                if zones_store.add_zone(min, max, axis, grid.size.x).is_some() {
                                    creation_state.start_voxel = None;
                                    creation_state.current_hover_voxel = None;
                                }
                            } else {
                                creation_state.start_voxel = Some(hit_voxel);
                            }
                        }
                    }
                }
            }

            if let (Some(start), Some(current)) =
                (creation_state.start_voxel, creation_state.current_hover_voxel)
            {
                let min = UVec3::new(
                    start.x.min(current.x),
                    start.y.min(current.y),
                    start.z.min(current.z),
                );
                let max = UVec3::new(
                    start.x.max(current.x),
                    start.y.max(current.y),
                    start.z.max(current.z),
                );
                let size = max - min + UVec3::ONE;

                ui.painter().text(
                    img_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!(
                        "Start: {:?}\nEnd: {:?}\nРазмер: {:?}\n(ЛКМ: подтвердить, ПКМ: отмена)",
                        start, current, size
                    ),
                    egui::FontId::proportional(14.0),
                    p::WARN,
                );
            }
        } else {
            // При активном создании зоны (is_drawing && !frame_tab_active) камера
            // заморожена — иначе клик по зоне уходит в drag. Иначе — навигация.
            w::navigate_3d_camera(ui, &response, &mut transform);
        }
    });
}

fn screen_to_voxel(
    pos: egui::Pos2,
    img_rect: egui::Rect,
    image_size: egui::Vec2,
    transform: &Transform,
    projection: &Projection,
    grid: &VoxelGrid,
) -> Option<UVec3> {
    if !img_rect.contains(pos) {
        return None;
    }
    let local_pos = bevy::prelude::Vec2::new(pos.x - img_rect.min.x, pos.y - img_rect.min.y);
    let img_vec2 = bevy::prelude::Vec2::new(image_size.x, image_size.y);
    let (ray_origin, ray_dir) = compute_ray_from_screen(local_pos, img_vec2, transform, projection)?;
    raycast_editor_grid(ray_origin, ray_dir, grid).map(|hit| UVec3::from_slice(&hit.voxel))
}