//! Пути, имена файлов, генерация `*.object.rhai` и планирование записи проекта.
//!
//! Ре-экспорт: [`sanitize_id`] транслитерирует кириллицу и оставляет только `[a-z0-9_]`.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::format::write_mpp;
use crate::model::ProjectDoc;

// ============================================================================
// Транслитерация и ID
// ============================================================================

fn translit_char(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a", 'б' => "b", 'в' => "v", 'г' => "g", 'д' => "d",
        'е' => "e", 'ё' => "e", 'ж' => "zh", 'з' => "z", 'и' => "i",
        'й' => "y", 'к' => "k", 'л' => "l", 'м' => "m", 'н' => "n",
        'о' => "o", 'п' => "p", 'р' => "r", 'с' => "s", 'т' => "t",
        'у' => "u", 'ф' => "f", 'х' => "h", 'ц' => "c", 'ч' => "ch",
        'ш' => "sh", 'щ' => "sch", 'ъ' => "", 'ы' => "y", 'ь' => "",
        'э' => "e", 'ю' => "yu", 'я' => "ya",
        _ => return None,
    })
}

/// «Магазин барабанный» → «magazin_barabannyy». Безопасно для имён файлов и ключей Rhai.
pub fn sanitize_id(s: &str) -> String {
    let mut raw = String::new();

    for c in s.trim().chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            raw.push(c);
        } else if let Some(t) = translit_char(c) {
            raw.push_str(t);
        } else {
            raw.push('_');
        }
    }

    let mut out = String::new();
    for c in raw.chars() {
        if c == '_' && (out.is_empty() || out.ends_with('_')) {
            continue;
        }
        out.push(c);
    }

    while out.ends_with('_') {
        out.pop();
    }

    out
}

// ============================================================================
// Каталоги
// ============================================================================

/// Корень каталога объектов. Пусто → `<ближайшая папка GNAX выше cwd, иначе cwd>/assets/objects`.
pub fn find_objects_root(cwd: &Path, configured: &str) -> PathBuf {
    let cfg = configured.trim();

    if !cfg.is_empty() {
        let p = PathBuf::from(cfg);
        if p.is_absolute() {
            return p;
        }
    }

    let base = cwd
        .ancestors()
        .find(|a| {
            a.file_name()
                .map_or(false, |n| n.to_string_lossy().eq_ignore_ascii_case("GNAX"))
        })
        .unwrap_or(cwd);

    if cfg.is_empty() {
        base.join("assets").join("objects")
    } else {
        base.join(cfg)
    }
}

/// `<root>/<категория>/<подкатегория (можно a/b)>/<id>`
pub fn object_dir(root: &Path, category: &str, sub: &str, id: &str) -> PathBuf {
    let mut p = root.to_path_buf();

    let cat = sanitize_id(category);
    p.push(if cat.is_empty() { "misc".to_string() } else { cat });

    for seg in sub.split(|c| c == '/' || c == '\\') {
        let s = sanitize_id(seg);
        if !s.is_empty() {
            p.push(s);
        }
    }

    p.push(id);
    p
}

// ============================================================================
// Поиск объектов (библиотека)
// ============================================================================

pub struct ObjEntry {
    pub label: String,
    pub path: PathBuf,
}

/// Рекурсивно ищет `*.mpp` в каталоге объектов (для «библиотеки»).
pub fn scan_objects(root: &Path) -> Vec<ObjEntry> {
    let mut out = Vec::new();
    scan_rec(root, root, 0, &mut out);
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

fn scan_rec(root: &Path, dir: &Path, depth: usize, out: &mut Vec<ObjEntry>) {
    if depth > 5 {
        return;
    }

    let Ok(rd) = fs::read_dir(dir) else { return };

    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            scan_rec(root, &p, depth + 1, out);
        } else if p.extension().map_or(false, |x| x.eq_ignore_ascii_case("mpp")) {
            let rel = p.strip_prefix(root).unwrap_or(&p).with_extension("");
            out.push(ObjEntry {
                label: rel.to_string_lossy().replace('\\', "/"),
                path: p,
            });
        }
    }
}

// ============================================================================
// Генерация Rhai
// ============================================================================

