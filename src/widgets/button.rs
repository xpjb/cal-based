use crate::*;
use based::glam::{Vec2, Vec4};

#[derive(Clone, Copy)]
pub struct ButtonStyle {
    pub surface: SurfaceStyle,
    pub text: Vec4,
    pub radius: f32,
    pub border_width: f32,
}

pub const NEUTRAL_BUTTON: ButtonStyle = ButtonStyle {
    surface: SurfaceStyle {
        fill: StateColors::new(
            Vec4::new(0.065, 0.08, 0.105, 1.0),
            Vec4::new(0.10, 0.13, 0.17, 1.0),
            Vec4::new(0.045, 0.055, 0.075, 1.0),
            Vec4::new(0.045, 0.055, 0.075, 1.0),
        ),
        border: StateColors::new(
            Vec4::new(0.22, 0.28, 0.36, 1.0),
            Vec4::new(0.36, 0.48, 0.62, 1.0),
            Vec4::new(0.28, 0.46, 0.68, 1.0),
            Vec4::new(0.28, 0.46, 0.68, 1.0),
        ),
    },
    text: TEXT,
    radius: 8.0,
    border_width: 1.0,
};

pub const PRIMARY_BUTTON: ButtonStyle = ButtonStyle {
    surface: SurfaceStyle {
        fill: StateColors::new(
            Vec4::new(0.09, 0.24, 0.40, 1.0),
            Vec4::new(0.13, 0.32, 0.52, 1.0),
            Vec4::new(0.065, 0.18, 0.31, 1.0),
            Vec4::new(0.065, 0.18, 0.31, 1.0),
        ),
        border: StateColors::new(
            Vec4::new(0.26, 0.46, 0.66, 1.0),
            Vec4::new(0.45, 0.67, 0.88, 1.0),
            Vec4::new(0.20, 0.38, 0.58, 1.0),
            Vec4::new(0.20, 0.38, 0.58, 1.0),
        ),
    },
    text: Vec4::ONE,
    radius: 8.0,
    border_width: 1.0,
};

pub const DANGER_BUTTON: ButtonStyle = ButtonStyle {
    surface: SurfaceStyle {
        fill: StateColors::new(
            Vec4::new(0.22, 0.07, 0.10, 1.0),
            Vec4::new(0.32, 0.10, 0.14, 1.0),
            Vec4::new(0.16, 0.045, 0.07, 1.0),
            Vec4::new(0.16, 0.045, 0.07, 1.0),
        ),
        border: StateColors::new(
            Vec4::new(0.46, 0.20, 0.27, 1.0),
            Vec4::new(0.66, 0.28, 0.36, 1.0),
            Vec4::new(0.39, 0.16, 0.22, 1.0),
            Vec4::new(0.39, 0.16, 0.22, 1.0),
        ),
    },
    text: Vec4::new(1.0, 0.86, 0.88, 1.0),
    radius: 8.0,
    border_width: 1.0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonRelease {
    Ignored,
    Cancelled,
    Activated,
}

#[derive(Default)]
pub struct Button {
    rect: Rect,
    hovered: bool,
    active_pointer: Option<u64>,
    active_inside: bool,
}

impl Button {
    pub fn set_rect(&mut self, rect: Rect) {
        self.rect = rect;
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn contains(&self, point: Vec2) -> bool {
        self.rect.contains(point)
    }

    pub fn set_hover(&mut self, point: Vec2) -> bool {
        let before = self.state();
        self.hovered = self.rect.contains(point);
        before != self.state()
    }

    pub fn pointer_moved(&mut self, pointer: u64, point: Vec2, hover_capable: bool) -> bool {
        let before = self.state();
        if hover_capable {
            self.hovered = self.rect.contains(point);
        }
        if self.active_pointer == Some(pointer) {
            self.active_inside = self.rect.contains(point);
        }
        before != self.state()
    }

    pub fn press(&mut self, pointer: u64, point: Vec2) -> bool {
        if !self.rect.contains(point) {
            return false;
        }
        self.active_pointer = Some(pointer);
        self.active_inside = true;
        true
    }

    pub fn release(&mut self, pointer: u64, point: Vec2) -> ButtonRelease {
        if self.active_pointer != Some(pointer) {
            return ButtonRelease::Ignored;
        }
        let activated = self.active_inside && self.rect.contains(point);
        self.active_pointer = None;
        self.active_inside = false;
        if activated {
            ButtonRelease::Activated
        } else {
            ButtonRelease::Cancelled
        }
    }

    pub fn cancel(&mut self) -> bool {
        let changed = self.active_pointer.is_some();
        self.active_pointer = None;
        self.active_inside = false;
        changed
    }

    pub fn state(&self) -> SurfaceState {
        if self.active_pointer.is_some() {
            if self.active_inside {
                SurfaceState::Pressed
            } else {
                SurfaceState::Idle
            }
        } else if self.hovered {
            SurfaceState::Hovered
        } else {
            SurfaceState::Idle
        }
    }

    pub fn paint(
        &self,
        context: &mut AppContext<'_>,
        label: &str,
        text_size: f32,
        scale: f32,
        style: ButtonStyle,
    ) {
        paint_surface(
            context,
            self.rect,
            style.radius * scale,
            style.border_width * scale,
            self.state(),
            style.surface,
        );
        let _ = context.text(
            label,
            self.rect,
            text_size * scale,
            style.text,
            HorizontalAlign::Center,
            VerticalAlign::Center,
            false,
        );
    }
}
