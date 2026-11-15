//! Системы: setup/cleanup, применение отложенных операций (импорт/переключение/ресайз),
//! пересборка меша при изменениях, синхронизация зон и компоновок.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::prelude::*;
use bevy::render::camera::ScalingMode;
use bevy::render::view::RenderLayers;
use bevy::tasks::ComputeTaskPool;

use crate::app::state::{
    ActiveFrame, ChunkMeshMap, EditorCameras, EditorEntity, EditorStage, EditorTool,
    ExportUiState, FullscreenState, ModuleEditorState, PendingDocImport, PendingFrameSwitch,
    PendingGridResize, PerspectiveCamState, PhotoImportState, PlacedModuleVisual,
    PlacementVisualState, ProjectDocResource, RectSelectionState, ScriptUiState,
    ScriptZoneVisual, ScriptZonesStore, SmoothUiState, SocketPlacementState,
    TextureUiState, ToolSettings, ViewType, VoxelChunk, ZoneCreationState, ZoneWireframe,
};
use crate::canvases::OrthoCanvases;
use crate::constants::{
    ALLOWED_GRID_SIZES, CHUNK_SIZE, DEFAULT_GRID_SIZE, ROOT_FRAME_ID, ZONE_COLORS_F32,
};
use crate::core::mawt_engine::mawt_utils::embedded_3d_view;
use crate::core::mawt_engine::mawt_utils::embedded_3d_view::EMBEDDED_3D_RENDER_LAYER;
use crate::format::read_mpp;
use crate::grid::grid::VoxelGrid;
use crate::meshing::{
    empty_chunk_mesh, generate_frame_chunk_mesh, generate_wireframe_cube,
};
use crate::model::ProjectDoc;
use crate::render::textures::sample_voxel_color;
use crate::sync::{ensure_chunk_entities, load_frame_into_editor, save_active_frame};

// ============================================================================
// Setup / Cleanup
// ============================================================================

