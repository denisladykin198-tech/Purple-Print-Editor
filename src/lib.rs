//! # Purple Print 3D Editor
//!
//! Воксельный редактор: воксели → модули → слоты → скрипты → экспорт.
//! Формат MPP (RLE) + автоген `*.object.rhai`.
//!
//! ## Структура
//!
//! * `crate::core::mawt_engine::mawt_utils` — утилиты рендера камер в текстуры
//!   (`embedded_3d_view`, `load_from_memory`, `DynamicImage`).
//! * `crate::app` — состояние и системы редактора.
//! * `crate::core` — воксельная сетка, мешинг, модель, синхронизация.
//! * `crate::format` — чтение/запись MPP, экспорт фото.
//! * `crate::ui` — egui-интерфейс по стадиям.
//! * `crate::rhai` — шаблоны и редактор скриптов.
//!
//! ## Регистрация
//!
//! Standalone-бинарь (`src/main.rs`) регистрирует системы вручную.
//! Для встраивания в игру используйте `PurplePrintEditorPlugin`.
//! Не совмещайте оба способа — системы задублируются.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use bevy::prelude::*;

pub mod app {
    pub mod state;
    pub mod systems;

    pub use state::*;
    pub use systems::*;
}

pub mod core {
    pub mod canvases;
    pub mod constants;
    pub mod grid {
        pub mod grid;
        pub mod raycast;
        pub mod smoothing;
    }
    pub mod mawt_engine {
        pub mod mawt_utils {
            pub mod graphics {
                pub mod embedded_3d_view;
                pub mod image_stub;
            }
            pub use graphics::*;
        }
    }
    pub mod mawt_ui {
        pub mod assets {
            pub mod svg;
        }
    }
    pub mod meshing;
    pub mod model;
    pub mod sync;
}

pub mod format {
    pub mod io;
    pub mod paths;
    pub mod photo;
    pub mod read;
    pub mod rle;
    pub mod write;

    pub use read::read_mpp;
    pub use rle::{rle_decode, rle_encode};
    pub use write::write_mpp;
}

pub mod render {
    pub mod textures;
}

pub mod rhai {
    pub mod editor;
    pub mod templates;
    pub mod tokenizer;

    pub use editor::rhai_edit;
    pub use templates::{default_zone_script, module_script_template};
    pub use tokenizer::{tokenize_rhai, Tok};
}

pub mod ui;
pub use ui::*;

// Плоские ре-экспорты для потребителей крейта.
pub use core::mawt_engine::mawt_utils;
pub use core::*;

pub use constants::*;
pub use model::*;
pub use sync::*;

pub use ui::dgs::{photo_file_dialog_gate_system, PhotoDialogGate};
pub use ui::editor_ui_system;

pub struct PurplePrintEditorPlugin;

impl Plugin for PurplePrintEditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, app::systems::setup_editor_system)
            .add_systems(
                PostStartup,
                (
                    app::systems::disable_ortho_cameras_system,
                    app::systems::strip_prepass_from_editor_cameras_system,
                ),
            )
            .add_systems(
                Update,
                (
                    app::systems::apply_pending_doc_import_system,
                    app::systems::apply_pending_frame_switch_system,
                    app::systems::resize_grid_system,
                    app::systems::update_voxel_mesh_system,
                    app::systems::sync_placements_system,
                    app::systems::sync_zones_to_entities_system,
                    ui::editor_ui_system,
                )
                    .chain(),
            );

        // cleanup_editor_system намеренно не регистрируется: она удаляет
        // все EditorEntity и очищает ChunkMeshMap. Вызывающий код должен
        // запустить её в OnExit(state) или по AppExit.
    }
}