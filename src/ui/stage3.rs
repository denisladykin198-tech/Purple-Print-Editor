// stage3.rs
//! Stage 3: слоты фрейма + 3D-превью с постановкой слотов кликом по модели.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use bevy_egui::egui::RichText;

use crate::core::mawt_engine::mawt_utils::embedded_3d_view;

use crate::constants::NO_FRAME;
use crate::grid::grid::{compute_ray_from_screen, raycast_editor_grid, VoxelGrid};
use crate::model::{tag_matches, ProjectDoc, SocketOption, ZoneRec};
use crate::grid::raycast::RayHit;
use crate::app::state::{
    ActiveFrame, Axis, FullscreenState, ModuleEditorState, PlacementVisualState,
    ScriptZonesStore, SocketPlacementState, ViewType,
};

use super::theme::palette as p;
use super::widgets as w;

#[allow(clippy::too_many_arguments)]
pub fn draw_stage_sockets(
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
    mui: &mut ModuleEditorState,
    socket_state: &mut SocketPlacementState,
    fullscreen: &mut FullscreenState,
    zones_store: &mut ScriptZonesStore,
    placement_state: &mut PlacementVisualState,
) {
    let frame_name = doc
        .frame(active.id)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "?".to_string());

    w::toolbar(ui, |ui| {
        ui.label(RichText::new(format!("Слоты · {}", frame_name)).strong().size(14.0));
        ui.separator();

        if w::tool_toggle_icon(ui, "target", socket_state.placing, "Клик по 3D: добавить слот").clicked() {
            socket_state.placing = !socket_state.placing;
        }

        if w::ghost_button_icon(ui, "plus", "Слот в центре").clicked() {
            let anchor = doc
                .frame(active.id)
                .and_then(|f| f.bbox())
                .map(|(mn, mx)| [
                    (mn[0] as f32 + mx[0] as f32 + 1.0) * 0.5,
                    (mn[1] as f32 + mx[1] as f32 + 1.0) * 0.5,
                    (mn[2] as f32 + mx[2] as f32 + 1.0) * 0.5,
                ])
                .unwrap_or([grid.size.x as f32 * 0.5; 3]);

            if let Some(id) = add_socket_with_zone(doc, zones_store, active.id, anchor, grid.size.x) {
                mui.selected_socket = Some(id);
            }
        }

        ui.separator();

        if w::ghost_button_icon(ui, "eye", "Визуализировать модули")
            .on_hover_text("Перестроить призрачные меши размещённых модулей")
            .clicked()
        {
            placement_state.force_refresh = true;
            placement_state.show_debug = true;
            info!("Ручной запуск визуализации размещений (frame #{})", active.id);
        }

        if w::tool_toggle_icon(ui, "bug", placement_state.show_debug, "Отладка").clicked() {
            placement_state.show_debug = !placement_state.show_debug;
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
    // Панель: 320…420 px, но не больше 45% ширины.
    let panel_width = 420.0_f32.min(available.x * 0.45).max(320.0);
    // Между панелью и вьюпортом — ровно один `item_spacing`.
    let gap = ui.spacing().item_spacing.x;
    let view_width = (available.x - panel_width - gap).max(160.0);

    if placement_state.show_debug {
        ui.add_space(4.0);
        w::card(ui, |ui| {
            ui.horizontal(|ui| {
                w::section(ui, "Отладка размещений");
                ui.separator();

                // `last_signature == 0` — система ещё ни разу не отрабатывала
                // (стартовое значение) или её нет в расписании.
                let never_ran =
                    placement_state.last_signature == 0 && !placement_state.force_refresh;
                let (txt, col) = if never_ran {
                    ("sync: не запускался", p::DANGER)
                } else {
                    ("sync: ок", p::SUCCESS)
                };
                w::badge_icon(ui, "warning", txt, col);
                w::badge(
                    ui,
                    format!("sig={:#018x}", placement_state.last_signature),
                    p::TEXT_DIM,
                );
                w::badge(
                    ui,
                    format!("force={}", placement_state.force_refresh),
                    p::TEXT_MUTED,
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if w::icon_button(ui, "x", 14.0, "Скрыть").clicked() {
                        placement_state.show_debug = false;
                    }
                });
            });

            egui::ScrollArea::vertical()
                .max_height(160.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if placement_state.last_signature == 0 {
                        ui.horizontal(|ui| {
                            w::icon(ui, "warning", 11.0, p::DANGER);
                            ui.label(
                                RichText::new(
                                    "sync_placements_system ещё не отрабатывала.\n\
                                     Проверьте регистрацию в main.rs и нажмите \
                                     «Визуализировать модули».",
                                )
                                .color(p::DANGER)
                                .monospace()
                                .size(11.0),
                            );
                        });
                    }
                    if placement_state.debug_lines.is_empty() {
                        w::hint(ui, "Нет данных. Нажми «Визуализировать модули».");
                    } else {
                        for line in &placement_state.debug_lines {
                            ui.label(
                                RichText::new(line)
                                    .monospace()
                                    .size(11.0)
                                    .color(p::TEXT),
                            );
                        }
                    }
                });
        });
    }

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(panel_width);
            draw_socket_panel(ui, doc, active, mui);
        });
        ui.vertical(|ui| {
            ui.set_width(view_width);
            draw_perspective_view_socket(
                ui,
                egui_contexts,
                interaction_state,
                camera_entity,
                cam_query,
                egui::vec2(view_width, available.y - 20.0),
                grid,
                doc,
                active,
                mui,
                socket_state,
                fullscreen,
                zones_store,
            );
        });
    });
}

