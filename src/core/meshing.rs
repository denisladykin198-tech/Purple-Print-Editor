//! Greedy meshing воксельных чанков + вспомогательные меши (пустой чанк, wireframe).

use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::state::TextureMode;
use crate::constants::CHUNK_SIZE;
use crate::grid::grid::VoxelGrid;
use crate::render::textures::{sample_voxel_color, ColorParams};

/// Толщина «рёбер» wireframe-куба в мировых единицах.
const WIREFRAME_THICKNESS: f32 = 0.02;

/// Greedy meshing по `VoxelGrid` с кастомной раскраской.
/// `color_of(x, y, z)` возвращает `[r, g, b, a]` для вокселя в мировых
/// координатах сетки. Вызывается ровно один раз на quad — по anchor-вокселю.
pub fn generate_chunk_mesh_greedy<F>(
    grid: &VoxelGrid,
    color_of: F,
    chunk_x: usize,
    chunk_y: usize,
    chunk_z: usize,
) -> Mesh
where
    F: Fn(u32, u32, u32) -> [f32; 4] + Sync,
{
    let is_solid = |gx: i32, gy: i32, gz: i32| -> bool {
        if gx < 0 || gy < 0 || gz < 0 {
            return false;
        }
        let (gx, gy, gz) = (gx as u32, gy as u32, gz as u32);
        if gx >= grid.size.x || gy >= grid.size.y || gz >= grid.size.z {
            return false;
        }
        let idx =
            (gz as usize) * grid.stride_z + (gy as usize) * grid.stride_y + (gx as usize);
        grid.data.get(idx).copied().unwrap_or(0) == 1
    };

    generate_chunk_mesh_greedy_raw(grid.size, is_solid, color_of, chunk_x, chunk_y, chunk_z)
}

