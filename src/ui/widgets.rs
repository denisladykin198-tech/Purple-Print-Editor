//! Переиспользуемые компоненты: тулбары, кнопки, карточки, бейджи, шапки вьюпортов.

use bevy::prelude::{Transform, Vec3};

use bevy_egui::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke};
use super::theme::palette as p;
use crate::app::state::{FullscreenState, ViewType};
use crate::core::mawt_engine::mawt_utils::embedded_3d_view;

/// Горизонтальный тулбар в едином контейнере.
/// `horizontal_wrapped` позволяет кнопкам переноситься на узких экранах.
pub fn toolbar<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(p::SURFACE)
        .rounding(8.0)
        .stroke(Stroke::new(1.0, p::BORDER))
        .inner_margin(egui::Margin::symmetric(10.0, 7.0))
        .show(ui, |ui| ui.horizontal_wrapped(add).inner)
        .inner
}

/// Кнопка-переключатель инструмента (сегментированный контрол).
pub fn tool_toggle(ui: &mut egui::Ui, selected: bool, text: impl Into<String>) -> egui::Response {
    let label = RichText::new(text)
        .size(13.0)
        .color(if selected { p::TEXT } else { p::TEXT_DIM });
    ui.add(
        egui::Button::new(label)
            .fill(if selected { p::ACCENT.gamma_multiply(0.22) } else { Color32::TRANSPARENT })
            .stroke(if selected { Stroke::new(1.0, p::ACCENT) } else { Stroke::NONE })
            .rounding(6.0),
    )
}

#[derive(Clone, Copy)]
enum ButtonKind { Primary, Danger, Ghost }

fn apply_button_style(ui: &mut egui::Ui, kind: ButtonKind) {
    let w = &mut ui.style_mut().visuals.widgets;
    match kind {
        ButtonKind::Primary => {
            w.inactive.weak_bg_fill = p::ACCENT;
            w.inactive.bg_fill = p::ACCENT;
            w.inactive.bg_stroke = Stroke::NONE;
            w.inactive.fg_stroke = Stroke::new(1.0, Color32::WHITE);
            w.hovered.weak_bg_fill = p::ACCENT_HOVER;
            w.hovered.bg_fill = p::ACCENT_HOVER;
            w.hovered.bg_stroke = Stroke::NONE;
            w.active.weak_bg_fill = p::ACCENT_PRESSED;
            w.active.bg_fill = p::ACCENT_PRESSED;
        }
        ButtonKind::Danger => {
            w.inactive.weak_bg_fill = p::DANGER.gamma_multiply(0.15);
            w.inactive.bg_fill = p::DANGER.gamma_multiply(0.15);
            w.inactive.bg_stroke = Stroke::new(1.0, p::DANGER.gamma_multiply(0.4));
            w.inactive.fg_stroke = Stroke::new(1.0, p::DANGER);
            w.hovered.weak_bg_fill = p::DANGER.gamma_multiply(0.3);
            w.hovered.bg_fill = p::DANGER.gamma_multiply(0.3);
            w.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
            w.active.weak_bg_fill = p::DANGER.gamma_multiply(0.45);
            w.active.bg_fill = p::DANGER.gamma_multiply(0.45);
        }
        ButtonKind::Ghost => {
            w.inactive.weak_bg_fill = Color32::TRANSPARENT;
            w.inactive.bg_fill = Color32::TRANSPARENT;
            w.inactive.bg_stroke = Stroke::NONE;
            w.inactive.fg_stroke = Stroke::new(1.0, p::TEXT_DIM);
            w.hovered.weak_bg_fill = p::SURFACE_2;
            w.hovered.bg_fill = p::SURFACE_2;
            w.hovered.bg_stroke = Stroke::NONE;
            w.hovered.fg_stroke = Stroke::new(1.0, p::TEXT);
            w.active.weak_bg_fill = p::SURFACE_3;
            w.active.bg_fill = p::SURFACE_3;
        }
    }
}

fn button_impl(ui: &mut egui::Ui, text: impl Into<String>, kind: ButtonKind, full_width: bool) -> egui::Response {
    let mut label = RichText::new(text);
    if matches!(kind, ButtonKind::Primary) {
        label = label.strong();
    }
    ui.scope(|ui| {
        apply_button_style(ui, kind);
        if full_width {
            let size = egui::vec2(ui.available_width(), 30.0);
            ui.add_sized(size, egui::Button::new(label))
        } else {
            ui.button(label)
        }
    })
    .inner
}

