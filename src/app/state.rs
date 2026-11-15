//! Resource-структуры и Component-маркеры редактора.
//! Здесь же — общие UI-enums (`ViewType`, `Axis`, `EditorStage`, `EditorTool`).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::egui;

use crate::constants::ROOT_FRAME_ID;
use crate::model::{Issue, ProjectDoc};
use crate::format::photo::{ApplyMode, FitMode};
use crate::rhai::default_zone_script;
use crate::render::textures::ColorParams;

// ============================================================================
// Компоненты / маркеры
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub enum ViewType {
    Front,
    Top,
    Side,
    Perspective,
}

#[derive(Component)]
pub struct EditorEntity;

#[derive(Component)]
pub struct VoxelChunk;

#[derive(Component)]
pub struct ScriptZoneVisual {
    pub zone_id: u64,
}

#[derive(Component)]
pub struct ZoneWireframe;

/// Маркер призрачной копии модуля, отрисованной в собранной сцене
/// (`sync_placements_system`). Полезен для отладки и для точечного
/// despawn из cleanup-системы.
#[derive(Component)]
pub struct PlacedModuleVisual {
    pub frame_id: u32,
}

// ============================================================================
// UI-enums
// ============================================================================

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource, Default)]
pub enum EditorStage {
    #[default]
    Stage1_Voxels,
    Stage2_Modules,
    Stage3_Scripts,
    Stage4_Sockets,
    Stage5_Export,
    Stage6_Smooth,
    Stage7_Textures,
}

impl EditorStage {
    pub fn name(&self) -> &'static str {
        match self {
            EditorStage::Stage1_Voxels   => "Воксели",
            EditorStage::Stage2_Modules  => "Модули",
            EditorStage::Stage3_Scripts  => "Скрипты",
            EditorStage::Stage4_Sockets  => "Слоты",
            EditorStage::Stage5_Export   => "Экспорт",
            EditorStage::Stage6_Smooth   => "Сглаживание",
            EditorStage::Stage7_Textures => "Текстуры",
        }
    }

    /// Логическая цепочка совпадает с порядком вкладок:
    /// Voxels → (Modules) → Sockets → Scripts → Smooth → Textures → Export
    pub fn next(&self) -> Self {
        match self {
            EditorStage::Stage1_Voxels   => EditorStage::Stage2_Modules,
            EditorStage::Stage2_Modules  => EditorStage::Stage4_Sockets,
            EditorStage::Stage4_Sockets  => EditorStage::Stage3_Scripts,
            EditorStage::Stage3_Scripts  => EditorStage::Stage6_Smooth,
            EditorStage::Stage6_Smooth   => EditorStage::Stage7_Textures,
            EditorStage::Stage7_Textures => EditorStage::Stage5_Export,
            EditorStage::Stage5_Export   => EditorStage::Stage5_Export,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            EditorStage::Stage1_Voxels   => EditorStage::Stage1_Voxels,
            EditorStage::Stage2_Modules  => EditorStage::Stage1_Voxels,
            EditorStage::Stage4_Sockets  => EditorStage::Stage2_Modules,
            EditorStage::Stage3_Scripts  => EditorStage::Stage4_Sockets,
            EditorStage::Stage6_Smooth   => EditorStage::Stage3_Scripts,
            EditorStage::Stage7_Textures => EditorStage::Stage6_Smooth,
            EditorStage::Stage5_Export   => EditorStage::Stage7_Textures,
        }
    }
}

/// Камера перспективы. `translation` — текущее «живое» положение для
/// активного фрейма; `per_frame` — последняя позиция для каждого фрейма,
/// чтобы при переключении фрейм возвращался в свой ракурс, а не в дефолт.
#[derive(Resource, Clone)]
pub struct PerspectiveCamState {
    pub translation: Vec3,
    pub per_frame: HashMap<u32, Vec3>,
}

impl Default for PerspectiveCamState {
    fn default() -> Self {
        Self {
            translation: Vec3::new(80.0, 80.0, 80.0),
            per_frame: HashMap::new(),
        }
    }
}

impl PerspectiveCamState {
    /// Дефолтное положение камеры для фрейма размера `size`.
    pub fn default_for_size(size: u32) -> Vec3 {
        Vec3::splat(size as f32 * 0.625)
    }

    /// Восстановить (или посчитать) положение камеры для `frame_id`.
    pub fn camera_for(&self, frame_id: u32, frame_size: u32) -> Vec3 {
        self.per_frame
            .get(&frame_id)
            .copied()
            .unwrap_or_else(|| Self::default_for_size(frame_size))
    }

