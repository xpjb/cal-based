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
    pub fn paint(&self, context: &mut AppServices<'_>, rect: Rect) {
        let size = self.size * context.scale;
        if let Some(frame) = context.frame.as_deref_mut() {
            frame.text(context.text, self.text.as_ref(), rect, size, self.color,
                self.horizontal, self.vertical, self.wrap);
        }
    }
}

impl<T: AsRef<str>> Widget<ViewContext<&mut AppServices<'_>>, AppEvent> for Label<T> {
    fn handle_event(
        &mut self,
        context: &mut ViewContext<&mut AppServices<'_>>,
        _event: &AppEvent,
    ) -> EventFlow {
        let rect = context.rect;
        self.paint(context, rect);
        EventFlow::NotConsumed
    }
}