pub fn rhai_quote(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');

    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{{{:04x}}}", c as u32)),
            c => o.push(c),
        }
    }

    o.push('"');
    o
}

fn unique_keys<'a>(names: impl Iterator<Item = &'a str>, fallback: &str) -> Vec<String> {
    let mut used = HashSet::new();
    let mut out = Vec::new();

    for n in names {
        let mut base = sanitize_id(n);
        if base.is_empty() {
            base = fallback.to_string();
        }

        let mut k = base.clone();
        let mut i = 2;
        while !used.insert(k.clone()) {
            k = format!("{}_{}", base, i);
            i += 1;
        }

        out.push(k);
    }

    out
}

pub fn frame_keys(doc: &ProjectDoc) -> Vec<String> {
    unique_keys(doc.frames.iter().map(|f| f.name.as_str()), "frame")
}

fn script_rel(key: &str) -> String {
    format!("scripts/{}.rhai", key)
}

fn zone_script_rel(key: &str, idx: usize, zkey: &str) -> String {
    format!("scripts/{}.zone{}_{}.rhai", key, idx, zkey)
}

fn f3_to_str(v: [f32; 3]) -> String {
    format!("[{:.3}, {:.3}, {:.3}]", v[0], v[1], v[2])
}

/// Полный текст `<id>.object.rhai` (декларация структуры + ссылки на скрипты).
pub fn build_object_rhai(doc: &ProjectDoc) -> String {
    let keys = frame_keys(doc);
    debug_assert_eq!(
        keys.len(),
        doc.frames.len(),
        "frame_keys must return one key per frame"
    );

    let sid = sanitize_id(&doc.meta.id);
    let m = &doc.meta;

    let mut s = String::new();
    s.push_str("// Автогенерация Purple Print. Перезаписывается при сохранении из редактора.\n");
    s.push_str("// Скрипты фреймов и зон лежат рядом, в scripts/.\n#{\n");
    s.push_str("    \"format\": 2,\n");
    s.push_str(&format!("    \"id\": {},\n", rhai_quote(&sid)));
    s.push_str(&format!("    \"name\": {},\n", rhai_quote(&m.name)));
    s.push_str(&format!(
        "    \"category\": {},\n",
        rhai_quote(&sanitize_id(&m.category))
    ));
    s.push_str(&format!(
        "    \"subcategory\": {},\n",
        rhai_quote(&m.subcategory.replace('\\', "/"))
    ));
    s.push_str(&format!(
        "    \"description\": {},\n",
        rhai_quote(&m.description)
    ));

    let tags: Vec<String> = m.tags.iter().map(|t| rhai_quote(t)).collect();
    s.push_str(&format!("    \"tags\": [{}],\n", tags.join(", ")));
    s.push_str(&format!("    \"color\": {},\n", f3_to_str(m.base_color)));
    s.push_str(&format!(
        "    \"model\": {},\n",
        rhai_quote(&format!("{}.mpp", sid))
    ));
    s.push_str(&format!("    \"root\": {},\n", rhai_quote(&keys[0])));
    s.push_str("    \"frames\": [\n");

    for (fi, f) in doc.frames.iter().enumerate() {
        let key = &keys[fi];

        s.push_str("        #{\n");
        s.push_str(&format!("            \"key\": {},\n", rhai_quote(key)));
        s.push_str(&format!("            \"name\": {},\n", rhai_quote(&f.name)));
        s.push_str(&format!("            \"tag\": {},\n", rhai_quote(&f.tag)));
        s.push_str(&format!("            \"root\": {},\n", f.is_root));
        s.push_str(&format!("            \"size\": {},\n", f.size));
        s.push_str(&format!(
            "            \"pivot\": {},\n",
            f3_to_str(f.pivot)
        ));
        s.push_str(&format!(
            "            \"script\": {},\n",
            rhai_quote(&script_rel(key))
        ));

        let zkeys = unique_keys(f.zones.iter().map(|z| z.name.as_str()), "zone");
        s.push_str("            \"zones\": [\n");

        for (zi, z) in f.zones.iter().enumerate() {
            let axis = match z.axis {
                0 => "X",
                1 => "Y",
                _ => "Z",
            };

            s.push_str(&format!(
                "                #{{ \"name\": {}, \"axis\": \"{}\", \"min\": [{}, {}, {}], \"max\": [{}, {}, {}], \"script\": {} }},\n",
                rhai_quote(&z.name),
                axis,
                z.min[0], z.min[1], z.min[2],
                z.max[0], z.max[1], z.max[2],
                rhai_quote(&zone_script_rel(key, zi, &zkeys[zi])),
            ));
        }

        s.push_str("            ],\n");

        let skeys = unique_keys(f.sockets.iter().map(|k| k.name.as_str()), "socket");
        s.push_str("            \"sockets\": [\n");

        for (si, sock) in f.sockets.iter().enumerate() {
            s.push_str("                #{\n");
            s.push_str(&format!(
                "                    \"key\": {},\n",
                rhai_quote(&skeys[si])
            ));
            s.push_str(&format!(
                "                    \"name\": {},\n",
                rhai_quote(&sock.name)
            ));
            s.push_str(&format!(
                "                    \"tag\": {},\n",
                rhai_quote(&sock.tag)
            ));
            s.push_str(&format!(
                "                    \"anchor\": {},\n",
                f3_to_str(sock.anchor)
            ));
            s.push_str(&format!(
                "                    \"rot\": [{}, {}, {}],\n",
                sock.rot[0], sock.rot[1], sock.rot[2]
            ));
            s.push_str(&format!(
                "                    \"required\": {},\n",
                sock.required
            ));
            s.push_str(&format!(
                "                    \"default\": {},\n",
                sock.selected
            ));
            match sock.zone_id {
                Some(zid) => {
                    s.push_str(&format!("                    \"zone_id\": {},\n", zid))
                }
                None => s.push_str("                    \"zone_id\": (),\n"),
            }
            s.push_str("                    \"options\": [\n");

            for o in &sock.options {
                let module = if o.frame_id >= 0 {
                    doc.frames
                        .iter()
                        .position(|fr| fr.id == o.frame_id as u32)
                        .map(|i| rhai_quote(&keys[i]))
                        .unwrap_or_else(|| "()".to_string())
                } else {
                    "()".to_string()
                };

                s.push_str(&format!(
                    "                        #{{ \"module\": {}, \"label\": {} }},\n",
                    module,
                    rhai_quote(&o.label)
                ));
            }

            s.push_str("                    ],\n");
            s.push_str("                },\n");
        }

        s.push_str("            ],\n");
        s.push_str("        },\n");
    }

    s.push_str("    ],\n}\n");
    s
}

