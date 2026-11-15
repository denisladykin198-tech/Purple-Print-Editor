//! UI-слой: роутинг по стадиям + SystemParam.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::app::state::{
    ActiveFrame, EditorCameras, EditorStage, EditorTool, ExportUiState, FullscreenState,
    ModuleEditorState, PendingDocImport, PendingFrameSwitch, PendingGridResize,
    PerspectiveCamState, PhotoImportState, PlacementVisualState, ProjectDocResource,
    RectSelectionState, ScriptUiState, ScriptZonesStore, SmoothUiState, SocketPlacementState,
    TextureUiState, ToolSettings, ZoneCreationState,
};
use crate::canvases::OrthoCanvases;
use crate::core::mawt_engine::mawt_utils::embedded_3d_view;
use crate::grid::grid::VoxelGrid;

pub mod dgs;
pub mod icons;
pub mod modal;
pub mod stage1;
pub mod stage2;
pub mod stage3;
pub mod stage4;
pub mod stage5;
pub mod stage6_smooth;
pub mod stage7_textures;
pub mod theme;
pub mod widgets;

use modal::draw_modal_dialog;
use stage1::{draw_photo_import, draw_stage_voxels};
use stage2::draw_stage_modules;
use stage3::draw_stage_sockets;
use stage4::draw_stage_scripts;
use stage5::draw_stage_export;
use stage6_smooth::draw_stage_smooth;
use stage7_textures::draw_stage_textures;
use theme::palette as p;
use widgets as w;

#[derive(SystemParam)]
pub struct EditorUiResMut<'w> {
    pub tool: ResMut<'w, EditorTool>,
    pub selection_state: ResMut<'w, RectSelectionState>,
    pub fullscreen: ResMut<'w, FullscreenState>,
    pub pending_resize: ResMut<'w, PendingGridResize>,
    pub pending_doc_import: ResMut<'w, PendingDocImport>,
    pub pending_frame_switch: ResMut<'w, PendingFrameSwitch>,
    pub stage: ResMut<'w, EditorStage>,
    pub tool_settings: ResMut<'w, ToolSettings>,
    pub zones_store: ResMut<'w, ScriptZonesStore>,
    pub creation_state: ResMut<'w, ZoneCreationState>,
    pub module_ui: ResMut<'w, ModuleEditorState>,
    pub socket_ui: ResMut<'w, SocketPlacementState>,
    pub photo: ResMut<'w, PhotoImportState>,
    pub script_ui: ResMut<'w, ScriptUiState>,
    pub export_ui: ResMut<'w, ExportUiState>,
    pub persp_cam: ResMut<'w, PerspectiveCamState>,
    pub placement_visual: ResMut<'w, PlacementVisualState>,
    pub smooth_ui: ResMut<'w, SmoothUiState>,
    pub texture_ui: ResMut<'w, TextureUiState>,
}

/// Пропускает служебную `Stage2_Modules`, если она всплыла в навигации.
fn skip_stage2_forward(stage: EditorStage) -> EditorStage {
    let s = stage.next();
    if s == EditorStage::Stage2_Modules {
        s.next()
    } else {
        s
    }
}

fn skip_stage2_backward(stage: EditorStage) -> EditorStage {
    let s = stage.prev();
    if s == EditorStage::Stage2_Modules {
        s.prev()
    } else {
        s
    }
}

