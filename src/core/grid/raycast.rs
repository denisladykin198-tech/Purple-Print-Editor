//! DDA-рейкаст по воксельной сетке (Amanatides & Woo).
//! Работает в координатах сетки `0..size` по каждой оси.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub voxel: [u32; 3],
    /// Нормаль грани, через которую луч вошёл в воксель.
    /// `[0, 0, 0]` — если стартовая клетка уже солидная (луч «начался внутри»).
    pub normal: [i32; 3],
    pub t: f32,
}

/// origin/dir — в координатах сетки (0..size по каждой оси).
pub fn raycast_grid(
    o: [f32; 3],
    d: [f32; 3],
    size: [u32; 3],
    solid: &dyn Fn(u32, u32, u32) -> bool,
) -> Option<RayHit> {
    const EPS: f32 = 1e-8;

    if d[0].abs() < EPS && d[1].abs() < EPS && d[2].abs() < EPS {
        return None;
    }

    let mut t_min = 0.0f32;
    let mut t_max = f32::INFINITY;
    let mut entry_axis: Option<usize> = None;

    for i in 0..3 {
        let s = size[i] as f32;
        if d[i].abs() < EPS {
            if o[i] < 0.0 || o[i] > s {
                return None;
            }
        } else {
            let inv = 1.0 / d[i];
            let mut t1 = (0.0 - o[i]) * inv;
            let mut t2 = (s - o[i]) * inv;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            if t1 > t_min {
                t_min = t1;
                entry_axis = Some(i);
            }
            if t2 < t_max {
                t_max = t2;
            }
            if t_min > t_max {
                return None;
            }
        }
    }

    if t_max < 0.0 {
        return None;
    }

    let p = [
        o[0] + d[0] * t_min,
        o[1] + d[1] * t_min,
        o[2] + d[2] * t_min,
    ];

    let mut cell = [0i32; 3];
    for i in 0..3 {
        cell[i] = (p[i].floor() as i32).clamp(0, size[i] as i32 - 1);
    }

    let mut step = [0i32; 3];
    let mut t_delta = [f32::INFINITY; 3];
    let mut t_next = [f32::INFINITY; 3];

    for i in 0..3 {
        if d[i] > EPS {
            step[i] = 1;
            t_delta[i] = 1.0 / d[i];
            t_next[i] = t_min + ((cell[i] as f32 + 1.0) - p[i]) / d[i];
        } else if d[i] < -EPS {
            step[i] = -1;
            t_delta[i] = 1.0 / (-d[i]);
            t_next[i] = t_min + (p[i] - cell[i] as f32) / (-d[i]);
        }
    }

    let mut normal = [0i32; 3];
    if let Some(a) = entry_axis {
        normal[a] = if d[a] > 0.0 { -1 } else { 1 };
    }

    let mut t = t_min;
    // DDA проходит не более size.x + size.y + size.z клеток.
    // Запас ×2 — на диагональные зигзаги вдоль рёбер.
    let max_iter = (size[0] + size[1] + size[2]) as usize * 2;

    for _ in 0..max_iter {
        if solid(cell[0] as u32, cell[1] as u32, cell[2] as u32) {
            return Some(RayHit {
                voxel: [cell[0] as u32, cell[1] as u32, cell[2] as u32],
                normal,
                t,
            });
        }

        let axis = if t_next[0] <= t_next[1] && t_next[0] <= t_next[2] {
            0
        } else if t_next[1] <= t_next[2] {
            1
        } else {
            2
        };

        if step[axis] == 0 {
            return None;
        }

        t = t_next[axis];
        if t > t_max {
            return None;
        }

        cell[axis] += step[axis];
        if cell[axis] < 0 || cell[axis] >= size[axis] as i32 {
            return None;
        }

        t_next[axis] += t_delta[axis];

        normal = [0; 3];
        normal[axis] = -step[axis];
    }

    None
}