/// Главное действие экрана.
pub fn primary_button(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    button_impl(ui, text, ButtonKind::Primary, false)
}
/// Главное действие на всю ширину контейнера (узкие панели).
pub fn primary_button_full(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    button_impl(ui, text, ButtonKind::Primary, true)
}
/// Разрушающее действие (удалить, пересоздать).
pub fn danger_button(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    button_impl(ui, text, ButtonKind::Danger, false)
}
pub fn danger_button_full(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    button_impl(ui, text, ButtonKind::Danger, true)
}
/// Второстепенное действие без рамки.
pub fn ghost_button(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    button_impl(ui, text, ButtonKind::Ghost, false)
}
pub fn ghost_button_full(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    button_impl(ui, text, ButtonKind::Ghost, true)
}

/// Заголовок секции с акцентной полоской.
pub fn section(ui: &mut egui::Ui, title: impl Into<String>) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 14.0), Sense::hover());
        ui.painter().rect_filled(rect, 1.5, p::ACCENT);
        ui.label(RichText::new(title).size(14.0).strong().color(p::TEXT));
    });
    ui.add_space(2.0);
}

/// Карточка-контейнер для панелей свойств.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(p::PANEL)
        .rounding(8.0)
        .stroke(Stroke::new(1.0, p::BORDER))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, add)
        .inner
}

/// Цветной бейдж-счётчик.
pub fn badge(ui: &mut egui::Ui, text: impl Into<String>, color: Color32) {
    egui::Frame::none()
        .fill(color.gamma_multiply(0.15))
        .rounding(4.0)
        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(color));
        });
}

/// Приглушённая подсказка.
pub fn hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).size(12.0).color(p::TEXT_MUTED));
}

/// Заглушка для пустых списков.
pub fn empty_state(ui: &mut egui::Ui, icon: &str, text: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(16.0);
        ui.label(RichText::new(icon).size(24.0).color(p::TEXT_MUTED));
        ui.add_space(4.0);
        ui.label(RichText::new(text).size(12.0).color(p::TEXT_MUTED));
        ui.add_space(16.0);
    });
}

/// Метрика-плитка для экрана экспорта.
pub fn stat(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
    egui::Frame::none()
        .fill(p::SURFACE)
        .rounding(8.0)
        .stroke(Stroke::new(1.0, p::BORDER))
        .inner_margin(egui::Margin::symmetric(14.0, 8.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(value).size(20.0).strong().color(color));
                ui.label(RichText::new(label).size(11.0).color(p::TEXT_MUTED));
            });
        });
}

fn toggle_fullscreen(fullscreen: &mut FullscreenState, view: ViewType) {
    fullscreen.active = if fullscreen.active == Some(view) { None } else { Some(view) };
}

/// Единая шапка вьюпорта (2D-холст или 3D): заголовок-клик + ⛶ + доп. кнопки.
pub fn viewport_header(
    ui: &mut egui::Ui,
    icon_name: &str,
    title: &str,
    view: ViewType,
    fullscreen: &mut FullscreenState,
    extra: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        let resp = ui
            .horizontal(|ui| {
                icon_impl(ui, icon_name, 14.0, p::TEXT);
                ui.add(
                    egui::Label::new(RichText::new(title).strong().size(13.0).color(p::TEXT))
                        .sense(Sense::click()),
                )
            })
            .response
            .on_hover_text("Клик — развернуть/свернуть");
        if resp.clicked() {
            toggle_fullscreen(fullscreen, view);
        }
        extra(ui);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let is_full = fullscreen.active == Some(view);
            let name = if is_full { "compress" } else { "expand" };
            if icon_button(ui, name, 14.0, "Полноэкранный режим").clicked() {
                toggle_fullscreen(fullscreen, view);
            }
        });
    });
}

// ============================================================================
// 3D-камера: вращение + зум
// ============================================================================

