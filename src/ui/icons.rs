//! SVG-иконки интерфейса Purple Print.
//!
//! Все иконки — viewBox 0 0 24 24, монохромные. Рендерятся через `svg_engine`
//! (тот же движок, что для игровых SVG-ассетов), кэшируются в `egui::Context`
//! по ключу `(name, px_size, color)`. Цвет вшивается в SVG-строку до парсинга —
//! работает без `Image::tint` на любых версиях egui.

use bevy_egui::egui;

use crate::mawt_ui::assets::svg::svg_engine;

macro_rules! stroke_icon {
    ($name:ident, $($d:expr),+ $(,)?) => {
        pub const $name: &str = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">"##,
            $( concat!(
                r##"<path d=""##, $d,
                r##"" fill="none" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"##
            ), )+
            "</svg>"
        );
    };
}

macro_rules! fill_icon {
    ($name:ident, $($d:expr),+ $(,)?) => {
        pub const $name: &str = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">"##,
            $( concat!(
                r##"<path d=""##, $d, r##"" fill="#FFFFFF" stroke="none"/>"##
            ), )+
            "</svg>"
        );
    };
}

stroke_icon!(BRUSH,
    "M4 20 L4 17 L15 6 L18 9 L7 20 Z",
    "M13 4 L20 11",
);
stroke_icon!(ERASER,
    "M5 20 L10 15",
    "M15 15 L19 19 M19 19 L14 19 L10 15 L15 10 L20 15 L19 19",
);
fill_icon!(SPARKLE,
    "M12 3 L13.5 10.5 L21 12 L13.5 13.5 L12 21 L10.5 13.5 L3 12 L10.5 10.5 Z",
);
stroke_icon!(ARROW_LEFT,
    "M14 6 L8 12 L14 18",
    "M8 12 L20 12",
);
stroke_icon!(EXPAND,
    "M4 9 L4 4 L9 4",
    "M15 4 L20 4 L20 9",
    "M20 15 L20 20 L15 20",
    "M9 20 L4 20 L4 15",
);
stroke_icon!(COMPRESS,
    "M9 4 L9 9 L4 9",
    "M15 4 L15 9 L20 9",
    "M20 15 L15 15 L15 20",
    "M4 15 L9 15 L9 20",
);
stroke_icon!(EYE,
    "M2 12 C2 12 6 4 12 4 C18 4 22 12 22 12 C22 12 18 20 12 20 C6 20 2 12 2 12 Z",
    "M12 8 Q16 8 16 12 Q16 16 12 16 Q8 16 8 12 Q8 8 12 8 Z",
);
stroke_icon!(CAMERA,
    "M4 7 L4 5 L9 5 L11 3 L13 3 L15 5 L20 5 L20 7 L20 19 L4 19 Z",
    "M12 8 Q16 8 16 12 Q16 16 12 16 Q8 16 8 12 Q8 8 12 8 Z",
);
stroke_icon!(GLOBE,
    "M12 3 Q4 3 4 12 Q4 21 12 21 Q20 21 20 12 Q20 3 12 3 Z",
    "M4 12 L20 12",
    "M12 3 C12 3 8 8 8 12 C8 16 12 21 12 21",
    "M12 3 C12 3 16 8 16 12 C16 16 12 21 12 21",
);
stroke_icon!(ROTATE_LEFT,
    "M4 4 L4 10 L10 10",
    "M4 10 Q4 4 12 4 Q20 4 20 12 Q20 20 12 20 Q6 20 4 14",
);
stroke_icon!(ROTATE_RIGHT,
    "M20 4 L20 10 L14 10",
    "M20 10 Q20 4 12 4 Q4 4 4 12 Q4 20 12 20 Q18 20 20 14",
);
stroke_icon!(CHECK,
    "M5 12 L10 17 L19 7",
);
stroke_icon!(X,
    "M6 6 L18 18",
    "M18 6 L6 18",
);
stroke_icon!(WARNING,
    "M12 3 L22 21 L2 21 Z",
    "M12 9 L12 15",
    "M11 17 L13 17 L13 19 L11 19 Z",
);
stroke_icon!(INFO,
    "M12 3 Q4 3 4 12 Q4 21 12 21 Q20 21 20 12 Q20 3 12 3 Z",
    "M12 11 L12 17",
    "M11 7 L13 7 L13 9 L11 9 Z",
);
stroke_icon!(HOME,
    "M3 12 L12 3 L21 12",
    "M5 10 L5 21 L19 21 L19 10",
    "M10 21 L10 15 L14 15 L14 21",
);
stroke_icon!(PACKAGE,
    "M12 3 L21 8 L21 16 L12 21 L3 16 L3 8 Z",
    "M3 8 L12 13 L21 8",
    "M12 13 L12 21",
);
stroke_icon!(PENCIL,
    "M4 20 L4 16 L16 4 L20 8 L8 20 Z",
    "M14 6 L18 10",
);
stroke_icon!(SCROLL,
    "M6 4 L18 4 L18 20 L6 20 Z",
    "M9 8 L15 8",
    "M9 12 L15 12",
    "M9 16 L15 16",
);
stroke_icon!(SOCKET,
    "M8 4 L8 10 Q8 14 12 14 Q16 14 16 10 L16 4",
    "M12 14 L12 20",
    "M8 4 L16 4",
    "M10 20 L14 20",
);
stroke_icon!(COPY,
    "M4 4 L4 14 L14 14",
    "M8 8 L8 20 L20 20 L20 8 L8 8 Z",
);
stroke_icon!(TRASH,
    "M4 6 L20 6",
    "M9 6 L9 4 L15 4 L15 6",
    "M6 6 L6 20 L18 20 L18 6",
    "M10 10 L10 17",
    "M14 10 L14 17",
);
stroke_icon!(PLUS,
    "M12 4 L12 20",
    "M4 12 L20 12",
);
stroke_icon!(TARGET,
    "M12 3 Q4 3 4 12 Q4 21 12 21 Q20 21 20 12 Q20 3 12 3 Z",
    "M12 8 Q8 8 8 12 Q8 16 12 16 Q16 16 16 12 Q16 8 12 8 Z",
    "M11 11 L13 11 L13 13 L11 13 Z",
);
stroke_icon!(BUG,
    "M8 8 Q12 4 16 8",
    "M8 8 L8 16 Q8 20 12 20 Q16 20 16 16 L16 8",
    "M6 12 L8 12",
    "M16 12 L18 12",
    "M6 16 L8 16",
    "M16 16 L18 16",
    "M10 4 L10 8",
    "M14 4 L14 8",
);
stroke_icon!(LINK,
    "M10 14 L14 10",
    "M8 10 L6 12 Q4 14 6 16 Q8 18 10 16 L12 14",
    "M16 14 L18 12 Q20 10 18 8 Q16 6 14 8 L12 10",
);
stroke_icon!(CIRCLE,
    "M12 4 Q5 4 5 12 Q5 20 12 20 Q19 20 19 12 Q19 4 12 4 Z",
);
stroke_icon!(GRID,
    "M4 4 L20 4 L20 20 L4 20 Z",
    "M4 10 L20 10",
    "M4 15 L20 15",
    "M10 4 L10 20",
    "M15 4 L15 20",
);
stroke_icon!(RULER,
    "M2 15 L15 2 L20 7 L7 20 Z",
    "M5 12 L7 14",
    "M8 9 L10 11",
    "M11 6 L13 8",
);
stroke_icon!(IMAGE,
    "M3 4 L21 4 L21 20 L3 20 Z",
    "M3 16 L9 10 L14 15 L17 12 L21 16",
    "M7 7 Q9 7 9 9 Q9 11 7 11 Q5 11 5 9 Q5 7 7 7 Z",
);
stroke_icon!(SAVE,
    "M4 4 L18 4 L20 6 L20 20 L4 20 Z",
    "M8 4 L8 10 L16 10 L16 4",
    "M7 20 L7 14 L17 14 L17 20",
);
stroke_icon!(FOLDER,
    "M3 6 L11 6 L13 8 L21 8 L21 18 L3 18 Z",
    "M3 18 L6 12 L23 12 L20 18 Z",
);
stroke_icon!(SEARCH,
    "M11 4 Q5 4 5 10 Q5 16 11 16 Q17 16 17 10 Q17 4 11 4 Z",
    "M15 15 L21 21",
);
stroke_icon!(POINTING_LEFT,
    "M12 4 Q10 4 10 6 L10 11 L4 11 Q2 11 2 13 Q2 15 4 15 L10 15 L10 18 Q10 20 12 20 L16 20 Q18 20 18 18 L18 10 Q18 8 16 8 L14 8 L14 6 Q14 4 12 4 Z",
);
stroke_icon!(MIRROR,
    "M12 2 L12 22",
    "M4 6 L4 18 L9 12 Z",
    "M20 6 L20 18 L15 12 Z",
);
stroke_icon!(MIRROR_Y,
    "M2 12 L22 12",
    "M6 4 L18 4 L12 9 Z",
    "M6 20 L18 20 L12 15 Z",
);
stroke_icon!(WAND,
    "M4 20 L14 10",
    "M14 10 L18 6",
    "M16 4 L16 6",
    "M18 8 L20 8",
    "M20 12 L22 12",
    "M18 4 L20 6",
);
stroke_icon!(SQUARE,
    "M5 5 L19 5 L19 19 L5 19 Z",
);

