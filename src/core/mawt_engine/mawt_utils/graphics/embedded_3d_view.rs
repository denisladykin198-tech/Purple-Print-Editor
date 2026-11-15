use std::sync::atomic::{AtomicU64, Ordering};

use bevy::{
    prelude::*,
    render::{
        camera::{ClearColorConfig, RenderTarget},
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        view::RenderLayers,
    },
    window::PrimaryWindow,
};
use bevy_egui::{egui, EguiContexts};

pub const EMBEDDED_3D_RENDER_LAYER: u8 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
pub struct Embedded3dViewId(pub u64);

static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Resource, Default, Debug)]
pub struct Embedded3dInteractionState {
    pub hovered_view: Option<Embedded3dViewId>,
    pub dragged_view: Option<Embedded3dViewId>,
}

#[derive(Component)]
pub struct Embedded3dCamera {
    pub id: Embedded3dViewId,
    pub texture_handle: Handle<Image>,
    pub size: UVec2,
    pub target_size: Vec2,
    pub scale_factor: f32,
}

pub struct Embedded3dViewConfig {
    pub initial_size: Vec2,
    pub clear_color: Color,
    pub transform: Transform,
    pub projection: Projection,
}

impl Default for Embedded3dViewConfig {
    fn default() -> Self {
        Self {
            initial_size: Vec2::new(300.0, 200.0),
            clear_color: Color::rgba(0.15, 0.15, 0.18, 1.0),
            transform: Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: std::f32::consts::PI / 4.0,
                // Пересчитывается под фактический размер
                // в `sync_embedded_cameras_system`.
                aspect_ratio: 1.0,
                near: 0.1,
                far: 100.0,
            }),
        }
    }
}

pub struct Embedded3dViewPlugin;

impl Plugin for Embedded3dViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Embedded3dInteractionState>()
            .add_systems(Update, sync_embedded_cameras_system);
    }
}

fn sync_embedded_cameras_system(
    mut images: ResMut<Assets<Image>>,
    mut query: Query<(&mut Embedded3dCamera, &mut Projection, &mut Camera)>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = primary_window.get_single() else {
        return;
    };
    let scale_factor = window.scale_factor() as f32;

    for (mut cam, mut projection, mut camera) in query.iter_mut() {
        cam.scale_factor = scale_factor;
        let new_physical_size = UVec2::new(
            (cam.target_size.x * scale_factor).max(1.0) as u32,
            (cam.target_size.y * scale_factor).max(1.0) as u32,
        );

        if cam.size != new_physical_size {
            if let Some(image) = images.get_mut(&cam.texture_handle) {
                image.resize(Extent3d {
                    width: new_physical_size.x,
                    height: new_physical_size.y,
                    depth_or_array_layers: 1,
                });
                image.texture_descriptor.usage =
                    TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;
            }
            cam.size = new_physical_size;
            camera.viewport = Some(bevy::render::camera::Viewport {
                physical_position: UVec2::new(0, 0),
                physical_size: new_physical_size,
                depth: 0.0..1.0,
            });
        }

        // Аспект зависит только от `target_size`, а не от физического
        // размера текстуры — пересчитываем каждый кадр.
        if let Projection::Perspective(p) = &mut *projection {
            p.aspect_ratio = cam.target_size.x / cam.target_size.y;
        }
    }
}

pub fn spawn_embedded_3d_camera(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    config: Embedded3dViewConfig,
) -> Entity {
    let id = Embedded3dViewId(NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed));
    let initial_physical = Extent3d {
        width: config.initial_size.x.max(1.0) as u32,
        height: config.initial_size.y.max(1.0) as u32,
        depth_or_array_layers: 1,
    };

    let mut image = Image::new_fill(
        initial_physical,
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;
    let texture_handle = images.add(image);

    let mut camera = Camera3dBundle::default();
    camera.camera.target = RenderTarget::Image(texture_handle.clone());
    camera.camera.clear_color = ClearColorConfig::Custom(config.clear_color);
    camera.camera.viewport = Some(bevy::render::camera::Viewport {
        physical_position: UVec2::new(0, 0),
        physical_size: UVec2::new(initial_physical.width, initial_physical.height),
        depth: 0.0..1.0,
    });
    camera.transform = config.transform;
    camera.projection = config.projection;

    commands
        .spawn((
            camera,
            Embedded3dCamera {
                id,
                texture_handle,
                size: UVec2::new(initial_physical.width, initial_physical.height),
                target_size: config.initial_size,
                scale_factor: 1.0,
            },
            RenderLayers::layer(EMBEDDED_3D_RENDER_LAYER),
        ))
        .id()
}

pub struct Embedded3dViewResponse {
    pub hovered: bool,
    pub clicked: bool,
    pub dragged: bool,
    pub drag_delta: Vec2,
    /// Курсор в физических пикселях Bevy; origin — левый нижний угол viewport.
    pub local_cursor_pos: Option<Vec2>,
    /// Курсор в координатах egui — для отрисовки оверлеев поверх viewport.
    pub cursor_pos_egui: Option<egui::Pos2>,
    pub rect: egui::Rect,
}

pub fn draw_embedded_3d_view(
    ui: &mut egui::Ui,
    egui_contexts: &mut EguiContexts,
    camera_data: &mut Embedded3dCamera,
    interaction_state: &mut Embedded3dInteractionState,
    image_size: egui::Vec2,
) -> Embedded3dViewResponse {
    camera_data.target_size = Vec2::new(image_size.x, image_size.y);

    let texture_id = egui_contexts.add_image(camera_data.texture_handle.clone_weak());

    let response = ui.add(
        egui::Image::new(egui::load::SizedTexture::new(texture_id, image_size))
            .maintain_aspect_ratio(false)
            .sense(egui::Sense::click_and_drag()),
    );

    if response.hovered() {
        interaction_state.hovered_view = Some(camera_data.id);
    } else if interaction_state.hovered_view == Some(camera_data.id) {
        interaction_state.hovered_view = None;
    }

    if response.dragged() {
        interaction_state.dragged_view = Some(camera_data.id);
    } else if response.drag_stopped() && interaction_state.dragged_view == Some(camera_data.id) {
        interaction_state.dragged_view = None;
    }

    let cursor_pos_egui = response
        .interact_pointer_pos()
        .or_else(|| response.hover_pos());

    let local_cursor_pos = cursor_pos_egui.map(|pos| {
        let raw_x = pos.x - response.rect.min.x;
        let raw_y = pos.y - response.rect.min.y;
        // Bevy viewport считает (0,0) в левом НИЖНЕМ углу — инвертируем Y.
        let bottom_up_y = image_size.y - raw_y;
        Vec2::new(
            raw_x * camera_data.scale_factor,
            bottom_up_y * camera_data.scale_factor,
        )
    });

    Embedded3dViewResponse {
        hovered: response.hovered(),
        clicked: response.clicked_by(egui::PointerButton::Primary),
        dragged: response.dragged_by(egui::PointerButton::Primary),
        drag_delta: Vec2::new(response.drag_delta().x, response.drag_delta().y),
        local_cursor_pos,
        cursor_pos_egui,
        rect: response.rect,
    }
}

/// Строит мировой луч из координат курсора внутри viewport.
///
/// `local_pos` — в физических пикселях Bevy, origin в левом нижнем углу.
pub fn cursor_to_ray(
    local_pos: Vec2,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) -> Option<Ray3d> {
    camera.viewport_to_world(camera_transform, local_pos)
}