/// Управление орбитальной камерой для 3D-вьюпорта.
///
/// **Багфикс (2024):** раньше питч был `Quat::from_rotation_x(...)`, т.е.
/// вращение вокруг **мировой** оси X. Когда камера смотрит сбоку (стоит
/// на оси X, вид YZ), позиция камеры коллинеарна оси вращения — она
/// практически не двигалась и дёргалась от float-погрешностей.
///
/// Теперь работаем в сферических координатах:
/// * `theta` — полярный угол от +Y (0 = камера сверху над origin),
///   клампится к `[0.05, π−0.05]` — вьюпорт никогда не упирается в полюс;
/// * `phi` — азимут в плоскости XZ.
///
/// Питч меняет `theta`, йоу меняет `phi` — это работает **одинаково
/// из любого азимута**, включая «боковой» вид.
///
/// * **drag** → вращение;
/// * **scroll** → зум **только если курсор над вьюпортом** (защита от
///   «зум при прокрутке над тулбаром»).
pub fn navigate_3d_camera(
    ui: &egui::Ui,
    response: &embedded_3d_view::Embedded3dViewResponse,
    transform: &mut Transform,
) {
    if response.dragged {
        let delta = response.drag_delta;
        let offset = transform.translation;
        let radius = offset.length().max(1.0);
        let dir = offset / radius;

        // Сферические координаты текущей позиции.
        let theta = dir.y.clamp(-1.0, 1.0).acos(); // 0 = сверху, π = снизу
        let phi = dir.z.atan2(dir.x);

        // Знаки подобраны так, чтобы сохранить прежние «ощущения» drag'а:
        //   вправо → камера уходит влево (как `from_rotation_y(-dx)`);
        //   вниз   → камера поднимается, фигура уходит вниз (как `from_rotation_x(-dy)`).
        let new_phi = phi + delta.x * 0.008;
        let new_theta = (theta - delta.y * 0.008)
            .clamp(0.05, std::f32::consts::PI - 0.05);

        let sin_t = new_theta.sin();
        let new_dir = Vec3::new(
            sin_t * new_phi.cos(),
            new_theta.cos(),
            sin_t * new_phi.sin(),
        );

        transform.translation = new_dir * radius;
    }

    // Зум — только над вьюпортом и без активного drag.
    let is_hovered = ui
        .input(|i| i.pointer.latest_pos())
        .map_or(false, |p| response.rect.contains(p));

    if is_hovered && !response.dragged {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            let radius = transform.translation.length().max(1.0);
            let radius = (radius - scroll_delta * radius * 0.005).clamp(10.0, 1000.0);
            transform.translation = transform.translation.normalize() * radius;
        }
    }

    // Theta клампится от полюсов → forward никогда не параллелен Y,
    // `look_at(_, Y)` определён всегда. Fallback — defense-in-depth.
    let forward = -transform.translation.normalize();
    let up = if forward.dot(Vec3::Y).abs() > 0.9999 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    transform.look_at(Vec3::ZERO, up);
}

// ============================================================================
// СВГ-иконки
// ============================================================================

use super::icons;

/// Inline-иконка (без фона, некликабельная).
pub fn icon(ui: &mut egui::Ui, name: &str, size: f32, color: Color32) {
    let _ = icon_impl(ui, name, size, color);
}

/// Inline-иконка с возвратом `Response` (для .on_hover_text и пр.).
pub fn icon_impl(ui: &mut egui::Ui, name: &str, size: f32, color: Color32) -> egui::Response {
    if let Some(tex) = icons::texture(ui.ctx(), name, size, color) {
        ui.add(
            egui::Image::new(&tex)
                .fit_to_exact_size(egui::vec2(size, size))
                .sense(Sense::hover()),
        )
    } else {
        ui.label(RichText::new("?").color(color))
    }
}

/// Кнопка-иконка (квадратная, с hover и tooltip).
pub fn icon_button(
    ui: &mut egui::Ui,
    name: &str,
    size: f32,
    hint: &str,
) -> egui::Response {
    icon_button_colored(ui, name, size, p::TEXT_DIM, p::TEXT, hint)
}

