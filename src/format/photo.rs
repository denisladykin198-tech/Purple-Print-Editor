//! Импорт фото в силуэт: grayscale → порог → 2D-маска → «вписать/растянуть/заполнить».
//! Всё чистое, без bevy и egui.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FitMode {
    /// растянуть на весь холст
    Stretch,
    /// вписать с сохранением пропорций (поля пустые)
    #[default]
    Fit,
    /// заполнить холст с обрезкой краёв
    Fill,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ApplyMode {
    #[default]
    Replace,
    Union,
    Subtract,
    Intersect,
}

/// Серое изображение, подогнанное под холст size×size.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub size: usize,
    pub gray: Vec<u8>,
    /// 1 = пиксель покрыт исходником, 0 = поля (всегда «пусто»)
    pub valid: Vec<u8>,
}

/// RGBA → яркость; прозрачное считается белым фоном.
pub fn rgba_to_gray_on_white(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .map(|p| {
            let l = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
            let a = p[3] as f32 / 255.0;
            (l * a + 255.0 * (1.0 - a)).round().clamp(0.0, 255.0) as u8
        })
        .collect()
}

/// Проверка целостности серого буфера (страйд должен совпадать с шириной).
pub fn gray_is_valid(gray: &[u8], w: usize, h: usize) -> bool {
    w > 0 && h > 0 && gray.len() == w.saturating_mul(h)
}

/// (покрытых пикселей, пикселей маски) — для диагностики в UI.
pub fn mask_stats(p: &Prepared, mask: &[u8]) -> (usize, usize) {
    let valid = p.valid.iter().filter(|&&v| v == 1).count();
    let filled = mask.iter().zip(&p.valid).filter(|(&m, &v)| m == 1 && v == 1).count();
    (valid, filled)
}

/// Отражения (в исходных осях), затем поворот по часовой стрелке на rotate×90°.
pub fn transform_gray(
    gray: &[u8],
    w: usize,
    h: usize,
    rotate: u8,
    flip_h: bool,
    flip_v: bool,
) -> (Vec<u8>, usize, usize) {
    let mut cur = gray.to_vec();
    let (mut cw, mut ch) = (w, h);

    if flip_h {
        for y in 0..ch {
            cur[y * cw..(y + 1) * cw].reverse();
        }
    }

    if flip_v {
        for y in 0..ch / 2 {
            for x in 0..cw {
                cur.swap(y * cw + x, (ch - 1 - y) * cw + x);
            }
        }
    }

    for _ in 0..(rotate % 4) {
        let mut nxt = vec![0u8; cw * ch];
        for y in 0..ch {
            for x in 0..cw {
                nxt[x * ch + (ch - 1 - y)] = cur[y * cw + x];
            }
        }
        cur = nxt;
        std::mem::swap(&mut cw, &mut ch);
    }

    (cur, cw, ch)
}

/// Площадная выборка (усреднение) исходника в холст size×size согласно режиму подгонки.
pub fn prepare_canvas(gray: &[u8], sw: usize, sh: usize, size: usize, fit: FitMode) -> Prepared {
    let mut out = vec![255u8; size * size];
    let mut valid = vec![0u8; size * size];

    if sw == 0 || sh == 0 || size == 0 || gray.len() != sw * sh {
        return Prepared { size, gray: out, valid };
    }

    let s = size as f32;
    let (kx, ky, ox, oy) = match fit {
        FitMode::Stretch => (s / sw as f32, s / sh as f32, 0.0, 0.0),
        FitMode::Fit => {
            let k = (s / sw as f32).min(s / sh as f32);
            (k, k, (s - sw as f32 * k) * 0.5, (s - sh as f32 * k) * 0.5)
        }
        FitMode::Fill => {
            let k = (s / sw as f32).max(s / sh as f32);
            (k, k, (s - sw as f32 * k) * 0.5, (s - sh as f32 * k) * 0.5)
        }
    };

    for dy in 0..size {
        let cy = (dy as f32 + 0.5 - oy) / ky;
        if cy < 0.0 || cy >= sh as f32 {
            continue;
        }

        let y0f = (dy as f32 - oy) / ky;
        let y1f = (dy as f32 + 1.0 - oy) / ky;
        let y0 = (y0f.floor().max(0.0) as usize).min(sh - 1);
        let y1 = (y1f.ceil().max(0.0) as usize).max(y0 + 1).min(sh);

        for dx in 0..size {
            let cx = (dx as f32 + 0.5 - ox) / kx;
            if cx < 0.0 || cx >= sw as f32 {
                continue;
            }

            let x0f = (dx as f32 - ox) / kx;
            let x1f = (dx as f32 + 1.0 - ox) / kx;
            let x0 = (x0f.floor().max(0.0) as usize).min(sw - 1);
            let x1 = (x1f.ceil().max(0.0) as usize).max(x0 + 1).min(sw);

            let mut sum = 0u64;
            let mut cnt = 0u64;
            for y in y0..y1 {
                let row = &gray[y * sw..(y + 1) * sw];
                for x in x0..x1 {
                    sum += row[x] as u64;
                    cnt += 1;
                }
            }

            out[dy * size + dx] = (sum / cnt.max(1)) as u8;
            valid[dy * size + dx] = 1;
        }
    }

    Prepared { size, gray: out, valid }
}

