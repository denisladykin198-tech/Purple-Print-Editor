//! Standalone-редактор Purple Print — без игры, без GameState, без меню.
//!
//! Запуск: `cargo run --bin editor`

use bevy::prelude::*;
use bevy::window::WindowMode;
use bevy_egui::EguiPlugin;
use mimalloc::MiMalloc;

use purple_print::app::state::ChunkMeshMap;
use purple_print::app::systems::{
    apply_pending_doc_import_system,
    apply_pending_frame_switch_system,
    disable_ortho_cameras_system,
    resize_grid_system,
    setup_editor_system,
    strip_prepass_from_editor_cameras_system,
    sync_placements_system,
    sync_zones_to_entities_system,
    update_voxel_mesh_system,
};
use purple_print::mawt_utils::embedded_3d_view::Embedded3dViewPlugin;
use purple_print::{editor_ui_system, photo_file_dialog_gate_system, PhotoDialogGate};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    info!("Запуск standalone Purple Print editor");

    let mut app = App::new();

    app.init_resource::<ChunkMeshMap>()
        .init_resource::<PhotoDialogGate>()
        .insert_resource(ClearColor(Color::BLACK));

    app.add_plugins(
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Purple Print Editor".into(),
                resolution: (1600.0, 900.0).into(),
                mode: WindowMode::Windowed,
                present_mode: bevy::window::PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }),
    )
    .add_plugins(EguiPlugin)
    .add_plugins(Embedded3dViewPlugin);

    app.add_systems(
        Startup,
        (
            |mut commands: Commands| {
                commands.spawn(Camera2dBundle::default());
            },
            setup_editor_system,
            disable_ortho_cameras_system,
            strip_prepass_from_editor_cameras_system,
        )
            .chain(),
    );

    app.add_systems(
        Update,
        (
            apply_pending_doc_import_system,
            apply_pending_frame_switch_system,
            resize_grid_system,
            update_voxel_mesh_system,
            sync_placements_system,
            sync_zones_to_entities_system,
            editor_ui_system,
            photo_file_dialog_gate_system,
        )
            .chain(),
    );

    app.insert_resource(Msaa::Off).run();
}