pub fn setup_editor_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let grid_size = DEFAULT_GRID_SIZE;

    let grid = VoxelGrid::new(UVec3::splat(grid_size));
    let canvases = OrthoCanvases::new(grid_size);
    let doc = ProjectDoc::new(grid_size);

    commands.insert_resource(ProjectDocResource(doc));
    commands.insert_resource(ActiveFrame { id: ROOT_FRAME_ID });
    commands.insert_resource(EditorStage::default());
    commands.insert_resource(EditorTool::default());
    commands.insert_resource(ToolSettings::default());
    commands.insert_resource(RectSelectionState::default());
    commands.insert_resource(FullscreenState::default());
    commands.insert_resource(PendingGridResize::default());
    commands.insert_resource(PendingDocImport::default());
    commands.insert_resource(PendingFrameSwitch::default());
    commands.insert_resource(ScriptZonesStore::default());
    commands.insert_resource(ZoneCreationState::default());
    commands.insert_resource(ModuleEditorState::default());
    commands.insert_resource(SocketPlacementState::default());
    commands.insert_resource(ScriptUiState::default());
    commands.insert_resource(PhotoImportState::default());
    commands.insert_resource(ExportUiState::default());
    commands.insert_resource(SmoothUiState::default());
    commands.insert_resource(TextureUiState::default());
    commands.insert_resource(grid);
    commands.insert_resource(canvases);
    commands.insert_resource(PerspectiveCamState::default());
    commands.insert_resource(PlacementVisualState::default());

    let mut chunk_map = ChunkMeshMap::default();
    let chunks_axis = (grid_size as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;
    let total_chunks = chunks_axis * chunks_axis * chunks_axis;

    chunk_map.meshes = vec![Handle::default(); total_chunks];

    let offset = grid_size as f32 * 0.5;
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        ..default()
    });

    for cz in 0..chunks_axis {
        for cy in 0..chunks_axis {
            for cx in 0..chunks_axis {
                let idx = cz * chunks_axis * chunks_axis + cy * chunks_axis + cx;
                let mesh = meshes.add(empty_chunk_mesh());
                chunk_map.meshes[idx] = mesh.clone();

                commands.spawn((
                    PbrBundle {
                        mesh,
                        material: material.clone(),
                        transform: Transform::from_translation(Vec3::splat(-offset)),
                        ..default()
                    },
                    EditorEntity,
                    VoxelChunk,
                    RenderLayers::layer(EMBEDDED_3D_RENDER_LAYER),
                ));
            }
        }
    }

    commands.insert_resource(chunk_map);

    commands.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight {
                illuminance: 10000.0,
                ..default()
            },
            transform: Transform::from_xyz(10.0, 20.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        RenderLayers::layer(EMBEDDED_3D_RENDER_LAYER),
        EditorEntity,
    ));

    commands.insert_resource(AmbientLight {
        brightness: 0.5,
        ..default()
    });

    let view_size = Vec2::new(400.0, 400.0);

    let spawn_cam = |cmd: &mut Commands,
                     imgs: &mut Assets<Image>,
                     x: f32,
                     y: f32,
                     z: f32,
                     look: Vec3,
                     view: ViewType| {
        let cam = embedded_3d_view::spawn_embedded_3d_camera(
            cmd,
            imgs,
            embedded_3d_view::Embedded3dViewConfig {
                initial_size: view_size,
                transform: Transform::from_xyz(x, y, z).looking_at(look, Vec3::Y),
                projection: Projection::Orthographic(OrthographicProjection {
                    scaling_mode: ScalingMode::FixedVertical(150.0),
                    near: 0.1,
                    far: 2000.0,
                    ..default()
                }),
                clear_color: Color::rgba(0.15, 0.15, 0.18, 1.0),
                ..default()
            },
        );
        cmd.entity(cam).insert((EditorEntity, view));
        cam
    };

    // Ortho-камеры не имеют читателя render-target — UI рисует через
    // egui::TextureHandle, собранный из Vec<u8>. Отключаются в
    // disable_ortho_cameras_system, чтобы не гонять лишние render-проходы.
    let front = spawn_cam(&mut commands, &mut images, 0.0, 0.0, 100.0, Vec3::ZERO, ViewType::Front);
    let side = spawn_cam(&mut commands, &mut images, 100.0, 0.0, 0.0, Vec3::ZERO, ViewType::Side);
    let top = spawn_cam(&mut commands, &mut images, 0.0, 100.0, 0.0, Vec3::Z, ViewType::Top);

    let pers_cam = embedded_3d_view::spawn_embedded_3d_camera(
        &mut commands,
        &mut images,
        embedded_3d_view::Embedded3dViewConfig {
            initial_size: view_size,
            transform: Transform::from_xyz(80.0, 80.0, 80.0).looking_at(Vec3::ZERO, Vec3::Y),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: std::f32::consts::PI / 4.0,
                near: 0.1,
                far: 2000.0,
                ..default()
            }),
            clear_color: Color::rgba(0.15, 0.15, 0.18, 1.0),
            ..default()
        },
    );

    commands.entity(pers_cam).insert((EditorEntity, ViewType::Perspective));

    commands.insert_resource(EditorCameras {
        front,
        side,
        top,
        perspective: pers_cam,
    });
}

/// Отключает рендер у фронт/топ/сайд-камер: их render-target никто не читает,
/// но каждая активная камера заставляет Bevy делать полный проход рендера сцены.
pub fn disable_ortho_cameras_system(
    cameras: Res<EditorCameras>,
    mut query: Query<&mut Camera>,
) {
    for entity in [cameras.front, cameras.side, cameras.top] {
        if let Ok(mut cam) = query.get_mut(entity) {
            if cam.is_active {
                cam.is_active = false;
            }
        }
    }
}

pub fn cleanup_editor_system(
    mut commands: Commands,
    query: Query<Entity, With<EditorEntity>>,
    mut chunk_map: ResMut<ChunkMeshMap>,
    mut placement_state: ResMut<PlacementVisualState>,
) {
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }
    chunk_map.meshes.clear();
    placement_state.entities.clear();
    placement_state.last_signature = 0;
}

// ============================================================================
// Отложенные операции
// ============================================================================

