//! Дизайн-токены и глобальная тема редактора.

use bevy_egui::egui::{self, Color32, Rounding, Stroke};

pub mod palette {
    use bevy_egui::egui::Color32;

    pub const BG: Color32        = Color32::from_rgb(18, 17, 24);   // фон приложения
    pub const PANEL: Color32     = Color32::from_rgb(24, 23, 31);   // панели, окна
    pub const SURFACE: Color32   = Color32::from_rgb(30, 29, 40);   // тулбары, карточки
    pub const SURFACE_2: Color32 = Color32::from_rgb(38, 36, 50);   // кнопки, инпуты
    pub const SURFACE_3: Color32 = Color32::from_rgb(48, 45, 64);   // hover
    pub const INPUT_BG: Color32  = Color32::from_rgb(14, 13, 19);   // фон TextEdit
    pub const BORDER: Color32    = Color32::from_rgb(52, 49, 68);

    pub const TEXT: Color32       = Color32::from_rgb(232, 230, 240);
    pub const TEXT_DIM: Color32   = Color32::from_rgb(154, 151, 168);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(110, 107, 123);

    // Брендовый фиолетовый, чуть поднятый по яркости для контраста на тёмном.
    pub const ACCENT: Color32         = Color32::from_rgb(158, 84, 255);
    pub const ACCENT_HOVER: Color32   = Color32::from_rgb(178, 112, 255);
    pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(130, 60, 220);

    pub const SUCCESS: Color32 = Color32::from_rgb(61, 220, 151);
    pub const WARN: Color32    = Color32::from_rgb(255, 196, 66);
    pub const DANGER: Color32  = Color32::from_rgb(255, 92, 92);
    pub const INFO: Color32    = Color32::from_rgb(96, 165, 250);
}

use palette as p;

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = p::BG;
    v.window_fill = p::PANEL;
    v.faint_bg_color = p::SURFACE;
    v.extreme_bg_color = p::INPUT_BG;
    v.code_bg_color = p::INPUT_BG;
    v.window_rounding = Rounding::same(10.0);
    v.menu_rounding = Rounding::same(8.0);
    v.window_stroke = Stroke::new(1.0, p::BORDER);
    v.selection.bg_fill = p::ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p::ACCENT);
    v.hyperlink_color = p::ACCENT_HOVER;
    v.warn_fg_color = p::WARN;
    v.error_fg_color = p::DANGER;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p::SURFACE;
    w.noninteractive.weak_bg_fill = p::PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p::BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p::TEXT_DIM);
    w.noninteractive.rounding = Rounding::same(6.0);

    w.inactive.bg_fill = p::SURFACE_2;
    w.inactive.weak_bg_fill = p::SURFACE_2;
    w.inactive.bg_stroke = Stroke::new(1.0, p::BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, p::TEXT);
    w.inactive.rounding = Rounding::same(6.0);

    w.hovered.bg_fill = p::SURFACE_3;
    w.hovered.weak_bg_fill = p::SURFACE_3;
    w.hovered.bg_stroke = Stroke::new(1.0, p::ACCENT.gamma_multiply(0.7));
    w.hovered.fg_stroke = Stroke::new(1.5, p::TEXT);
    w.hovered.rounding = Rounding::same(6.0);

    w.active.bg_fill = p::ACCENT.gamma_multiply(0.8);
    w.active.weak_bg_fill = p::ACCENT.gamma_multiply(0.8);
    w.active.bg_stroke = Stroke::new(1.0, p::ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.active.rounding = Rounding::same(6.0);

    w.open = w.hovered;
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.menu_margin = egui::Margin::same(6.0);
        s.spacing.window_margin = egui::Margin::same(10.0);
        s.spacing.interact_size.y = 26.0;
        s.spacing.slider_width = 140.0;
        s.spacing.combo_width = 120.0;
    });
}