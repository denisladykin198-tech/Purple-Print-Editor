//! Stage 5: экспорт проекта в GNAX/assets/objects и импорт `.mpp`.

use bevy_egui::egui;
use bevy_egui::egui::RichText;

use crate::canvases::OrthoCanvases;
use crate::constants::{category_label, category_subs, CATEGORIES};
use crate::grid::grid::VoxelGrid;
use crate::model::{ProjectDoc, Severity};
use crate::format::paths::{find_objects_root, plan_project_files, write_planned};
use crate::app::state::{
    ActiveFrame, ExportUiState, PendingDocImport, ScriptZonesStore,
};
use crate::sync::save_active_frame;

use super::theme::palette as p;
use super::widgets as w;

#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;

#[allow(clippy::too_many_arguments)]
pub fn draw_stage_export(
    ui: &mut egui::Ui,
    doc: &mut ProjectDoc,
    grid: &VoxelGrid,
    canvases: &OrthoCanvases,
    zones: &ScriptZonesStore,
    active: &ActiveFrame,
    export_ui: &mut ExportUiState,
    pending_import: &mut PendingDocImport,
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(16.0);
        let width = 680.0_f32.min(ui.available_width() - 24.0);
        let margin = ((ui.available_width() - width) * 0.5).max(0.0);

        ui.horizontal(|ui| {
            ui.add_space(margin);
            ui.vertical(|ui| {
                ui.set_width(width);

                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Экспорт объекта").strong().size(22.0));
                    w::hint(ui, "Сохранение в GNAX/assets/objects");
                });
                ui.add_space(12.0);

                // Метрики
                let active_voxels = grid.data.iter().filter(|&&v| v == 1).count();
                ui.horizontal(|ui| {
                    w::stat(ui, "Фреймов", &doc.frames.len().to_string(), p::ACCENT);
                    w::stat(ui, "Вокселей активного", &active_voxels.to_string(), p::SUCCESS);
                    w::stat(ui, "Зон активного", &zones.zones.len().to_string(), p::WARN);
                });
                ui.add_space(12.0);

                // Метаданные
                w::card(ui, |ui| {
                    w::section(ui, "Метаданные");
                    egui::Grid::new("export_meta").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                        ui.label(RichText::new("ID:").color(p::TEXT_DIM));
                        ui.add(egui::TextEdit::singleline(&mut doc.meta.id)
                            .desired_width(220.0).hint_text("pistol, tank_turret..."));
                        ui.end_row();

                        ui.label(RichText::new("Имя:").color(p::TEXT_DIM));
                        ui.add(egui::TextEdit::singleline(&mut doc.meta.name).desired_width(220.0));
                        ui.end_row();

                        ui.label(RichText::new("Категория:").color(p::TEXT_DIM));
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_source("export_category")
                                .selected_text(category_label(&doc.meta.category))
                                .show_ui(ui, |ui| {
                                    for (key, label, _) in CATEGORIES {
                                        if ui.selectable_label(doc.meta.category == *key, *label).clicked() {
                                            doc.meta.category = key.to_string();
                                            doc.meta.subcategory.clear();
                                        }
                                    }
                                });
                        });
                        ui.end_row();

                        ui.label(RichText::new("Подкатегория:").color(p::TEXT_DIM));
                        ui.horizontal(|ui| {
                            let subs = category_subs(&doc.meta.category);
                            if subs.is_empty() {
                                ui.add(egui::TextEdit::singleline(&mut doc.meta.subcategory).desired_width(160.0));
                            } else {
                                egui::ComboBox::from_id_source("export_subcategory")
                                    .selected_text(if doc.meta.subcategory.is_empty() { "—" } else { doc.meta.subcategory.as_str() })
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(doc.meta.subcategory.is_empty(), "—").clicked() {
                                            doc.meta.subcategory.clear();
                                        }
                                        for sub in subs {
                                            if ui.selectable_label(doc.meta.subcategory == *sub, *sub).clicked() {
                                                doc.meta.subcategory = sub.to_string();
                                            }
                                        }
                                    });
                                ui.add(egui::TextEdit::singleline(&mut doc.meta.subcategory)
                                    .desired_width(120.0).hint_text("или своя"));
                            }
                        });
                        ui.end_row();

                        ui.label(RichText::new("Теги:").color(p::TEXT_DIM));
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut export_ui.tags_text)
                                .desired_width(220.0).hint_text("mawt,editor,weapon"));
                            ui.label(RichText::new("Цвет:").color(p::TEXT_DIM));
                            ui.color_edit_button_rgb(&mut doc.meta.base_color);
                        });
                        ui.end_row();
                    });

                    ui.add_space(6.0);
                    ui.add(egui::TextEdit::multiline(&mut doc.meta.description)
                        .desired_width(f32::INFINITY).desired_rows(2).hint_text("Описание объекта..."));
                });
                ui.add_space(12.0);

                // Кнопки
                ui.horizontal(|ui| {
                    if w::ghost_button(ui, "🔎 Проверить проект").clicked() {
                        save_active_frame(doc, active.id, grid, canvases, zones);
                        export_ui.issues = doc.validate();
                    }
                    if w::primary_button(ui, "💾 Сохранить в GNAX/assets/objects").clicked() {
                        save_active_frame(doc, active.id, grid, canvases, zones);
                        doc.meta.tags = export_ui.tags_text
                            .split(',').map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty()).collect();

                        let cwd = std::env::current_dir().unwrap_or_default();
                        let root = find_objects_root(&cwd, &export_ui.objects_root_override);
                        match plan_project_files(doc, &root) {
                            Ok(files) => match write_planned(&files) {
                                Ok(_) => export_ui.status = Some(format!(
                                    "✅ Сохранено {} файлов в {}", files.len(), root.display()
                                )),
                                Err(e) => export_ui.status = Some(format!("❌ Ошибка записи: {}", e)),
                            },
                            Err(e) => export_ui.status = Some(format!("❌ Ошибка экспорта: {}", e)),
                        }
                    }
                });

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if w::ghost_button(ui, "📂 Импорт .mpp").clicked() {
                        #[cfg(not(target_arch = "wasm32"))]
                        if let Some(path) = FileDialog::new()
                            .add_filter("Mawt Purple Print", &["mpp"]).pick_file()
                        {
                            match std::fs::read(&path) {
                                Ok(bytes) => {
                                    pending_import.bytes = Some(bytes);
                                    export_ui.status = Some("Загружаю файл...".to_string());
                                }
                                Err(e) => export_ui.status = Some(format!("❌ Ошибка чтения: {}", e)),
                            }
                        }
                    }
                    ui.label(RichText::new("Корень объектов:").size(12.0).color(p::TEXT_DIM));
                    ui.add(egui::TextEdit::singleline(&mut export_ui.objects_root_override)
                        .desired_width(200.0).hint_text("пусто = автопоиск GNAX"));
                });

                if let Some(status) = &export_ui.status {
                    ui.add_space(8.0);
                    let col = if status.starts_with('❌') { p::DANGER } else { p::SUCCESS };
                    ui.label(RichText::new(status).size(12.0).color(col));
                }

                if !export_ui.issues.is_empty() {
                    ui.add_space(10.0);
                    w::section(ui, "Валидация проекта");
                    w::card(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                            for issue in &export_ui.issues {
                                let (icon, color) = match issue.severity {
                                    Severity::Error => ("❌", p::DANGER),
                                    Severity::Warn  => ("⚠", p::WARN),
                                    Severity::Info  => ("ℹ", p::INFO),
                                };
                                ui.label(RichText::new(format!("{} {}", icon, issue.text))
                                    .color(color).size(12.5));
                            }
                        });
                    });
                }
            });
        });
    });
}