    pub fn save_for(&mut self, frame_id: u32, translation: Vec3) {
        self.per_frame.insert(frame_id, translation);
        self.translation = translation;
    }
}

// ============================================================================
// Инструменты редактирования
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource, Default)]
pub enum EditorTool {
    #[default]
    Brush,
    Line,
    /// Прямоугольник. Shift — идеальный квадрат.
    RectFill,
    /// Эллипс. Shift — идеальный круг.
    Circle,
    FloodFill,
    NoiseBrush,
    SmoothBrush,
}

impl EditorTool {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Brush => "✏",
            Self::Line => "╱",
            Self::RectFill => "▭",
            Self::Circle => "◯",
            Self::FloodFill => "🪣",
            Self::NoiseBrush => "💥",
            Self::SmoothBrush => "✨",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Brush => "Кисть",
            Self::Line => "Линия",
            Self::RectFill => "Прямоугольник",
            Self::Circle => "Круг / эллипс",
            Self::FloodFill => "Заливка",
            Self::NoiseBrush => "Шум",
            Self::SmoothBrush => "Сглаживание",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Brush => "ЛКМ — рисовать · ПКМ — стирать",
            Self::Line => "Линия. ЛКМ — рисовать, ПКМ — стереть той же формой",
            Self::RectFill => "Прямоугольник. ЛКМ — рисовать, ПКМ — стереть. Shift — идеальный квадрат",
            Self::Circle => "Эллипс. ЛКМ — рисовать, ПКМ — стереть. Shift — идеальный круг",
            Self::FloodFill => "Залить (ЛКМ) / стереть (ПКМ) связную область того же цвета",
            Self::NoiseBrush => "Шум (ЛКМ) / стирание шумом (ПКМ) с той же плотностью",
            Self::SmoothBrush => "Медианное сглаживание 3×3 (ЛКМ) · ПКМ — стирание",
        }
    }

    pub fn is_brush_like(self) -> bool {
        matches!(self, Self::Brush | Self::NoiseBrush | Self::SmoothBrush)
    }

    pub fn is_drag_shape(self) -> bool {
        matches!(self, Self::Line | Self::RectFill | Self::Circle)
    }
}

/// Глобальные настройки инструментов (общие для всех вью).
#[derive(Resource)]
pub struct ToolSettings {
    pub show_panel: bool,
    pub brush_size: u32,
    pub symmetry_x: bool,
    pub symmetry_y: bool,
    pub noise_density: f32,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            show_panel: true,
            brush_size: 3,
            symmetry_x: false,
            symmetry_y: false,
            noise_density: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub fn to_u8(self) -> u8 {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Axis::X,
            1 => Axis::Y,
            _ => Axis::Z,
        }
    }
}

// ============================================================================
// Ресурсы: активный фрейм / проект
// ============================================================================

#[derive(Resource)]
pub struct ProjectDocResource(pub ProjectDoc);

#[derive(Resource)]
pub struct ActiveFrame {
    pub id: u32,
}

#[derive(Resource)]
pub struct EditorCameras {
    pub front: Entity,
    pub top: Entity,
    pub side: Entity,
    pub perspective: Entity,
}

#[derive(Resource, Default)]
pub struct ChunkEntityMap {
    pub entities: Vec<Handle<Mesh>>,
}

/// Хэндлы мешей воксельных чанков. Индекс — линейный индекс чанка
/// в `VoxelGrid` (cz * chunks² + cy * chunks + cx).
#[derive(Resource, Default)]
pub struct ChunkMeshMap {
    pub meshes: Vec<Handle<Mesh>>,
}

/// Состояние визуализации размещённых модулей.
#[derive(Resource)]
pub struct PlacementVisualState {
    pub last_signature: u64,
    pub entities: Vec<Entity>,
    /// Триггер принудительной перестройки из UI (кнопка «Визуализировать»).
    pub force_refresh: bool,
    /// Показывать панель диагностики в UI.
    pub show_debug: bool,
    /// Строки для панели диагностики, заполняются sync-системой.
    pub debug_lines: Vec<String>,
    /// Показывать размещённые модули в сцене.
    pub show_placements: bool,
}

impl Default for PlacementVisualState {
    fn default() -> Self {
        Self {
            last_signature: 0,
            entities: Vec::new(),
            force_refresh: false,
            show_debug: false,
            debug_lines: Vec::new(),
            show_placements: true,
        }
    }
}

