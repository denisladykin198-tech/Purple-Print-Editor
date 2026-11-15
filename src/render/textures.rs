//! Раскраска вокселей: единый источник истины для мешера (`meshing.rs`)
//! и превью в Stage 7.
//!
//! Функция чистая: `(mode, x, y, z, size, params) -> [r, g, b, a]`.
//! Никаких egui-зависимостей — можно звать из вычислительных потоков.

use bevy::math::UVec3;

use crate::app::state::TextureMode;

/// Параметры раскраски, отделённые от `TextureUiState`
/// (чтобы не тянуть egui-зависимости в мешер).
#[derive(Clone, Copy, Debug)]
pub struct ColorParams {
    pub primary: [f32; 3],
    pub secondary: [f32; 3],
    pub noise_scale: f32,
    pub noise_seed: u64,
}

impl Default for ColorParams {
    fn default() -> Self {
        Self {
            primary: [0.55, 0.28, 0.85],
            secondary: [0.95, 0.72, 0.20],
            noise_scale: 4.0,
            noise_seed: 42,
        }
    }
}

/// Цвет вокселя `(x, y, z)` в сетке `size`.
///
/// `mode = None` — дефолтный rainbow-by-Y (совместимость с прежним
/// поведением `meshing::rainbow_from_y`).
///
/// `z` зарезервирован для будущих режимов раскраски по глубине;
/// сейчас ни один из режимов его не использует.
pub fn sample_voxel_color(
    mode: Option<TextureMode>,
    x: u32,
    y: u32,
    _z: u32,
    size: UVec3,
    params: ColorParams,
) -> [f32; 4] {
    let mode = mode.unwrap_or(TextureMode::RainbowHeight);

    let y_size = size.y.max(1) as f32;
    // t = 0 — низ, t = 1 — верх (Y растёт вверх, как в 3D).
    let t = 1.0 - (y as f32 / y_size);

    let rgb: [f32; 3] = match mode {
        TextureMode::RainbowHeight => rainbow_from_height(t),
        TextureMode::Solid => params.primary,
        TextureMode::HeightGradient => lerp3(params.primary, params.secondary, t),
        TextureMode::Checker => {
            // Шахматка в плоскости XY, клетка 8 вокселей.
            let on = ((x as usize / 8) + (y as usize / 8)) % 2 == 0;
            if on {
                params.primary
            } else {
                params.secondary
            }
        }
        TextureMode::Noise => {
            let nv = hash_noise(
                (x as f32 * params.noise_scale) as u32,
                (y as f32 * params.noise_scale) as u32,
                params.noise_seed,
            );
            lerp3(params.primary, params.secondary, nv)
        }
    };

    [rgb[0], rgb[1], rgb[2], 1.0]
}

/// Единая rainbow-палитра: используется и мешером, и превью Stage 7.
/// `t = 0` — низ, `t = 1` — верх.
pub fn rainbow_from_height(t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    let c = 0.68;
    let x_val = c * (1.0 - ((t * 6.0) % 2.0 - 1.0).abs());
    let m = 0.6 - c / 2.0;

    let (r, g, b) = if t < 1.0 / 6.0 {
        (c, x_val, 0.0)
    } else if t < 2.0 / 6.0 {
        (x_val, c, 0.0)
    } else if t < 3.0 / 6.0 {
        (0.0, c, x_val)
    } else if t < 4.0 / 6.0 {
        (0.0, x_val, c)
    } else if t < 5.0 / 6.0 {
        (x_val, 0.0, c)
    } else {
        (c, 0.0, x_val)
    };

    [r + m, g + m, b + m]
}

pub fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] * (1.0 - t) + b[0] * t,
        a[1] * (1.0 - t) + b[1] * t,
        a[2] * (1.0 - t) + b[2] * t,
    ]
}

/// Детерминированный 2D-noise для `TextureMode::Noise` (splitmix64).
pub fn hash_noise(x: u32, y: u32, seed: u64) -> f32 {
    let mut h = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((x as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add((y as u64).wrapping_mul(0x94D0_49BB_1331_11EB));
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    (h & 0xFFFF) as f32 / 65535.0
}