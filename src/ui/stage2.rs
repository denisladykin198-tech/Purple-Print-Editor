//! Stage 2: список фреймов/модулей, их свойства и навигация.
//! Адаптивная раскладка:
//!   • узкая панель (встроена в Stage 1) — одна колонка, кнопки на всю ширину;
//!   • широкий экран — список слева, свойства и действия справа.

use bevy_egui::egui::{self, RichText};

use crate::constants::{ALLOWED_GRID_SIZES, MODULE_TAG_PRESETS, ROOT_FRAME_ID};
use crate::model::{Face, ProjectDoc};
use crate::app::state::{ActiveFrame, EditorStage, ModuleEditorState, PendingFrameSwitch};

use super::theme::palette as p;
use super::widgets as w;

/// Ниже этой ширины включается одноколоночный (панельный) режим.
const COMPACT_BREAKPOINT: f32 = 560.0;
/// Высота списка фреймов в компактном режиме (внешний ScrollArea уже есть).
const LIST_MAX_H_COMPACT: f32 = 200.0;

pub fn draw_stage_modules(
    ui: &mut egui::Ui,
    doc: &mut ProjectDoc,
    active: &ActiveFrame,
    mui: &mut ModuleEditorState,
    pending_switch: &mut PendingFrameSwitch,
    stage: &mut EditorStage,
) {
    // По умолчанию показываем свойства активного фрейма, а не пустую заглушку.
    if mui.selected_frame.is_none() {
        mui.selected_frame = Some(active.id);
    }

    let compact = ui.available_width() < COMPACT_BREAKPOINT;

    if compact {
        // ── Одна колонка: создание → список → свойства → действия ──
        w::card(ui, |ui| {
            w::section(ui, "Новый модуль");
            new_module_form(ui, doc, mui, true);
        });
        ui.add_space(8.0);

        frames_card(ui, doc, active, mui, true);
        ui.add_space(8.0);

        if let Some(fid) = mui.selected_frame {
            props_card(ui, doc, fid, true);
            ui.add_space(8.0);
            actions_block(ui, doc, active, mui, pending_switch, stage, fid, true);
        }
    } else {
        // ── Широкая раскладка (как раньше) ──
        w::toolbar(ui, |ui| {
            new_module_form(ui, doc, mui, false);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                w::badge(ui, format!("Фреймов: {}", doc.frames.len()), p::ACCENT);
            });
        });
        ui.add_space(8.0);

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(280.0);
                frames_card(ui, doc, active, mui, false);
            });

            ui.vertical(|ui| {
                let Some(fid) = mui.selected_frame else {
                    w::empty_state_icon(ui, "package", "Выбери фрейм из списка слева");
                    return;
                };
                props_card(ui, doc, fid, false);
                ui.add_space(8.0);
                actions_block(ui, doc, active, mui, pending_switch, stage, fid, false);
            });
        });
    }
}

// ============================================================================
// Блок: создание модуля
// ============================================================================

fn create_module(doc: &mut ProjectDoc, mui: &mut ModuleEditorState) {
    let id = doc.add_module(&mui.new_module_name, &mui.new_module_tag, mui.new_module_size);
    mui.selected_frame = Some(id);
}

fn new_module_form(ui: &mut egui::Ui, doc: &mut ProjectDoc, mui: &mut ModuleEditorState, compact: bool) {
    let tag_label = MODULE_TAG_PRESETS
        .iter()
        .find(|(tag, _)| *tag == mui.new_module_tag.as_str())
        .map(|(_, label)| *label)
        .unwrap_or("Свой тип");

    if compact {
        ui.label(RichText::new("Название").size(12.0).color(p::TEXT_DIM));
        ui.add(
            egui::TextEdit::singleline(&mut mui.new_module_name)
                .desired_width(ui.available_width())
                .hint_text("turret_base"),
        );
        ui.add_space(6.0);

        let half = ((ui.available_width() - 8.0) / 2.0).max(80.0);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_source("new_module_tag")
                .width(half)
                .selected_text(tag_label)
                .show_ui(ui, |ui| {
                    for (tag, label) in MODULE_TAG_PRESETS {
                        ui.selectable_value(&mut mui.new_module_tag, tag.to_string(), *label);
                    }
                });
            egui::ComboBox::from_id_source("new_module_size")
                .width(half)
                .selected_text(format!("{}³", mui.new_module_size))
                .show_ui(ui, |ui| {
                    for &sz in ALLOWED_GRID_SIZES {
                        ui.selectable_value(&mut mui.new_module_size, sz, format!("{}³", sz));
                    }
                });
        });
        ui.add_space(8.0);

        if w::primary_button_icon_full(ui, "plus", "Создать модуль").clicked() {
            create_module(doc, mui);
        }
    } else {
        ui.label(RichText::new("Новый модуль:").size(12.5).color(p::TEXT_DIM));
        ui.add(egui::TextEdit::singleline(&mut mui.new_module_name).desired_width(140.0).hint_text("turret_base"));

        egui::ComboBox::from_id_source("new_module_tag")
            .selected_text(tag_label)
            .show_ui(ui, |ui| {
                for (tag, label) in MODULE_TAG_PRESETS {
                    ui.selectable_value(&mut mui.new_module_tag, tag.to_string(), *label);
                }
            });
        egui::ComboBox::from_id_source("new_module_size")
            .selected_text(format!("{}³", mui.new_module_size))
            .show_ui(ui, |ui| {
                for &sz in ALLOWED_GRID_SIZES {
                    ui.selectable_value(&mut mui.new_module_size, sz, format!("{}³", sz));
                }
            });

        if w::primary_button_icon(ui, "plus", "Создать").clicked() {
            create_module(doc, mui);
        }
    }
}

