use based::{Event, EventFlow, HostEvent, InputEvent, Widget, glam::Vec2};

#[derive(Default)]
pub struct Input {
    pointer: Option<Vec2>,
}

impl Input {
    pub fn pointer(&self) -> Option<Vec2> {
        self.pointer
    }
}

impl<T> Widget<(), Event<T>> for Input {
    fn handle_event(&mut self, _context: &mut (), event: &Event<T>) -> EventFlow {
        match event {
            Event::Input(InputEvent::CursorMoved { position, .. }) => {
                self.pointer = Some(position.as_vec2());
            }
            Event::Input(InputEvent::CursorLeft { .. })
            | Event::Host(HostEvent::Focused { focused: false }) => self.pointer = None,
            _ => {}
        }
        EventFlow::NotConsumed
    }
}