// ============================================================================
// Ресурсы: ожидания / очереди переходов
// ============================================================================

#[derive(Resource, Default)]
pub struct PendingGridResize {
    pub new_size: Option<u32>,
    pub confirm_dialog_open: bool,
}

#[derive(Resource, Default)]
pub struct PendingDocImport {
    pub bytes: Option<Vec<u8>>,
}

#[derive(Resource, Default)]
pub struct PendingFrameSwitch {
    pub id: Option<u32>,
}

// ============================================================================
// Ресурсы: состояние UI стадий
// ============================================================================

#[derive(Resource, Default)]
pub struct RectSelectionState {
    pub is_active: bool,
    pub start_pos: Option<egui::Pos2>,
    pub current_pos: Option<egui::Pos2>,
    pub start_pixel: Option<(u32, u32)>,
    pub current_pixel: Option<(u32, u32)>,
    pub view_type: Option<ViewType>,
    pub erase_mode: bool,   // ← NEW: захвачен в момент drag_started
}

#[derive(Resource, Default)]
pub struct FullscreenState {
    pub active: Option<ViewType>,
}

#[derive(Resource, Default)]
pub struct SocketPlacementState {
    pub placing: bool,
}

#[derive(Resource)]
pub struct ModuleEditorState {
    pub selected_frame: Option<u32>,
    pub selected_socket: Option<u32>,
    pub new_module_name: String,
    pub new_module_tag: String,
    pub new_module_size: u32,
    pub new_socket_name: String,
    pub new_socket_tag: String,
    pub option_module: Option<u32>,
}

impl Default for ModuleEditorState {
    fn default() -> Self {
        Self {
            selected_frame: Some(ROOT_FRAME_ID),
            selected_socket: None,
            new_module_name: "module".to_string(),
            new_module_tag: "custom".to_string(),
            new_module_size: 64,
            new_socket_name: "socket".to_string(),
            new_socket_tag: String::new(),
            option_module: None,
        }
    }
}

#[derive(Resource, Default)]
pub struct ScriptUiState {
    pub frame_tab: bool,
}

#[derive(Resource)]
pub struct ExportUiState {
    pub tags_text: String,
    pub objects_root_override: String,
    pub status: Option<String>,
    pub issues: Vec<Issue>,
}

impl Default for ExportUiState {
    fn default() -> Self {
        Self {
            tags_text: "mawt,editor".to_string(),
            objects_root_override: String::new(),
            status: None,
            issues: Vec::new(),
        }
    }
}

#[derive(Resource, Default)]
pub struct PhotoImportState {
    pub open: bool,
    pub target: Option<ViewType>,
    pub source_gray: Vec<u8>,
    pub src_w: usize,
    pub src_h: usize,
    pub rotate: u8,
    pub flip_h: bool,
    pub flip_v: bool,
    pub invert: bool,
    pub threshold: u8,
    pub auto_threshold: bool,
    pub fit: FitMode,
    pub apply_mode: ApplyMode,
    pub preview_tex: Option<egui::TextureHandle>,
    pub status: Option<String>,
    pub pending_dialog_request: Option<ViewType>,
}

// ============================================================================
// Ресурсы: зоны
// ============================================================================

/// Чистые данные зоны (для UI и сериализации).
#[derive(Clone, Debug)]
pub struct ScriptZoneData {
    pub id: u64,
    pub name: String,
    pub axis: Axis,
    pub min: UVec3,
    pub max: UVec3,
    pub color_idx: usize,
    pub script: String,
}

#[derive(Resource, Default)]
pub struct ScriptZonesStore {
    pub zones: Vec<ScriptZoneData>,
    pub selected_zone_id: Option<u64>,
    pub next_id: u64,
}

impl ScriptZonesStore {
    pub fn get_zone_mut(&mut self, id: u64) -> Option<&mut ScriptZoneData> {
        self.zones.iter_mut().find(|z| z.id == id)
    }

    pub fn add_zone(&mut self, min: UVec3, max: UVec3, axis: Axis, grid_size: u32) -> Option<u64> {
        if min.x >= grid_size || min.y >= grid_size || min.z >= grid_size {
            return None;
        }
        if max.x >= grid_size || max.y >= grid_size || max.z >= grid_size {
            return None;
        }

        let id = self.next_id;
        self.next_id += 1;

        let color_idx =
            (id as usize) % crate::constants::ZONE_COLORS_F32.len();
        let name = format!("zone_{}", id);

        self.zones.push(ScriptZoneData {
            id,
            name,
            axis,
            min,
            max,
            color_idx,
            script: default_zone_script(color_idx),
        });

        self.selected_zone_id = Some(id);
        Some(id)
    }