/// Кнопка-иконка с настраиваемыми цветами idle/hover.
pub fn icon_button_colored(
    ui: &mut egui::Ui,
    name: &str,
    size: f32,
    idle: Color32,
    hover: Color32,
    hint: &str,
) -> egui::Response {
    let pad = 4.0;
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(size + pad * 2.0, size + pad * 2.0),
        Sense::click(),
    );

    let color = if resp.hovered() { hover } else { idle };
    let bg = if resp.hovered() { p::SURFACE_2 } else { Color32::TRANSPARENT };
    ui.painter().rect_filled(rect, 6.0, bg);

    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(size, size));
    if let Some(tex) = icons::texture(ui.ctx(), name, size, color) {
        egui::Image::new(&tex)
            .fit_to_exact_size(egui::vec2(size, size))
            .paint_at(ui, icon_rect);
    }

    if !hint.is_empty() {
        resp.on_hover_text(hint)
    } else {
        resp
    }
}

// ── Кнопки «иконка + текст» с сохранением стилей ────────────────────────────

fn icon_text_button_impl(
    ui: &mut egui::Ui,
    icon_name: &str,
    text: impl Into<String>,
    _kind: ButtonKind,
    full_width: bool,
) -> egui::Response {
    let text = text.into();
    let img_size = 14.0;
    let pad_x = 10.0;
    let gap = 5.0;
    let height = 26.0;

    let galley = ui.painter().layout_no_wrap(
        text.clone(),
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );
    let content_w = img_size + gap + galley.size().x;

    let total_w = if full_width {
        ui.available_width()
    } else {
        (content_w + pad_x * 2.0).max(40.0)
    };

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(total_w, height), Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }

    let visuals = ui.style().interact(&resp);
    let fg = visuals.fg_stroke.color;

    ui.painter().rect(rect, 6.0, visuals.weak_bg_fill, visuals.bg_stroke);

    // Центрируем контент при full_width, иначе — от левого края.
    let content_x = if full_width {
        rect.center().x - content_w * 0.5
    } else {
        rect.min.x + pad_x
    };

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(content_x + img_size * 0.5, rect.center().y),
        egui::vec2(img_size, img_size),
    );
    if let Some(tex) = icons::texture(ui.ctx(), icon_name, img_size, fg) {
        egui::Image::new(&tex)
            .fit_to_exact_size(egui::vec2(img_size, img_size))
            .paint_at(ui, icon_rect);
    }

    let text_pos = egui::pos2(
        content_x + img_size + gap,
        rect.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(text_pos, galley, fg);

    resp
}

/// Главное действие с иконкой.
pub fn primary_button_icon(ui: &mut egui::Ui, icon_name: &str, text: impl Into<String>) -> egui::Response {
    ui.scope(|ui| {
        apply_button_style(ui, ButtonKind::Primary);
        icon_text_button_impl(ui, icon_name, text, ButtonKind::Primary, false)
    })
    .inner
}

/// Главное действие на всю ширину, с иконкой.
pub fn primary_button_icon_full(ui: &mut egui::Ui, icon_name: &str, text: impl Into<String>) -> egui::Response {
    ui.scope(|ui| {
        apply_button_style(ui, ButtonKind::Primary);
        icon_text_button_impl(ui, icon_name, text, ButtonKind::Primary, true)
    })
    .inner
}

/// Разрушающее действие с иконкой.
pub fn danger_button_icon(ui: &mut egui::Ui, icon_name: &str, text: impl Into<String>) -> egui::Response {
    ui.scope(|ui| {
        apply_button_style(ui, ButtonKind::Danger);
        icon_text_button_impl(ui, icon_name, text, ButtonKind::Danger, false)
    })
    .inner
}
pub fn danger_button_icon_full(ui: &mut egui::Ui, icon_name: &str, text: impl Into<String>) -> egui::Response {
    ui.scope(|ui| {
        apply_button_style(ui, ButtonKind::Danger);
        icon_text_button_impl(ui, icon_name, text, ButtonKind::Danger, true)
    })
    .inner
}

/// Второстепенное действие с иконкой.
pub fn ghost_button_icon(ui: &mut egui::Ui, icon_name: &str, text: impl Into<String>) -> egui::Response {
    ui.scope(|ui| {
        apply_button_style(ui, ButtonKind::Ghost);
        icon_text_button_impl(ui, icon_name, text, ButtonKind::Ghost, false)
    })
    .inner
}
pub fn ghost_button_icon_full(ui: &mut egui::Ui, icon_name: &str, text: impl Into<String>) -> egui::Response {
    ui.scope(|ui| {
        apply_button_style(ui, ButtonKind::Ghost);
        icon_text_button_impl(ui, icon_name, text, ButtonKind::Ghost, true)
    })
    .inner
}