/// Тёмное (< порога) = объём. `invert` — наоборот. Поля всегда пустые.
pub fn threshold_mask(p: &Prepared, threshold: u8, invert: bool) -> Vec<u8> {
    p.gray
        .iter()
        .zip(p.valid.iter())
        .map(|(&g, &v)| if v == 0 { 0 } else { ((g < threshold) ^ invert) as u8 })
        .collect()
}

/// Автопорог по методу Оцу (по покрытым пикселям).
pub fn otsu_threshold(p: &Prepared) -> u8 {
    let mut hist = [0u64; 256];
    for (&g, &v) in p.gray.iter().zip(p.valid.iter()) {
        if v == 1 {
            hist[g as usize] += 1;
        }
    }

    let total: u64 = hist.iter().sum();
    if total == 0 {
        return 128;
    }

    let sum_all: f64 = hist.iter().enumerate().map(|(i, &h)| i as f64 * h as f64).sum();
    let (mut w_b, mut sum_b, mut best, mut max_var) = (0u64, 0f64, 128usize, -1.0f64);

    for t in 0..256usize {
        w_b += hist[t];
        if w_b == 0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0 {
            break;
        }
        sum_b += t as f64 * hist[t] as f64;
        let m_b = sum_b / w_b as f64;
        let m_f = (sum_all - sum_b) / w_f as f64;
        let var = w_b as f64 * w_f as f64 * (m_b - m_f) * (m_b - m_f);
        if var > max_var {
            max_var = var;
            best = t;
        }
    }

    (best + 1).clamp(1, 255) as u8
}

pub fn apply_mask(dst: &mut [u8], mask: &[u8], mode: ApplyMode) {
    for (d, &m) in dst.iter_mut().zip(mask.iter()) {
        let cur = *d == 1;
        let mk = m == 1;
        let r = match mode {
            ApplyMode::Replace => mk,
            ApplyMode::Union => cur || mk,
            ApplyMode::Subtract => cur && !mk,
            ApplyMode::Intersect => cur && mk,
        };
        *d = r as u8;
    }
}

/// Как `apply_mask`, но не трогает пиксели, где `valid == 0`
/// (вне площади фото). Для `Intersect`/`Replace` это принципиально:
/// поля фотографии не должны затирать холст.
pub fn apply_mask_opt(
    dst: &mut [u8],
    mask: &[u8],
    valid: &[u8],
    mode: ApplyMode,
) {
    for i in 0..dst.len().min(mask.len()).min(valid.len()) {
        if valid[i] == 0 {
            continue;
        }
        let cur = dst[i] == 1;
        let mk = mask[i] == 1;
        let r = match mode {
            ApplyMode::Replace => mk,
            ApplyMode::Union => cur || mk,
            ApplyMode::Subtract => cur && !mk,
            ApplyMode::Intersect => cur && mk,
        };
        dst[i] = r as u8;
    }
}