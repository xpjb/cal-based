use crate::*;
use std::{
    ops::{Deref, DerefMut},
    time::Duration,
};

use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, MouseButton, ScrollDelta, Widget,
    glam::Vec2,
    web_time::Instant,
};

const DRAG_FRAME: Duration = Duration::from_millis(16);
const DRAG_MAX_STEP: f32 = 0.05;
const DRAG_RATE: f32 = 0.05;

#[derive(Clone, Copy, Default)]
pub(crate) struct Viewport {
    pub frame: Rect,
    pub client: Rect,
    pub content: Rect,
}

impl Viewport {
    pub fn to_screen(self, point: Vec2) -> Vec2 {
        self.client.position + point - self.content.position
    }

    pub fn to_content(self, point: Vec2) -> Vec2 {
        self.content.position + point - self.client.position
    }
}

pub(crate) struct ScrollContext<'a, 'context> {
    pub inner: &'a mut AppContext<'context>,
    viewport: &'a mut Viewport,
    content_height: &'a mut f32,
    dragging: &'a mut bool,
    pointer: Vec2,
    scrolled: bool,
}

impl ScrollContext<'_, '_> {
    pub fn viewport(&self) -> Viewport {
        *self.viewport
    }

    pub fn pointer(&self) -> Vec2 {
        self.pointer
    }

    pub fn scrolled(&self) -> bool {
        self.scrolled
    }

    pub fn set_content_height(&mut self, height: f32) {
        *self.content_height = height;
        let before = self.viewport.content.position.y;
        self.viewport.content.position.y = before.clamp(
            0.0,
            (height - self.viewport.client.size.y).max(0.0),
        );
        self.scrolled |= self.viewport.content.position.y != before;
    }

    pub fn reveal(&mut self, top: f32, bottom: f32, padding: f32) {
        let before = self.viewport.content.position.y;
        if top < before + padding {
            self.viewport.content.position.y = top - padding;
        } else if bottom > before + self.viewport.client.size.y - padding {
            self.viewport.content.position.y = bottom - self.viewport.client.size.y + padding;
        }
        self.viewport.content.position.y = self.viewport.content.position.y.clamp(
            0.0,
            (*self.content_height - self.viewport.client.size.y).max(0.0),
        );
        self.scrolled |= self.viewport.content.position.y != before;
    }

    pub fn capture_drag(&mut self) {
        *self.dragging = true;
    }
}

impl<'context> Deref for ScrollContext<'_, 'context> {
    type Target = AppContext<'context>;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl DerefMut for ScrollContext<'_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

pub(crate) struct Scroll<W> {
    pub child: W,
    pub viewport: Viewport,
    pub pointer: Vec2,
    content_height: f32,
    line_step: f32,
    dragging: bool,
    drag_updated: Option<Instant>,
}

impl<W> Scroll<W> {
    pub fn with_child(child: W) -> Self {
        Self {
            child,
            viewport: Viewport::default(),
            pointer: Vec2::ZERO,
            content_height: 0.0,
            line_step: 0.0,
            dragging: false,
            drag_updated: None,
        }
    }

    pub fn set_frame(&mut self, frame: Rect, line_step: f32) {
        self.viewport.frame = frame;
        self.viewport.client = frame;
        self.viewport.content.size = frame.size;
        self.line_step = line_step;
        self.viewport.content.position.y = self.viewport.content.position.y.clamp(
            0.0,
            (self.content_height - self.viewport.client.size.y).max(0.0),
        );
    }

    fn move_by(&mut self, delta: f32) -> bool {
        let before = self.viewport.content.position.y;
        self.viewport.content.position.y = (before + delta).clamp(
            0.0,
            (self.content_height - self.viewport.client.size.y).max(0.0),
        );
        self.viewport.content.position.y != before
    }
}

impl<W> Widget<AppContext<'_>, AppEvent> for Scroll<W>
where
    W: for<'a, 'context> Widget<ScrollContext<'a, 'context>, AppEvent>,
{
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &AppEvent) -> EventFlow {
        if let Event::Input(InputEvent::CursorMoved { position, .. }) = event {
            self.pointer = position.as_vec2();
        } else if let Some(pointer) = context.input.pointer() {
            self.pointer = pointer;
        }

        match event {
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            })
            | Event::Host(HostEvent::Focused { focused: false }) => {
                self.dragging = false;
                self.drag_updated = None;
            }
            _ => {}
        }

        let mut scrolled = false;
        if matches!(event, Event::Host(HostEvent::AboutToWait)) && self.dragging {
            let client = self.viewport.client;
            let overshoot = if self.pointer.y < client.position.y {
                self.pointer.y - client.position.y
            } else if self.pointer.y > client.bottom() {
                self.pointer.y - client.bottom()
            } else {
                self.drag_updated = None;
                0.0
            };
            if overshoot != 0.0 {
                let dt = self
                    .drag_updated
                    .map(|last| {
                        context
                            .now
                            .duration_since(last)
                            .as_secs_f32()
                            .min(DRAG_MAX_STEP)
                    })
                    .unwrap_or(0.0);
                self.drag_updated = Some(context.now);
                if dt == 0.0 {
                    context.wake_at(context.now + DRAG_FRAME);
                } else if self.move_by(
                    (DRAG_RATE * overshoot * overshoot).copysign(overshoot) * dt,
                ) {
                    scrolled = true;
                    context.window.request_redraw();
                    context.wake_at(context.now + DRAG_FRAME);
                } else {
                    self.drag_updated = None;
                }
            }
        }

        let wheel = if let Event::Input(InputEvent::MouseWheel { delta, .. }) = event
            && self.viewport.frame.contains(self.pointer)
        {
            let amount = match delta {
                ScrollDelta::Lines { delta } => delta.y * self.line_step,
                ScrollDelta::Pixels { delta } => delta.y as f32,
            };
            scrolled |= self.move_by(-amount);
            context.window.request_redraw();
            true
        } else {
            false
        };

        let mut child_context = ScrollContext {
            inner: context,
            viewport: &mut self.viewport,
            content_height: &mut self.content_height,
            dragging: &mut self.dragging,
            pointer: self.pointer,
            scrolled,
        };
        let child_flow = self.child.handle_event(&mut child_context, event);
        if wheel {
            EventFlow::Consumed
        } else {
            child_flow
        }
    }
}