fn add_socket_with_zone(
    doc: &mut ProjectDoc,
    zones_store: &mut ScriptZonesStore,
    frame_id: u32,
    anchor: [f32; 3],
    grid_size: u32,
) -> Option<u32> {
    let max_v = grid_size.saturating_sub(1);
    let vx = (anchor[0].max(0.0) as u32).min(max_v);
    let vy = (anchor[1].max(0.0) as u32).min(max_v);
    let vz = (anchor[2].max(0.0) as u32).min(max_v);

    let zone_id = zones_store.add_zone(
        UVec3::new(vx, vy, vz),
        UVec3::new(vx, vy, vz),
        Axis::Z,
        grid_size,
    )?;

    let name = format!("socket_{}", doc.next_socket_id);
    let socket_id = doc.add_socket(frame_id, &name, "", anchor)?;

    if let Some(frame) = doc.frame_mut(frame_id) {
        if let Some(sock) = frame.sockets.iter_mut().find(|s| s.id == socket_id) {
            sock.zone_id = Some(zone_id);
        }
    }
    Some(socket_id)
}

fn draw_socket_panel(
    ui: &mut egui::Ui,
    doc: &mut ProjectDoc,
    active: &ActiveFrame,
    mui: &mut ModuleEditorState,
) {
    let Some(frame) = doc.frame(active.id) else {
        ui.label("Фрейм не найден");
        return;
    };

    let sockets: Vec<(u32, String, Option<String>)> = frame
        .sockets
        .iter()
        .map(|s| {
            let current = s
                .selected_frame()
                .and_then(|mid| doc.frame(mid))
                .map(|f| f.name.clone());
            (s.id, s.name.clone(), current)
        })
        .collect();

    w::card(ui, |ui| {
        w::section(ui, "Слоты");
        egui::ScrollArea::vertical()
            .max_height(140.0)
            .show(ui, |ui| {
                if sockets.is_empty() {
                    w::empty_state_icon(
                        ui,
                        "socket",
                        "Нет слотов.\nВключи режим добавления и кликни по модели.",
                    );
                }
                for (sid, name, current) in &sockets {
                    let icon_name = if current.is_some() { "link" } else { "circle" };
                    let label = match current {
                        Some(n) => format!("{} → {}", name, n),
                        None    => format!("{} → (пусто)", name),
                    };
                    if w::tool_toggle_icon(ui, icon_name, mui.selected_socket == Some(*sid), label)
                        .clicked()
                    {
                        mui.selected_socket = Some(*sid);
                    }
                }
            });
    });

    ui.add_space(6.0);

    let module_list: Vec<(u32, String, String)> = doc
        .frames
        .iter()
        .filter(|f| !f.is_root)
        .map(|f| (f.id, f.name.clone(), f.tag.clone()))
        .collect();

    let zones: Vec<(u64, String, ZoneRec)> = doc
        .frame(active.id)
        .map(|f| f.zones.iter().map(|z| (z.id, z.name.clone(), z.clone())).collect())
        .unwrap_or_default();

    let Some(frame) = doc.frame_mut(active.id) else {
        return;
    };
    let Some(sock) = frame
        .sockets
        .iter_mut()
        .find(|s| Some(s.id) == mui.selected_socket)
    else {
        w::hint(ui, "Выбери слот из списка выше.");
        return;
    };

    w::card(ui, |ui| {
        w::section(ui, "Свойства слота");

        egui::Grid::new("sock_props")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(RichText::new("Имя:").color(p::TEXT_DIM));
                ui.add(egui::TextEdit::singleline(&mut sock.name).desired_width(180.0));
                ui.end_row();

                ui.label(RichText::new("Фильтр типов:").color(p::TEXT_DIM));
                ui.add(egui::TextEdit::singleline(&mut sock.tag).desired_width(180.0));
                ui.end_row();

                ui.label(RichText::new("Обязательный:").color(p::TEXT_DIM));
                ui.checkbox(&mut sock.required, "");
                ui.end_row();

                ui.label(RichText::new("Поворот:").color(p::TEXT_DIM));
                ui.horizontal(|ui| {
                    for i in 0..3 {
                        ui.add(egui::DragValue::new(&mut sock.rot[i]).clamp_range(0..=3));
                    }
                });
                ui.end_row();

                ui.label(RichText::new("Anchor:").color(p::TEXT_DIM));
                ui.horizontal(|ui| {
                    for i in 0..3 {
                        ui.add(egui::DragValue::new(&mut sock.anchor[i]).speed(0.1));
                    }
                });
                ui.end_row();
            });

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Зона:").color(p::TEXT_DIM));
            let current = sock
                .zone_id
                .and_then(|id| zones.iter().find(|(zid, _, _)| *zid == id).map(|(_, n, _)| n.clone()))
                .unwrap_or_else(|| "— свободно —".to_string());

            egui::ComboBox::from_id_source("socket_zone")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(sock.zone_id.is_none(), "— свободно —")
                        .clicked()
                    {
                        sock.zone_id = None;
                    }
                    for (zid, name, zone) in &zones {
                        if ui.selectable_label(sock.zone_id == Some(*zid), name).clicked() {
                            sock.zone_id = Some(*zid);
                            sock.snap_anchor_to_zone(zone);
                        }
                    }
                });
        });
    });

    ui.add_space(8.0);

    w::card(ui, |ui| {
        w::section(ui, "Варианты модулей");

        let mut remove_opt: Option<usize> = None;
        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
            if sock.options.is_empty() {
                w::hint(ui, "Нет вариантов. Добавь модуль ниже.");
            }
            for (idx, opt) in sock.options.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .radio(sock.selected == idx, "")
                        .on_hover_text("По умолчанию")
                        .clicked()
                    {
                        sock.selected = idx;
                    }
                    let module_name = if opt.frame_id < 0 {
                        "Без модуля".to_string()
                    } else {
                        module_list
                            .iter()
                            .find(|(id, _, _)| *id == opt.frame_id as u32)
                            .map(|(_, name, _)| name.clone())
                            .unwrap_or_else(|| format!("#{}", opt.frame_id))
                    };
                    ui.label(module_name);
                    ui.add(egui::TextEdit::singleline(&mut opt.label).desired_width(90.0));

                    if opt.frame_id >= 0 {
                        let mid = opt.frame_id as u32;
                        let ok = module_list
                            .iter()
                            .find(|(id, _, _)| *id == mid)
                            .map(|(_, _, tag)| tag_matches(&sock.tag, tag))
                            .unwrap_or(false);
                        if !ok {
                            w::badge_icon(ui, "warning", "тип", p::WARN);
                        }
                    }
                    if w::icon_button(ui, "trash", 14.0, "Удалить вариант").clicked() {
                        remove_opt = Some(idx);
                    }
                });
            }
        });

        if let Some(idx) = remove_opt {
            sock.options.remove(idx);
            if sock.selected >= sock.options.len() {
                sock.selected = 0;
            }
        }

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Добавить:").color(p::TEXT_DIM));
            egui::ComboBox::from_id_source("socket_add_module")
                .selected_text(
                    mui.option_module
                        .and_then(|id| {
                            module_list
                                .iter()
                                .find(|(mid, _, _)| *mid == id)
                                .map(|(_, name, _)| name.clone())
                        })
                        .unwrap_or_else(|| "модуль".to_string()),
                )
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(mui.option_module.is_none(), "Без модуля")
                        .clicked()
                    {
                        mui.option_module = None;
                    }
                    for (mid, name, tag) in &module_list {
                        let label = if tag_matches(&sock.tag, tag) {
                            name.clone()
                        } else {
                            format!("{} ⚠", name)
                        };
                        if ui
                            .selectable_label(mui.option_module == Some(*mid), label)
                            .clicked()
                        {
                            mui.option_module = Some(*mid);
                        }
                    }
                });

            if w::icon_button(ui, "plus", 14.0, "Добавить вариант").clicked() {
                let frame_id = mui.option_module.map(|v| v as i32).unwrap_or(NO_FRAME);
                let label = module_list
                    .iter()
                    .find(|(mid, _, _)| Some(*mid) == mui.option_module)
                    .map(|(_, name, _)| name.clone())
                    .unwrap_or_else(|| "Без модуля".to_string());
                let was_empty = sock.selected_frame().is_none();
                sock.options.push(SocketOption { frame_id, label });
                if sock.options.len() == 1 {
                    sock.selected = 0;
                } else if was_empty && frame_id >= 0 {
                    sock.selected = sock.options.len() - 1;
                }
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_perspective_view_socket(
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
    grid: &VoxelGrid,
    doc: &mut ProjectDoc,
    active: &ActiveFrame,
    mui: &mut ModuleEditorState,
    socket_state: &mut SocketPlacementState,
    fullscreen: &mut FullscreenState,
    zones_store: &mut ScriptZonesStore,
) {
    ui.group(|ui| {
        ui.set_width(view_size.x);

        w::viewport_header(ui, "globe", "Perspective — слоты", ViewType::Perspective, fullscreen, |_| {});

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
        let is_hovering_viewport = pointer_pos.map_or(false, |p| img_rect.contains(p));

        if socket_state.placing {
            if let Some(pos) = pointer_pos {
                if primary_clicked && is_hovering_viewport {
                    if let Some(hit) = screen_to_hit(
                        pos,
                        img_rect,
                        image_size,
                        &*transform,
                        projection,
                        grid,
                    ) {
                        let anchor = [
                            hit.voxel[0] as f32 + 0.5 + hit.normal[0] as f32 * 0.5,
                            hit.voxel[1] as f32 + 0.5 + hit.normal[1] as f32 * 0.5,
                            hit.voxel[2] as f32 + 0.5 + hit.normal[2] as f32 * 0.5,
                        ];
                        if let Some(id) =
                            add_socket_with_zone(doc, zones_store, active.id, anchor, grid.size.x)
                        {
                            socket_state.placing = false;
                            mui.selected_socket = Some(id);
                        }
                    }
                }
            }
            ui.painter().text(
                img_rect.center(),
                egui::Align2::CENTER_CENTER,
                "Кликни по поверхности, чтобы поставить слот",
                egui::FontId::proportional(14.0),
                p::WARN,
            );
            } else {
                // В режиме `placing` камера намеренно "заморожена" — иначе клик
                // для постановки слота конфликтует с drag-ом. В обычном режиме —
                // штатная навигация с проверкой hover для scroll.
                w::navigate_3d_camera(ui, &response, &mut transform);
            }
    });
}

fn screen_to_hit(
    pos: egui::Pos2,
    img_rect: egui::Rect,
    image_size: egui::Vec2,
    transform: &Transform,
    projection: &Projection,
    grid: &VoxelGrid,
) -> Option<RayHit> {
    if !img_rect.contains(pos) {
        return None;
    }
    let local_pos = bevy::prelude::Vec2::new(pos.x - img_rect.min.x, pos.y - img_rect.min.y);
    let img_vec2 = bevy::prelude::Vec2::new(image_size.x, image_size.y);
    let (ray_origin, ray_dir) = compute_ray_from_screen(local_pos, img_vec2, transform, projection)?;
    raycast_editor_grid(ray_origin, ray_dir, grid)
}