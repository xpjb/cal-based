use crate::*;
use based::{Event, EventFlow, HostEvent, Widget, glam::Vec4};

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

pub struct TextButton {
    label: &'static str,
    size: f32,
    style: ButtonStyle,
}

impl TextButton {
    pub const fn new(label: &'static str, size: f32, style: ButtonStyle) -> Self {
        Self { label, size, style }
    }
}

impl Widget<ButtonContext<&mut ViewContext<&mut AppServices<'_>>>, AppEvent> for TextButton {
    fn handle_event(
        &mut self,
        context: &mut ButtonContext<&mut ViewContext<&mut AppServices<'_>>>,
        event: &AppEvent,
    ) -> EventFlow {
        if !matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            return EventFlow::NotConsumed;
        }
        let state = if context.pressed {
            SurfaceState::Pressed
        } else if context.hovered {
            SurfaceState::Hovered
        } else {
            SurfaceState::Idle
        };
        let rect = context.rect;
        let scale = context.scale;
        paint_surface(
            context,
            rect,
            self.style.radius * scale,
            self.style.border_width * scale,
            state,
            self.style.surface,
        );
        Label::centered(self.label, self.size, self.style.text).paint(context, rect);
        EventFlow::NotConsumed
    }
}