stroke_icon!(LINE, "M4 20 L20 4");
stroke_icon!(SQUARE_OUTLINE, "M5 5 L19 5 L19 19 L5 19 Z");
stroke_icon!(RECT_DELETE,
    "M5 5 L19 5 L19 19 L5 19 Z",
    "M9 9 L15 15",
    "M15 9 L9 15",
);
stroke_icon!(FLOOD_FILL,
    "M4 14 L10 8 L16 14 L10 20 Z",
    "M10 8 L10 4",
    "M18 14 Q20 16 18 18 Q16 16 18 14 Z",
);
stroke_icon!(PICK,
    "M4 20 L10 14",
    "M10 14 Q8 12 10 10 L14 6 Q16 4 18 6 Q20 8 18 10 L14 14 Q12 16 10 14 Z",
);
stroke_icon!(EDIT,
    "M4 20 L4 16 L16 4 L20 8 L8 20 Z",
);
stroke_icon!(CHEVRON_RIGHT, "M9 6 L15 12 L9 18");
stroke_icon!(CHEVRON_LEFT,  "M15 6 L9 12 L15 18");

/// SVG-шаблон иконки по имени (для ручного рендера / отладки).
pub fn get_icon(name: &str) -> Option<&'static str> {
    Some(match name {
        "brush"         => BRUSH,
        "eraser"        => ERASER,
        "sparkle"       => SPARKLE,
        "arrow-left"    => ARROW_LEFT,
        "expand"        => EXPAND,
        "compress"      => COMPRESS,
        "eye"           => EYE,
        "camera"        => CAMERA,
        "globe"         => GLOBE,
        "rotate-left"   => ROTATE_LEFT,
        "rotate-right"  => ROTATE_RIGHT,
        "check"         => CHECK,
        "x"             => X,
        "warning"       => WARNING,
        "info"          => INFO,
        "home"          => HOME,
        "package"       => PACKAGE,
        "pencil"        => PENCIL,
        "scroll"        => SCROLL,
        "socket"        => SOCKET,
        "copy"          => COPY,
        "trash"         => TRASH,
        "plus"          => PLUS,
        "target"        => TARGET,
        "bug"           => BUG,
        "link"          => LINK,
        "circle"        => CIRCLE,
        "grid"          => GRID,
        "ruler"         => RULER,
        "image"         => IMAGE,
        "save"          => SAVE,
        "folder"        => FOLDER,
        "search"        => SEARCH,
        "pointing-left" => POINTING_LEFT,
        "mirror"        => MIRROR,
        "mirror-y"      => MIRROR_Y,
        "wand"          => WAND,
        "square"        => SQUARE,

        "line"           => LINE,
        "square-outline" => SQUARE_OUTLINE,
        "rect-delete"    => RECT_DELETE,
        "flood-fill"     => FLOOD_FILL,
        "pick"           => PICK,
        "edit"           => EDIT,
        "chevron-right"  => CHEVRON_RIGHT,
        "chevron-left"   => CHEVRON_LEFT,

        _ => return None,
    })
}

