use crate::*;
use std::{ops::Range, time::Duration};

use based::{
    Event, EventFlow, HostEvent, InputEvent, ScrollDelta, Widget,
    glam::Vec2,
    web_time::Instant,
};

const DRAG_FRAME: Duration = Duration::from_millis(16);
const DRAG_MAX_STEP: f32 = 0.05;
const DRAG_RATE: f32 = 0.05;

#[derive(Clone, Copy, Default, PartialEq)]
pub struct Viewport {
    pub client: Rect,
    offset: f32,
}

impl Viewport {
    pub fn to_screen(self, point: Vec2) -> Vec2 {
        self.client.position + point - Vec2::new(0.0, self.offset)
    }

    pub fn to_content(self, point: Vec2) -> Vec2 {
        point - self.client.position + Vec2::new(0.0, self.offset)
    }
}

#[derive(Default)]
pub struct ScrollState {
    pub viewport: Viewport,
    pub content_height: f32,
    pub reveal: Option<Range<f32>>,
    pub viewport_changed: bool,
}

pub trait ScrollContent {
    fn scroll_state(&mut self) -> (&mut ScrollState, Option<f32>);
}

pub struct Scroll<W> {
    pub child: W,
    bounds: Rect,
    line_step: f32,
    offset: f32,
    drag_updated: Option<Instant>,
}

impl<W: ScrollContent> Scroll<W> {
    pub fn new(child: W) -> Self {
        Self {
            child,
            bounds: Rect::default(),
            line_step: 0.0,
            offset: 0.0,
            drag_updated: None,
        }
    }

    pub fn set_bounds(&mut self, bounds: Rect, line_step: f32) {
        self.bounds = bounds;
        self.line_step = line_step;
        self.clamp_offset();
        self.sync_viewport();
    }

    fn clamp_offset(&mut self) {
        let height = self.child.scroll_state().0.content_height;
        self.offset = self
            .offset
            .clamp(0.0, (height - self.bounds.size.y).max(0.0));
    }

    fn move_by(&mut self, delta: f32) -> bool {
        let before = self.offset;
        self.offset += delta;
        self.clamp_offset();
        self.offset != before
    }

    fn sync_viewport(&mut self) {
        let viewport = Viewport {
            client: self.bounds,
            offset: self.offset,
        };
        let state = self.child.scroll_state().0;
        state.viewport_changed |= state.viewport != viewport;
        state.viewport = viewport;
    }
}

impl<W> Widget<AppContext<'_>, AppEvent> for Scroll<W>
where
    W: ScrollContent + for<'context> Widget<AppContext<'context>, AppEvent>,
{
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &AppEvent) -> EventFlow {
        self.clamp_offset();
        let waiting = matches!(event, Event::Host(HostEvent::AboutToWait));

        if waiting {
            let overshoot = match self.child.scroll_state().1 {
                Some(y) if y < self.bounds.position.y => y - self.bounds.position.y,
                Some(y) if y > self.bounds.bottom() => y - self.bounds.bottom(),
                _ => 0.0,
            };
            if overshoot == 0.0 {
                self.drag_updated = None;
            } else {
                let dt = self
                    .drag_updated
                    .replace(context.now)
                    .map(|last| {
                        context
                            .now
                            .duration_since(last)
                            .as_secs_f32()
                            .min(DRAG_MAX_STEP)
                    })
                    .unwrap_or(0.0);
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
        }

        let wheel = if let Event::Input(InputEvent::MouseWheel { delta, .. }) = event
            && context
                .input
                .pointer()
                .is_some_and(|point| self.bounds.contains(point))
        {
            let amount = match delta {
                ScrollDelta::Lines { delta } => delta.y * self.line_step,
                ScrollDelta::Pixels { delta } => delta.y as f32,
            };
            self.move_by(-amount);
            context.window.request_redraw();
            true
        } else {
            false
        };

        self.sync_viewport();
        let child_flow = self.child.handle_event(context, event);

        let before = self.offset;
        self.clamp_offset();
        if let Some(range) = self.child.scroll_state().0.reveal.take() {
            if range.start < self.offset {
                self.offset = range.start;
            } else if range.end > self.offset + self.bounds.size.y {
                self.offset = range.end - self.bounds.size.y;
            }
            self.clamp_offset();
        }
        if self.offset != before {
            self.sync_viewport();
            context.window.request_redraw();
        }

        if let Some(y) = self.child.scroll_state().1 {
            if y >= self.bounds.position.y && y <= self.bounds.bottom() {
                self.drag_updated = None;
            } else if !waiting && self.drag_updated.is_none() {
                context.wake_at(context.now + DRAG_FRAME);
            }
        } else {
            self.drag_updated = None;
        }

        if wheel {
            EventFlow::Consumed
        } else {
            child_flow
        }
    }
}
