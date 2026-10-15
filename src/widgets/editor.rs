use crate::*;
use std::{ops::Range, time::Duration};

use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, ModifiersState, MouseButton,
    NamedKey, ScrollDelta, Widget,
    backend::winit::raw::window::CursorIcon,
    glam::{Vec2, Vec4},
    widget::sanscale::raw::{self as text, Caret, Layout, Motion, WordBoundaries},
};
use chrono::NaiveDate;
use ropey::Rope;

const BASE_FONT_SIZE: f32 = 18.0;
const CARET_PERIOD: Duration = Duration::from_millis(530);
const EDITOR_BG: Vec4 = Vec4::new(0.075, 0.088, 0.108, 1.0);
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
enum EditorTarget {
    Day(NaiveDate),
    Event {
        id: EventId,
        start_date: NaiveDate,
        end_date: NaiveDate,
    },
}

struct Document {
    rope: Rope,
}

impl Document {
    fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(&text.replace("\r\n", "\n").replace('\r', "\n")),
        }
    }

    fn replace(&mut self, range: Range<usize>, insert: &str) {
        let start = self.rope.byte_to_char(range.start);
        let end = self.rope.byte_to_char(range.end);
        self.rope.remove(start..end);
        self.rope.insert(start, insert);
    }
}

impl WordBoundaries for Document {
    fn prev_word(&self, byte: usize) -> Option<usize> {
        let mut chars = self.rope.chars_at(self.rope.byte_to_char(byte));
        let mut offset = byte;
        let mut in_word = false;
        while let Some(character) = chars.prev() {
            if in_word && !character.is_alphanumeric() && character != '_' {
                break;
            }
            if character.is_alphanumeric() || character == '_' {
                in_word = true;
            }
            offset -= character.len_utf8();
            if in_word && offset == 0 {
                break;
            }
        }
        Some(offset)
    }

    fn next_word(&self, byte: usize) -> Option<usize> {
        let mut offset = byte;
        let mut in_word = false;
        for character in self.rope.chars_at(self.rope.byte_to_char(byte)) {
            if in_word && !(character.is_alphanumeric() || character == '_') {
                break;
            }
            if character.is_alphanumeric() || character == '_' {
                in_word = true;
            }
            offset += character.len_utf8();
        }
        Some(offset)
    }
}

struct Session {
    target: EditorTarget,
    document: Document,
    caret: Caret,
    anchor: Option<usize>,
    goal: Option<f32>,
    scroll: f32,
    error: Option<String>,
}

impl Session {
    fn selection(&self) -> Option<Range<usize>> {
        let anchor = self.anchor?;
        let start = anchor.min(self.caret.byte_index);
        let end = anchor.max(self.caret.byte_index);
        (start != end).then_some(start..end)
    }

    fn place(&mut self, caret: Caret, select: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret.byte_index);
            }
        } else {
            self.anchor = None;
        }
        self.caret = caret;
    }

    fn motion(&mut self, layout: &Layout, motion: Motion, select: bool) {
        let caret = layout.caret_move(
            self.caret,
            motion,
            &mut self.goal,
            &self.document,
        );
        self.place(caret, select);
    }

    fn replace(&mut self, range: Range<usize>, insert: &str) {
        let at = range.start;
        self.document.replace(range, insert);
        self.edit_placed(at + insert.len());
    }

    fn insert(&mut self, text: &str) {
        let byte = self.caret.byte_index;
        let range = self.selection().unwrap_or(byte..byte);
        self.replace(range, text);
    }

    fn backspace(&mut self, layout: &Layout) -> bool {
        let byte = self.caret.byte_index;
        let range = self
            .selection()
            .or_else(|| layout.prev_caret_stop(byte).map(|start| start..byte));
        let Some(range) = range else {
            return false;
        };
        self.replace(range, "");
        true
    }

    fn delete(&mut self, layout: &Layout) -> bool {
        let byte = self.caret.byte_index;
        let range = self
            .selection()
            .or_else(|| layout.next_caret_stop(byte).map(|end| byte..end));
        let Some(range) = range else {
            return false;
        };
        self.replace(range, "");
        true
    }

    fn edit_placed(&mut self, byte: usize) {
        self.caret.byte_index = byte;
        self.anchor = None;
        self.goal = None;
    }

    fn selected_text(&self) -> Option<String> {
        let range = self.selection()?;
        let chars = self.document.rope.byte_to_char(range.start)
            ..self.document.rope.byte_to_char(range.end);
        Some(self.document.rope.slice(chars).to_string())
    }

    fn settle(&mut self, layout: &Layout) {
        self.caret = layout.caret_after_edit(self.caret.byte_index);
    }
}

