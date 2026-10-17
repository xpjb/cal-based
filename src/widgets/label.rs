use crate::*;
use based::{EventFlow, Widget, glam::Vec4};

pub struct Label<T> {
    text: T,
    size: f32,
    color: Vec4,
    horizontal: HorizontalAlign,
    vertical: VerticalAlign,
    wrap: bool,
}

impl<T> Label<T> {
    pub const fn new(text: T, size: f32, color: Vec4) -> Self {
        Self { text, size, color, horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Center, wrap: false }
    }

    pub const fn centered(text: T, size: f32, color: Vec4) -> Self {
        let mut label = Self::new(text, size, color);
        label.horizontal = HorizontalAlign::Center;
        label
    }

    pub const fn top(mut self, wrap: bool) -> Self {
        self.vertical = VerticalAlign::Top;
        self.wrap = wrap;
        self
    }
}

impl<T: AsRef<str>> Label<T> {
    pub fn paint(&self, context: &mut AppContext<'_>, rect: Rect) {
        let size = self.size * context.scale;
        if let Some(frame) = context.frame.as_deref_mut() {
            frame.text(context.text, self.text.as_ref(), rect, size, self.color,
                self.horizontal, self.vertical, self.wrap);
        }
    }
}

impl<T: AsRef<str>, D> Widget<ViewContext<'_, '_, D>, AppEvent> for Label<T> {
    fn handle_event(&mut self, context: &mut ViewContext<'_, '_, D>, _event: &AppEvent) -> EventFlow {
        self.paint(context.app, context.rect);
        EventFlow::NotConsumed
    }
}