pub fn apply_pending_doc_import_system(
    mut commands: Commands,
    mut pending: ResMut<PendingDocImport>,
    mut doc_res: ResMut<ProjectDocResource>,
    mut active: ResMut<ActiveFrame>,
    mut grid: ResMut<VoxelGrid>,
    mut canvases: ResMut<OrthoCanvases>,
    mut zones: ResMut<ScriptZonesStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_map: ResMut<ChunkMeshMap>,
    chunk_query: Query<Entity, With<VoxelChunk>>,
    mut export_ui: ResMut<ExportUiState>,
) {
    let Some(bytes) = pending.bytes.take() else { return };

    match read_mpp(&bytes) {
        Ok(doc) => {
            *doc_res = ProjectDocResource(doc);
            active.id = ROOT_FRAME_ID;

            load_frame_into_editor(&doc_res.0, active.id, &mut grid, &mut canvases, &mut zones);
            ensure_chunk_entities(
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut chunk_map,
                &grid,
                chunk_query.iter().collect(),
            );

            export_ui.status = Some("Проект загружен".to_string());
        }
        Err(e) => {
            export_ui.status = Some(format!("Ошибка импорта: {}", e));
        }
    }
}

pub fn apply_pending_frame_switch_system(
    mut commands: Commands,
    mut pending: ResMut<PendingFrameSwitch>,
    mut doc_res: ResMut<ProjectDocResource>,
    mut active: ResMut<ActiveFrame>,
    mut grid: ResMut<VoxelGrid>,
    mut canvases: ResMut<OrthoCanvases>,
    mut zones: ResMut<ScriptZonesStore>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_map: ResMut<ChunkMeshMap>,
    chunk_query: Query<Entity, With<VoxelChunk>>,
    cameras: Res<EditorCameras>,
    mut persp_cam: ResMut<PerspectiveCamState>,
    mut cam_query: Query<(
        &mut embedded_3d_view::Embedded3dCamera,
        &mut Transform,
        &Camera,
        &GlobalTransform,
        &Projection,
    )>,
) {
    let Some(new_id) = pending.id.take() else { return };
    if doc_res.0.frame(new_id).is_none() {
        return;
    }

    if let Ok((_, transform, ..)) = cam_query.get(cameras.perspective) {
        persp_cam.save_for(active.id, transform.translation);
    }

    save_active_frame(&mut doc_res.0, active.id, &grid, &canvases, &zones);

    active.id = new_id;
    load_frame_into_editor(&doc_res.0, active.id, &mut grid, &mut canvases, &mut zones);

    ensure_chunk_entities(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut chunk_map,
        &grid,
        chunk_query.iter().collect(),
    );

    let frame_size = doc_res.0.frame(new_id).map(|f| f.size).unwrap_or(DEFAULT_GRID_SIZE);
    let new_translation = persp_cam.camera_for(new_id, frame_size);

    if let Ok((_, mut transform, ..)) = cam_query.get_mut(cameras.perspective) {
        transform.translation = new_translation;
        transform.look_at(Vec3::ZERO, Vec3::Y);
    }
    persp_cam.translation = new_translation;
}

pub fn resize_grid_system(
    mut commands: Commands,
    mut pending: ResMut<PendingGridResize>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunk_map: ResMut<ChunkMeshMap>,
    mut grid: ResMut<VoxelGrid>,
    mut canvases: ResMut<OrthoCanvases>,
    mut doc_res: ResMut<ProjectDocResource>,
    active: Res<ActiveFrame>,
    chunk_query: Query<Entity, With<VoxelChunk>>,
) {
    let Some(new_size) = pending.new_size.take() else { return };
    pending.confirm_dialog_open = false;

    if new_size == canvases.size || !ALLOWED_GRID_SIZES.contains(&new_size) {
        return;
    }

    *grid = VoxelGrid::new(UVec3::splat(new_size));
    *canvases = OrthoCanvases::new(new_size);

    if let Some(f) = doc_res.0.frame_mut(active.id) {
        let n2 = (new_size as usize) * (new_size as usize);
        let n3 = n2 * new_size as usize;
        f.size = new_size;
        f.voxels = vec![1; n3];
        f.front = vec![1; n2];
        f.top = vec![1; n2];
        f.side = vec![1; n2];
        f.revision += 1;
    }

    ensure_chunk_entities(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut chunk_map,
        &grid,
        chunk_query.iter().collect(),
    );

    info!("Размер активного фрейма изменён на {}³", new_size);
}

