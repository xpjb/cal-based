use super::text_edit::TextEdit;
use crate::*;
use std::time::Duration;

use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, ModifiersState, MouseButton,
    NamedKey, ScrollDelta, Widget,
    glam::{Vec2, Vec4},
    web_time::Instant,
    widget::{
        sanscale::raw::{self as text, Caret, Motion},
        window::CursorIcon,
    },
};

const BASE_FONT_SIZE: f32 = 18.0;
const CARET_PERIOD: Duration = Duration::from_millis(530);
const INPUT_BG: Vec4 = Vec4::new(0.031, 0.038, 0.049, 1.0);
const SELECTION: Vec4 = Vec4::new(0.17, 0.36, 0.68, 0.62);
const INPUT_SURFACE: SurfaceStyle = SurfaceStyle {
    fill: StateColors::new(
        INPUT_BG,
        Vec4::new(0.045, 0.055, 0.071, 1.0),
        Vec4::new(0.038, 0.047, 0.061, 1.0),
        Vec4::new(0.038, 0.047, 0.061, 1.0),
    ),
    border: StateColors::new(
        GRID,
        Vec4::new(0.35, 0.49, 0.67, 1.0),
        ACCENT,
        ACCENT,
    ),
};

#[derive(Clone, Copy)]
struct TextAreaLayout {
    rect: Rect,
    scale: f32,
    font_size: f32,
}

pub struct TextArea {
    edit: TextEdit,
    scroll: f32,
    layout: Option<TextAreaLayout>,
    origin: Vec2,
    cursor: Vec2,
    selecting: bool,
    modifiers: ModifiersState,
    last_input: Instant,
    blink_phase: u64,
}

impl TextArea {
    pub fn new(content: &str, now: Instant) -> Self {
        Self {
            edit: TextEdit::new(content),
            scroll: 0.0,
            layout: None,
            origin: Vec2::ZERO,
            cursor: Vec2::ZERO,
            selecting: false,
            modifiers: ModifiersState::empty(),
            last_input: now,
            blink_phase: 0,
        }
    }

    pub fn content(&self) -> String {
        self.edit.content()
    }

    pub fn set_rect(&mut self, rect: Rect, scale: f32) {
        self.layout = Some(TextAreaLayout {
            rect,
            scale,
            font_size: BASE_FONT_SIZE * scale,
        });
    }

    pub fn control_key(&self) -> bool {
        self.modifiers.control_key()
    }

    pub fn cursor_icon(&self) -> CursorIcon {
        if self
            .layout
            .is_some_and(|layout| layout.rect.contains(self.cursor))
        {
            CursorIcon::Text
        } else {
            CursorIcon::Default
        }
    }

    fn shape(&mut self, context: &mut AppContext<'_>) -> Option<text::ShapedHandle> {
        let layout = self.layout?;
        if self.edit.is_empty() {
            self.origin = layout.rect.position + Vec2::new(12.0, 11.0) * layout.scale;
            return None;
        }
        let content = self.edit.content();
        let mut style = context.text.style;
        style.wrap_em = Some(
            ((layout.rect.size.x - 24.0 * layout.scale) / layout.font_size).max(2.0),
        );
        style.align = text::Align::Left;
        style.line_spacing = 1.18;
        let handle = context.text.raw.shape_transient(&content, &style)?;
        self.origin = layout.rect.position
            + Vec2::new(12.0 * layout.scale, 11.0 * layout.scale - self.scroll);
        Some(handle)
    }

    fn place_at_cursor(&mut self, context: &mut AppContext<'_>, select: bool) {
        let Some(layout) = self.layout else {
            return;
        };
        let handle = self.shape(context);
        if let Some(handle) = handle {
            let em = text::Vec2::new(
                (self.cursor.x - self.origin.x) / layout.font_size,
                (self.cursor.y - self.origin.y) / layout.font_size,
            );
            if let Some(caret) = context.text.raw.measure(handle).hit_test(em) {
                self.edit.place(caret, select);
            }
        } else {
            self.edit.place(
                Caret {
                    byte_index: 0,
                    line_index: 0,
                },
                select,
            );
        }
        self.last_input = context.now;
        self.blink_phase = 0;
        context.window.request_redraw();
    }

    fn keep_caret_visible(&mut self, context: &mut AppContext<'_>, edited: bool) {
        let Some(layout) = self.layout else {
            return;
        };
        let Some(handle) = self.shape(context) else {
            self.scroll = 0.0;
            return;
        };
        let text_layout = context.text.raw.measure(handle);
        if edited {
            self.edit.settle(text_layout);
        } else {
            self.edit.clamp(text_layout);
        }
        let caret = text_layout.caret_rect(self.edit.caret());
        let top = 11.0 * layout.scale + caret.y_em * layout.font_size - self.scroll;
        let bottom = top + caret.height_em.max(1.0) * layout.font_size;
        if top < 8.0 * layout.scale {
            self.scroll = (self.scroll - (8.0 * layout.scale - top)).max(0.0);
        } else if bottom > layout.rect.size.y - 8.0 * layout.scale {
            self.scroll += bottom - (layout.rect.size.y - 8.0 * layout.scale);
        }
    }
}

