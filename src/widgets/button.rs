use crate::*;
use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, NamedKey, TouchPhase, Widget,
};
use std::ops::{Deref, DerefMut};

pub trait ButtonContext {
    fn button_hovered(&self) -> bool;
    fn button_pressed(&self) -> bool;
}

impl<Context: ButtonContext + ?Sized> ButtonContext for &mut Context {
    fn button_hovered(&self) -> bool {
        (**self).button_hovered()
    }

    fn button_pressed(&self) -> bool {
        (**self).button_pressed()
    }
}

pub struct ButtonBinding<ParentContext> {
    pub parent: ParentContext,
    hovered: bool,
    pressed: bool,
}

impl<ParentContext> ButtonContext for ButtonBinding<ParentContext> {
    fn button_hovered(&self) -> bool {
        self.hovered
    }

    fn button_pressed(&self) -> bool {
        self.pressed
    }
}

impl<ParentContext: ViewContext> ViewContext for ButtonBinding<ParentContext> {
    fn rect(&self) -> Rect {
        self.parent.rect()
    }
}

impl<ParentContext: ButtonContext> ButtonContext for ViewBinding<ParentContext> {
    fn button_hovered(&self) -> bool {
        self.parent.button_hovered()
    }

    fn button_pressed(&self) -> bool {
        self.parent.button_pressed()
    }
}

impl<ParentContext> Deref for ButtonBinding<ParentContext> {
    type Target = ParentContext;

    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}

impl<ParentContext> DerefMut for ButtonBinding<ParentContext> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parent
    }
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

impl<W, Context> Widget<Context, AppEvent> for Button<W>
where
    Context: AppContext + ViewContext,
    for<'a> W: Widget<ButtonBinding<&'a mut Context>, AppEvent>,
{
    fn handle_event(&mut self, context: &mut Context, event: &AppEvent) -> EventFlow {
        let before = (self.hovered, self.pressed());
        let redraw = matches!(event, Event::Host(HostEvent::RedrawRequested));
        let mut consumed = false;

        if redraw {
            if self.active.is_none() {
                self.hovered = context
                    .pointer()
                    .is_some_and(|point| context.rect().contains(point));
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
            } else if let Some((pointer, point, phase, hover)) =
                context.pointer_event(event)
            {
                let inside = point.is_some_and(|point| context.rect().contains(point));
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
            context.request_redraw();
        }
        let mut child_context = ButtonBinding {
            parent: context,
            hovered: self.hovered,
            pressed,
        };
        let child_flow = self.child.handle_event(&mut child_context, event);
        if consumed || child_flow == EventFlow::Consumed {
            EventFlow::Consumed
        } else {
            EventFlow::NotConsumed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_button_and_view_bindings_win() {
        let outer_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let inner_rect = Rect::new(10.0, 20.0, 30.0, 40.0);
        let outer_view = ViewBinding { parent: (), rect: outer_rect };
        let outer_button = ButtonBinding {
            parent: outer_view,
            hovered: true,
            pressed: true,
        };
        let inner_view = ViewBinding { parent: outer_button, rect: inner_rect };

        assert!(inner_view.button_hovered());
        assert!(inner_view.button_pressed());
        assert_eq!(inner_view.rect(), inner_rect);

        let inner_button = ButtonBinding {
            parent: inner_view,
            hovered: false,
            pressed: false,
        };

        assert!(!inner_button.button_hovered());
        assert!(!inner_button.button_pressed());
        assert_eq!(inner_button.rect(), inner_rect);
    }
}