// ============================================================================
// Блок: список фреймов
// ============================================================================

fn short_name(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        name.to_string()
    } else {
        let s: String = name.chars().take(max).collect();
        format!("{s}…")
    }
}

fn frames_card(
    ui: &mut egui::Ui,
    doc: &ProjectDoc,
    active: &ActiveFrame,
    mui: &mut ModuleEditorState,
    compact: bool,
) {
    w::card(ui, |ui| {
        ui.horizontal(|ui| {
            w::section(ui, "Фреймы");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                w::badge(ui, format!("всего: {}", doc.frames.len()), p::ACCENT);
            });
        });

        let mut scroll = egui::ScrollArea::vertical()
            .id_source("frames_list_scroll")
            .auto_shrink([false, true]);
        if compact {
            scroll = scroll.max_height(LIST_MAX_H_COMPACT);
        }

        scroll.show(ui, |ui| {
            let name_max = if compact { 16 } else { 24 };
            for f in doc.frames.iter() {
                let selected = mui.selected_frame == Some(f.id);
                let icon_name = if f.is_root { "home" } else { "package" };
                let short = short_name(&f.name, name_max);
                ui.horizontal(|ui| {
                    let resp = w::tool_toggle_icon(ui, icon_name, selected, short)
                        .on_hover_text(&f.name);
                    if resp.clicked() {
                        mui.selected_frame = Some(f.id);
                        mui.selected_socket = f.sockets.first().map(|s| s.id);
                    }
                    if f.id == active.id {
                        w::badge(ui, "активен", p::WARN);
                    }
                });
                ui.add_space(2.0);
            }
        });
    });
}

// ============================================================================
// Блок: свойства
// ============================================================================

fn tag_preset_combo(ui: &mut egui::Ui, tag: &mut String, id: &str, width: f32) {
    egui::ComboBox::from_id_source(id)
        .width(width)
        .selected_text("пресет")
        .show_ui(ui, |ui| {
            for (t, label) in MODULE_TAG_PRESETS {
                if ui.selectable_label(false, format!("{} ({})", label, t)).clicked() {
                    *tag = t.to_string();
                }
            }
        });
}