impl Widget<AppContext<'_>, AppEvent> for TextArea {
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &AppEvent) -> EventFlow {
        if let Event::Input(InputEvent::ModifiersChanged { modifiers }) = event {
            self.modifiers = *modifiers;
            return EventFlow::NotConsumed;
        }
        let Some(layout) = self.layout else {
            return EventFlow::NotConsumed;
        };

        if matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            if let Some(point) = context.input.pointer() {
                self.cursor = point;
            }
            let state = if self.selecting {
                SurfaceState::Pressed
            } else if layout.rect.contains(self.cursor) {
                SurfaceState::Hovered
            } else {
                SurfaceState::Idle
            };
            paint_surface(
                context,
                layout.rect,
                8.0 * layout.scale,
                layout.scale,
                state,
                INPUT_SURFACE,
            );

            let handle = self.shape(context);
            if let Some(handle) = handle {
                let (selection_rects, caret_rect) = {
                    let text_layout = context.text.raw.measure(handle);
                    let selection_rects = self.edit.selection().map_or_else(Vec::new, |selection| {
                        text_layout
                            .selection(selection)
                            .into_iter()
                            .filter_map(|span| {
                                Rect::new(
                                    self.origin.x + span.x_em * layout.font_size,
                                    self.origin.y + span.y_em * layout.font_size,
                                    (span.width_em * layout.font_size).max(2.0 * layout.scale),
                                    span.height_em * layout.font_size,
                                )
                                .intersection(layout.rect.inset(2.0 * layout.scale))
                            })
                            .collect()
                    });
                    let caret = text_layout.caret_rect(text_layout.clamp_caret(self.edit.caret()));
                    let caret_rect = Rect::new(
                        self.origin.x + caret.x_em * layout.font_size,
                        self.origin.y + caret.y_em * layout.font_size,
                        1.5 * layout.scale,
                        caret.height_em.max(1.0) * layout.font_size,
                    );
                    (selection_rects, caret_rect)
                };
                for rect in selection_rects {
                    context.quad(rect, SELECTION);
                }
                context.draw_text(
                    handle,
                    self.origin,
                    layout.font_size,
                    Vec4::new(0.86, 0.88, 0.91, 1.0),
                    layout.rect.inset(2.0 * layout.scale),
                );
                let phase = context.now.duration_since(self.last_input).as_millis() as u64
                    / CARET_PERIOD.as_millis() as u64;
                if phase.is_multiple_of(2)
                    && let Some(caret_rect) = caret_rect
                        .intersection(layout.rect.inset(2.0 * layout.scale))
                {
                    context.quad(caret_rect, Vec4::new(0.95, 0.97, 1.0, 1.0));
                }
            } else {
                context.text(
                    "Write something…",
                    layout.rect.inset(12.0 * layout.scale),
                    layout.font_size,
                    Vec4::new(0.34, 0.38, 0.44, 1.0),
                    HorizontalAlign::Left,
                    VerticalAlign::Top,
                    false,
                );
                let phase = context.now.duration_since(self.last_input).as_millis() as u64
                    / CARET_PERIOD.as_millis() as u64;
                if phase.is_multiple_of(2) {
                    context.quad(
                        Rect::new(
                            layout.rect.position.x + 12.0 * layout.scale,
                            layout.rect.position.y + 11.0 * layout.scale,
                            1.5 * layout.scale,
                            layout.font_size * 1.15,
                        ),
                        Vec4::new(0.95, 0.97, 1.0, 1.0),
                    );
                }
            }
            return EventFlow::Consumed;
        }

        match event {
            Event::Host(HostEvent::AboutToWait) => {
                let elapsed = context.now.duration_since(self.last_input);
                let phase = (elapsed.as_millis() / CARET_PERIOD.as_millis()) as u64;
                if phase != self.blink_phase {
                    self.blink_phase = phase;
                    context.window.request_redraw();
                }
                let next = self.last_input
                    + CARET_PERIOD * u32::try_from(phase + 1).unwrap_or(u32::MAX);
                context.wake_at(next);
            }
            Event::Host(HostEvent::Focused { focused: false }) => {
                self.modifiers = ModifiersState::empty();
                if self.selecting {
                    self.selecting = false;
                    context.window.request_redraw();
                }
            }
            Event::Input(InputEvent::CursorMoved { position, .. }) => {
                let hovered = layout.rect.contains(self.cursor);
                self.cursor = position.as_vec2();
                if self.selecting {
                    self.place_at_cursor(context, true);
                    return EventFlow::Consumed;
                }
                if hovered != layout.rect.contains(self.cursor) {
                    context.window.request_redraw();
                }
                if layout.rect.contains(self.cursor) {
                    return EventFlow::Consumed;
                }
            }
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            }) => {
                if let Some(point) = context.input.pointer() {
                    self.cursor = point;
                }
                if layout.rect.contains(self.cursor) {
                    self.selecting = true;
                    self.place_at_cursor(context, self.modifiers.shift_key());
                    return EventFlow::Consumed;
                }
            }
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            }) => {
                if let Some(point) = context.input.pointer() {
                    self.cursor = point;
                }
                if self.selecting {
                    self.selecting = false;
                    context.window.request_redraw();
                    return EventFlow::Consumed;
                }
            }
            Event::Input(InputEvent::MouseWheel { delta, .. }) => {
                if layout.rect.contains(self.cursor) {
                    let amount = match delta {
                        ScrollDelta::Lines { delta } => delta.y * 48.0 * layout.scale,
                        ScrollDelta::Pixels { delta } => delta.y as f32,
                    };
                    self.scroll = (self.scroll - amount).max(0.0);
                    context.window.request_redraw();
                    return EventFlow::Consumed;
                }
            }
            Event::Input(InputEvent::InputMethod(based::InputMethodEvent::Commit { text })) => {
                self.edit.insert_normalized(text);
                self.last_input = context.now;
                self.blink_phase = 0;
                self.keep_caret_visible(context, true);
                context.window.request_redraw();
                return EventFlow::Consumed;
            }
            Event::Input(InputEvent::Keyboard(key)) if key.state == ElementState::Pressed => {
                let ctrl = self.modifiers.control_key();
                let shift = self.modifiers.shift_key();
                let handle = self.shape(context);
                let text_layout = handle.map(|handle| context.text.raw.measure(handle));
                let motion = match &key.logical_key {
                    Key::Named(NamedKey::ArrowLeft) if ctrl => Some(Motion::WordLeft),
                    Key::Named(NamedKey::ArrowRight) if ctrl => Some(Motion::WordRight),
                    Key::Named(NamedKey::ArrowLeft) => Some(Motion::Left),
                    Key::Named(NamedKey::ArrowRight) => Some(Motion::Right),
                    Key::Named(NamedKey::ArrowUp) => Some(Motion::Up),
                    Key::Named(NamedKey::ArrowDown) => Some(Motion::Down),
                    Key::Named(NamedKey::Home) if ctrl => Some(Motion::DocStart),
                    Key::Named(NamedKey::End) if ctrl => Some(Motion::DocEnd),
                    Key::Named(NamedKey::Home) => Some(Motion::Home),
                    Key::Named(NamedKey::End) => Some(Motion::End),
                    _ => None,
                };
                let mut edited = false;
                let mut moved = false;
                let mut handled = motion.is_some();
                if let Some(motion) = motion {
                    if let Some(layout) = text_layout {
                        self.edit.motion(layout, motion, shift);
                        moved = true;
                    }
                } else {
                    match &key.logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            handled = true;
                            if let Some(layout) = text_layout {
                                edited = self.edit.backspace(layout);
                            }
                        }
                        Key::Named(NamedKey::Delete) => {
                            handled = true;
                            if let Some(layout) = text_layout {
                                edited = self.edit.delete(layout);
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            self.edit.insert("\n");
                            edited = true;
                            handled = true;
                        }
                        Key::Named(NamedKey::Tab) => {
                            self.edit.insert("    ");
                            edited = true;
                            handled = true;
                        }
                        Key::Character(character) if ctrl => match character.as_str() {
                            "a" | "A" => {
                                self.edit.select_all();
                                moved = true;
                                handled = true;
                            }
                            "c" | "C" => {
                                if let Some(selected) = self.edit.selected_text() {
                                    let _ = arboard::Clipboard::new().and_then(
                                        |mut clipboard| clipboard.set_text(selected),
                                    );
                                }
                                handled = true;
                            }
                            "x" | "X" => {
                                if let Some(selected) = self.edit.selected_text()
                                    && arboard::Clipboard::new()
                                        .and_then(|mut clipboard| clipboard.set_text(selected))
                                        .is_ok()
                                    && self.edit.delete_selection()
                                {
                                    edited = true;
                                }
                                handled = true;
                            }
                            "v" | "V" => {
                                if let Ok(value) = arboard::Clipboard::new()
                                    .and_then(|mut clipboard| clipboard.get_text())
                                {
                                    self.edit.insert_normalized(&value);
                                    edited = true;
                                }
                                handled = true;
                            }
                            _ => {}
                        },
                        _ => {
                            if !ctrl
                                && let Some(value) = &key.text
                                && value.chars().all(|character| !character.is_control())
                            {
                                self.edit.insert(value);
                                edited = true;
                                handled = true;
                            }
                        }
                    }
                }
                if edited || moved {
                    self.last_input = context.now;
                    self.blink_phase = 0;
                    self.keep_caret_visible(context, edited);
                    context.window.request_redraw();
                }
                if handled {
                    return EventFlow::Consumed;
                }
            }
            _ => {}
        }
        EventFlow::NotConsumed
    }
}
