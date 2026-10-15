use based::glam::Vec4;

use super::{
    AppContext,
    paint::{HorizontalAlign, Rect, VerticalAlign},
};

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
            Vec4::new(0.13, 0.15, 0.18, 1.0),
            Vec4::new(0.18, 0.22, 0.28, 1.0),
            Vec4::new(0.09, 0.11, 0.14, 1.0),
            Vec4::new(0.09, 0.11, 0.14, 1.0),
        ),
        border: StateColors::new(
            Vec4::new(0.32, 0.39, 0.49, 1.0),
            Vec4::new(0.48, 0.62, 0.78, 1.0),
            Vec4::new(0.37, 0.58, 0.82, 1.0),
            Vec4::new(0.37, 0.58, 0.82, 1.0),
        ),
    },
    text: TEXT,
    radius: 8.0,
    border_width: 1.0,
};

pub const PRIMARY_BUTTON: ButtonStyle = ButtonStyle {
    surface: SurfaceStyle {
        fill: StateColors::new(
            Vec4::new(0.25, 0.47, 0.72, 1.0),
            Vec4::new(0.32, 0.57, 0.84, 1.0),
            Vec4::new(0.18, 0.37, 0.61, 1.0),
            Vec4::new(0.18, 0.37, 0.61, 1.0),
        ),
        border: StateColors::new(
            Vec4::new(0.43, 0.63, 0.84, 1.0),
            Vec4::new(0.66, 0.82, 0.98, 1.0),
            Vec4::new(0.31, 0.53, 0.78, 1.0),
            Vec4::new(0.31, 0.53, 0.78, 1.0),
        ),
    },
    text: Vec4::ONE,
    radius: 8.0,
    border_width: 1.0,
};

pub const DANGER_BUTTON: ButtonStyle = ButtonStyle {
    surface: SurfaceStyle {
        fill: StateColors::new(
            Vec4::new(0.35, 0.16, 0.20, 1.0),
            Vec4::new(0.48, 0.20, 0.26, 1.0),
            Vec4::new(0.27, 0.11, 0.15, 1.0),
            Vec4::new(0.27, 0.11, 0.15, 1.0),
        ),
        border: StateColors::new(
            Vec4::new(0.57, 0.29, 0.35, 1.0),
            Vec4::new(0.78, 0.39, 0.47, 1.0),
            Vec4::new(0.53, 0.24, 0.31, 1.0),
            Vec4::new(0.53, 0.24, 0.31, 1.0),
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

    pub fn contains(&self, point: based::glam::Vec2) -> bool {
        self.rect.contains(point)
    }

    pub fn set_hover(&mut self, point: based::glam::Vec2) -> bool {
        let before = self.state();
        self.hovered = self.rect.contains(point);
        before != self.state()
    }

    pub fn pointer_moved(
        &mut self,
        pointer: u64,
        point: based::glam::Vec2,
        hover_capable: bool,
    ) -> bool {
        let before = self.state();
        if hover_capable {
            self.hovered = self.rect.contains(point);
        }
        if self.active_pointer == Some(pointer) {
            self.active_inside = self.rect.contains(point);
        }
        before != self.state()
    }

    pub fn press(&mut self, pointer: u64, point: based::glam::Vec2) -> bool {
        if !self.rect.contains(point) {
            return false;
        }
        self.active_pointer = Some(pointer);
        self.active_inside = true;
        true
    }

    pub fn release(&mut self, pointer: u64, point: based::glam::Vec2) -> ButtonRelease {
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
