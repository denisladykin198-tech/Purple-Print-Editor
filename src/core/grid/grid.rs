//! Воксельная сетка редактора, конверсии world↔voxel, raycast-хелперы,
//! построение 3D из 2D-холстов и обратная проекция.

use bevy::prelude::*;

use crate::app::state::{TextureMode, ViewType};
use crate::canvases::OrthoCanvases;
use crate::constants::CHUNK_SIZE;
use crate::grid::raycast::{raycast_grid, RayHit};

// ============================================================================
// VoxelGrid
// ============================================================================

#[derive(Resource, Debug)]
pub struct VoxelGrid {
    pub size: UVec3,
    pub stride_y: usize,
    pub stride_z: usize,
    pub data: Vec<u8>,
    pub dirty_chunks: Vec<bool>,
    pub chunks_per_axis: usize,
    pub has_changes: bool,
    /// Схема раскраски, применённая к текущим чанк-мешам.
    /// `None` — rainbow по умолчанию. Меняется из `update_voxel_mesh_system`,
    /// когда `TextureUiState.applied_mode` отличается.
    pub applied_mode: Option<TextureMode>,
}

impl VoxelGrid {
    pub fn new(size: UVec3) -> Self {
        debug_assert!(
            size.x == size.y && size.y == size.z,
            "VoxelGrid поддерживает только кубические сетки; получено {:?}",
            size,
        );

        let total_voxels = (size.x * size.y * size.z) as usize;
        let chunks_x = (size.x as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let chunks_y = (size.y as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let chunks_z = (size.z as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let total_chunks = chunks_x * chunks_y * chunks_z;

        Self {
            data: vec![1u8; total_voxels],
            size,
            stride_y: size.x as usize,
            stride_z: (size.x * size.y) as usize,
            dirty_chunks: vec![true; total_chunks],
            chunks_per_axis: chunks_x,
            has_changes: true,
            applied_mode: None,
        }
    }

    /// Залить всю сетку одним значением. Эффективнее, чем `n³` вызовов `set`.
    pub fn fill(&mut self, val: bool) {
        self.data.fill(if val { 1 } else { 0 });
        self.mark_all_dirty();
    }

    #[inline]
    pub fn get(&self, x: u32, y: u32, z: u32) -> bool {
        if x >= self.size.x || y >= self.size.y || z >= self.size.z {
            return false;
        }
        let idx = (z as usize) * self.stride_z + (y as usize) * self.stride_y + x as usize;
        self.data.get(idx).copied().unwrap_or(0) == 1
    }

    #[inline]
    pub fn set(&mut self, x: u32, y: u32, z: u32, val: bool) {
        if x >= self.size.x || y >= self.size.y || z >= self.size.z {
            return;
        }

        let idx = (z as usize) * self.stride_z + (y as usize) * self.stride_y + x as usize;
        let new_val = if val { 1u8 } else { 0u8 };
        if self.data.get(idx).copied() == Some(new_val) {
            return;
        }
        if idx >= self.data.len() {
            return;
        }
        self.data[idx] = new_val;
        self.has_changes = true;

        let cx = (x as usize) / CHUNK_SIZE;
        let cy = (y as usize) / CHUNK_SIZE;
        let cz = (z as usize) / CHUNK_SIZE;

        let cpa = self.chunks_per_axis;
        let dirty = &mut self.dirty_chunks;

        let mut mark = |x: usize, y: usize, z: usize| {
            if x < cpa && y < cpa && z < cpa {
                let chunk_idx = z * (cpa * cpa) + y * cpa + x;
                if chunk_idx < dirty.len() {
                    dirty[chunk_idx] = true;
                }
            }
        };

        mark(cx, cy, cz);
        if x % CHUNK_SIZE as u32 == 0 && cx > 0 {
            mark(cx - 1, cy, cz);
        }
        if (x + 1) % CHUNK_SIZE as u32 == 0 && cx + 1 < cpa {
            mark(cx + 1, cy, cz);
        }
        if y % CHUNK_SIZE as u32 == 0 && cy > 0 {
            mark(cx, cy - 1, cz);
        }
        if (y + 1) % CHUNK_SIZE as u32 == 0 && cy + 1 < cpa {
            mark(cx, cy + 1, cz);
        }
        if z % CHUNK_SIZE as u32 == 0 && cz > 0 {
            mark(cx, cy, cz - 1);
        }
        if (z + 1) % CHUNK_SIZE as u32 == 0 && cz + 1 < cpa {
            mark(cx, cy, cz + 1);
        }
    }

    pub fn mark_all_dirty(&mut self) {
        for d in self.dirty_chunks.iter_mut() {
            *d = true;
        }
        self.has_changes = true;
    }

    #[inline]
    pub fn voxel_to_world(&self, voxel: UVec3) -> Vec3 {
        let offset = self.size.as_vec3() * 0.5;
        Vec3::new(
            voxel.x as f32 - offset.x,
            voxel.y as f32 - offset.y,
            voxel.z as f32 - offset.z,
        )
    }

    #[inline]
    pub fn world_to_voxel(&self, world: Vec3) -> IVec3 {
        let offset = self.size.as_vec3() * 0.5;
        IVec3::new(
            (world.x + offset.x).floor() as i32,
            (world.y + offset.y).floor() as i32,
            (world.z + offset.z).floor() as i32,
        )
    }
}

// ============================================================================
// Raycast из world в grid
// ============================================================================

/// Переводит луч `(origin, dir)` из мировых координат в координаты сетки и делает DDA.
pub fn raycast_editor_grid(origin: Vec3, dir: Vec3, grid: &VoxelGrid) -> Option<RayHit> {
    let offset = grid.size.as_vec3() * 0.5;
    let o = origin + offset;

    raycast_grid(
        [o.x, o.y, o.z],
        [dir.x, dir.y, dir.z],
        grid.size.to_array(),
        &|x, y, z| grid.get(x, y, z),
    )
}

/// Строит мировой луч из координат на экране внутри viewport.
///
/// `image_size` — размер viewport в пикселях, `bevy::prelude::Vec2`
/// (в UI обычно приходит как `egui::Vec2` — конвертируйте на вызывающей стороне).
///
/// * Перспектива: origin — позиция камеры, dir — направление через NDC.
/// * Ортокамера: origin сдвинут в плоскости экрана, dir — направление взгляда.
pub fn compute_ray_from_screen(
    screen_pos: Vec2,
    image_size: Vec2,
    cam_transform: &Transform,
    projection: &Projection,
) -> Option<(Vec3, Vec3)> {
    if image_size.x <= 0.0 || image_size.y <= 0.0 {
        return None;
    }

    let ndc_x = (screen_pos.x / image_size.x) * 2.0 - 1.0;
    let ndc_y = -(screen_pos.y / image_size.y) * 2.0 + 1.0;

    match projection {
        Projection::Perspective(persp) => {
            let tan_half_fov_y = (persp.fov * 0.5).tan();
            let aspect = persp.aspect_ratio;
            let dir_local = Vec3::new(
                ndc_x * tan_half_fov_y * aspect,
                ndc_y * tan_half_fov_y,
                -1.0,
            )
            .normalize();
            let dir_world = cam_transform.rotation * dir_local;
            Some((cam_transform.translation, dir_world))
        }
        Projection::Orthographic(ortho) => {
            // `Rect::half_size()` — метод, не поле. Bevy обновляет `area` каждый кадр,
            // но если оно ещё невалидно — fallback на `ortho.scale`.
            let half = ortho.area.half_size();
            let (half_w, half_h) = if half.x > 0.0 && half.y > 0.0 {
                (half.x, half.y)
            } else {
                let hh = ortho.scale * 0.5;
                let aspect = (image_size.x / image_size.y).max(f32::EPSILON);
                (hh * aspect, hh)
            };

            let origin_local = Vec3::new(ndc_x * half_w, ndc_y * half_h, 0.0);
            let origin_world = cam_transform.translation + cam_transform.rotation * origin_local;
            let dir_world = cam_transform.rotation * Vec3::NEG_Z;
            Some((origin_world, dir_world))
        }
    }
}

// ============================================================================
// 2D ↔ 3D
// ============================================================================

/// Пересечение трёх ортопроекций: solid(x,y,z) ⇔ front(x,y) ∧ top(x,z) ∧ side(y,z).
pub fn generate_3d_from_canvases(canvases: &OrthoCanvases, grid: &mut VoxelGrid) {
    let size = canvases
        .size
        .min(grid.size.x)
        .min(grid.size.y)
        .min(grid.size.z);

    grid.fill(false);

    for z in 0..size {
        for y in 0..size {
            let canvas_y = size - 1 - y;
            for x in 0..size {
                if canvases.get(ViewType::Front, x, canvas_y)
                    && canvases.get(ViewType::Top, x, z)
                    && canvases.get(ViewType::Side, z, canvas_y)
                {
                    grid.set(x, y, z, true);
                }
            }
        }
    }

    grid.mark_all_dirty();
}

/// Обратная операция: спроецировать воксели на 2D-холсты (для превью).
pub fn project_grid_to_canvases(grid: &VoxelGrid, canvases: &mut OrthoCanvases) {
    let size = canvases
        .size
        .min(grid.size.x)
        .min(grid.size.y)
        .min(grid.size.z);

    canvases.front.fill(0);
    canvases.top.fill(0);
    canvases.side.fill(0);

    for z in 0..size {
        for y in 0..size {
            let canvas_y = size - 1 - y;
            for x in 0..size {
                if grid.get(x, y, z) {
                    canvases.front[(canvas_y * size + x) as usize] = 1;
                    canvases.top[(z * size + x) as usize] = 1;
                    canvases.side[(canvas_y * size + z) as usize] = 1;
                }
            }
        }
    }

    canvases.dirty_front = true;
    canvases.dirty_top = true;
    canvases.dirty_side = true;
}