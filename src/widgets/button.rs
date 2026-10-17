use crate::*;
use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, NamedKey, TouchPhase, Widget,
    glam::Vec4,
};

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

pub struct Button<W> {
    child: W,
    style: ButtonStyle,
    hovered: bool,
    active: Option<(u64, bool)>,
}

impl<W> Button<W> {
    pub const fn new(child: W, style: ButtonStyle) -> Self {
        Self { child, style, hovered: false, active: None }
    }

    fn state(&self) -> SurfaceState {
        match (self.active, self.hovered) {
            (Some((_, true)), _) => SurfaceState::Pressed,
            (None, true) => SurfaceState::Hovered,
            _ => SurfaceState::Idle,
        }
    }

    fn reset(&mut self) {
        self.active = None;
    }

    #[must_use]
    pub fn handle_event<D>(
        &mut self,
        context: &mut AppContext<'_>,
        event: &AppEvent,
        rect: Rect,
        data: D,
    ) -> (EventFlow, bool)
    where
        for<'a, 'b> W: Widget<ViewContext<'a, 'b, D>, AppEvent>,
    {
        let before = self.state();
        if matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            if self.active.is_none() {
                self.hovered = context.input.pointer().is_some_and(|point| rect.contains(point));
            }
            let scale = context.scale;
            paint_surface(context, rect, self.style.radius * scale,
                self.style.border_width * scale, self.state(), self.style.surface);
            let mut child_context = ViewContext {
                app: context,
                rect,
                data,
            };
            return (self.child.handle_event(&mut child_context, event), false);
        }
        let cancelled = match event {
            Event::Host(HostEvent::Focused { focused: false }) => {
                self.hovered = false;
                true
            }
            Event::Input(InputEvent::MouseWheel { .. }) => true,
            Event::Input(InputEvent::Keyboard(key)) => key.state == ElementState::Pressed
                && matches!(key.logical_key, Key::Named(NamedKey::Escape)),
            _ => false,
        };
        if cancelled {
            self.reset();
            if before != self.state() {
                context.window.request_redraw();
            }
            return (EventFlow::NotConsumed, false);
        }
        let Some((pointer, point, phase, hover)) = context.pointer_event(event) else {
            return (EventFlow::NotConsumed, false);
        };
        let inside = point.is_some_and(|point| rect.contains(point));
        if hover {
            self.hovered = inside;
        }
        let (consumed, activated) = match phase {
            TouchPhase::Started if inside && self.active.is_none() => {
                self.active = Some((pointer, true));
                (true, false)
            }
            TouchPhase::Moved if self.active.is_some_and(|(active, _)| active == pointer) => {
                self.active = Some((pointer, inside));
                (true, false)
            }
            TouchPhase::Moved if hover && inside => (true, false),
            TouchPhase::Ended if self.active.is_some_and(|(active, _)| active == pointer) => {
                let activated = self.active.is_some_and(|(_, active_inside)| active_inside && inside);
                self.reset();
                if !hover {
                    self.hovered = false;
                }
                (true, activated)
            }
            TouchPhase::Cancelled if self.active.is_some_and(|(active, _)| active == pointer) => {
                self.reset();
                self.hovered = false;
                (true, false)
            }
            _ => (false, false),
        };
        if before != self.state() {
            context.window.request_redraw();
        }
        (match consumed { true => EventFlow::Consumed, false => EventFlow::NotConsumed }, activated)
    }
}