// ============================================================================
// Меши вокселей
// ============================================================================

pub fn update_voxel_mesh_system(
    mut grid: ResMut<VoxelGrid>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunk_map: Res<ChunkMeshMap>,
    tex: Res<TextureUiState>,
) {
    if tex.applied_mode != grid.applied_mode {
        grid.applied_mode = tex.applied_mode;
        grid.mark_all_dirty();
    }

    if !grid.has_changes {
        return;
    }

    let chunks_x = grid.chunks_per_axis;
    let chunks_y = (grid.size.y as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;
    let xy_chunks = chunks_x * chunks_y;

    let dirty_list: Vec<(usize, usize, usize, usize)> = grid
        .dirty_chunks
        .iter()
        .enumerate()
        .filter(|(_, &is_dirty)| is_dirty)
        .map(|(idx, _)| {
            let cz = idx / xy_chunks;
            let rem = idx % xy_chunks;
            let cy = rem / chunks_x;
            let cx = rem % chunks_x;
            (idx, cx, cy, cz)
        })
        .collect();

    if dirty_list.is_empty() {
        grid.has_changes = false;
        return;
    }

    let applied_mode = grid.applied_mode;
    let color_params = tex.color_params();
    let size = grid.size;

    let color_of = move |x: u32, y: u32, z: u32| -> [f32; 4] {
        sample_voxel_color(applied_mode, x, y, z, size, color_params)
    };

    let grid_ref: &VoxelGrid = &*grid;
    let color_ref = &color_of;
    let pool = ComputeTaskPool::get();

    let generated: Vec<(usize, Mesh)> = pool.scope(|scope| {
        for &(idx, cx, cy, cz) in &dirty_list {
            let grid_ref = grid_ref;
            let color_ref = color_ref;
            scope.spawn(async move {
                (
                    idx,
                    crate::meshing::generate_chunk_mesh_greedy(grid_ref, color_ref, cx, cy, cz),
                )
            });
        }
    });

    for (idx, new_mesh) in generated {
        if let Some(handle) = chunk_map.meshes.get(idx) {
            if let Some(mesh_res) = meshes.get_mut(handle) {
                *mesh_res = new_mesh;
            }
        }
        if idx < grid.dirty_chunks.len() {
            grid.dirty_chunks[idx] = false;
        }
    }

    grid.has_changes = false;
}

// ============================================================================
// Зоны: синхронизация сущностей
// ============================================================================

pub fn sync_zones_to_entities_system(
    mut commands: Commands,
    store: Res<ScriptZonesStore>,
    solid_entities: Query<(Entity, &ScriptZoneVisual), Without<ZoneWireframe>>,
    wire_entities: Query<(Entity, &ScriptZoneVisual), With<ZoneWireframe>>,
    mut solid_query: Query<
        (&ScriptZoneVisual, &mut Handle<Mesh>, &mut Handle<StandardMaterial>, &mut Transform),
        Without<ZoneWireframe>,
    >,
    mut wire_query: Query<
        (&ScriptZoneVisual, &mut Handle<Mesh>, &mut Transform),
        With<ZoneWireframe>,
    >,
    grid: Res<VoxelGrid>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut creation_state: ResMut<ZoneCreationState>,
) {
    let live_ids: HashSet<u64> = store.zones.iter().map(|z| z.id).collect();

    for (entity, visual) in solid_entities.iter() {
        if !live_ids.contains(&visual.zone_id) {
            commands.entity(entity).despawn_recursive();
        }
    }
    for (entity, visual) in wire_entities.iter() {
        if !live_ids.contains(&visual.zone_id) {
            commands.entity(entity).despawn_recursive();
        }
    }

    creation_state.material_cache.retain(|id, _| live_ids.contains(id));
    creation_state.mesh_cache.retain(|id, _| live_ids.contains(id));
    creation_state.wireframe_cache.retain(|id, _| live_ids.contains(id));

    if store.zones.is_empty() {
        return;
    }

    let mut solid_by_id: HashMap<u64, Entity> = HashMap::new();
    for (entity, visual) in solid_entities.iter() {
        solid_by_id.insert(visual.zone_id, entity);
    }
    let mut wire_by_id: HashMap<u64, Entity> = HashMap::new();
    for (entity, visual) in wire_entities.iter() {
        wire_by_id.insert(visual.zone_id, entity);
    }

    let offset = grid.size.x as f32 * 0.5;

    for zone_data in &store.zones {
        let world_center = Vec3::new(
            (zone_data.min.x as f32 + zone_data.max.x as f32 + 1.0) * 0.5 - offset,
            (zone_data.min.y as f32 + zone_data.max.y as f32 + 1.0) * 0.5 - offset,
            (zone_data.min.z as f32 + zone_data.max.z as f32 + 1.0) * 0.5 - offset,
        );

        let size_vox = Vec3::new(
            (zone_data.max.x - zone_data.min.x + 1) as f32,
            (zone_data.max.y - zone_data.min.y + 1) as f32,
            (zone_data.max.z - zone_data.min.z + 1) as f32,
        );
        let half_size = size_vox * 0.5;

        let color_f32 = ZONE_COLORS_F32[zone_data.color_idx % ZONE_COLORS_F32.len()];
        let is_selected = store.selected_zone_id == Some(zone_data.id);

        let alpha = if is_selected { 0.75 } else { 0.35 };
        let emissive_strength = if is_selected { 3.0 } else { 1.0 };
        let emissive = Color::rgb_linear(
            color_f32[0] * emissive_strength,
            color_f32[1] * emissive_strength,
            color_f32[2] * emissive_strength,
        );
        let desired_base = Color::rgba(color_f32[0], color_f32[1], color_f32[2], alpha);

        let mat_handle = match creation_state.material_cache.get(&zone_data.id).cloned() {
            Some(existing) => {
                let needs_update = materials
                    .get(&existing)
                    .map_or(true, |m| m.base_color != desired_base || m.emissive != emissive);
                if needs_update {
                    if let Some(mat_res) = materials.get_mut(&existing) {
                        mat_res.base_color = desired_base;
                        mat_res.emissive = emissive;
                    }
                }
                existing
            }
            None => {
                let mat = materials.add(StandardMaterial {
                    base_color: desired_base,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    alpha_mode: AlphaMode::Blend,
                    emissive,
                    ..default()
                });
                creation_state.material_cache.insert(zone_data.id, mat.clone());
                mat
            }
        };

        let mesh_handle = match creation_state.mesh_cache.get(&zone_data.id) {
            Some((cached_size, handle)) if *cached_size == half_size => handle.clone(),
            _ => {
                let mesh = meshes.add(Cuboid { half_size });
                creation_state
                    .mesh_cache
                    .insert(zone_data.id, (half_size, mesh.clone()));
                mesh
            }
        };

        let wf_handle = match creation_state.wireframe_cache.get(&zone_data.id) {
            Some((cached_size, handle)) if *cached_size == half_size => handle.clone(),
            _ => {
                let wf_mesh = meshes.add(generate_wireframe_cube(half_size));
                creation_state
                    .wireframe_cache
                    .insert(zone_data.id, (half_size, wf_mesh.clone()));
                wf_mesh
            }
        };

        if let Some(entity) = solid_by_id.get(&zone_data.id) {
            if let Ok((_, mut mesh_ref, mut mat_ref, mut transform)) = solid_query.get_mut(*entity) {
                transform.translation = world_center;
                *mesh_ref = mesh_handle;
                *mat_ref = mat_handle;
            }
        } else {
            commands.spawn((
                PbrBundle {
                    mesh: mesh_handle,
                    material: mat_handle,
                    transform: Transform::from_translation(world_center),
                    ..default()
                },
                ScriptZoneVisual { zone_id: zone_data.id },
                RenderLayers::layer(EMBEDDED_3D_RENDER_LAYER),
            ));
        }

        if let Some(entity) = wire_by_id.get(&zone_data.id) {
            if let Ok((_, mut mesh_ref, mut transform)) = wire_query.get_mut(*entity) {
                transform.translation = world_center;
                *mesh_ref = wf_handle;
            }
        } else {
            let wf_material = materials.add(StandardMaterial {
                base_color: Color::rgba(1.0, 1.0, 1.0, if is_selected { 1.0 } else { 0.8 }),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                emissive: Color::WHITE,
                ..default()
            });

            commands.spawn((
                PbrBundle {
                    mesh: wf_handle,
                    material: wf_material,
                    transform: Transform::from_translation(world_center)
                        .with_scale(Vec3::splat(1.01)),
                    ..default()
                },
                ScriptZoneVisual { zone_id: zone_data.id },
                ZoneWireframe,
                RenderLayers::layer(EMBEDDED_3D_RENDER_LAYER),
            ));
        }
    }
}

// ============================================================================
// Компоновки: превью призрачных модулей в собранной сцене
// ============================================================================

fn compute_placement_signature(
    doc: &ProjectDoc,
    active_id: u32,
    show_placements: bool,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;

    let mut h = DefaultHasher::new();
    active_id.hash(&mut h);
    show_placements.hash(&mut h);
    doc.frames.len().hash(&mut h);

    for f in &doc.frames {
        f.id.hash(&mut h);
        f.revision.hash(&mut h);
        for a in f.pivot {
            a.to_bits().hash(&mut h);
        }
        f.sockets.len().hash(&mut h);

        for sock in &f.sockets {
            sock.id.hash(&mut h);
            sock.selected.hash(&mut h);
            for a in sock.anchor {
                a.to_bits().hash(&mut h);
            }
            sock.rot.hash(&mut h);
            sock.options.len().hash(&mut h);
            for o in &sock.options {
                o.frame_id.hash(&mut h);
            }
        }
    }
    h.finish()
}

fn frame_rgb(frame_id: u32) -> [f32; 3] {
    const PALETTE: &[[f32; 3]] = &[
        [0.85, 0.35, 0.35],
        [0.35, 0.85, 0.35],
        [0.35, 0.55, 0.90],
        [0.90, 0.80, 0.35],
        [0.85, 0.35, 0.85],
        [0.35, 0.85, 0.85],
    ];
    PALETTE[(frame_id as usize) % PALETTE.len()]
}

pub fn sync_placements_system(
    mut commands: Commands,
    doc_res: Res<ProjectDocResource>,
    active: Res<ActiveFrame>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<PlacementVisualState>,
    existing: Query<Entity, With<PlacedModuleVisual>>,
) {
    let signature = compute_placement_signature(
        &doc_res.0,
        active.id,
        state.show_placements,
    );

    if signature == state.last_signature && !state.force_refresh {
        return;
    }

    let forced = state.force_refresh;
    state.force_refresh = false;

    for e in existing.iter() {
        commands.entity(e).despawn_recursive();
    }
    state.entities.clear();
    state.last_signature = signature;
    state.debug_lines.clear();

    if !state.show_placements {
        state.debug_lines.push(
            "Показ размещённых модулей отключён (Stage 1 → кнопка «Модули»)".to_string(),
        );
        return;
    }

    let placements = doc_res.0.placements_from(active.id);
    state.debug_lines.push(format!("Активный фрейм: #{}", active.id));
    state.debug_lines.push(format!("Всего размещений: {}", placements.len()));
    if forced {
        state.debug_lines.push("Принудительная перестройка".to_string());
    }

    if placements.len() <= 1 {
        state.debug_lines.push(format!(
            "Активный фрейм #{} без вложенных модулей.",
            active.id
        ));
        return;
    }

    let transforms = compute_placement_transforms(&placements, &doc_res.0);
    let mut mat_cache: HashMap<u32, Handle<StandardMaterial>> = HashMap::new();
    let mut spawned_count = 0;

    for (idx, p) in placements.iter().enumerate() {
        if p.parent.is_none() {
            continue;
        }

        let Some(xf) = transforms[idx] else {
            state.debug_lines.push(format!("idx={} пропущен: нет трансформа", idx));
            continue;
        };

        let Some(module) = doc_res.0.frame(p.frame_id) else {
            continue;
        };
        if module.voxels.is_empty() {
            state.debug_lines.push(format!("idx={} пропущен: voxels пуст", idx));
            continue;
        }

        let material = mat_cache
            .entry(p.frame_id)
            .or_insert_with(|| {
                let [r, g, b] = frame_rgb(p.frame_id);
                materials.add(StandardMaterial {
                    base_color: Color::rgba(r, g, b, 0.75),
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })
            })
            .clone();

        let chunks_axis = (module.size as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;
        for cx in 0..chunks_axis {
            for cy in 0..chunks_axis {
                for cz in 0..chunks_axis {
                    let mesh = generate_frame_chunk_mesh(
                        module.size, &module.voxels, cx, cy, cz,
                    );
                    let entity = commands
                        .spawn((
                            PbrBundle {
                                mesh: meshes.add(mesh),
                                material: material.clone(),
                                transform: xf,
                                ..default()
                            },
                            PlacedModuleVisual { frame_id: p.frame_id },
                            EditorEntity,
                            RenderLayers::layer(EMBEDDED_3D_RENDER_LAYER),
                        ))
                        .id();
                    state.entities.push(entity);
                    spawned_count += 1;
                }
            }
        }
    }

    info!(
        "Синхронизация размещений: {} чанков для {} модулей.",
        spawned_count,
        placements.len() - 1
    );
    state.debug_lines.push(format!(
        "Создано {} чанков для {} модулей.",
        spawned_count,
        placements.len() - 1
    ));
}

pub fn strip_prepass_from_editor_cameras_system(
    mut commands: Commands,
    query: Query<Entity, (With<EditorEntity>, Or<(With<DepthPrepass>, With<NormalPrepass>)>)>,
) {
    for entity in &query {
        commands
            .entity(entity)
            .remove::<DepthPrepass>()
            .remove::<NormalPrepass>();
    }
}

fn compute_placement_transforms(
    placements: &[crate::model::Placement],
    doc: &ProjectDoc,
) -> Vec<Option<Transform>> {
    let mut transforms: Vec<Option<Transform>> = vec![None; placements.len()];

    for (idx, p) in placements.iter().enumerate() {
        if p.parent.is_none() {
            let frame_size = doc.frame(p.frame_id).map(|f| f.size).unwrap_or(1) as f32;
            transforms[idx] =
                Some(Transform::from_translation(Vec3::splat(-frame_size / 2.0)));
            continue;
        }

        let parent_idx = p.parent.unwrap();
        let (Some(parent_xf), Some(parent_frame), Some(module)) = (
            transforms[parent_idx],
            doc.frame(placements[parent_idx].frame_id),
            doc.frame(p.frame_id),
        ) else {
            continue;
        };

        let Some(socket) = parent_frame.sockets.iter().find(|s| Some(s.id) == p.socket_id)
        else {
            continue;
        };

        let anchor = Vec3::from_array(socket.anchor);
        let pivot = Vec3::from_array(module.pivot);

        // Поворот слота: четверти оборота X, Y, Z; порядок применения Z → Y → X.
        let r = socket.rot;
        let q = Quat::from_rotation_x(r[0] as f32 * std::f32::consts::FRAC_PI_2)
            * Quat::from_rotation_y(r[1] as f32 * std::f32::consts::FRAC_PI_2)
            * Quat::from_rotation_z(r[2] as f32 * std::f32::consts::FRAC_PI_2);

        let rotated_pivot = q * pivot;
        let translation = parent_xf.translation + parent_xf.rotation * (anchor - rotated_pivot);

        transforms[idx] = Some(Transform {
            translation,
            rotation: parent_xf.rotation * q,
            scale: Vec3::ONE,
        });
    }

    transforms
}