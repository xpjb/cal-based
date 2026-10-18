use crate::*;
use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, NamedKey, TouchPhase, Widget,
};

pub struct ButtonContext<ParentContext> {
    pub parent: ParentContext,
    pub hovered: bool,
    pub pressed: bool,
}

pub struct Button<W> {
    child: W,
    pub activated: bool,
    hovered: bool,
    active: Option<(u64, bool)>,
}

impl<W> Button<W> {
    pub const fn new(child: W) -> Self {
        Self { child, activated: false, hovered: false, active: None }
    }

    fn pressed(&self) -> bool {
        self.active.is_some_and(|(_, inside)| inside)
    }

    fn reset(&mut self) {
        self.active = None;
    }
}

impl<W, ParentContext> Widget<ViewContext<'_, '_, ParentContext>, AppEvent> for Button<W>
where
    for<'a, 'b> W:
        Widget<ViewContext<'a, 'b, ButtonContext<&'a mut ParentContext>>, AppEvent>,
{
    fn handle_event(
        &mut self,
        context: &mut ViewContext<'_, '_, ParentContext>,
        event: &AppEvent,
    ) -> EventFlow {
        let before = (self.hovered, self.pressed());
        let redraw = matches!(event, Event::Host(HostEvent::RedrawRequested));
        let mut consumed = false;

        if redraw {
            if self.active.is_none() {
                self.hovered = context
                    .app
                    .input
                    .pointer()
                    .is_some_and(|point| context.app.hit_test(context.rect, point));
            }
        } else {
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
            } else if let Some((pointer, point, phase, hover)) = context.app.pointer_event(event) {
                let inside = point.is_some_and(|point| context.app.hit_test(context.rect, point));
                if hover {
                    self.hovered = inside;
                }
                match phase {
                    TouchPhase::Started if inside && self.active.is_none() => {
                        self.active = Some((pointer, true));
                        consumed = true;
                    }
                    TouchPhase::Moved
                        if self.active.is_some_and(|(active, _)| active == pointer) =>
                    {
                        self.active = Some((pointer, inside));
                        consumed = true;
                    }
                    TouchPhase::Moved if hover && inside => consumed = true,
                    TouchPhase::Ended
                        if self.active.is_some_and(|(active, _)| active == pointer) =>
                    {
                        if self
                            .active
                            .is_some_and(|(_, active_inside)| active_inside && inside)
                        {
                            self.activated = true;
                        }
                        self.reset();
                        if !hover {
                            self.hovered = false;
                        }
                        consumed = true;
                    }
                    TouchPhase::Cancelled
                        if self.active.is_some_and(|(active, _)| active == pointer) =>
                    {
                        self.reset();
                        self.hovered = false;
                        consumed = true;
                    }
                    _ => {}
                }
            }
        }

        let pressed = self.pressed();
        if !redraw && before != (self.hovered, pressed) {
            context.app.window.request_redraw();
        }
        let mut child_context = ViewContext {
            app: &mut *context.app,
            rect: context.rect,
            parent: ButtonContext {
                parent: &mut context.parent,
                hovered: self.hovered,
                pressed,
            },
        };
        let child_flow = self.child.handle_event(&mut child_context, event);
        if consumed || child_flow == EventFlow::Consumed {
            EventFlow::Consumed
        } else {
            EventFlow::NotConsumed
        }
    }
}
