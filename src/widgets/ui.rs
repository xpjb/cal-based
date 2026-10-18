use crate::*;
use based::glam::Vec4;
use std::ops::{Deref, DerefMut};

pub const BACKGROUND: Vec4 = Vec4::new(0.035, 0.043, 0.055, 1.0);
pub const GRID: Vec4 = Vec4::new(0.15, 0.175, 0.21, 1.0);
pub const TEXT: Vec4 = Vec4::new(0.90, 0.92, 0.95, 1.0);
pub const MUTED: Vec4 = Vec4::new(0.49, 0.54, 0.61, 1.0);
pub const ACCENT: Vec4 = Vec4::new(0.40, 0.66, 0.96, 1.0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SurfaceState {
    #[default]
    Idle,
    Hovered,
    Pressed,
    Dragging,
}

pub trait AppContextChain {
    fn with_app<R>(&mut self, use_app: impl FnOnce(&mut AppContext<'_>) -> R) -> R;
}

impl AppContextChain for AppContext<'_> {
    fn with_app<R>(&mut self, use_app: impl FnOnce(&mut AppContext<'_>) -> R) -> R {
        use_app(self)
    }
}

impl<Context> AppContextChain for Context
where
    Context: DerefMut,
    Context::Target: AppContextChain,
{
    fn with_app<R>(&mut self, use_app: impl FnOnce(&mut AppContext<'_>) -> R) -> R {
        self.deref_mut().with_app(use_app)
    }
}

pub struct ViewContext<ParentContext> {
    pub parent: ParentContext,
    pub rect: Rect,
}

impl<ParentContext> Deref for ViewContext<ParentContext> {
    type Target = ParentContext;

    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}

impl<ParentContext> DerefMut for ViewContext<ParentContext> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parent
    }
}

#[derive(Clone, Copy)]
pub struct StateColors {
    pub idle: Vec4,
    pub hovered: Vec4,
    pub pressed: Vec4,
    pub dragging: Vec4,
}

impl StateColors {
    pub const fn new(idle: Vec4, hovered: Vec4, pressed: Vec4, dragging: Vec4) -> Self {
        Self {
            idle,
            hovered,
            pressed,
            dragging,
        }
    }

    fn get(self, state: SurfaceState) -> Vec4 {
        match state {
            SurfaceState::Idle => self.idle,
            SurfaceState::Hovered => self.hovered,
            SurfaceState::Pressed => self.pressed,
            SurfaceState::Dragging => self.dragging,
        }
    }
}

#[derive(Clone, Copy)]
pub struct SurfaceStyle {
    pub fill: StateColors,
    pub border: StateColors,
}

pub fn paint_surface(
    context: &mut AppContext<'_>,
    rect: Rect,
    radius: f32,
    border_width: f32,
    state: SurfaceState,
    style: SurfaceStyle,
) {
    context.surface(
        rect,
        radius,
        border_width,
        style.fill.get(state),
        style.border.get(state),
    );
}