    pub fn remove_zone(&mut self, id: u64) {
        self.zones.retain(|z| z.id != id);
        if self.selected_zone_id == Some(id) {
            self.selected_zone_id = None;
        }
    }
}

/// Кэши мешей/материалов зон.
#[derive(Resource, Default)]
pub struct ZoneCreationState {
    pub is_drawing: bool,
    pub start_voxel: Option<UVec3>,
    pub current_hover_voxel: Option<UVec3>,
    pub material_cache: HashMap<u64, Handle<StandardMaterial>>,
    pub mesh_cache: HashMap<u64, (Vec3, Handle<Mesh>)>,
    pub wireframe_cache: HashMap<u64, (Vec3, Handle<Mesh>)>,
}

// ============================================================================
// Stage 6: сглаживание
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SmoothAlgorithm {
    Laplacian,
    #[default]
    Median,
    Gaussian,
    Taubin,
}

impl SmoothAlgorithm {
    pub fn label(self) -> &'static str {
        match self {
            Self::Laplacian => "Laplacian",
            Self::Median    => "Median (удаление шума)",
            Self::Gaussian  => "Gaussian",
            Self::Taubin    => "Taubin (без сжатия)",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Laplacian => "Классическое сглаживание: каждый воксель стремится к среднему соседей.",
            Self::Median    => "Убирает изолированные воксели и заполняет дыры по принципу большинства.",
            Self::Gaussian  => "Взвешенное сглаживание с гауссовым ядром по окрестности 3×3×3.",
            Self::Taubin    => "Laplacian с антисжатием: не «сдувает» объём модели.",
        }
    }

    /// Реализовано ли уже в этой версии.
    pub fn implemented(self) -> bool {
        true
    }
}

#[derive(Resource)]
pub struct SmoothUiState {
    pub algorithm: SmoothAlgorithm,
    pub iterations: u32,
    pub strength: f32,
    pub status: Option<String>,
}

impl Default for SmoothUiState {
    fn default() -> Self {
        Self {
            algorithm: SmoothAlgorithm::default(),
            iterations: 1,
            strength: 0.5,
            status: None,
        }
    }
}

// ============================================================================
// Stage 7: текстуры
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextureMode {
    #[default]
    RainbowHeight,
    Solid,
    HeightGradient,
    Checker,
    Noise,
}

impl TextureMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::RainbowHeight => "Радуга по высоте",
            Self::Solid         => "Однотонный",
            Self::HeightGradient=> "Градиент по высоте",
            Self::Checker       => "Шахматный",
            Self::Noise         => "Шум",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::RainbowHeight => "Цвет зависит от Y: снизу красный — сверху фиолетовый.",
            Self::Solid         => "Один цвет на всю модель.",
            Self::HeightGradient=> "Плавный переход между двумя цветами по высоте.",
            Self::Checker       => "Шахматный паттерн в плоскости XY.",
            Self::Noise         => "Процедурный шум — переход между двумя цветами.",
        }
    }
}

#[derive(Resource)]
pub struct TextureUiState {
    pub mode: TextureMode,
    pub primary_color: [f32; 3],
    pub secondary_color: [f32; 3],
    pub noise_scale: f32,
    pub noise_seed: u64,
    pub status: Option<String>,
    pub preview_tex: Option<egui::TextureHandle>,
    pub preview_dirty: bool,
    /// Какая схема применена к воксельному мешу.
    /// `None` — дефолтный rainbow (совместимость с прежним поведением).
    /// Меняется только по кнопке «Применить»; `update_voxel_mesh_system`
    /// замечает разницу и пересобирает чанки.
    pub applied_mode: Option<TextureMode>,
}

impl Default for TextureUiState {
    fn default() -> Self {
        Self {
            mode: TextureMode::default(),
            primary_color: [0.55, 0.28, 0.85],
            secondary_color: [0.95, 0.72, 0.20],
            noise_scale: 4.0,
            noise_seed: 42,
            status: None,
            preview_tex: None,
            preview_dirty: true,
            applied_mode: None,
        }
    }
}

impl TextureUiState {
    pub fn color_params(&self) -> ColorParams {
        ColorParams {
            primary: self.primary_color,
            secondary: self.secondary_color,
            noise_scale: self.noise_scale,
            noise_seed: self.noise_seed,
        }
    }
}