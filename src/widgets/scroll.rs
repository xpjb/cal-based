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
pub(crate) struct Viewport {
    pub client: Rect,
    pub offset: Vec2,
}

impl Viewport {
    pub fn to_screen(self, point: Vec2) -> Vec2 {
        self.client.position + point - self.offset
    }

    pub fn to_content(self, point: Vec2) -> Vec2 {
        self.offset + point - self.client.position
    }
}

pub(crate) trait ScrollContent {
    fn set_viewport(&mut self, viewport: Viewport);
    fn content_height(&self) -> f32;

    fn drag_y(&self) -> Option<f32> {
        None
    }

    fn take_reveal(&mut self) -> Option<Range<f32>> {
        None
    }
}

pub(crate) struct Scroll<W> {
    child: W,
    bounds: Rect,
    line_step: f32,
    offset: f32,
    drag_updated: Option<Instant>,
}

impl<W: ScrollContent> Scroll<W> {
    pub fn with_child(child: W) -> Self {
        Self {
            child,
            bounds: Rect::default(),
            line_step: 0.0,
            offset: 0.0,
            drag_updated: None,
        }
    }

    pub fn child(&self) -> &W {
        &self.child
    }

    pub fn child_mut(&mut self) -> &mut W {
        &mut self.child
    }

    pub fn set_bounds(&mut self, bounds: Rect, line_step: f32) {
        self.bounds = bounds;
        self.line_step = line_step;
        self.offset = self.offset.clamp(0.0, self.max_offset());
        let viewport = self.viewport();
        self.child.set_viewport(viewport);
    }

    fn viewport(&self) -> Viewport {
        Viewport {
            client: self.bounds,
            offset: Vec2::new(0.0, self.offset),
        }
    }

    fn max_offset(&self) -> f32 {
        (self.child.content_height() - self.bounds.size.y).max(0.0)
    }

    fn move_by(&mut self, delta: f32) -> bool {
        let before = self.offset;
        self.offset = (before + delta).clamp(0.0, self.max_offset());
        self.offset != before
    }
}

impl<W> Widget<AppContext<'_>, AppEvent> for Scroll<W>
where
    W: ScrollContent + for<'context> Widget<AppContext<'context>, AppEvent>,
{
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &AppEvent) -> EventFlow {
        self.offset = self.offset.clamp(0.0, self.max_offset());
        let waiting = matches!(event, Event::Host(HostEvent::AboutToWait));

        if waiting {
            if let Some(y) = self.child.drag_y() {
                let overshoot = if y < self.bounds.position.y {
                    y - self.bounds.position.y
                } else if y > self.bounds.bottom() {
                    y - self.bounds.bottom()
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
                        context.window.request_redraw();
                        context.wake_at(context.now + DRAG_FRAME);
                    } else {
                        self.drag_updated = None;
                    }
                }
            } else {
                self.drag_updated = None;
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

        let viewport = self.viewport();
        self.child.set_viewport(viewport);
        let child_flow = self.child.handle_event(context, event);

        let before = self.offset;
        self.offset = self.offset.clamp(0.0, self.max_offset());
        if let Some(range) = self.child.take_reveal() {
            if range.start < self.offset {
                self.offset = range.start;
            } else if range.end > self.offset + self.bounds.size.y {
                self.offset = range.end - self.bounds.size.y;
            }
            self.offset = self.offset.clamp(0.0, self.max_offset());
        }
        if self.offset != before {
            let viewport = self.viewport();
            self.child.set_viewport(viewport);
            context.window.request_redraw();
        }

        if let Some(y) = self.child.drag_y() {
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
