use crate::*;
use std::{
    ops::{Deref, DerefMut, Range},
    time::Duration,
};

use based::{
    Event, EventFlow, HostEvent, InputEvent, ScrollDelta, Widget,
    glam::Vec2,
    web_time::Instant,
};

const DRAG_FRAME: Duration = Duration::from_millis(16);
const DRAG_MAX_STEP: f32 = 0.05;
const DRAG_RATE: f32 = 0.05;

pub struct ScrollContext<ParentContext> {
    parent: ParentContext,
    pub viewport: Rect,
    pub content_height: f32,
    pub reveal: Option<Range<f32>>,
    pub drag_y: Option<f32>,
    pub scrolled: bool,
}

impl<ParentContext> Deref for ScrollContext<ParentContext> {
    type Target = ParentContext;

    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}

impl<ParentContext> DerefMut for ScrollContext<ParentContext> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parent
    }
}

pub struct Scroll<W> {
    pub child: W,
    line_step: f32,
    offset: f32,
    content_height: f32,
    drag_y: Option<f32>,
    drag_updated: Option<Instant>,
}

impl<W> Scroll<W> {
    pub fn new(child: W, line_step: f32) -> Self {
        Self {
            child,
            line_step,
            offset: 0.0,
            content_height: 0.0,
            drag_y: None,
            drag_updated: None,
        }
    }

    fn clamp_offset(&mut self, viewport_height: f32) {
        self.offset = self
            .offset
            .clamp(0.0, (self.content_height - viewport_height).max(0.0));
    }

    fn move_by(&mut self, delta: f32, viewport_height: f32) -> bool {
        let before = self.offset;
        self.offset += delta;
        self.clamp_offset(viewport_height);
        self.offset != before
    }
}

impl<W, ParentContext> Widget<ViewContext<'_, '_, ParentContext>, AppEvent> for Scroll<W>
where
    for<'a, 'b> W:
        Widget<ViewContext<'a, 'b, ScrollContext<&'a mut ParentContext>>, AppEvent>,
{
    fn handle_event(
        &mut self,
        context: &mut ViewContext<'_, '_, ParentContext>,
        event: &AppEvent,
    ) -> EventFlow {
        let viewport = context.rect;
        self.clamp_offset(viewport.size.y);
        let offset_before = self.offset;
        let waiting = matches!(event, Event::Host(HostEvent::AboutToWait));

        if waiting {
            let overshoot = match self.drag_y {
                Some(y) if y < viewport.position.y => y - viewport.position.y,
                Some(y) if y > viewport.bottom() => y - viewport.bottom(),
                _ => 0.0,
            };
            if overshoot == 0.0 {
                self.drag_updated = None;
            } else {
                let dt = self
                    .drag_updated
                    .replace(context.app.now)
                    .map(|last| {
                        context
                            .app
                            .now
                            .duration_since(last)
                            .as_secs_f32()
                            .min(DRAG_MAX_STEP)
                    })
                    .unwrap_or(0.0);
                if dt == 0.0 {
                    context.app.wake_at(context.app.now + DRAG_FRAME);
                } else if self.move_by(
                    (DRAG_RATE * overshoot * overshoot).copysign(overshoot) * dt,
                    viewport.size.y,
                ) {
                    context.app.window.request_redraw();
                    context.app.wake_at(context.app.now + DRAG_FRAME);
                } else {
                    self.drag_updated = None;
                }
            }
        }

        let wheel = if let Event::Input(InputEvent::MouseWheel { delta, .. }) = event
            && context
                .app
                .input
                .pointer()
                .is_some_and(|point| context.hit_test(point))
        {
            let amount = match delta {
                ScrollDelta::Lines { delta } => delta.y * self.line_step * context.app.scale,
                ScrollDelta::Pixels { delta } => delta.y as f32,
            };
            self.move_by(-amount, viewport.size.y);
            context.app.window.request_redraw();
            true
        } else {
            false
        };

        let content = Rect {
            position: viewport.position - Vec2::new(0.0, self.offset),
            size: Vec2::new(
                viewport.size.x,
                self.content_height.max(viewport.size.y),
            ),
        };
        let parent_clip = context.app.clip;
        context.app.clip = parent_clip.intersection(viewport).unwrap_or_default();
        let mut child_context = ViewContext {
            app: &mut *context.app,
            rect: content,
            parent: ScrollContext {
                parent: &mut context.parent,
                viewport,
                content_height: self.content_height,
                reveal: None,
                drag_y: None,
                scrolled: self.offset != offset_before,
            },
        };
        let child_flow = self.child.handle_event(&mut child_context, event);
        self.content_height = child_context.parent.content_height;
        self.drag_y = child_context.parent.drag_y;
        let reveal = child_context.parent.reveal.take();
        drop(child_context);
        context.app.clip = parent_clip;

        let before = self.offset;
        self.clamp_offset(viewport.size.y);
        if let Some(range) = reveal {
            if range.start < self.offset {
                self.offset = range.start;
            } else if range.end > self.offset + viewport.size.y {
                self.offset = range.end - viewport.size.y;
            }
            self.clamp_offset(viewport.size.y);
        }
        if self.offset != before {
            context.app.window.request_redraw();
        }

        if let Some(y) = self.drag_y {
            if y >= viewport.position.y && y <= viewport.bottom() {
                self.drag_updated = None;
            } else if !waiting && self.drag_updated.is_none() {
                context.app.wake_at(context.app.now + DRAG_FRAME);
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
