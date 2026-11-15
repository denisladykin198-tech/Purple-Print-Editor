//! Отложенное открытие нативного файл-диалога для выбора фото.
//!
//! `PhotoDialogGate` разрывает связь между кадром, в котором UI запросил
//! диалог, и кадром, в котором он реально откроется: `rfd` блокирует поток,
//! а окно должно быть в `Windowed` и на переднем плане, иначе диалог
//! потеряется за полноэкранным окном.

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowMode};

use crate::app::{PhotoImportState, ViewType};
use crate::ui::stage1::pick_photo_for_view;

#[derive(Resource, Default)]
pub struct PhotoDialogGate {
    pub request_view: Option<ViewType>,
    pub restore_mode: Option<WindowMode>,
}

pub fn photo_file_dialog_gate_system(
    mut gate: ResMut<PhotoDialogGate>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut photo: ResMut<PhotoImportState>,
) {
    // Фаза 1: UI установил pending_dialog_request. Забираем его и готовим окно.
    if let Some(view) = photo.pending_dialog_request.take() {
        match windows.get_single_mut() {
            Ok(mut window) => {
                gate.restore_mode = Some(window.mode);
                // Не открываем rfd в этом же кадре: сначала меняем режим окна.
                window.mode = WindowMode::Windowed;
                window.visible = true;
                window.focused = true;
                gate.request_view = Some(view);
            }
            Err(_) => {
                // Окно не найдено — не открываем диалог, чтобы не потерять режим.
                gate.request_view = None;
                gate.restore_mode = None;
            }
        }
        return;
    }

    // Фаза 2: следующий кадр, окно уже в Windowed — открываем диалог.
    if let Some(view) = gate.request_view.take() {
        pick_photo_for_view(view, &mut photo);

        // Восстанавливаем исходный режим окна.
        if let Ok(mut window) = windows.get_single_mut() {
            if let Some(mode) = gate.restore_mode.take() {
                window.mode = mode;
            }
        } else {
            gate.restore_mode = None;
        }
    }
}