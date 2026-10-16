use crate::*;
use std::time::Duration;

use based::{
    Event, EventFlow, HostEvent, InputEvent, ScrollDelta, Widget,
    web_time::Instant,
};

const DRAG_FRAME: Duration = Duration::from_millis(16);
const DRAG_MAX_STEP: f32 = 0.05;
const DRAG_RATE: f32 = 0.05;

#[derive(Default)]
pub struct Scroll {
    viewport: Rect,
    max_offset: f32,
    line_step: f32,
    offset: f32,
    drag_y: Option<f32>,
    drag_updated: Option<Instant>,
}

impl Scroll {
    pub fn offset(&self) -> f32 {
        self.offset
    }

    pub fn set_layout(&mut self, viewport: Rect, content_height: f32, line_step: f32) {
        self.viewport = viewport;
        self.max_offset = (content_height - viewport.size.y).max(0.0);
        self.line_step = line_step;
        self.offset = self.offset.clamp(0.0, self.max_offset);
    }

    pub fn reveal(&mut self, top: f32, bottom: f32, padding: f32) {
        if top < self.offset + padding {
            self.offset = top - padding;
        } else if bottom > self.offset + self.viewport.size.y - padding {
            self.offset = bottom - self.viewport.size.y + padding;
        }
        self.offset = self.offset.clamp(0.0, self.max_offset);
    }

    pub fn set_drag_y(&mut self, drag_y: Option<f32>) {
        self.drag_y = drag_y;
        if drag_y.is_none_or(|y| {
            y >= self.viewport.position.y && y <= self.viewport.bottom()
        }) {
            self.drag_updated = None;
        }
    }

    fn move_by(&mut self, delta: f32) -> bool {
        let before = self.offset;
        self.offset = (self.offset + delta).clamp(0.0, self.max_offset);
        self.offset != before
    }
}

impl Widget<AppContext<'_>, AppEvent> for Scroll {
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &AppEvent) -> EventFlow {
        match event {
            Event::Host(HostEvent::AboutToWait) => {
                let Some(y) = self.drag_y else {
                    self.drag_updated = None;
                    return EventFlow::NotConsumed;
                };
                let overshoot = if y < self.viewport.position.y {
                    y - self.viewport.position.y
                } else if y > self.viewport.bottom() {
                    y - self.viewport.bottom()
                } else {
                    self.drag_updated = None;
                    return EventFlow::NotConsumed;
                };
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
                    context.window.request_redraw();
                    context.wake_at(context.now + DRAG_FRAME);
                } else {
                    self.drag_updated = None;
                }
            }
            Event::Input(InputEvent::MouseWheel { delta, .. })
                if context
                    .input
                    .pointer()
                    .is_some_and(|point| self.viewport.contains(point)) =>
            {
                let amount = match delta {
                    ScrollDelta::Lines { delta } => delta.y * self.line_step,
                    ScrollDelta::Pixels { delta } => delta.y as f32,
                };
                self.move_by(-amount);
                context.window.request_redraw();
                return EventFlow::Consumed;
            }
            _ => {}
        }
        EventFlow::NotConsumed
    }
}