#[derive(Clone, Copy)]
struct EditorLayout {
    panel: Rect,
    text: Rect,
    save: Rect,
    cancel: Rect,
    delete: Option<Rect>,
    scale: f32,
    font_size: f32,
}

pub struct EditorWidget {
    session: Option<Session>,
    layout: Option<EditorLayout>,
    handle: Option<text::ShapedHandle>,
    origin: Vec2,
    cursor: Vec2,
    selecting: bool,
    modifiers: ModifiersState,
    last_input: based::web_time::Instant,
    blink_phase: u64,
    save_button: Button,
    cancel_button: Button,
    delete_button: Button,
}

impl EditorWidget {
    pub fn new() -> Self {
        Self {
            session: None,
            layout: None,
            handle: None,
            origin: Vec2::ZERO,
            cursor: Vec2::ZERO,
            selecting: false,
            modifiers: ModifiersState::empty(),
            last_input: based::web_time::Instant::now(),
            blink_phase: 0,
            save_button: Button::default(),
            cancel_button: Button::default(),
            delete_button: Button::default(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.session.is_some()
    }

    pub fn open_day(
        &mut self,
        context: &mut AppContext<'_>,
        date: NaiveDate,
        description: String,
    ) {
        let document = Document::new(&description);
        self.session = Some(Session {
            target: EditorTarget::Day(date),
            caret: Caret {
                byte_index: document.rope.len_bytes(),
                line_index: usize::MAX,
            },
            document,
            anchor: None,
            goal: None,
            scroll: 0.0,
            error: None,
        });
        self.handle = None;
        self.last_input = context.now();
        self.blink_phase = 0;
        context.set_ime_allowed(true);
        context.request_redraw();
    }

    pub fn open_event(&mut self, context: &mut AppContext<'_>, event: CalendarEvent) {
        let document = Document::new(&event.description);
        self.session = Some(Session {
            target: EditorTarget::Event {
                id: event.id,
                start_date: event.start_date,
                end_date: event.end_date,
            },
            caret: Caret {
                byte_index: document.rope.len_bytes(),
                line_index: usize::MAX,
            },
            document,
            anchor: None,
            goal: None,
            scroll: 0.0,
            error: None,
        });
        self.handle = None;
        self.last_input = context.now();
        self.blink_phase = 0;
        context.set_ime_allowed(true);
        context.request_redraw();
    }

    fn close(&mut self, context: &mut AppContext<'_>) {
        self.session = None;
        self.handle = None;
        self.layout = None;
        self.selecting = false;
        self.save_button.cancel();
        self.cancel_button.cancel();
        self.delete_button.cancel();
        context.set_ime_allowed(false);
        context.set_cursor(CursorIcon::Default);
        context.request_redraw();
    }

    fn save(&mut self, context: &mut AppContext<'_>) {
        let Some(session) = &self.session else {
            return;
        };
        let description = session.document.rope.to_string();
        let operation = match session.target {
            EditorTarget::Day(date) => CalendarTxn::SetDayDescription { date, description },
            EditorTarget::Event { id, .. } => {
                CalendarTxn::SetEventDescription { id, description }
            }
        };
        match context.model.transact(vec![operation]) {
            Ok(()) => self.close(context),
            Err(error) => {
                if let Some(session) = &mut self.session {
                    session.error = Some(error.to_string());
                }
                context.request_redraw();
            }
        }
    }

    fn shape(&mut self, context: &mut AppContext<'_>, layout: EditorLayout) -> Option<text::ShapedHandle> {
        let session = self.session.as_ref()?;
        if session.document.rope.len_bytes() == 0 {
            self.handle = None;
            self.origin = layout.text.position
                + Vec2::new(12.0 * layout.scale, 11.0 * layout.scale - session.scroll);
            return None;
        }
        let content = session.document.rope.to_string();
        let mut style = context.text.style;
        style.wrap_em = Some(
            ((layout.text.size.x - 24.0 * layout.scale) / layout.font_size).max(2.0),
        );
        style.align = text::Align::Left;
        style.line_spacing = 1.18;
        let handle = context.text.raw.shape_transient(&content, &style)?;
        self.handle = Some(handle);
        self.origin = layout.text.position
            + Vec2::new(12.0 * layout.scale, 11.0 * layout.scale - session.scroll);
        Some(handle)
    }

    fn place_at_cursor(&mut self, context: &mut AppContext<'_>, select: bool) {
        let Some(layout) = self.layout else {
            return;
        };
        let handle = self.shape(context, layout);
        let Some(session) = &mut self.session else {
            return;
        };
        if let Some(handle) = handle {
            let em = text::Vec2::new(
                (self.cursor.x - self.origin.x) / layout.font_size,
                (self.cursor.y - self.origin.y) / layout.font_size,
            );
            if let Some(caret) = context.text.raw.measure(handle).hit_test(em) {
                session.place(caret, select);
            }
        } else {
            session.place(
                Caret {
                    byte_index: 0,
                    line_index: 0,
                },
                select,
            );
        }
        session.goal = None;
        self.last_input = context.now();
        self.blink_phase = 0;
        context.request_redraw();
    }

    fn keep_caret_visible(&mut self, context: &mut AppContext<'_>, edited: bool) {
        let Some(layout) = self.layout else {
            return;
        };
        let Some(handle) = self.shape(context, layout) else {
            if let Some(session) = &mut self.session {
                session.scroll = 0.0;
            }
            return;
        };
        let Some(session) = &mut self.session else {
            return;
        };
        let text_layout = context.text.raw.measure(handle);
        if edited {
            session.settle(text_layout);
        } else {
            session.caret = text_layout.clamp_caret(session.caret);
        }
        let caret = text_layout.caret_rect(session.caret);
        let top = 11.0 * layout.scale + caret.y_em * layout.font_size - session.scroll;
        let bottom = top + caret.height_em.max(1.0) * layout.font_size;
        if top < 8.0 * layout.scale {
            session.scroll = (session.scroll - (8.0 * layout.scale - top)).max(0.0);
        } else if bottom > layout.text.size.y - 8.0 * layout.scale {
            session.scroll += bottom - (layout.text.size.y - 8.0 * layout.scale);
        }
    }
}

impl Widget<AppContext<'_>, Event> for EditorWidget {
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &Event) -> EventFlow {
        if self.session.is_none() {
            return EventFlow::NotConsumed;
        }

        if matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            if let Some(point) = context.input.pointer() {
                self.cursor = point;
            }
            let viewport = context.viewport().as_vec2();
            let scale = context.scale_factor();
            let font_size = BASE_FONT_SIZE * scale;
            let panel_width = (viewport.x - 48.0 * scale)
                .min(760.0 * scale)
                .max(420.0 * scale);
            let panel_height = (viewport.y - 48.0 * scale)
                .min(620.0 * scale)
                .max(420.0 * scale);
            let panel = Rect::new(
                (viewport.x - panel_width) * 0.5,
                (viewport.y - panel_height) * 0.5,
                panel_width,
                panel_height,
            );
            let text_rect = Rect::new(
                panel.position.x + 24.0 * scale,
                panel.position.y + 104.0 * scale,
                panel.size.x - 48.0 * scale,
                panel.size.y - 184.0 * scale,
            );
            let save = Rect::new(
                panel.right() - 124.0 * scale,
                panel.bottom() - 62.0 * scale,
                100.0 * scale,
                40.0 * scale,
            );
            let cancel = Rect::new(
                save.position.x - 112.0 * scale,
                save.position.y,
                100.0 * scale,
                40.0 * scale,
            );
            let delete = matches!(
                self.session.as_ref().map(|session| session.target),
                Some(EditorTarget::Event { .. })
            )
            .then_some(Rect::new(
                panel.position.x + 24.0 * scale,
                save.position.y,
                100.0 * scale,
                40.0 * scale,
            ));
            let layout = EditorLayout {
                panel,
                text: text_rect,
                save,
                cancel,
                delete,
                scale,
                font_size,
            };
            self.layout = Some(layout);
            self.save_button.set_rect(save);
            self.cancel_button.set_rect(cancel);
            self.delete_button.set_rect(delete.unwrap_or_default());
            self.save_button.set_hover(self.cursor);
            self.cancel_button.set_hover(self.cursor);
            self.delete_button.set_hover(self.cursor);

            context.quad(
                Rect::new(0.0, 0.0, viewport.x, viewport.y),
                Vec4::new(0.0, 0.0, 0.0, 0.72),
            );
            context.rounded_rect(
                panel,
                12.0 * scale,
                Vec4::new(0.30, 0.35, 0.42, 1.0),
            );
            context.rounded_rect(panel.inset(scale), 11.0 * scale, EDITOR_BG);
            let (title, subtitle) = match self.session.as_ref().expect("checked above").target {
                EditorTarget::Day(date) => (
                    "Day description".to_owned(),
                    date.format("%A, %B %-d, %Y").to_string(),
                ),
                EditorTarget::Event {
                    start_date,
                    end_date,
                    ..
                } => (
                    "Event description".to_owned(),
                    if start_date == end_date {
                        start_date.format("%A, %B %-d, %Y").to_string()
                    } else {
                        format!(
                            "{}  →  {}",
                            start_date.format("%b %-d, %Y"),
                            end_date.format("%b %-d, %Y")
                        )
                    },
                ),
            };
            let _ = context.text(
                &title,
                Rect::new(
                    panel.position.x + 24.0 * scale,
                    panel.position.y + 18.0 * scale,
                    panel.size.x - 48.0 * scale,
                    36.0 * scale,
                ),
                22.0 * scale,
                TEXT,
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );
            let _ = context.text(
                &subtitle,
                Rect::new(
                    panel.position.x + 24.0 * scale,
                    panel.position.y + 60.0 * scale,
                    panel.size.x - 48.0 * scale,
                    28.0 * scale,
                ),
                14.0 * scale,
                ACCENT,
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );
            let text_state = if self.selecting {
                SurfaceState::Pressed
            } else if text_rect.contains(self.cursor) {
                SurfaceState::Hovered
            } else {
                SurfaceState::Idle
            };
            paint_surface(
                context,
                text_rect,
                8.0 * scale,
                scale,
                text_state,
                INPUT_SURFACE,
            );

            let handle = self.shape(context, layout);
            if let Some(handle) = handle {
                let (selection_rects, caret_rect) = {
                    let session = self.session.as_ref().expect("checked above");
                    let text_layout = context.text.raw.measure(handle);
                    let selection_rects = session.selection().map_or_else(Vec::new, |selection| {
                        text_layout
                            .selection(selection)
                            .into_iter()
                            .filter_map(|span| {
                                Rect::new(
                                    self.origin.x + span.x_em * layout.font_size,
                                    self.origin.y + span.y_em * layout.font_size,
                                    (span.width_em * layout.font_size).max(2.0 * scale),
                                    span.height_em * layout.font_size,
                                )
                                .intersection(text_rect.inset(2.0 * scale))
                            })
                            .collect()
                    });
                    let caret = text_layout.caret_rect(text_layout.clamp_caret(session.caret));
                    let caret_rect = Rect::new(
                        self.origin.x + caret.x_em * layout.font_size,
                        self.origin.y + caret.y_em * layout.font_size,
                        1.5 * scale,
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
                    text_rect.inset(2.0 * scale),
                );
                let phase = context.now().duration_since(self.last_input).as_millis() as u64
                    / CARET_PERIOD.as_millis() as u64;
                if phase.is_multiple_of(2)
                    && let Some(caret_rect) = caret_rect.intersection(text_rect.inset(2.0 * scale))
                {
                    context.quad(caret_rect, Vec4::new(0.95, 0.97, 1.0, 1.0));
                }
            } else {
                let _ = context.text(
                    "Write something…",
                    text_rect.inset(12.0 * scale),
                    layout.font_size,
                    Vec4::new(0.34, 0.38, 0.44, 1.0),
                    HorizontalAlign::Left,
                    VerticalAlign::Top,
                    false,
                );
                let phase = context.now().duration_since(self.last_input).as_millis() as u64
                    / CARET_PERIOD.as_millis() as u64;
                if phase.is_multiple_of(2) {
                    context.quad(
                        Rect::new(
                            text_rect.position.x + 12.0 * scale,
                            text_rect.position.y + 11.0 * scale,
                            1.5 * scale,
                            layout.font_size * 1.15,
                        ),
                        Vec4::new(0.95, 0.97, 1.0, 1.0),
                    );
                }
            }

            let footer = Rect::new(
                panel.position.x + 138.0 * scale,
                panel.bottom() - 61.0 * scale,
                cancel.position.x - panel.position.x - 150.0 * scale,
                38.0 * scale,
            );
            if let Some(error) = self
                .session
                .as_ref()
                .and_then(|session| session.error.as_deref())
            {
                let _ = context.text(
                    error,
                    footer,
                    12.5 * scale,
                    Vec4::new(1.0, 0.55, 0.58, 1.0),
                    HorizontalAlign::Left,
                    VerticalAlign::Center,
                    false,
                );
            }

            self.cancel_button
                .paint(context, "Cancel", 13.0, scale, NEUTRAL_BUTTON);
            self.save_button
                .paint(context, "Save", 13.0, scale, PRIMARY_BUTTON);
            if delete.is_some() {
                self.delete_button
                    .paint(context, "Delete", 13.0, scale, DANGER_BUTTON);
            }
            return EventFlow::Consumed;
        }

        match event {
            Event::Host(HostEvent::AboutToWait) => {
                let elapsed = context.now().duration_since(self.last_input);
                let phase = (elapsed.as_millis() / CARET_PERIOD.as_millis()) as u64;
                if phase != self.blink_phase {
                    self.blink_phase = phase;
                    context.request_redraw();
                }
                let next = self.last_input
                    + CARET_PERIOD * u32::try_from(phase + 1).unwrap_or(u32::MAX);
                context.wake_at(next);
            }
            Event::Host(HostEvent::Focused { focused: false }) => {
                self.selecting = false;
                self.save_button.cancel();
                self.cancel_button.cancel();
                self.delete_button.cancel();
                context.set_cursor(CursorIcon::Default);
                context.request_redraw();
            }
            Event::Input(InputEvent::ModifiersChanged { modifiers }) => {
                self.modifiers = *modifiers;
            }
            Event::Input(InputEvent::CursorMoved { position, .. }) => {
                self.cursor = position.as_vec2();
                let buttons_changed = self.save_button.pointer_moved(u64::MAX, self.cursor, true)
                    | self.cancel_button.pointer_moved(u64::MAX, self.cursor, true)
                    | self.delete_button.pointer_moved(u64::MAX, self.cursor, true);
                if self.selecting {
                    self.place_at_cursor(context, true);
                } else if buttons_changed {
                    context.request_redraw();
                }
                let cursor = self.layout.map_or(CursorIcon::Default, |layout| {
                    if layout.text.contains(self.cursor) {
                        CursorIcon::Text
                    } else if layout.save.contains(self.cursor)
                        || layout.cancel.contains(self.cursor)
                        || layout.delete.is_some_and(|rect| rect.contains(self.cursor))
                    {
                        CursorIcon::Pointer
                    } else {
                        CursorIcon::Default
                    }
                });
                context.set_cursor(cursor);
            }
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            }) => {
                if let Some(point) = context.input.pointer() {
                    self.cursor = point;
                }
                let Some(layout) = self.layout else {
                    return EventFlow::Consumed;
                };
                if !layout.panel.contains(self.cursor) {
                    self.close(context);
                    return EventFlow::Consumed;
                }
                if self.save_button.press(u64::MAX, self.cursor)
                    || self.cancel_button.press(u64::MAX, self.cursor)
                    || (layout.delete.is_some()
                        && self.delete_button.press(u64::MAX, self.cursor))
                {
                    context.request_redraw();
                } else if layout.text.contains(self.cursor) {
                    self.selecting = true;
                    self.place_at_cursor(context, self.modifiers.shift_key());
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
                self.selecting = false;
                context.request_redraw();
                let save = self.save_button.release(u64::MAX, self.cursor);
                let cancel = self.cancel_button.release(u64::MAX, self.cursor);
                let delete = self.delete_button.release(u64::MAX, self.cursor);
                if save == ButtonRelease::Activated {
                    self.save(context);
                } else if cancel == ButtonRelease::Activated {
                    self.close(context);
                } else if delete == ButtonRelease::Activated {
                    let target = self.session.as_ref().map(|session| session.target);
                    if let Some(EditorTarget::Event { id, .. }) = target {
                        match context.model.transact(vec![CalendarTxn::DeleteEvent { id }]) {
                            Ok(()) => self.close(context),
                            Err(error) => {
                                if let Some(session) = &mut self.session {
                                    session.error = Some(error.to_string());
                                }
                                context.request_redraw();
                            }
                        }
                    }
                } else if save != ButtonRelease::Ignored
                    || cancel != ButtonRelease::Ignored
                    || delete != ButtonRelease::Ignored
                {
                    context.request_redraw();
                }
            }
            Event::Input(InputEvent::MouseWheel { delta, .. }) => {
                if let (Some(layout), Some(session)) = (self.layout, &mut self.session)
                    && layout.text.contains(self.cursor)
                {
                    let amount = match delta {
                        ScrollDelta::Lines { delta } => delta.y * 48.0 * layout.scale,
                        ScrollDelta::Pixels { delta } => delta.y as f32,
                    };
                    session.scroll = (session.scroll - amount).max(0.0);
                    self.handle = None;
                    context.request_redraw();
                }
            }
            Event::Input(InputEvent::InputMethod(based::InputMethodEvent::Commit { text })) => {
                let Some(session) = &mut self.session else {
                    return EventFlow::Consumed;
                };
                let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
                session.insert(&normalized);
                self.handle = None;
                self.last_input = context.now();
                self.blink_phase = 0;
                self.keep_caret_visible(context, true);
                context.request_redraw();
            }
            Event::Input(InputEvent::Keyboard(key)) if key.state == ElementState::Pressed => {
                let ctrl = self.modifiers.control_key();
                let shift = self.modifiers.shift_key();
                if matches!(key.logical_key, Key::Named(NamedKey::Escape)) {
                    self.close(context);
                    return EventFlow::Consumed;
                }
                if ctrl && matches!(key.logical_key, Key::Named(NamedKey::Enter)) {
                    self.save(context);
                    return EventFlow::Consumed;
                }
                let layout = self.layout;
                let handle = layout.and_then(|layout| self.shape(context, layout));
                let mut edited = false;
                let mut moved = false;
                if let Some(session) = &mut self.session {
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
                    if let Some(motion) = motion {
                        if let Some(layout) = text_layout {
                            session.motion(layout, motion, shift);
                            moved = true;
                        }
                    } else {
                        match &key.logical_key {
                            Key::Named(NamedKey::Backspace) => {
                                if let Some(layout) = text_layout {
                                    edited = session.backspace(layout);
                                }
                            }
                            Key::Named(NamedKey::Delete) => {
                                if let Some(layout) = text_layout {
                                    edited = session.delete(layout);
                                }
                            }
                            Key::Named(NamedKey::Enter) => {
                                session.insert("\n");
                                edited = true;
                            }
                            Key::Named(NamedKey::Tab) => {
                                session.insert("    ");
                                edited = true;
                            }
                            Key::Character(character) if ctrl => match character.as_str() {
                                "a" | "A" => {
                                    session.anchor = Some(0);
                                    session.caret = Caret {
                                        byte_index: session.document.rope.len_bytes(),
                                        line_index: usize::MAX,
                                    };
                                    session.goal = None;
                                    moved = true;
                                }
                                "c" | "C" => {
                                    if let Some(selected) = session.selected_text() {
                                        let _ = arboard::Clipboard::new().and_then(
                                            |mut clipboard| clipboard.set_text(selected),
                                        );
                                    }
                                }
                                "x" | "X" => {
                                    if let Some(selected) = session.selected_text()
                                        && arboard::Clipboard::new()
                                            .and_then(|mut clipboard| clipboard.set_text(selected))
                                            .is_ok()
                                        && let Some(range) = session.selection()
                                    {
                                        session.replace(range, "");
                                        edited = true;
                                    }
                                }
                                "v" | "V" => {
                                    if let Ok(value) = arboard::Clipboard::new()
                                        .and_then(|mut clipboard| clipboard.get_text())
                                    {
                                        let value = value.replace("\r\n", "\n").replace('\r', "\n");
                                        session.insert(&value);
                                        edited = true;
                                    }
                                }
                                _ => {}
                            },
                            _ => {
                                if !ctrl
                                    && let Some(value) = &key.text
                                    && value.chars().all(|character| !character.is_control())
                                {
                                    session.insert(value);
                                    edited = true;
                                }
                            }
                        }
                    }
                }
                if edited {
                    self.handle = None;
                }
                if edited || moved {
                    self.last_input = context.now();
                    self.blink_phase = 0;
                    self.keep_caret_visible(context, edited);
                    context.request_redraw();
                }
            }
            _ => {}
        }
        EventFlow::Consumed
    }
}
