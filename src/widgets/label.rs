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
    pub fn paint<Context: AppContext>(&self, context: &mut Context, rect: Rect) {
        context.text(
            self.text.as_ref(),
            rect,
            self.size * context.scale(),
            self.color,
            self.horizontal,
            self.vertical,
            self.wrap,
        );
    }
}

impl<T: AsRef<str>, Context> Widget<Context, AppEvent> for Label<T>
where
    Context: AppContext + ViewContext,
{
    fn handle_event(&mut self, context: &mut Context, _event: &AppEvent) -> EventFlow {
        let rect = context.rect();
        self.paint(context, rect);
        EventFlow::NotConsumed
    }
}
