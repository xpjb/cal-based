use std::{ops::Range, time::Duration};

use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, ModifiersState, MouseButton,
    NamedKey, ScrollDelta, Widget,
    glam::{Vec2, Vec4},
    widget::sanscale::raw::{self as text, Caret, Motion, WordBoundaries},
};
use cal_based::model::{CalendarEvent, CalendarTxn, EventId};
use chrono::NaiveDate;
use ropey::Rope;

use super::{
    ACCENT, AppContext, GRID, MUTED, TEXT,
    paint::{HorizontalAlign, Rect, VerticalAlign},
};

const FONT_SIZE: f32 = 16.0;
const CARET_PERIOD: Duration = Duration::from_millis(530);
const EDITOR_BG: Vec4 = Vec4::new(0.075, 0.088, 0.108, 1.0);
const INPUT_BG: Vec4 = Vec4::new(0.031, 0.038, 0.049, 1.0);
const SELECTION: Vec4 = Vec4::new(0.17, 0.36, 0.68, 0.62);

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
        self.goal = None;
    }
}

#[derive(Clone, Copy)]
struct EditorLayout {
    text: Rect,
    save: Rect,
    cancel: Rect,
    delete: Option<Rect>,
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
        context.set_ime_allowed(false);
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
            self.origin = layout.text.position + Vec2::new(12.0, 11.0 - session.scroll);
            return None;
        }
        let content = session.document.rope.to_string();
        let mut style = context.text.style;
        style.wrap_em = Some(((layout.text.size.x - 24.0) / FONT_SIZE).max(2.0));
        style.align = text::Align::Left;
        style.line_spacing = 1.18;
        let handle = context.text.raw.shape_transient(&content, &style)?;
        self.handle = Some(handle);
        self.origin = layout.text.position + Vec2::new(12.0, 11.0 - session.scroll);
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
                (self.cursor.x - self.origin.x) / FONT_SIZE,
                (self.cursor.y - self.origin.y) / FONT_SIZE,
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
        self.last_input = context.now();
        self.blink_phase = 0;
        context.request_redraw();
    }

    fn keep_caret_visible(&mut self, context: &mut AppContext<'_>) {
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
        session.caret = text_layout.clamp_caret(session.caret);
        let caret = text_layout.caret_rect(session.caret);
        let top = 11.0 + caret.y_em * FONT_SIZE - session.scroll;
        let bottom = top + caret.height_em.max(1.0) * FONT_SIZE;
        if top < 8.0 {
            session.scroll = (session.scroll - (8.0 - top)).max(0.0);
        } else if bottom > layout.text.size.y - 8.0 {
            session.scroll += bottom - (layout.text.size.y - 8.0);
        }
    }
}

