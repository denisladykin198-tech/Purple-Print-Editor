//! Модель проекта MPP: фреймы, слоты, зоны, метаданные, валидация, размещения.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::constants::{
    ALLOWED_GRID_SIZES, MAX_DEPTH, MAX_PLACEMENTS, NO_FRAME, ROOT_FRAME_ID,
};
use crate::format::paths::sanitize_id;
use crate::rhai::templates::module_script_template;

// ============================================================================
// Зоны / слоты
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct ZoneRec {
    pub id: u64,
    pub name: String,
    /// 0 = X, 1 = Y, 2 = Z
    pub axis: u8,
    pub min: [u32; 3],
    pub max: [u32; 3],
    pub color_idx: u32,
    pub script: String,
}

/// Один вариант содержимого слота. `frame_id == NO_FRAME` → «без модуля».
#[derive(Clone, Debug, PartialEq)]
pub struct SocketOption {
    pub frame_id: i32,
    pub label: String,
}

/// Слот (точка крепления) на фрейме-родителе.
/// Модуль крепится к скриптовой зоне (`zone_id`) или к свободному `anchor`.
#[derive(Clone, Debug, PartialEq)]
pub struct Socket {
    pub id: u32,
    pub name: String,
    /// Фильтр допустимых типов модулей, через запятую. Пусто = любые.
    pub tag: String,
    /// Точка крепления в сетке родителя (воксельные координаты).
    /// Если задан `zone_id`, обычно совпадает с центром зоны.
    pub anchor: [f32; 3],
    /// Повороты модуля вокруг X, Y, Z в четвертях оборота (0..=3).
    pub rot: [u8; 3],
    pub required: bool,
    pub options: Vec<SocketOption>,
    /// Какой вариант выбран по умолчанию (он же показан в превью).
    pub selected: usize,
    /// Скриптовая зона, к которой привязан слот (модуль = интерактивный
    /// / подвижный фрагмент зоны). `None` — свободная точка без зоны.
    pub zone_id: Option<u64>,
}

impl Socket {
    pub fn new(id: u32, name: &str, tag: &str, anchor: [f32; 3]) -> Self {
        Self {
            id,
            name: name.to_string(),
            tag: tag.to_string(),
            anchor,
            rot: [0; 3],
            required: false,
            options: vec![SocketOption {
                frame_id: NO_FRAME,
                label: "Без модуля".to_string(),
            }],
            selected: 0,
            zone_id: None,
        }
    }

    pub fn selected_frame(&self) -> Option<u32> {
        self.options.get(self.selected).and_then(|o| {
            if o.frame_id >= 0 {
                Some(o.frame_id as u32)
            } else {
                None
            }
        })
    }

    /// Центр зоны → anchor (воксельные координаты, центр ячейки).
    pub fn snap_anchor_to_zone(&mut self, zone: &ZoneRec) {
        self.zone_id = Some(zone.id);
        self.anchor = [
            (zone.min[0] as f32 + zone.max[0] as f32 + 1.0) * 0.5,
            (zone.min[1] as f32 + zone.max[1] as f32 + 1.0) * 0.5,
            (zone.min[2] as f32 + zone.max[2] as f32 + 1.0) * 0.5,
        ];
    }
}

// ============================================================================
// Фреймы
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    NegX,
    PosX,
    NegY,
    PosY,
    NegZ,
    PosZ,
    Center,
}

/// Фрейм = отдельная воксельная модель (корпус или модуль).
#[derive(Clone, Debug)]
pub struct Frame {
    pub id: u32,
    pub name: String,
    pub tag: String,
    pub is_root: bool,
    pub size: u32,
    /// Точка самого модуля, которая совмещается с точкой слота.
    pub pivot: [f32; 3],
    pub voxels: Vec<u8>,
    pub front: Vec<u8>,
    pub top: Vec<u8>,
    pub side: Vec<u8>,
    pub sockets: Vec<Socket>,
    pub zones: Vec<ZoneRec>,
    pub script: String,
    /// Растёт при изменении вокселей — по нему можно пересобирать меши.
    /// Не сериализуется.
    pub revision: u64,

    /// Кэш `voxel_count()`, инвалидируется при смене `revision`.
    /// `Arc<Mutex<_>>` — чтобы Frame оставался `Clone + Send + Sync`,
    /// а клоны не тянули за собой тяжёлый пересчёт.
    pub(crate) voxel_count_cache: Arc<Mutex<Option<(u64, usize)>>>,
}

