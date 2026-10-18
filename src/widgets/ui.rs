use crate::*;
use based::{
    TouchPhase,
    glam::{Vec2, Vec4},
};
use std::ops::{Deref, DerefMut};

pub const BACKGROUND: Vec4 = Vec4::new(0.035, 0.043, 0.055, 1.0);
pub const GRID: Vec4 = Vec4::new(0.15, 0.175, 0.21, 1.0);
pub const TEXT: Vec4 = Vec4::new(0.90, 0.92, 0.95, 1.0);
pub const MUTED: Vec4 = Vec4::new(0.49, 0.54, 0.61, 1.0);
pub const ACCENT: Vec4 = Vec4::new(0.40, 0.66, 0.96, 1.0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SurfaceState {
    #[default]
    Idle,
    Hovered,
    Pressed,
    Dragging,
}

pub trait AppContext {
    fn pointer(&self) -> Option<Vec2>;
    fn pointer_event(
        &self,
        event: &AppEvent,
    ) -> Option<(u64, Option<Vec2>, TouchPhase, bool)>;
    fn request_redraw(&self);
    fn scale(&self) -> f32;
    fn surface(
        &mut self,
        rect: Rect,
        radius: f32,
        border_width: f32,
        fill: Vec4,
        border: Vec4,
    );
    fn text(
        &mut self,
        content: &str,
        rect: Rect,
        size: f32,
        color: Vec4,
        horizontal: HorizontalAlign,
        vertical: VerticalAlign,
        wrap: bool,
    );
}

impl AppContext for AppServices<'_> {
    fn pointer(&self) -> Option<Vec2> {
        self.input.pointer()
    }

    fn pointer_event(
        &self,
        event: &AppEvent,
    ) -> Option<(u64, Option<Vec2>, TouchPhase, bool)> {
        AppServices::pointer_event(self, event)
    }

    fn request_redraw(&self) {
        self.window.request_redraw();
    }

    fn scale(&self) -> f32 {
        self.scale
    }

    fn surface(
        &mut self,
        rect: Rect,
        radius: f32,
        border_width: f32,
        fill: Vec4,
        border: Vec4,
    ) {
        AppServices::surface(self, rect, radius, border_width, fill, border);
    }

    fn text(
        &mut self,
        content: &str,
        rect: Rect,
        size: f32,
        color: Vec4,
        horizontal: HorizontalAlign,
        vertical: VerticalAlign,
        wrap: bool,
    ) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.text(
                self.text,
                content,
                rect,
                size,
                color,
                horizontal,
                vertical,
                wrap,
            );
        }
    }
}

impl<Context> AppContext for Context
where
    Context: DerefMut,
    Context::Target: AppContext,
{
    fn pointer(&self) -> Option<Vec2> {
        self.deref().pointer()
    }

    fn pointer_event(
        &self,
        event: &AppEvent,
    ) -> Option<(u64, Option<Vec2>, TouchPhase, bool)> {
        self.deref().pointer_event(event)
    }

    fn request_redraw(&self) {
        self.deref().request_redraw();
    }

    fn scale(&self) -> f32 {
        self.deref().scale()
    }

    fn surface(
        &mut self,
        rect: Rect,
        radius: f32,
        border_width: f32,
        fill: Vec4,
        border: Vec4,
    ) {
        self.deref_mut()
            .surface(rect, radius, border_width, fill, border);
    }

    fn text(
        &mut self,
        content: &str,
        rect: Rect,
        size: f32,
        color: Vec4,
        horizontal: HorizontalAlign,
        vertical: VerticalAlign,
        wrap: bool,
    ) {
        self.deref_mut().text(
            content,
            rect,
            size,
            color,
            horizontal,
            vertical,
            wrap,
        );
    }
}

pub trait ViewContext {
    fn rect(&self) -> Rect;
}

impl<Context: ViewContext + ?Sized> ViewContext for &mut Context {
    fn rect(&self) -> Rect {
        (**self).rect()
    }
}

pub struct ViewBinding<ParentContext> {
    pub parent: ParentContext,
    pub rect: Rect,
}

impl<ParentContext> ViewContext for ViewBinding<ParentContext> {
    fn rect(&self) -> Rect {
        self.rect
    }
}

impl<ParentContext> Deref for ViewBinding<ParentContext> {
    type Target = ParentContext;

    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}

impl<ParentContext> DerefMut for ViewBinding<ParentContext> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parent
    }
}

#[derive(Clone, Copy)]
pub struct StateColors {
    pub idle: Vec4,
    pub hovered: Vec4,
    pub pressed: Vec4,
    pub dragging: Vec4,
}

impl StateColors {
    pub const fn new(idle: Vec4, hovered: Vec4, pressed: Vec4, dragging: Vec4) -> Self {
        Self {
            idle,
            hovered,
            pressed,
            dragging,
        }
    }

    fn get(self, state: SurfaceState) -> Vec4 {
        match state {
            SurfaceState::Idle => self.idle,
            SurfaceState::Hovered => self.hovered,
            SurfaceState::Pressed => self.pressed,
            SurfaceState::Dragging => self.dragging,
        }
    }
}

#[derive(Clone, Copy)]
pub struct SurfaceStyle {
    pub fill: StateColors,
    pub border: StateColors,
}

pub fn paint_surface<Context: AppContext>(
    context: &mut Context,
    rect: Rect,
    radius: f32,
    border_width: f32,
    state: SurfaceState,
    style: SurfaceStyle,
) {
    context.surface(
        rect,
        radius,
        border_width,
        style.fill.get(state),
        style.border.get(state),
    );
}
