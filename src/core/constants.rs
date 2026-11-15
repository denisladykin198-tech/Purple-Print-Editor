//! Константы, категории, пресеты типов модулей, палитра зон.
//! Никакой логики — только данные и мелкие pure-хелперы.

use bevy_egui::egui;

// --- Сетка ---
pub const CHUNK_SIZE: usize = 64;
pub const DEFAULT_GRID_SIZE: u32 = 128;
pub const ALLOWED_GRID_SIZES: &[u32] = &[64, 128, 256, 512];

// --- Дерево фреймов ---
pub const NO_FRAME: i32 = -1;
pub const ROOT_FRAME_ID: u32 = 0;
pub const MAX_DEPTH: usize = 6;
pub const MAX_PLACEMENTS: usize = 256;

// --- Формат ---
pub const MAX_STR: usize = 4 * 1024 * 1024;
pub const MAX_FRAMES: u32 = 1024;
pub const MAX_SOCKETS: u32 = 4096;
pub const MAX_OPTIONS: u32 = 1024;
pub const MAX_ZONES: u32 = 65536;

// --- Палитра зон (индексируется по color_idx % len) ---
pub const ZONE_COLORS_F32: &[[f32; 3]] = &[
    [0.86, 0.24, 0.24],
    [0.90, 0.55, 0.16],
    [0.24, 0.71, 0.31],
    [0.24, 0.55, 0.90],
    [0.71, 0.31, 0.86],
    [0.86, 0.78, 0.20],
    [0.20, 0.78, 0.78],
    [0.78, 0.39, 0.59],
];

/// Цвет зоны в формате egui. `idx` заворачивается по длине палитры.
pub fn zone_color_egui(idx: usize) -> egui::Color32 {
    let c = ZONE_COLORS_F32[idx % ZONE_COLORS_F32.len()];
    egui::Color32::from_rgb(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
    )
}

// --- Категории объектов: (ключ, подпись, рекомендуемые подпапки) ---
pub const CATEGORIES: &[(&str, &str, &[&str])] = &[
    (
        "weapons",
        "Оружие",
        &["pistols", "rifles", "smg", "shotguns", "snipers", "melee", "grenades", "attachments"],
    ),
    (
        "vehicles",
        "Техника",
        &["tanks", "cars", "aircraft", "boats", "turrets", "parts"],
    ),
    ("npc", "НПС", &["humans", "creatures", "robots"]),
    (
        "props",
        "Предметы окружения",
        &["crates", "furniture", "containers", "decor"],
    ),
    ("buildings", "Строения", &["houses", "bunkers", "towers"]),
    (
        "items",
        "Снаряжение",
        &["armor", "ammo", "consumables", "tools"],
    ),
    ("misc", "Прочее", &[]),
];

pub fn category_label(key: &str) -> &'static str {
    CATEGORIES
        .iter()
        .find(|c| c.0 == key)
        .map(|c| c.1)
        .unwrap_or("Прочее")
}

pub fn category_subs(key: &str) -> &'static [&'static str] {
    CATEGORIES
        .iter()
        .find(|c| c.0 == key)
        .map(|c| c.2)
        .unwrap_or(&[])
}

// --- Типы модулей (тег, подпись) ---
pub const MODULE_TAG_PRESETS: &[(&str, &str)] = &[
    ("weapon_mod", "Обвес"),
    ("magazine", "Магазин"),
    ("barrel", "Ствол"),
    ("sight", "Прицел"),
    ("stock", "Приклад"),
    ("turret", "Башня"),
    ("gun", "Орудие"),
    ("wheel", "Колесо"),
    ("tracks", "Гусеницы"),
    ("armor", "Броня"),
    ("custom", "Свой тип"),
];