/// Переключатель инструмента с иконкой (сегментированный контрол).
pub fn tool_toggle_icon(
    ui: &mut egui::Ui,
    icon_name: &str,
    selected: bool,
    text: impl Into<String>,
) -> egui::Response {
    let text = text.into();
    let img_size = 13.0;
    let pad_x = 8.0;
    let gap = 4.0;
    let height = 24.0;

    let galley = ui.painter().layout_no_wrap(
        text.clone(),
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );
    let content_w = img_size + gap + galley.size().x;

    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(content_w + pad_x * 2.0, height),
        Sense::click(),
    );
    if !ui.is_rect_visible(rect) {
        return resp;
    }

    let (bg, stroke, fg) = if selected {
        (
            p::ACCENT.gamma_multiply(0.22),
            Stroke::new(1.0, p::ACCENT),
            p::TEXT,
        )
    } else {
        (Color32::TRANSPARENT, Stroke::NONE, p::TEXT_DIM)
    };

    ui.painter().rect(rect, 6.0, bg, stroke);

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.min.x + pad_x + img_size * 0.5, rect.center().y),
        egui::vec2(img_size, img_size),
    );
    if let Some(tex) = icons::texture(ui.ctx(), icon_name, img_size, fg) {
        egui::Image::new(&tex)
            .fit_to_exact_size(egui::vec2(img_size, img_size))
            .paint_at(ui, icon_rect);
    }

    let text_pos = egui::pos2(
        rect.min.x + pad_x + img_size + gap,
        rect.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(text_pos, galley, fg);

    resp
}

/// Квадратная кнопка-иконка **без текста**, с рамкой и tooltip.
/// Используется в панели инструментов — названия живут только в hover-подсказке.
pub fn icon_toggle_square(
    ui: &mut egui::Ui,
    icon_name: &str,
    selected: bool,
    hint: &str,
) -> egui::Response {
    let size = 34.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), Sense::click());

    let (bg, stroke, fg) = if selected {
        (
            p::ACCENT.gamma_multiply(0.22),
            Stroke::new(1.5, p::ACCENT),
            p::TEXT,
        )
    } else if resp.hovered() {
        (
            p::SURFACE_2,
            Stroke::new(1.0, p::ACCENT.gamma_multiply(0.5)),
            p::TEXT,
        )
    } else {
        (p::SURFACE, Stroke::new(1.0, p::BORDER), p::TEXT_DIM)
    };

    ui.painter().rect(rect, 5.0, bg, stroke);

    let icon_size = 18.0;
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(icon_size, icon_size));
    if let Some(tex) = icons::texture(ui.ctx(), icon_name, icon_size, fg) {
        egui::Image::new(&tex)
            .fit_to_exact_size(egui::vec2(icon_size, icon_size))
            .paint_at(ui, icon_rect);
    }

    if !hint.is_empty() {
        resp.on_hover_text(hint)
    } else {
        resp
    }
}

/// Бейдж с иконкой (используется, где раньше вставляли эмодзи).
pub fn badge_icon(ui: &mut egui::Ui, icon_name: &str, text: impl Into<String>, color: Color32) {
    egui::Frame::none()
        .fill(color.gamma_multiply(0.15))
        .rounding(4.0)
        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icon(ui, icon_name, 11.0, color);
                ui.label(RichText::new(text).size(11.0).color(color));
            });
        });
}

/// Бейдж, где раньше стоял один эмодзи (без текста).
pub fn badge_only_icon(ui: &mut egui::Ui, icon_name: &str, color: Color32) {
    egui::Frame::none()
        .fill(color.gamma_multiply(0.15))
        .rounding(4.0)
        .inner_margin(egui::Margin::symmetric(5.0, 2.0))
        .show(ui, |ui| {
            icon(ui, icon_name, 12.0, color);
        });
}

pub fn empty_state_icon(ui: &mut egui::Ui, icon_name: &str, text: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(16.0);
        icon(ui, icon_name, 24.0, p::TEXT_MUTED);
        ui.add_space(4.0);
        ui.label(RichText::new(text).size(12.0).color(p::TEXT_MUTED));
        ui.add_space(16.0);
    });
}