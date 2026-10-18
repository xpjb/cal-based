use crate::*;
use based::glam::Vec4;

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

pub struct ViewContext<'a, 'b, ParentContext> {
    pub app: &'a mut AppContext<'b>,
    pub rect: Rect,
    pub parent: ParentContext,
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