/// Greedy meshing с произвольным источником «солидности» и раскраски.
///
/// * `is_solid(x, y, z)` — воксель заполнен?
/// * `color_of(x, y, z)` — цвет вокселя (используется anchor quads, а не avg).
pub fn generate_chunk_mesh_greedy_raw<F, G>(
    size: UVec3,
    is_solid: F,
    color_of: G,
    chunk_x: usize,
    chunk_y: usize,
    chunk_z: usize,
) -> Mesh
where
    F: Fn(i32, i32, i32) -> bool + Sync,
    G: Fn(u32, u32, u32) -> [f32; 4] + Sync,
{
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let start_x = chunk_x * CHUNK_SIZE;
    let end_x = ((chunk_x + 1) * CHUNK_SIZE).min(size.x as usize);
    let start_y = chunk_y * CHUNK_SIZE;
    let end_y = ((chunk_y + 1) * CHUNK_SIZE).min(size.y as usize);
    let start_z = chunk_z * CHUNK_SIZE;
    let end_z = ((chunk_z + 1) * CHUNK_SIZE).min(size.z as usize);

    let size_x = end_x - start_x;
    let size_y = end_y - start_y;
    let size_z = end_z - start_z;

    if size_x == 0 || size_y == 0 || size_z == 0 {
        return empty_chunk_mesh();
    }

    for axis in 0..3 {
        for &sign in &[1i32, -1i32] {
            let (u_size, v_size) = match axis {
                0 => (size_y, size_z),
                1 => (size_x, size_z),
                _ => (size_x, size_y),
            };
            let slice_count = match axis {
                0 => size_x,
                1 => size_y,
                _ => size_z,
            };

            let mut mask = vec![0u8; u_size * v_size];
            let mut visited = vec![false; u_size * v_size];

            for slice in 0..slice_count {
                mask.fill(0);
                visited.fill(false);

                // Заполняем mask для текущего среза.
                for v in 0..v_size {
                    for u in 0..u_size {
                        let (gx, gy, gz) = match axis {
                            0 => (
                                start_x as i32 + slice as i32,
                                start_y as i32 + u as i32,
                                start_z as i32 + v as i32,
                            ),
                            1 => (
                                start_x as i32 + u as i32,
                                start_y as i32 + slice as i32,
                                start_z as i32 + v as i32,
                            ),
                            _ => (
                                start_x as i32 + u as i32,
                                start_y as i32 + v as i32,
                                start_z as i32 + slice as i32,
                            ),
                        };

                        if !is_solid(gx, gy, gz) {
                            continue;
                        }

                        let (nx, ny, nz) = match axis {
                            0 => (gx + sign, gy, gz),
                            1 => (gx, gy + sign, gz),
                            _ => (gx, gy, gz + sign),
                        };

                        if !is_solid(nx, ny, nz) {
                            mask[v * u_size + u] = 1;
                        }
                    }
                }

                // Greedy-развёртка маски в quads.
                for v in 0..v_size {
                    for u in 0..u_size {
                        let idx = v * u_size + u;
                        if mask[idx] == 0 || visited[idx] {
                            continue;
                        }

                        let mut width = 1;
                        while u + width < u_size
                            && mask[v * u_size + (u + width)] == 1
                            && !visited[v * u_size + (u + width)]
                        {
                            width += 1;
                        }

                        let mut height = 1;
                        'height: while v + height < v_size {
                            for w in 0..width {
                                if mask[(v + height) * u_size + (u + w)] == 0
                                    || visited[(v + height) * u_size + (u + w)]
                                {
                                    break 'height;
                                }
                            }
                            height += 1;
                        }

                        for h in 0..height {
                            for w in 0..width {
                                visited[(v + h) * u_size + (u + w)] = true;
                            }
                        }

                        let (x0, y0, z0, x1, y1, z1) = match axis {
                            0 => {
                                let x = if sign > 0 {
                                    (start_x + slice + 1) as f32
                                } else {
                                    (start_x + slice) as f32
                                };
                                (
                                    x,
                                    (start_y + u) as f32,
                                    (start_z + v) as f32,
                                    x,
                                    (start_y + u + width) as f32,
                                    (start_z + v + height) as f32,
                                )
                            }
                            1 => {
                                let y = if sign > 0 {
                                    (start_y + slice + 1) as f32
                                } else {
                                    (start_y + slice) as f32
                                };
                                (
                                    (start_x + u) as f32,
                                    y,
                                    (start_z + v) as f32,
                                    (start_x + u + width) as f32,
                                    y,
                                    (start_z + v + height) as f32,
                                )
                            }
                            _ => {
                                let z = if sign > 0 {
                                    (start_z + slice + 1) as f32
                                } else {
                                    (start_z + slice) as f32
                                };
                                (
                                    (start_x + u) as f32,
                                    (start_y + v) as f32,
                                    z,
                                    (start_x + u + width) as f32,
                                    (start_y + v + height) as f32,
                                    z,
                                )
                            }
                        };

                        let (v0, v1, v2, v3) = match axis {
                            0 => {
                                if sign > 0 {
                                    ([x0, y0, z0], [x0, y1, z0], [x0, y1, z1], [x0, y0, z1])
                                } else {
                                    ([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0])
                                }
                            }
                            1 => {
                                if sign > 0 {
                                    ([x0, y0, z0], [x0, y0, z1], [x1, y0, z1], [x1, y0, z0])
                                } else {
                                    ([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1])
                                }
                            }
                            _ => {
                                if sign > 0 {
                                    ([x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0])
                                } else {
                                    ([x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0])
                                }
                            }
                        };

                        let normal = match axis {
                            0 => [sign as f32, 0.0, 0.0],
                            1 => [0.0, sign as f32, 0.0],
                            _ => [0.0, 0.0, sign as f32],
                        };

                        // Anchor-воксель quads — по нему берём цвет.
                        let (ax, ay, az) = match axis {
                            0 => (
                                (start_x + slice) as u32,
                                (start_y + u) as u32,
                                (start_z + v) as u32,
                            ),
                            1 => (
                                (start_x + u) as u32,
                                (start_y + slice) as u32,
                                (start_z + v) as u32,
                            ),
                            _ => (
                                (start_x + u) as u32,
                                (start_y + v) as u32,
                                (start_z + slice) as u32,
                            ),
                        };
                        let color = color_of(ax, ay, az);

                        let base = positions.len() as u32;

                        positions.extend_from_slice(&[v0, v1, v2, v3]);
                        normals.extend_from_slice(&[normal; 4]);
                        uvs.extend_from_slice(&[
                            [0.0, 0.0],
                            [1.0, 0.0],
                            [1.0, 1.0],
                            [0.0, 1.0],
                        ]);
                        colors.extend_from_slice(&[color; 4]);
                        indices.extend_from_slice(&[
                            base,
                            base + 1,
                            base + 2,
                            base,
                            base + 2,
                            base + 3,
                        ]);
                    }
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    if !positions.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(indices));
    }

    mesh
}

pub fn empty_chunk_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    mesh.insert_indices(Indices::U32(Vec::new()));
    mesh
}

