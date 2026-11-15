//! Алгоритмы сглаживания воксельной сетки.
//!
//! Все работают по окрестности 26 соседей (3×3×3 без центра).
//! `strength ∈ [0.1, 1.0]` управляет «агрессивностью»:
//! больше → больше вокселей меняется.

use crate::app::state::SmoothAlgorithm;
use crate::grid::grid::VoxelGrid;

/// Применяет выбранный алгоритм `iterations` раз.
/// Возвращает число изменённых вокселей (суммарно по всем проходам).
pub fn apply(
    grid: &mut VoxelGrid,
    algo: SmoothAlgorithm,
    iterations: u32,
    strength: f32,
) -> usize {
    match algo {
        SmoothAlgorithm::Median => median(grid, iterations, strength),
        SmoothAlgorithm::Laplacian => six_neighbor(grid, iterations, strength),
        SmoothAlgorithm::Gaussian => gaussian(grid, iterations, strength),
        SmoothAlgorithm::Taubin => two_pass_median(grid, iterations, strength),
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn solid_at(snap: &[u8], sx: usize, sy: usize, sz: usize, x: i32, y: i32, z: i32) -> bool {
    if x < 0 || y < 0 || z < 0 {
        return false;
    }
    let (x, y, z) = (x as usize, y as usize, z as usize);
    if x >= sx || y >= sy || z >= sz {
        return false;
    }
    snap[z * sx * sy + y * sx + x] == 1
}

/// Считает «взвешенное число солидных соседей» вокруг вокселя.
/// `weights` — 27 значений; индексация `(dz+1)*9 + (dy+1)*3 + (dx+1)`.
/// Центр (dx=dy=dz=0) не учитывается.
fn weighted_neighbors(
    snap: &[u8],
    sx: usize,
    sy: usize,
    sz: usize,
    x: u32,
    y: u32,
    z: u32,
    weights: &[u8; 27],
) -> u32 {
    let mut total = 0u32;
    for dz in -1..=1i32 {
        for dy in -1..=1i32 {
            for dx in -1..=1i32 {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }
                let k = ((dz + 1) * 9 + (dy + 1) * 3 + (dx + 1)) as usize;
                if solid_at(snap, sx, sy, sz, x as i32 + dx, y as i32 + dy, z as i32 + dz) {
                    total += weights[k] as u32;
                }
            }
        }
    }
    total
}

/// Общий проход: считает `should_be_solid` через замыкание `classify`
/// и применяет изменения в конце (чтобы не влиять на соседей внутри прохода).
fn one_pass<F>(grid: &mut VoxelGrid, classify: F) -> usize
where
    F: Fn(&[u8], usize, usize, usize, u32, u32, u32) -> bool,
{
    let snap = grid.data.clone();
    let (sx, sy, sz) = (
        grid.size.x as usize,
        grid.size.y as usize,
        grid.size.z as usize,
    );

    let mut changes: Vec<(u32, u32, u32, bool)> = Vec::new();
    for z in 0..grid.size.z {
        for y in 0..grid.size.y {
            for x in 0..grid.size.x {
                let is_solid_now = solid_at(&snap, sx, sy, sz, x as i32, y as i32, z as i32);
                let should_be_solid = classify(&snap, sx, sy, sz, x, y, z);
                if should_be_solid != is_solid_now {
                    changes.push((x, y, z, should_be_solid));
                }
            }
        }
    }

    for (x, y, z, val) in &changes {
        grid.set(*x, *y, *z, *val);
    }
    grid.mark_all_dirty();
    changes.len()
}

/// Порог «сколько взвешенных соседей нужно, чтобы воксель стал solid».
/// `strength = 0.5` даёт `mid`; больше — порог ниже (агрессивнее).
fn threshold_for(strength: f32, mid: f32, span: f32, lo: u32, hi: u32) -> u32 {
    ((mid - (strength - 0.5) * span) as i32).clamp(lo as i32, hi as i32) as u32
}

// ============================================================================
// Median
// ============================================================================

fn median(grid: &mut VoxelGrid, iterations: u32, strength: f32) -> usize {
    // 13 = половина из 26 (медиана). strength=1.0 → 5, strength=0.1 → 19.
    let threshold = threshold_for(strength, 13.0, 16.0, 2, 24);
    let weights = [1u8; 27];

    let mut total = 0;
    for _ in 0..iterations.max(1) {
        total += one_pass(grid, |snap, sx, sy, sz, x, y, z| {
            weighted_neighbors(snap, sx, sy, sz, x, y, z, &weights) >= threshold
        });
    }
    total
}

// ============================================================================
// Six-neighbor (бывший «Laplacian»)
// ============================================================================

/// Учитывает только 6 face-соседей. Не настоящий Laplacian
/// (для воксельной сетки он не имеет смысла), но интуитивно ожидаемое
/// «сглаживание по граням».
fn six_neighbor(grid: &mut VoxelGrid, iterations: u32, strength: f32) -> usize {
    // Медиана из 6 — 3. strength=1.0 → 1, strength=0.1 → 5.
    let threshold = threshold_for(strength, 3.0, 4.0, 1, 6);

    let mut weights = [0u8; 27];
    let mut k = 0usize;
    for dz in -1..=1i32 {
        for dy in -1..=1i32 {
            for dx in -1..=1i32 {
                let degree = dx.abs() + dy.abs() + dz.abs();
                weights[k] = if degree == 1 { 1 } else { 0 };
                k += 1;
            }
        }
    }

    let mut total = 0;
    for _ in 0..iterations.max(1) {
        total += one_pass(grid, |snap, sx, sy, sz, x, y, z| {
            weighted_neighbors(snap, sx, sy, sz, x, y, z, &weights) >= threshold
        });
    }
    total
}

// ============================================================================
// Gaussian
// ============================================================================

fn gaussian(grid: &mut VoxelGrid, iterations: u32, strength: f32) -> usize {
    // Классическое 3D-ядро: face=4, edge=2, corner=1.
    // Σ весов = 6*4 + 12*2 + 8*1 = 56; медиана = 28.
    let mut weights = [0u8; 27];
    let mut k = 0usize;
    for dz in -1..=1i32 {
        for dy in -1..=1i32 {
            for dx in -1..=1i32 {
                let degree = dx.abs() + dy.abs() + dz.abs();
                weights[k] = match degree {
                    0 => 0,
                    1 => 4,
                    2 => 2,
                    _ => 1,
                };
                k += 1;
            }
        }
    }

    // strength=0.5 → 28, strength=1.0 → 22, strength=0.1 → 33.
    let threshold = threshold_for(strength, 28.0, 11.2, 8, 48);

    let mut total = 0;
    for _ in 0..iterations.max(1) {
        total += one_pass(grid, |snap, sx, sy, sz, x, y, z| {
            weighted_neighbors(snap, sx, sy, sz, x, y, z, &weights) >= threshold
        });
    }
    total
}

// ============================================================================
// Two-pass median (бывший «Taubin»)
// ============================================================================

/// Двухфазное сглаживание: сначала «сжать» (median с высоким порогом),
/// затем «разжать» (median с низким порогом). Компенсирует усадку объёма,
/// которую даёт обычный median.
///
/// Это **не** классический Taubin-фильтр (тот определён на меш-геометрии),
/// а эвристика на воксельной сетке.
fn two_pass_median(grid: &mut VoxelGrid, iterations: u32, strength: f32) -> usize {
    let weights = [1u8; 27];
    let mut total = 0;

    for _ in 0..iterations.max(1) {
        // Forward (shrink): высокий порог.
        total += one_pass(grid, |snap, sx, sy, sz, x, y, z| {
            weighted_neighbors(snap, sx, sy, sz, x, y, z, &weights) >= 12
        });

        // Backward (inflate): чуть ниже порог, чтобы вернуть «сжатое».
        let backward_threshold = (11.0 - strength * 4.0) as u32;
        total += one_pass(grid, |snap, sx, sy, sz, x, y, z| {
            weighted_neighbors(snap, sx, sy, sz, x, y, z, &weights) >= backward_threshold
        });
    }
    total
}