impl Widget<AppContext<'_>, Event> for EditorWidget {
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &Event) -> EventFlow {
        if self.session.is_none() {
            return EventFlow::NotConsumed;
        }

        if matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            let viewport = context.viewport().as_vec2();
            let panel_width = (viewport.x - 48.0).min(720.0).max(420.0);
            let panel_height = (viewport.y - 48.0).min(570.0).max(380.0);
            let panel = Rect::new(
                (viewport.x - panel_width) * 0.5,
                (viewport.y - panel_height) * 0.5,
                panel_width,
                panel_height,
            );
            let text_rect = Rect::new(
                panel.position.x + 24.0,
                panel.position.y + 92.0,
                panel.size.x - 48.0,
                panel.size.y - 164.0,
            );
            let save = Rect::new(panel.right() - 112.0, panel.bottom() - 54.0, 88.0, 34.0);
            let cancel = Rect::new(save.position.x - 98.0, save.position.y, 88.0, 34.0);
            let delete = matches!(
                self.session.as_ref().map(|session| session.target),
                Some(EditorTarget::Event { .. })
            )
            .then_some(Rect::new(panel.position.x + 24.0, save.position.y, 88.0, 34.0));
            let layout = EditorLayout {
                text: text_rect,
                save,
                cancel,
                delete,
            };
            self.layout = Some(layout);

            context.quad(
                Rect::new(0.0, 0.0, viewport.x, viewport.y),
                Vec4::new(0.0, 0.0, 0.0, 0.72),
            );
            context.quad(panel, EDITOR_BG);
            context.border(panel, 1.0, Vec4::new(0.30, 0.35, 0.42, 1.0));
            let (title, subtitle) = match self.session.as_ref().expect("checked above").target {
                EditorTarget::Day(date) => (
                    "DAY DESCRIPTION".to_owned(),
                    date.format("%A, %B %-d, %Y").to_string(),
                ),
                EditorTarget::Event {
                    start_date,
                    end_date,
                    ..
                } => (
                    "EVENT DESCRIPTION".to_owned(),
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
                Rect::new(panel.position.x + 24.0, panel.position.y + 18.0, panel.size.x - 48.0, 30.0),
                19.0,
                TEXT,
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );
            let _ = context.text(
                &subtitle,
                Rect::new(panel.position.x + 24.0, panel.position.y + 51.0, panel.size.x - 48.0, 24.0),
                12.0,
                ACCENT,
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );
            context.quad(text_rect, INPUT_BG);
            context.border(text_rect, 1.0, GRID);

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
                                    self.origin.x + span.x_em * FONT_SIZE,
                                    self.origin.y + span.y_em * FONT_SIZE,
                                    (span.width_em * FONT_SIZE).max(2.0),
                                    span.height_em * FONT_SIZE,
                                )
                                .intersection(text_rect.inset(2.0))
                            })
                            .collect()
                    });
                    let caret = text_layout.caret_rect(text_layout.clamp_caret(session.caret));
                    let caret_rect = Rect::new(
                        self.origin.x + caret.x_em * FONT_SIZE,
                        self.origin.y + caret.y_em * FONT_SIZE,
                        1.5,
                        caret.height_em.max(1.0) * FONT_SIZE,
                    );
                    (selection_rects, caret_rect)
                };
                for rect in selection_rects {
                    context.quad(rect, SELECTION);
                }
                context.draw_text(
                    handle,
                    self.origin,
                    FONT_SIZE,
                    Vec4::new(0.86, 0.88, 0.91, 1.0),
                    text_rect.inset(2.0),
                );
                let phase = context.now().duration_since(self.last_input).as_millis() as u64
                    / CARET_PERIOD.as_millis() as u64;
                if phase.is_multiple_of(2)
                    && let Some(caret_rect) = caret_rect.intersection(text_rect.inset(2.0))
                {
                    context.quad(caret_rect, Vec4::new(0.95, 0.97, 1.0, 1.0));
                }
            } else {
                let _ = context.text(
                    "Write something…",
                    text_rect.inset(12.0),
                    FONT_SIZE,
                    Vec4::new(0.34, 0.38, 0.44, 1.0),
                    HorizontalAlign::Left,
                    VerticalAlign::Top,
                    false,
                );
                let phase = context.now().duration_since(self.last_input).as_millis() as u64
                    / CARET_PERIOD.as_millis() as u64;
                if phase.is_multiple_of(2) {
                    context.quad(
                        Rect::new(text_rect.position.x + 12.0, text_rect.position.y + 11.0, 1.5, FONT_SIZE * 1.15),
                        Vec4::new(0.95, 0.97, 1.0, 1.0),
                    );
                }
            }

            if let Some(error) = self
                .session
                .as_ref()
                .and_then(|session| session.error.as_deref())
            {
                let _ = context.text(
                    error,
                    Rect::new(panel.position.x + 122.0, panel.bottom() - 53.0, cancel.position.x - panel.position.x - 132.0, 32.0),
                    10.5,
                    Vec4::new(1.0, 0.55, 0.58, 1.0),
                    HorizontalAlign::Left,
                    VerticalAlign::Center,
                    false,
                );
            } else {
                let _ = context.text(
                    "Ctrl+Enter saves · Esc cancels",
                    Rect::new(panel.position.x + 122.0, panel.bottom() - 53.0, cancel.position.x - panel.position.x - 132.0, 32.0),
                    10.5,
                    MUTED,
                    HorizontalAlign::Left,
                    VerticalAlign::Center,
                    false,
                );
            }

            for (rect, label, fill) in [
                (cancel, "CANCEL", Vec4::new(0.13, 0.15, 0.18, 1.0)),
                (save, "SAVE", Vec4::new(0.25, 0.47, 0.72, 1.0)),
            ] {
                context.quad(rect, fill);
                context.border(rect, 1.0, Vec4::new(0.32, 0.39, 0.49, 1.0));
                let _ = context.text(
                    label,
                    rect,
                    11.0,
                    TEXT,
                    HorizontalAlign::Center,
                    VerticalAlign::Center,
                    false,
                );
            }
            if let Some(delete) = delete {
                context.quad(delete, Vec4::new(0.35, 0.16, 0.20, 1.0));
                context.border(delete, 1.0, Vec4::new(0.57, 0.29, 0.35, 1.0));
                let _ = context.text(
                    "DELETE",
                    delete,
                    11.0,
                    Vec4::new(1.0, 0.82, 0.84, 1.0),
                    HorizontalAlign::Center,
                    VerticalAlign::Center,
                    false,
                );
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
            Event::Host(HostEvent::Focused { focused: false }) => self.selecting = false,
            Event::Input(InputEvent::ModifiersChanged { modifiers }) => {
                self.modifiers = *modifiers;
            }
            Event::Input(InputEvent::CursorMoved { position, .. }) => {
                self.cursor = position.as_vec2();
                if self.selecting {
                    self.place_at_cursor(context, true);
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
                let Some(layout) = self.layout else {
                    return EventFlow::Consumed;
                };
                if layout.save.contains(self.cursor) {
                    self.save(context);
                } else if layout.cancel.contains(self.cursor) {
                    self.close(context);
                } else if layout.delete.is_some_and(|rect| rect.contains(self.cursor)) {
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
                } else if layout.text.contains(self.cursor) {
                    self.selecting = true;
                    self.place_at_cursor(context, self.modifiers.shift_key());
                }
            }
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            }) => self.selecting = false,
            Event::Input(InputEvent::MouseWheel { delta, .. }) => {
                if let (Some(layout), Some(session)) = (self.layout, &mut self.session)
                    && layout.text.contains(self.cursor)
                {
                    let amount = match delta {
                        ScrollDelta::Lines { delta } => delta.y * 42.0,
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
                let range = session
                    .selection()
                    .unwrap_or(session.caret.byte_index..session.caret.byte_index);
                let at = range.start;
                let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
                session.document.replace(range, &normalized);
                session.caret.byte_index = at + normalized.len();
                session.anchor = None;
                session.goal = None;
                self.handle = None;
                self.last_input = context.now();
                self.blink_phase = 0;
                self.keep_caret_visible(context);
                context.request_redraw();
            }
            Event::Input(InputEvent::Keyboard(key)) if key.state == ElementState::Pressed => {
                let ctrl = self.modifiers.control_key();
                let shift = self.modifiers.shift_key();
                if matches!(key.logical_key, Key::Named(NamedKey::Escape)) {
                    self.close(context);
                    return EventFlow::Consumed;
                }
                if ctrl
                    && matches!(key.logical_key, Key::Named(NamedKey::Enter))
                {
                    self.save(context);
                    return EventFlow::Consumed;
                }
                let layout = self.layout;
                let handle = layout.and_then(|layout| self.shape(context, layout));
                let mut edited = false;
                let mut moved = false;
                if let Some(session) = &mut self.session {
                    let text_layout = handle.map(|handle| context.text.raw.measure(handle));
                    match &key.logical_key {
                        Key::Named(NamedKey::ArrowLeft) if ctrl => {
                            if let Some(layout) = text_layout {
                                let caret = layout.caret_move(
                                    session.caret,
                                    Motion::WordLeft,
                                    &mut session.goal,
                                    &session.document,
                                );
                                session.place(caret, shift);
                                moved = true;
                            }
                        }
                        Key::Named(NamedKey::ArrowRight) if ctrl => {
                            if let Some(layout) = text_layout {
                                let caret = layout.caret_move(
                                    session.caret,
                                    Motion::WordRight,
                                    &mut session.goal,
                                    &session.document,
                                );
                                session.place(caret, shift);
                                moved = true;
                            }
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            if let Some(layout) = text_layout {
                                let caret = layout.caret_move(
                                    session.caret,
                                    Motion::Left,
                                    &mut session.goal,
                                    &session.document,
                                );
                                session.place(caret, shift);
                                moved = true;
                            }
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            if let Some(layout) = text_layout {
                                let caret = layout.caret_move(
                                    session.caret,
                                    Motion::Right,
                                    &mut session.goal,
                                    &session.document,
                                );
                                session.place(caret, shift);
                                moved = true;
                            }
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            if let Some(layout) = text_layout {
                                let caret = layout.caret_move(
                                    session.caret,
                                    Motion::Up,
                                    &mut session.goal,
                                    &session.document,
                                );
                                session.place(caret, shift);
                                moved = true;
                            }
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            if let Some(layout) = text_layout {
                                let caret = layout.caret_move(
                                    session.caret,
                                    Motion::Down,
                                    &mut session.goal,
                                    &session.document,
                                );
                                session.place(caret, shift);
                                moved = true;
                            }
                        }
                        Key::Named(NamedKey::Home) if ctrl => {
                            if let Some(layout) = text_layout {
                                let caret = layout.caret_move(
                                    session.caret,
                                    Motion::DocStart,
                                    &mut session.goal,
                                    &session.document,
                                );
                                session.place(caret, shift);
                                moved = true;
                            }
                        }
                        Key::Named(NamedKey::End) if ctrl => {
                            if let Some(layout) = text_layout {
                                let caret = layout.caret_move(
                                    session.caret,
                                    Motion::DocEnd,
                                    &mut session.goal,
                                    &session.document,
                                );
                                session.place(caret, shift);
                                moved = true;
                            }
                        }
                        Key::Named(NamedKey::Home) => {
                            if let Some(layout) = text_layout {
                                let caret = layout.caret_move(
                                    session.caret,
                                    Motion::Home,
                                    &mut session.goal,
                                    &session.document,
                                );
                                session.place(caret, shift);
                                moved = true;
                            }
                        }
                        Key::Named(NamedKey::End) => {
                            if let Some(layout) = text_layout {
                                let caret = layout.caret_move(
                                    session.caret,
                                    Motion::End,
                                    &mut session.goal,
                                    &session.document,
                                );
                                session.place(caret, shift);
                                moved = true;
                            }
                        }
                        Key::Named(NamedKey::Backspace) => {
                            let range = session.selection().or_else(|| {
                                text_layout.and_then(|layout| {
                                    layout
                                        .prev_caret_stop(session.caret.byte_index)
                                        .map(|start| start..session.caret.byte_index)
                                })
                            });
                            if let Some(range) = range {
                                let at = range.start;
                                session.document.replace(range, "");
                                session.caret.byte_index = at;
                                session.anchor = None;
                                session.goal = None;
                                edited = true;
                            }
                        }
                        Key::Named(NamedKey::Delete) => {
                            let range = session.selection().or_else(|| {
                                text_layout.and_then(|layout| {
                                    layout
                                        .next_caret_stop(session.caret.byte_index)
                                        .map(|end| session.caret.byte_index..end)
                                })
                            });
                            if let Some(range) = range {
                                let at = range.start;
                                session.document.replace(range, "");
                                session.caret.byte_index = at;
                                session.anchor = None;
                                session.goal = None;
                                edited = true;
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            let range = session
                                .selection()
                                .unwrap_or(session.caret.byte_index..session.caret.byte_index);
                            let at = range.start;
                            session.document.replace(range, "\n");
                            session.caret.byte_index = at + 1;
                            session.anchor = None;
                            session.goal = None;
                            edited = true;
                        }
                        Key::Named(NamedKey::Tab) => {
                            let range = session
                                .selection()
                                .unwrap_or(session.caret.byte_index..session.caret.byte_index);
                            let at = range.start;
                            session.document.replace(range, "    ");
                            session.caret.byte_index = at + 4;
                            session.anchor = None;
                            session.goal = None;
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
                                if let Some(range) = session.selection() {
                                    let chars = session.document.rope.byte_to_char(range.start)
                                        ..session.document.rope.byte_to_char(range.end);
                                    let selected = session.document.rope.slice(chars).to_string();
                                    let _ = arboard::Clipboard::new()
                                        .and_then(|mut clipboard| clipboard.set_text(selected));
                                }
                            }
                            "x" | "X" => {
                                if let Some(range) = session.selection() {
                                    let chars = session.document.rope.byte_to_char(range.start)
                                        ..session.document.rope.byte_to_char(range.end);
                                    let selected = session.document.rope.slice(chars).to_string();
                                    if arboard::Clipboard::new()
                                        .and_then(|mut clipboard| clipboard.set_text(selected))
                                        .is_ok()
                                    {
                                        let at = range.start;
                                        session.document.replace(range, "");
                                        session.caret.byte_index = at;
                                        session.anchor = None;
                                        session.goal = None;
                                        edited = true;
                                    }
                                }
                            }
                            "v" | "V" => {
                                if let Ok(value) = arboard::Clipboard::new()
                                    .and_then(|mut clipboard| clipboard.get_text())
                                {
                                    let value = value.replace("\r\n", "\n").replace('\r', "\n");
                                    let range = session.selection().unwrap_or(
                                        session.caret.byte_index..session.caret.byte_index,
                                    );
                                    let at = range.start;
                                    session.document.replace(range, &value);
                                    session.caret.byte_index = at + value.len();
                                    session.anchor = None;
                                    session.goal = None;
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
                                let range = session.selection().unwrap_or(
                                    session.caret.byte_index..session.caret.byte_index,
                                );
                                let at = range.start;
                                session.document.replace(range, value);
                                session.caret.byte_index = at + value.len();
                                session.anchor = None;
                                session.goal = None;
                                edited = true;
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
                    self.keep_caret_visible(context);
                    context.request_redraw();
                }
            }
            _ => {}
        }
        EventFlow::Consumed
    }
}