pub fn generate_wireframe_cube(half_size: Vec3) -> Mesh {
    let hs = half_size;
    let vertices: [[f32; 3]; 8] = [
        [-hs.x, -hs.y, -hs.z],
        [hs.x, -hs.y, -hs.z],
        [hs.x, hs.y, -hs.z],
        [-hs.x, hs.y, -hs.z],
        [-hs.x, -hs.y, hs.z],
        [hs.x, -hs.y, hs.z],
        [hs.x, hs.y, hs.z],
        [-hs.x, hs.y, hs.z],
    ];

    let edges: [(usize, usize); 12] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let thickness = WIREFRAME_THICKNESS;

    for &(a, b) in &edges {
        let va = Vec3::from_array(vertices[a]);
        let vb = Vec3::from_array(vertices[b]);
        let dir = (vb - va).normalize();
        let perp = if dir.abs().x < 0.9 {
            Vec3::X.cross(dir)
        } else {
            Vec3::Y.cross(dir)
        }
        .normalize();

        let p0 = va - perp * thickness;
        let p1 = va + perp * thickness;
        let p2 = vb + perp * thickness;
        let p3 = vb - perp * thickness;

        let base = positions.len() as u32;
        positions.extend_from_slice(&[
            p0.to_array(),
            p1.to_array(),
            p2.to_array(),
            p3.to_array(),
        ]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    let n = positions.len();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    // Wireframe рендерится с unlit-материалом; нормали не используются.
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; n]);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Сгенерировать меш одного чанка напрямую из сырых вокселей фрейма.
/// Раскраска — rainbow (используется для призрачных модулей).
pub fn generate_frame_chunk_mesh(
    size: u32,
    voxels: &[u8],
    chunk_x: usize,
    chunk_y: usize,
    chunk_z: usize,
) -> Mesh {
    let n = size as usize;
    if voxels.len() != n * n * n {
        return empty_chunk_mesh();
    }

    let stride_y = n;
    let stride_z = n * n;

    let is_solid = |gx: i32, gy: i32, gz: i32| -> bool {
        if gx < 0 || gy < 0 || gz < 0 {
            return false;
        }
        let (gx, gy, gz) = (gx as usize, gy as usize, gz as usize);
        if gx >= n || gy >= n || gz >= n {
            return false;
        }
        voxels[gz * stride_z + gy * stride_y + gx] == 1
    };

    let voxel_size = UVec3::splat(size);
    let color_of = move |x: u32, y: u32, z: u32| -> [f32; 4] {
        sample_voxel_color(
            Some(TextureMode::RainbowHeight),
            x,
            y,
            z,
            voxel_size,
            ColorParams::default(),
        )
    };

    generate_chunk_mesh_greedy_raw(voxel_size, is_solid, color_of, chunk_x, chunk_y, chunk_z)
}