impl Frame {
    pub fn new_module(id: u32, name: &str, tag: &str, size: u32) -> Self {
        let n2 = (size as usize) * (size as usize);
        let n3 = n2 * size as usize;
        Self {
            id,
            name: name.to_string(),
            tag: tag.to_string(),
            is_root: false,
            size,
            pivot: [size as f32 * 0.5, 0.0, size as f32 * 0.5],
            voxels: vec![1; n3],
            front: vec![1; n2],
            top: vec![1; n2],
            side: vec![1; n2],
            sockets: Vec::new(),
            zones: Vec::new(),
            script: module_script_template(tag, name),
            revision: 1,
            voxel_count_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new_root(size: u32) -> Self {
        let n2 = (size as usize) * (size as usize);
        let n3 = n2 * size as usize;
        Self {
            id: ROOT_FRAME_ID,
            name: "body".to_string(),
            tag: String::new(),
            is_root: true,
            size,
            pivot: [0.0; 3],
            voxels: vec![1; n3],
            front: vec![1; n2],
            top: vec![1; n2],
            side: vec![1; n2],
            sockets: Vec::new(),
            zones: Vec::new(),
            script: module_script_template("root", "body"),
            revision: 1,
            voxel_count_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn ensure_data(&mut self) {
        let n = self.size as usize;
        let n2 = n * n;
        let n3 = n2 * n;
        if self.voxels.len() != n3 {
            self.voxels = vec![1; n3];
            // Важно: bump revision, иначе кэш останется от старых вокселей.
            self.revision = self.revision.wrapping_add(1);
        }
        if self.front.len() != n2 {
            self.front = vec![1; n2];
        }
        if self.top.len() != n2 {
            self.top = vec![1; n2];
        }
        if self.side.len() != n2 {
            self.side = vec![1; n2];
        }
    }

    pub fn data_ok(&self) -> bool {
        let n = self.size as usize;
        self.voxels.len() == n * n * n
            && self.front.len() == n * n
            && self.top.len() == n * n
            && self.side.len() == n * n
    }

    /// Количество «твёрдых» вокселей. Кэшируется по `revision`.
    ///
    /// Для 128³ это ~2М итераций, поэтому вызывать без кэша из UI-цикла
    /// (например, `props_card` каждый кадр) — верный способ уронить FPS.
    pub fn voxel_count(&self) -> usize {
        let mut cache = self
            .voxel_count_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some((rev, count)) = *cache {
            if rev == self.revision {
                return count;
            }
        }
        let count = self.voxels.iter().filter(|&&v| v == 1).count();
        *cache = Some((self.revision, count));
        count
    }

    #[inline]
    pub fn is_solid(&self, x: u32, y: u32, z: u32) -> bool {
        if x >= self.size || y >= self.size || z >= self.size {
            return false;
        }
        let n = self.size as usize;
        let idx = (z as usize) * n * n + (y as usize) * n + x as usize;
        self.voxels.get(idx).copied().unwrap_or(0) == 1
    }

    /// Ограничивающий бокс заполненных вокселей (min, max включительно).
    pub fn bbox(&self) -> Option<([u32; 3], [u32; 3])> {
        let n = self.size as usize;
        if self.voxels.len() != n * n * n {
            return None;
        }
        let mut mn = [u32::MAX; 3];
        let mut mx = [0u32; 3];
        let mut any = false;

        for z in 0..n {
            for y in 0..n {
                let row = (z * n + y) * n;
                for x in 0..n {
                    if self.voxels[row + x] == 1 {
                        any = true;
                        let p = [x as u32, y as u32, z as u32];
                        for i in 0..3 {
                            mn[i] = mn[i].min(p[i]);
                            mx[i] = mx[i].max(p[i]);
                        }
                    }
                }
            }
        }

        if any {
            Some((mn, mx))
        } else {
            None
        }
    }

    /// Центр грани ограничивающего бокса (для пресетов pivot).
    pub fn pivot_at(&self, face: Face) -> [f32; 3] {
        let last = self.size.saturating_sub(1);
        let (mn, mx) = self.bbox().unwrap_or(([0; 3], [last; 3]));
        let c = |i: usize| (mn[i] as f32 + mx[i] as f32 + 1.0) * 0.5;
        let mut p = [c(0), c(1), c(2)];

        match face {
            Face::NegX => p[0] = mn[0] as f32,
            Face::PosX => p[0] = mx[0] as f32 + 1.0,
            Face::NegY => p[1] = mn[1] as f32,
            Face::PosY => p[1] = mx[1] as f32 + 1.0,
            Face::NegZ => p[2] = mn[2] as f32,
            Face::PosZ => p[2] = mx[2] as f32 + 1.0,
            Face::Center => {}
        }
        p
    }
}

// ============================================================================
// Метаданные / валидация
// ============================================================================

#[derive(Clone, Debug)]
pub struct ProjectMeta {
    pub id: String,
    pub name: String,
    pub category: String,
    pub subcategory: String,
    pub description: String,
    pub tags: Vec<String>,
    pub base_color: [f32; 3],
}

impl Default for ProjectMeta {
    fn default() -> Self {
        Self {
            id: "my_object".to_string(),
            name: "My Object".to_string(),
            category: "misc".to_string(),
            subcategory: String::new(),
            description: String::new(),
            tags: vec!["mawt".to_string(), "editor".to_string()],
            base_color: [0.6, 0.2, 0.8],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warn,
    Info,
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub severity: Severity,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Placement {
    pub frame_id: u32,
    /// Индекс родителя в том же векторе размещений.
    pub parent: Option<usize>,
    pub socket_id: Option<u32>,
    pub depth: usize,
}

// ============================================================================
// Проект
// ============================================================================

#[derive(Clone, Debug)]
pub struct ProjectDoc {
    pub meta: ProjectMeta,
    /// `frames[0]` — всегда корень.
    pub frames: Vec<Frame>,
    pub next_frame_id: u32,
    pub next_socket_id: u32,
}

/// Подходит ли модуль с тегом `tag` под фильтр слота `filter`
/// («a,b» — любой из списка).
pub fn tag_matches(filter: &str, tag: &str) -> bool {
    let f = filter.trim();
    if f.is_empty() {
        return true;
    }
    f.split(',')
        .any(|t| t.trim().eq_ignore_ascii_case(tag.trim()))
}

impl ProjectDoc {
    pub fn new(root_size: u32) -> Self {
        Self {
            meta: ProjectMeta::default(),
            frames: vec![Frame::new_root(root_size)],
            next_frame_id: 1,
            next_socket_id: 1,
        }
    }

    pub fn root(&self) -> &Frame {
        &self.frames[0]
    }

    pub fn root_mut(&mut self) -> &mut Frame {
        &mut self.frames[0]
    }

    pub fn frame(&self, id: u32) -> Option<&Frame> {
        self.frames.iter().find(|f| f.id == id)
    }

    pub fn frame_mut(&mut self, id: u32) -> Option<&mut Frame> {
        self.frames.iter_mut().find(|f| f.id == id)
    }

    pub fn module_ids(&self) -> Vec<u32> {
        self.frames
            .iter()
            .filter(|f| !f.is_root)
            .map(|f| f.id)
            .collect()
    }

    pub fn add_module(&mut self, name: &str, tag: &str, size: u32) -> u32 {
        let id = self.next_frame_id;
        self.next_frame_id += 1;

        let size = if ALLOWED_GRID_SIZES.contains(&size) {
            size
        } else {
            64
        };
        let name = if name.trim().is_empty() {
            format!("module_{}", id)
        } else {
            name.trim().to_string()
        };

        self.frames
            .push(Frame::new_module(id, &name, tag.trim(), size));
        id
    }

    pub fn duplicate_module(&mut self, id: u32) -> Option<u32> {
        let src = self.frame(id)?.clone();
        if src.is_root {
            return None;
        }

        let mut copy = src;
        copy.id = self.next_frame_id;
        self.next_frame_id += 1;
        copy.name = format!("{}_copy", copy.name);
        copy.revision = 1;
        // Свежий кэш: у копии другой Arc, чтобы не шарить с оригиналом.
        copy.voxel_count_cache = Arc::new(Mutex::new(None));

        for s in copy.sockets.iter_mut() {
            s.id = self.next_socket_id;
            self.next_socket_id += 1;
        }

        let new_id = copy.id;
        self.frames.push(copy);
        Some(new_id)
    }

    /// Удаляет модуль и вычищает его из вариантов всех слотов
    /// (выбор по умолчанию сохраняется).
    pub fn remove_module(&mut self, id: u32) {
        if id == ROOT_FRAME_ID {
            return;
        }

        self.frames.retain(|f| f.id != id);
        let rid = id as i32;

        for f in self.frames.iter_mut() {
            for s in f.sockets.iter_mut() {
                let mut new_sel = 0usize;
                let mut kept = 0usize;

                for (i, o) in s.options.iter().enumerate() {
                    if o.frame_id == rid {
                        continue;
                    }
                    if i == s.selected {
                        new_sel = kept;
                    }
                    kept += 1;
                }

                s.options.retain(|o| o.frame_id != rid);
                if s.options.is_empty() {
                    s.options.push(SocketOption {
                        frame_id: NO_FRAME,
                        label: "Без модуля".to_string(),
                    });
                    new_sel = 0;
                }
                s.selected = new_sel.min(s.options.len() - 1);
            }
        }
    }

    pub fn add_socket(
        &mut self,
        frame_id: u32,
        name: &str,
        tag: &str,
        anchor: [f32; 3],
    ) -> Option<u32> {
        let id = self.next_socket_id;
        let f = self.frame_mut(frame_id)?;
        f.sockets.push(Socket::new(id, name, tag, anchor));
        self.next_socket_id += 1;
        Some(id)
    }

    /// Добавляет модуль в варианты слота и сразу выбирает его —
    /// иначе он не попадёт в `placements()`.
    pub fn attach_module_to_socket(
        &mut self,
        frame_id: u32,
        socket_id: u32,
        module_id: u32,
    ) -> Result<(), String> {
        if module_id == ROOT_FRAME_ID {
            return Err("Корневой фрейм нельзя вложить в слот".into());
        }
        let Some(label) = self.frame(module_id).map(|m| m.name.clone()) else {
            return Err(format!("Модуль #{} не существует", module_id));
        };
        if self.reaches(module_id, frame_id) {
            return Err("Цикл: модуль уже содержит этот фрейм".into());
        }

        let Some(f) = self.frame_mut(frame_id) else {
            return Err(format!("Фрейм #{} не найден", frame_id));
        };
        let Some(s) = f.sockets.iter_mut().find(|s| s.id == socket_id) else {
            return Err(format!("Слот #{} не найден", socket_id));
        };

        let mid = module_id as i32;
        let idx = match s.options.iter().position(|o| o.frame_id == mid) {
            Some(i) => i,
            None => {
                s.options.push(SocketOption {
                    frame_id: mid,
                    label,
                });
                s.options.len() - 1
            }
        };
        s.selected = idx;
        Ok(())
    }

    /// Достижим ли `target` из `from` по вариантам слотов (включая сам `from`).
    fn reaches(&self, from: u32, target: u32) -> bool {
        let mut stack = vec![from];
        let mut seen = HashSet::new();
        while let Some(cur) = stack.pop() {
            if cur == target {
                return true;
            }
            if !seen.insert(cur) {
                continue;
            }
            let Some(f) = self.frame(cur) else { continue };
            for s in &f.sockets {
                for o in &s.options {
                    if o.frame_id >= 0 {
                        stack.push(o.frame_id as u32);
                    }
                }
            }
        }
        false
    }

    /// Есть ли цикл, проходящий через `start`: проверяем, достижим ли `start`
    /// из любого его прямого потомка.
    fn in_cycle(&self, start: u32) -> bool {
        let Some(f) = self.frame(start) else {
            return false;
        };
        for s in &f.sockets {
            for o in &s.options {
                if o.frame_id >= 0 && self.reaches(o.frame_id as u32, start) {
                    return true;
                }
            }
        }
        false
    }

    pub fn usage_count(&self, module_id: u32) -> usize {
        let mid = module_id as i32;
        self.frames
            .iter()
            .flat_map(|f| f.sockets.iter())
            .flat_map(|s| s.options.iter())
            .filter(|o| o.frame_id == mid)
            .count()
    }

    /// Что реально установлено при текущих выбранных вариантах
    /// (родитель всегда раньше потомка).
    pub fn placements(&self) -> Vec<Placement> {
        let mut out = vec![Placement {
            frame_id: self.root().id,
            parent: None,
            socket_id: None,
            depth: 0,
        }];
        let mut chain = vec![self.root().id];
        self.expand(0, &mut out, &mut chain);
        out
    }

    /// Как `placements()`, но начиная с произвольного фрейма.
    /// Возвращаемое дерево имеет `start_id` в качестве корня
    /// (`depth == 0`, `parent == None`).
    pub fn placements_from(&self, start_id: u32) -> Vec<Placement> {
        let mut out = vec![Placement {
            frame_id: start_id,
            parent: None,
            socket_id: None,
            depth: 0,
        }];
        let mut chain = vec![start_id];
        self.expand(0, &mut out, &mut chain);
        out
    }

    fn expand(&self, idx: usize, out: &mut Vec<Placement>, chain: &mut Vec<u32>) {
        debug_assert!(idx < out.len(), "expand: idx out of bounds");
        let fid = out[idx].frame_id;
        let depth = out[idx].depth;
        if depth >= MAX_DEPTH {
            return;
        }

        let Some(frame) = self.frame(fid) else { return };

        for s in &frame.sockets {
            if out.len() >= MAX_PLACEMENTS {
                return;
            }

            let Some(mid) = s.selected_frame() else { continue };
            if chain.contains(&mid) || self.frame(mid).is_none() {
                continue;
            }

            out.push(Placement {
                frame_id: mid,
                parent: Some(idx),
                socket_id: Some(s.id),
                depth: depth + 1,
            });

            let new_idx = out.len() - 1;
            chain.push(mid);
            self.expand(new_idx, out, chain);
            chain.pop();
        }
    }

    pub fn validate(&self) -> Vec<Issue> {
        let mut v = Vec::new();
        let mut push = |severity: Severity, text: String| v.push(Issue { severity, text });

        let sid = sanitize_id(&self.meta.id);
        if sid.is_empty() {
            push(
                Severity::Error,
                "ID объекта пуст — задай ID (кириллица будет транслитерирована).".into(),
            );
        } else if sid != self.meta.id {
            push(Severity::Info, format!("ID будет сохранён как «{}».", sid));
        }

        if self.meta.name.trim().is_empty() {
            push(Severity::Warn, "Не задано имя объекта.".into());
        }

        let mut seen = HashSet::new();
        for f in &self.frames {
            if !seen.insert(sanitize_id(&f.name)) {
                push(
                    Severity::Warn,
                    format!(
                        "Два фрейма с похожим именем «{}» — ключи получат суффикс _2.",
                        f.name
                    ),
                );
            }
        }

        for f in self.frames.iter().filter(|f| !f.is_root) {
            if !f.data_ok() {
                push(
                    Severity::Error,
                    format!("Модуль «{}»: данные повреждены.", f.name),
                );
            } else if f.voxel_count() == 0 {
                push(
                    Severity::Warn,
                    format!("Модуль «{}» пуст (нет вокселей).", f.name),
                );
            }

            if self.usage_count(f.id) == 0 {
                push(
                    Severity::Info,
                    format!("Модуль «{}» не назначен ни в один слот.", f.name),
                );
            }
        }

        for f in &self.frames {
            for s in &f.sockets {
                if s.options.is_empty() {
                    push(
                        Severity::Error,
                        format!("Слот «{}» ({}) без вариантов.", s.name, f.name),
                    );
                }

                for o in &s.options {
                    if o.frame_id >= 0 {
                        match self.frame(o.frame_id as u32) {
                            None => push(
                                Severity::Error,
                                format!("Слот «{}»: модуль #{} не существует.", s.name, o.frame_id),
                            ),
                            Some(m) => {
                                if !tag_matches(&s.tag, &m.tag) {
                                    push(
                                        Severity::Warn,
                                        format!(
                                            "Слот «{}» ждёт [{}], а «{}» имеет тип [{}].",
                                            s.name, s.tag, m.name, m.tag
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }

                if s.required && s.selected_frame().is_none() {
                    push(
                        Severity::Warn,
                        format!("Обязательный слот «{}» пуст по умолчанию.", s.name),
                    );
                }
            }
        }

        for f in &self.frames {
            if self.in_cycle(f.id) {
                push(
                    Severity::Error,
                    format!(
                        "Цикл: «{}» (прямо или через другие модули) вложен сам в себя.",
                        f.name
                    ),
                );
            }
        }

        v
    }
}