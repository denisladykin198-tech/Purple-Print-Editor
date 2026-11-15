//! Синхронизация между `ProjectDoc` и состоянием редактора
//! (VoxelGrid + OrthoCanvases + ScriptZonesStore), а также пересоздание чанк-сущностей.

use bevy::prelude::*;
use bevy::render::view::RenderLayers;

use crate::app::state::{
    Axis, ChunkMeshMap, EditorEntity, ScriptZoneData, ScriptZonesStore, VoxelChunk,
};
use crate::canvases::OrthoCanvases;
use crate::constants::CHUNK_SIZE;
use crate::core::mawt_engine::mawt_utils::embedded_3d_view::EMBEDDED_3D_RENDER_LAYER;
use crate::grid::grid::VoxelGrid;
use crate::meshing::empty_chunk_mesh;
use crate::model::{ProjectDoc, ZoneRec};

// ============================================================================
// Зоны: model ↔ store
// ============================================================================

pub fn zones_store_to_recs(store: &ScriptZonesStore) -> Vec<ZoneRec> {
    store
        .zones
        .iter()
        .map(|z| ZoneRec {
            id: z.id,
            name: z.name.clone(),
            axis: z.axis.to_u8(),
            min: [z.min.x, z.min.y, z.min.z],
            max: [z.max.x, z.max.y, z.max.z],
            color_idx: z.color_idx as u32,
            script: z.script.clone(),
        })
        .collect()
}

pub fn load_zones_from_recs(store: &mut ScriptZonesStore, recs: &[ZoneRec]) {
    store.zones = recs
        .iter()
        .map(|z| ScriptZoneData {
            id: z.id,
            name: z.name.clone(),
            axis: Axis::from_u8(z.axis),
            min: UVec3::from_slice(&z.min),
            max: UVec3::from_slice(&z.max),
            color_idx: z.color_idx as usize,
            script: z.script.clone(),
        })
        .collect();

    store.selected_zone_id = store.zones.first().map(|z| z.id);
    store.next_id = store.zones.iter().map(|z| z.id).max().unwrap_or(0) + 1;
}

// ============================================================================
// Frame ↔ editor state
// ============================================================================

/// Сохраняет текущее состояние редактора в фрейм `id`.
///
/// Не бампает `revision`, если ничего не поменялось — иначе
/// `compute_placement_signature` в `sync_placements_system` ложно
/// инвалидируется и все призрачные модули пересобираются на каждом
/// переключении фрейма / нажатии «Экспорт».
pub fn save_active_frame(
    doc: &mut ProjectDoc,
    id: u32,
    grid: &VoxelGrid,
    canvases: &OrthoCanvases,
    zones: &ScriptZonesStore,
) {
    let Some(f) = doc.frame_mut(id) else { return };
    let size = grid.size.x;
    let mut changed = false;

    // Voxels
    if f.size != size || f.voxels.len() != grid.data.len() {
        f.size = size;
        f.voxels = grid.data.clone();
        changed = true;
    } else if f.voxels != grid.data {
        f.voxels.copy_from_slice(&grid.data);
        changed = true;
    }

    // Canvases (front/top/side)
    if f.front.len() != canvases.front.len() {
        f.front = canvases.front.clone();
        changed = true;
    } else if f.front != canvases.front {
        f.front.copy_from_slice(&canvases.front);
        changed = true;
    }

    if f.top.len() != canvases.top.len() {
        f.top = canvases.top.clone();
        changed = true;
    } else if f.top != canvases.top {
        f.top.copy_from_slice(&canvases.top);
        changed = true;
    }

    if f.side.len() != canvases.side.len() {
        f.side = canvases.side.clone();
        changed = true;
    } else if f.side != canvases.side {
        f.side.copy_from_slice(&canvases.side);
        changed = true;
    }

    // Zones
    let new_zones = zones_store_to_recs(zones);
    if f.zones != new_zones {
        f.zones = new_zones;
        changed = true;
    }

    if changed {
        f.revision = f.revision.wrapping_add(1);
    }
}

/// Загружает данные фрейма `id` в состояние редактора.
pub fn load_frame_into_editor(
    doc: &ProjectDoc,
    id: u32,
    grid: &mut VoxelGrid,
    canvases: &mut OrthoCanvases,
    zones: &mut ScriptZonesStore,
) {
    let Some(f) = doc.frame(id) else { return };
    let size = f.size;

    if grid.size.x != size {
        *grid = VoxelGrid::new(UVec3::splat(size));
    }
    // Копируем данные фрейма всегда, а не только при совпадении размеров.
    // Иначе новый VoxelGrid останется заполненным единицами (сплошной куб),
    // и следующий save_active_frame затрёт реальные воксели фрейма этим кубом.
    if grid.data.len() == f.voxels.len() {
        grid.data.copy_from_slice(&f.voxels);
    }
    grid.mark_all_dirty();

    if canvases.size != size {
        *canvases = OrthoCanvases::new(size);
    }
    if canvases.front.len() == f.front.len() {
        canvases.front.copy_from_slice(&f.front);
    }
    if canvases.top.len() == f.top.len() {
        canvases.top.copy_from_slice(&f.top);
    }
    if canvases.side.len() == f.side.len() {
        canvases.side.copy_from_slice(&f.side);
    }
    canvases.dirty_front = true;
    canvases.dirty_top = true;
    canvases.dirty_side = true;

    load_zones_from_recs(zones, &f.zones);
}

// ============================================================================
// Чанк-сущности
// ============================================================================

pub fn total_chunks_for_size(size: u32) -> usize {
    let c = (size as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;
    c * c * c
}

/// Пересоздаёт чанк-сущности, если их количество не совпадает с ожидаемым
/// для `grid.size`. `existing_chunks` — уже существующие чанк-сущности
/// (будут despawn-нуты).
pub fn ensure_chunk_entities(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    chunk_map: &mut ChunkMeshMap,
    grid: &VoxelGrid,
    existing_chunks: Vec<Entity>,
) {
    let needed = total_chunks_for_size(grid.size.x);
    if chunk_map.meshes.len() == needed {
        return;
    }

    for entity in existing_chunks {
        commands.entity(entity).despawn_recursive();
    }

    chunk_map.meshes.clear();

    let size = grid.size.x;
    let chunks_axis = (size as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;
    let total_chunks = chunks_axis * chunks_axis * chunks_axis;

    chunk_map.meshes = vec![Handle::default(); total_chunks];

    let offset = size as f32 * 0.5;
    let world_pos = Vec3::splat(-offset);

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
                        transform: Transform::from_translation(world_pos),
                        ..default()
                    },
                    EditorEntity,
                    VoxelChunk,
                    RenderLayers::layer(EMBEDDED_3D_RENDER_LAYER),
                ));
            }
        }
    }
}