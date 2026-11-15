//! egui-виджет для редактирования Rhai с подсветкой синтаксиса.

use bevy_egui::egui;

use super::tokenizer::{tokenize_rhai, Tok};

/// Цветовая схема подсветки. По умолчанию — тёмная.
#[derive(Clone, Copy, Debug)]
pub struct RhaiTheme {
    pub text: egui::Color32,
    pub keyword: egui::Color32,
    pub string: egui::Color32,
    pub number: egui::Color32,
    pub comment: egui::Color32,
    pub call: egui::Color32,
    pub const_value: egui::Color32,
}

impl Default for RhaiTheme {
    fn default() -> Self {
        Self {
            text: egui::Color32::from_rgb(220, 220, 220),
            keyword: egui::Color32::from_rgb(197, 134, 255),
            string: egui::Color32::from_rgb(140, 220, 120),
            number: egui::Color32::from_rgb(255, 200, 120),
            comment: egui::Color32::from_rgb(110, 110, 110),
            call: egui::Color32::from_rgb(120, 180, 255),
            const_value: egui::Color32::from_rgb(255, 140, 180),
        }
    }
}

impl RhaiTheme {
    fn color_for(&self, tok: Tok) -> egui::Color32 {
        match tok {
            Tok::Keyword => self.keyword,
            Tok::Str => self.string,
            Tok::Number => self.number,
            Tok::Comment => self.comment,
            Tok::Call => self.call,
            Tok::Const => self.const_value,
        }
    }
}

/// Редактор Rhai с подсветкой. `desired_rows` — минимальная высота в строках.
pub fn rhai_edit(ui: &mut egui::Ui, text: &mut String, desired_rows: usize) {
    rhai_edit_with_theme(ui, text, desired_rows, RhaiTheme::default());
}

/// Как [`rhai_edit`], но с кастомной цветовой схемой.
pub fn rhai_edit_with_theme(
    ui: &mut egui::Ui,
    text: &mut String,
    desired_rows: usize,
    theme: RhaiTheme,
) {
    let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
        // Уважаем TextStyle::Monospace из темы egui.
        let font_size = ui
            .style()
            .text_styles
            .get(&egui::TextStyle::Monospace)
            .map(|f| f.size)
            .unwrap_or(13.0);
        let mono = egui::FontId::monospace(font_size);

        let default_format = egui::TextFormat {
            font_id: mono.clone(),
            color: theme.text,
            ..Default::default()
        };

        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = wrap_width;

        let mut pos = 0usize;
        for (a, b, tok) in tokenize_rhai(string) {
            if a > pos {
                job.append(&string[pos..a], 0.0, default_format.clone());
            }
            let fmt = egui::TextFormat {
                font_id: mono.clone(),
                color: theme.color_for(tok),
                ..Default::default()
            };
            job.append(&string[a..b], 0.0, fmt);
            pos = b;
        }
        if pos < string.len() {
            job.append(&string[pos..], 0.0, default_format);
        }

        ui.fonts(|f| f.layout_job(job))
    };

    ui.add(
        egui::TextEdit::multiline(text)
            .code_editor()
            .desired_rows(desired_rows)
            .desired_width(f32::INFINITY)
            .layouter(&mut layouter),
    );
}