fn props_card(ui: &mut egui::Ui, doc: &mut ProjectDoc, fid: u32, compact: bool) {
    let usage = doc.usage_count(fid);

    w::card(ui, |ui| {
        w::section(ui, "Свойства");

        let Some(f) = doc.frame_mut(fid) else {
            w::empty_state_icon(ui, "package", "Фрейм не найден");
            return;
        };

        if compact {
            ui.label(RichText::new("Имя").size(12.0).color(p::TEXT_DIM));
            ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(ui.available_width()));
            ui.add_space(6.0);

            ui.label(RichText::new("Тип").size(12.0).color(p::TEXT_DIM));
            ui.horizontal(|ui| {
                let combo_w = 96.0;
                let edit_w = (ui.available_width() - combo_w - 8.0).max(60.0);
                ui.add(egui::TextEdit::singleline(&mut f.tag).desired_width(edit_w));
                tag_preset_combo(ui, &mut f.tag, "frame_tag_preset", combo_w);
            });
        } else {
            egui::Grid::new("frame_props").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(RichText::new("Имя:").color(p::TEXT_DIM));
                ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(220.0));
                ui.end_row();

                ui.label(RichText::new("Тип:").color(p::TEXT_DIM));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut f.tag).desired_width(120.0));
                    tag_preset_combo(ui, &mut f.tag, "frame_tag_preset", 90.0);
                });
                ui.end_row();
            });
        }

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            w::badge(ui, format!("{}³", f.size), p::INFO);
            w::badge(ui, format!("вокселей: {}", f.voxel_count()), p::SUCCESS);
            w::badge(ui, format!("слотов: {}", f.sockets.len()), p::ACCENT);
            w::badge(ui, format!("использован: {}", usage), p::TEXT_DIM);
        });

        if !f.is_root {
            ui.add_space(8.0);
            w::section(ui, "Pivot — точка крепления");
            ui.horizontal_wrapped(|ui| {
                for (face, label) in [
                    (Face::NegX, "-X"), (Face::PosX, "+X"),
                    (Face::NegY, "-Y"), (Face::PosY, "+Y"),
                    (Face::NegZ, "-Z"), (Face::PosZ, "+Z"),
                    (Face::Center, "Центр"),
                ] {
                    if ui.small_button(label).clicked() {
                        f.pivot = f.pivot_at(face);
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for (i, axis) in ["x", "y", "z"].iter().enumerate() {
                    ui.add(egui::DragValue::new(&mut f.pivot[i]).speed(0.1).prefix(format!("{}: ", axis)));
                }
            });
        }
    });
}

// ============================================================================
// Блок: действия / навигация
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn actions_block(
    ui: &mut egui::Ui,
    doc: &mut ProjectDoc,
    active: &ActiveFrame,
    mui: &mut ModuleEditorState,
    pending_switch: &mut PendingFrameSwitch,
    stage: &mut EditorStage,
    fid: u32,
    compact: bool,
) {
    let usage = doc.usage_count(fid);
    let is_root = fid == ROOT_FRAME_ID;

    let mut go = |s: EditorStage| {
        if fid != active.id {
            pending_switch.id = Some(fid);
        }
        *stage = s;
    };

    if compact {
        w::card(ui, |ui| {
            w::section(ui, "Действия");

            if w::primary_button_icon_full(ui, "pencil", "Редактировать воксели").clicked() {
                go(EditorStage::Stage1_Voxels);
            }
            ui.add_space(4.0);
            if w::ghost_button_icon_full(ui, "scroll", "Скрипты / зоны").clicked() {
                go(EditorStage::Stage3_Scripts);
            }
            ui.add_space(4.0);
            if w::ghost_button_icon_full(ui, "socket", "Слоты").clicked() {
                go(EditorStage::Stage4_Sockets);
            }

            if !is_root {
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                if w::ghost_button_icon_full(ui, "copy", "Дублировать").clicked() {
                    if let Some(nid) = doc.duplicate_module(fid) {
                        mui.selected_frame = Some(nid);
                    }
                }
                ui.add_space(4.0);

                if usage == 0 {
                    if w::danger_button_icon_full(ui, "trash", "Удалить модуль")
                        .on_hover_text("Удалить модуль")
                        .clicked()
                    {
                        doc.remove_module(fid);
                        mui.selected_frame = Some(ROOT_FRAME_ID);
                    }
                } else {
                    ui.add_enabled_ui(false, |ui| {
                        w::danger_button_icon_full(ui, "trash", "Удалить модуль");
                    })
                    .response
                    .on_hover_text("Нельзя удалить модуль, пока он используется в слотах");
                }
            }
        });
    } else {
        w::toolbar(ui, |ui| {
            if w::primary_button_icon(ui, "pencil", "Редактировать воксели").clicked() {
                go(EditorStage::Stage1_Voxels);
            }
            if w::ghost_button_icon(ui, "scroll", "Скрипты / зоны").clicked() {
                go(EditorStage::Stage3_Scripts);
            }
            if w::ghost_button_icon(ui, "socket", "Слоты").clicked() {
                go(EditorStage::Stage4_Sockets);
            }
            if !is_root {
                if w::ghost_button_icon(ui, "copy", "Дублировать").clicked() {
                    if let Some(nid) = doc.duplicate_module(fid) {
                        mui.selected_frame = Some(nid);
                    }
                }
                let del = ui.add_enabled_ui(usage == 0, |ui| {
                    w::danger_button_icon(ui, "trash", "Удалить")
                }).inner;
                let del = if usage == 0 {
                    del.on_hover_text("Удалить модуль")
                } else {
                    del.on_hover_text("Нельзя удалить модуль, пока он используется в слотах")
                };
                if del.clicked() {
                    doc.remove_module(fid);
                    mui.selected_frame = Some(ROOT_FRAME_ID);
                }
            }
        });
    }
}