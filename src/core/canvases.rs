//! 2D-холсты (front/top/side): плоские `Vec<u8>` + egui-текстуры.
//! Кэш текстур ленивый: перестраиваем только при `dirty_*`.

use bevy::prelude::Resource;
use bevy_egui::egui;

use crate::app::state::ViewType;

#[derive(Resource)]
pub struct OrthoCanvases {
    pub size: u32,
    pub front: Vec<u8>,
    pub top: Vec<u8>,
    pub side: Vec<u8>,
    pub front_tex: Option<egui::TextureHandle>,
    pub top_tex: Option<egui::TextureHandle>,
    pub side_tex: Option<egui::TextureHandle>,
    pub dirty_front: bool,
    pub dirty_top: bool,
    pub dirty_side: bool,
}

impl OrthoCanvases {
    pub fn new(size: u32) -> Self {
        let total = (size as usize) * (size as usize);
        Self {
            size,
            front: vec![1u8; total],
            top: vec![1u8; total],
            side: vec![1u8; total],
            front_tex: None,
            top_tex: None,
            side_tex: None,
            dirty_front: true,
            dirty_top: true,
            dirty_side: true,
        }
    }

    #[inline]
    pub fn get(&self, view: ViewType, x: u32, y: u32) -> bool {
        if x >= self.size || y >= self.size {
            return false;
        }
        let idx = (y * self.size + x) as usize;
        match view {
            ViewType::Front => self.front.get(idx).copied().unwrap_or(0) == 1,
            ViewType::Top => self.top.get(idx).copied().unwrap_or(0) == 1,
            ViewType::Side => self.side.get(idx).copied().unwrap_or(0) == 1,
            ViewType::Perspective => false,
        }
    }

    #[inline]
    pub fn set(&mut self, view: ViewType, x: u32, y: u32, val: bool) {
        if x >= self.size || y >= self.size {
            return;
        }
        let idx = (y * self.size + x) as usize;
        let v = if val { 1u8 } else { 0u8 };

        match view {
            ViewType::Front => {
                if let Some(p) = self.front.get_mut(idx) {
                    if *p != v {
                        *p = v;
                        self.dirty_front = true;
                    }
                }
            }
            ViewType::Top => {
                if let Some(p) = self.top.get_mut(idx) {
                    if *p != v {
                        *p = v;
                        self.dirty_top = true;
                    }
                }
            }
            ViewType::Side => {
                if let Some(p) = self.side.get_mut(idx) {
                    if *p != v {
                        *p = v;
                        self.dirty_side = true;
                    }
                }
            }
            ViewType::Perspective => {}
        }
    }

    pub fn clear(&mut self, view: ViewType) {
        self.fill_with(view, 0);
    }

    pub fn fill(&mut self, view: ViewType) {
        self.fill_with(view, 1);
    }

    fn fill_with(&mut self, view: ViewType, val: u8) {
        let data = vec![val; (self.size as usize) * (self.size as usize)];
        match view {
            ViewType::Front => {
                self.front = data;
                self.dirty_front = true;
            }
            ViewType::Top => {
                self.top = data;
                self.dirty_top = true;
            }
            ViewType::Side => {
                self.side = data;
                self.dirty_side = true;
            }
            ViewType::Perspective => {}
        }
    }
}