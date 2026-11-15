//! Шаблоны Rhai-скриптов по умолчанию. Без зависимостей от egui/bevy.

/// Стартовый скрипт модуля по его типу.
pub fn module_script_template(tag: &str, name: &str) -> String {
    let head = format!(
        "// Модуль «{}» (тип: {})\n\
         // Жизненный цикл: on_attach (установка), on_detach (снятие),\n\
         //                 on_update(dt) (каждый кадр).\n\
         // API: add_stat / set_stat / mul_stat / get_stat,\n\
         //      log, has_module, module_name, socket_name\n\n",
        name,
        if tag.is_empty() { "—" } else { tag }
    );

    let body = match tag {
        "magazine" => {
            "fn on_attach() {\n    set_stat(\"capacity\", 30);\n    add_stat(\"weight\", 0.4);\n}\n\nfn on_detach() {\n    // снять эффекты при снятии магазина\n}\n"
        }
        "weapon_mod" | "stock" | "barrel" | "sight" => {
            "fn on_attach() {\n    add_stat(\"weight\", 0.2);\n    add_stat(\"ergonomics\", -1.0);\n}\n\nfn on_detach() {\n    // снять эффекты\n}\n"
        }
        "turret" | "gun" => {
            "fn on_attach() {\n    set_stat(\"rotate_speed\", 90.0);\n}\n\nfn on_detach() {\n    // остановить вращение\n}\n\nfn on_update(dt) {\n    // логика наведения здесь\n}\n"
        }
        "wheel" | "tracks" => {
            "fn on_attach() {\n    add_stat(\"speed\", 10.0);\n    add_stat(\"traction\", 1.0);\n}\n\nfn on_detach() {\n    // снять эффекты\n}\n"
        }
        "armor" => {
            "fn on_attach() {\n    add_stat(\"protection\", 5.0);\n    add_stat(\"weight\", 0.8);\n}\n\nfn on_detach() {\n    // снять эффекты\n}\n"
        }
        "root" => {
            "// Скрипт корпуса: общие значения и проверки сборки.\nfn on_attach() {\n    set_stat(\"weight\", 1.0);\n    if has_module(\"magazine\") {\n        log(\"магазин установлен\");\n    }\n}\n\nfn on_update(dt) {\n    // общая логика корпуса\n}\n"
        }
        _ => {
            "fn on_attach() {\n    log(\"attach: \" + module_name());\n}\n\nfn on_detach() {\n    // снять эффекты\n}\n"
        }
    };

    format!("{}{}", head, body)
}

/// Стартовый скрипт зоны по её цветовому индексу.
pub fn default_zone_script(color_idx: usize) -> String {
    let hint = match color_idx % 8 {
        0 => "Турель / точка крепления",
        1 => "Гусеницы / колёса",
        2 => "Двигатель / источник энергии",
        3 => "Оружие / эмиттер",
        4 => "Триггер / сенсор",
        5 => "Точка спавна",
        6 => "Хитбокс / коллизия",
        _ => "Произвольный скрипт зоны",
    };

    format!(
        "// {}\nfn on_enter() {{\n    log(\"zone enter\");\n}}\n\nfn on_exit() {{\n    // очистка\n}}\n",
        hint
    )
}