#[allow(clippy::too_many_arguments)]
pub fn editor_ui_system(
    mut egui_contexts: EguiContexts,
    mut interaction_state: ResMut<embedded_3d_view::Embedded3dInteractionState>,
    mut grid: ResMut<VoxelGrid>,
    cameras: Res<EditorCameras>,
    mut cam_query: Query<(
        &mut embedded_3d_view::Embedded3dCamera,
        &mut Transform,
        &Camera,
        &GlobalTransform,
        &Projection,
    )>,
    mut canvases: ResMut<OrthoCanvases>,
    mut doc_res: ResMut<ProjectDocResource>,
    active: ResMut<ActiveFrame>,
    ui_res: EditorUiResMut,
    mut theme_applied: Local<bool>,
) {
    let EditorUiResMut {
        mut tool,
        mut selection_state,
        mut fullscreen,
        mut pending_resize,
        mut pending_doc_import,
        mut pending_frame_switch,
        mut stage,
        mut zones_store,
        mut creation_state,
        mut module_ui,
        mut socket_ui,
        mut photo,
        mut script_ui,
        mut export_ui,
        mut persp_cam,
        mut placement_visual,
        mut smooth_ui,
        mut texture_ui,
        mut tool_settings,
    } = ui_res;

    let ctx = egui_contexts.ctx_mut().clone();

    if !*theme_applied {
        theme::apply(&ctx);
        *theme_applied = true;
    }

    // Escape закрывает fullscreen, если не открыта модалка:
    // модалка сама обрабатывает Escape в `draw_modal_dialog`.
    if fullscreen.active.is_some()
        && !pending_resize.confirm_dialog_open
        && ctx.input(|i| i.key_pressed(egui::Key::Escape))
    {
        fullscreen.active = None;
    }

    // Шапка: бренд + степпер стадий.
    egui::TopBottomPanel::top("stage_tabs")
        .frame(
            egui::Frame::none()
                .fill(p::PANEL)
                .stroke(egui::Stroke::new(1.0, p::BORDER))
                .inner_margin(egui::Margin::symmetric(12.0, 8.0)),
        )
        .show(&ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Purple Print")
                        .strong()
                        .size(16.0)
                        .color(p::ACCENT),
                );
                ui.label(
                    egui::RichText::new("voxel editor")
                        .size(11.0)
                        .color(p::TEXT_MUTED),
                );
                ui.add_space(10.0);
                ui.separator();

                // Порядок вкладок совпадает с логической цепочкой next()/prev().
                let stages = [
                    EditorStage::Stage1_Voxels,
                    EditorStage::Stage4_Sockets,
                    EditorStage::Stage3_Scripts,
                    EditorStage::Stage6_Smooth,
                    EditorStage::Stage7_Textures,
                    EditorStage::Stage5_Export,
                ];
                for (i, s) in stages.iter().enumerate() {
                    let label = format!("{}  {}", i + 1, s.name());
                    if w::tool_toggle(ui, *stage == *s, label).clicked() {
                        *stage = *s;
                    }
                }

                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if w::ghost_button(ui, "Далее").clicked() {
                            *stage = skip_stage2_forward(*stage);
                        }
                        if w::ghost_button(ui, "Назад").clicked() {
                            *stage = skip_stage2_backward(*stage);
                        }
                    },
                );
            });
        });

    // Статус-бар: контекстные подсказки + активный фрейм.
    egui::TopBottomPanel::bottom("status_bar")
        .frame(
            egui::Frame::none()
                .fill(p::PANEL)
                .stroke(egui::Stroke::new(1.0, p::BORDER))
                .inner_margin(egui::Margin::symmetric(12.0, 5.0)),
        )
        .show(&ctx, |ui| {
            ui.horizontal(|ui| {
                let hint = match *stage {
                    EditorStage::Stage1_Voxels | EditorStage::Stage2_Modules => {
                        "ЛКМ — рисовать · ПКМ — стирать · клик по заголовку вида — fullscreen · справа — модули"
                    }
                    EditorStage::Stage3_Scripts => {
                        "Два клика ЛКМ — поставить зону · ПКМ — отмена"
                    }
                    EditorStage::Stage4_Sockets => {
                        "Клик по поверхности модели — поставить слот · варианты модулей — как в Таркове"
                    }
                    EditorStage::Stage6_Smooth => {
                        "Median / Six-neighbor / Gaussian / Two-pass — все четыре доступны"
                    }
                    EditorStage::Stage7_Textures => {
                        "Настрой цветовую схему · превью обновляется в реальном времени"
                    }
                    EditorStage::Stage5_Export => {
                        "Проверь проект перед экспортом · .mpp можно импортировать обратно"
                    }
                };
                ui.label(
                    egui::RichText::new(hint)
                        .size(11.5)
                        .color(p::TEXT_MUTED),
                );
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let name = doc_res
                            .0
                            .frame(active.id)
                            .map(|f| f.name.clone())
                            .unwrap_or_default();
                        ui.label(
                            egui::RichText::new(format!("Фрейм: {}", name))
                                .size(11.5)
                                .color(p::TEXT_DIM),
                        );
                    },
                );
            });
        });

    // Контент стадии.
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(p::BG)
                .inner_margin(egui::Margin::same(10.0)),
        )
        .show(&ctx, |ui| match *stage {
            EditorStage::Stage1_Voxels => draw_stage_voxels(
                ui,
                &mut egui_contexts,
                &mut interaction_state,
                &mut grid,
                &cameras,
                &mut cam_query,
                &mut tool,
                &mut selection_state,
                &mut canvases,
                &mut fullscreen,
                &mut pending_resize,
                &active,
                &mut doc_res.0,
                &mut pending_frame_switch,
                &mut photo,
                &mut stage,
                &mut persp_cam,
                &mut placement_visual,
                &mut module_ui,
                &mut tool_settings,
            ),
            EditorStage::Stage2_Modules => draw_stage_modules(
                ui,
                &mut doc_res.0,
                &active,
                &mut module_ui,
                &mut pending_frame_switch,
                &mut stage,
            ),
            EditorStage::Stage3_Scripts => draw_stage_scripts(
                ui,
                &mut egui_contexts,
                &mut interaction_state,
                cameras.perspective,
                &mut cam_query,
                &mut zones_store,
                &mut creation_state,
                &grid,
                &mut doc_res.0,
                &active,
                &mut fullscreen,
                &mut script_ui,
            ),
            EditorStage::Stage4_Sockets => draw_stage_sockets(
                ui,
                &mut egui_contexts,
                &mut interaction_state,
                cameras.perspective,
                &mut cam_query,
                &grid,
                &mut doc_res.0,
                &active,
                &mut module_ui,
                &mut socket_ui,
                &mut fullscreen,
                &mut zones_store,
                &mut placement_visual,
            ),
            EditorStage::Stage6_Smooth => draw_stage_smooth(
                ui,
                &mut egui_contexts,
                &mut interaction_state,
                cameras.perspective,
                &mut cam_query,
                &mut grid,
                &mut doc_res.0,
                &active,
                &mut smooth_ui,
                &mut fullscreen,
            ),
            EditorStage::Stage7_Textures => draw_stage_textures(
                ui,
                &mut egui_contexts,
                &mut interaction_state,
                cameras.perspective,
                &mut cam_query,
                &grid,
                &mut doc_res.0,
                &active,
                &mut texture_ui,
                &mut fullscreen,
            ),
            EditorStage::Stage5_Export => draw_stage_export(
                ui,
                &mut doc_res.0,
                &grid,
                &canvases,
                &zones_store,
                &active,
                &mut export_ui,
                &mut pending_doc_import,
            ),
        });

    if photo.open && photo.target.is_some() {
        let mut open = photo.open;
        egui::Window::new("Фото → силуэт")
            .open(&mut open)
            .collapsible(false)
            .default_size([440.0, 600.0])
            .show(&ctx, |ui| draw_photo_import(ui, &mut photo, &mut canvases));
        photo.open = open;
    }
    if !photo.open {
        photo.target = None;
    }

    draw_modal_dialog(&ctx, &mut pending_resize);
}