/// Рендерит иконку в `egui::TextureHandle` (кэшируется по `name + px_size + color`).
///
/// * `logical_size` — размер в логических пикселях; рендер идёт в `logical_size * ppp`.
/// * `color` — целевой цвет (в него перекрашивается `#FFFFFF` до парсинга).
pub fn texture(
    ctx: &egui::Context,
    name: &str,
    logical_size: f32,
    color: egui::Color32,
) -> Option<egui::TextureHandle> {
    // Защита от гигантских текстур (кэш не безразмерный).
    const MAX_PX: u32 = 512;
    let ppp = ctx.pixels_per_point();
    let px_size = ((logical_size * ppp).ceil() as u32).clamp(8, MAX_PX);

    let key = egui::Id::new(("mawt_icon", name, px_size, color.to_array()));
    if let Some(h) = ctx.data_mut(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return Some(h);
    }

    let template = get_icon(name)?;
    let colored = template.replace(
        "#FFFFFF",
        &format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b()),
    );

    let tree = svg_engine::parse(&colored).ok()?;
    let pixmap = svg_engine::render(&tree, px_size, px_size).ok()?;
    let image = pixmap_to_egui(&pixmap);

    let handle = ctx.load_texture(
        format!("mawt_icon_{name}_{px_size}"),
        image,
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| d.insert_temp(key, handle.clone()));
    Some(handle)
}

fn pixmap_to_egui(p: &svg_engine::Pixmap) -> egui::ColorImage {
    let mut pixels = Vec::with_capacity((p.width * p.height) as usize);
    let data = &p.data;
    let mut i = 0;
    while i + 3 < data.len() {
        pixels.push(egui::Color32::from_rgba_unmultiplied(
            data[i],
            data[i + 1],
            data[i + 2],
            data[i + 3],
        ));
        i += 4;
    }
    egui::ColorImage {
        size: [p.width as usize, p.height as usize],
        pixels,
    }
}