// ============================================================================
// Планирование записи
// ============================================================================

pub struct PlannedFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// Собирает список файлов для записи (ничего не пишет на диск).
pub fn plan_project_files(
    doc: &ProjectDoc,
    objects_root: &Path,
) -> Result<Vec<PlannedFile>, String> {
    let id = sanitize_id(&doc.meta.id);
    if id.is_empty() {
        return Err("ID объекта пуст".into());
    }

    let dir = object_dir(
        objects_root,
        &doc.meta.category,
        &doc.meta.subcategory,
        &id,
    );
    let keys = frame_keys(doc);
    debug_assert_eq!(
        keys.len(),
        doc.frames.len(),
        "frame_keys must return one key per frame"
    );

    let mut files = Vec::new();

    files.push(PlannedFile {
        path: dir.join(format!("{}.mpp", id)),
        bytes: write_mpp(doc)?,
    });

    files.push(PlannedFile {
        path: dir.join(format!("{}.object.rhai", id)),
        bytes: build_object_rhai(doc).into_bytes(),
    });

    for (f, key) in doc.frames.iter().zip(keys.iter()) {
        files.push(PlannedFile {
            path: dir.join(script_rel(key)),
            bytes: f.script.clone().into_bytes(),
        });

        let zkeys = unique_keys(f.zones.iter().map(|z| z.name.as_str()), "zone");
        for (zi, z) in f.zones.iter().enumerate() {
            files.push(PlannedFile {
                path: dir.join(zone_script_rel(key, zi, &zkeys[zi])),
                bytes: z.script.clone().into_bytes(),
            });
        }
    }

    Ok(files)
}

pub fn write_planned(files: &[PlannedFile]) -> io::Result<()> {
    for f in files {
        if let Some(parent) = f.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&f.path, &f.bytes)?;
    }
    Ok(())
}