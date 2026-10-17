use crate::*;

use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, MouseButton, NamedKey, Widget,
    glam::{Vec2, Vec4},
    widget::window::CursorIcon,
};
use chrono::NaiveDate;

const EDITOR_BG: Vec4 = Vec4::new(0.075, 0.088, 0.108, 1.0);

#[derive(Clone, Copy)]
enum EditorTarget {
    Day(NaiveDate),
    Event {
        id: EventId,
        start_date: NaiveDate,
        end_date: NaiveDate,
    },
}

struct Session {
    target: EditorTarget,
    text: Scroll<TextContent>,
    error: Option<String>,
}

#[derive(Clone, Copy)]
struct EditorLayout {
    panel: Rect,
    delete: Option<Rect>,
}

pub struct EditorWidget {
    session: Option<Session>,
    layout: Option<EditorLayout>,
    cursor: Vec2,
    save_button: Button,
    cancel_button: Button,
    delete_button: Button,
}

impl EditorWidget {
    pub fn new() -> Self {
        Self {
            session: None,
            layout: None,
            cursor: Vec2::ZERO,
            save_button: Button::default(),
            cancel_button: Button::default(),
            delete_button: Button::default(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.session.is_some()
    }

    fn open(
        &mut self,
        context: &mut AppContext<'_>,
        target: EditorTarget,
        description: String,
    ) {
        self.session = Some(Session {
            target,
            text: Scroll::new(TextContent::new(&description, context.now)),
            error: None,
        });
        self.layout = None;
        context.window.set_ime_allowed(true);
        context.window.request_redraw();
    }

    pub fn open_day(
        &mut self,
        context: &mut AppContext<'_>,
        date: NaiveDate,
        description: String,
    ) {
        self.open(context, EditorTarget::Day(date), description);
    }

    pub fn open_event(&mut self, context: &mut AppContext<'_>, event: CalendarEvent) {
        self.open(
            context,
            EditorTarget::Event {
                id: event.id,
                start_date: event.start_date,
                end_date: event.end_date,
            },
            event.description,
        );
    }

    fn close(&mut self, context: &mut AppContext<'_>) {
        self.session = None;
        self.layout = None;
        self.save_button.cancel();
        self.cancel_button.cancel();
        self.delete_button.cancel();
        context.window.set_ime_allowed(false);
        context.set_cursor(CursorIcon::Default);
        context.window.request_redraw();
    }

    fn save(&mut self, context: &mut AppContext<'_>) {
        let Some(session) = &self.session else {
            return;
        };
        let description = session.text.content();
        let operation = match session.target {
            EditorTarget::Day(date) => CalendarTxn::SetDayDescription { date, description },
            EditorTarget::Event { id, .. } => {
                CalendarTxn::SetEventDescription { id, description }
            }
        };
        match context.state.transact(vec![operation]) {
            Ok(_) => self.close(context),
            Err(error) => {
                if let Some(session) = &mut self.session {
                    session.error = Some(error.to_string());
                }
                context.window.request_redraw();
            }
        }
    }
}

impl Widget<AppContext<'_>, AppEvent> for EditorWidget {
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &AppEvent) -> EventFlow {
        if self.session.is_none() {
            return EventFlow::NotConsumed;
        }

        if matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            if let Some(point) = context.input.pointer() {
                self.cursor = point;
            }
            let viewport = context.viewport.as_vec2();
            let scale = context.scale;
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
            let target = self.session.as_ref().expect("checked above").target;
            let delete = matches!(target, EditorTarget::Event { .. }).then_some(Rect::new(
                panel.position.x + 24.0 * scale,
                save.position.y,
                100.0 * scale,
                40.0 * scale,
            ));
            self.layout = Some(EditorLayout { panel, delete });
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
            let (title, subtitle) = match target {
                EditorTarget::Day(date) => (
                    "Day description",
                    date.format("%A, %B %-d, %Y").to_string(),
                ),
                EditorTarget::Event {
                    start_date,
                    end_date,
                    ..
                } => (
                    "Event description",
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
                title,
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

            if let Some(session) = &mut self.session {
                session.text.set_rect(text_rect, scale);
                let _ = session.text.handle_event(context, event);
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

        if let Event::Input(InputEvent::Keyboard(key)) = event
            && key.state == ElementState::Pressed
        {
            if matches!(key.logical_key, Key::Named(NamedKey::Escape)) {
                self.close(context);
                return EventFlow::Consumed;
            }
            let control = self
                .session
                .as_ref()
                .is_some_and(|session| session.text.control_key());
            if control && matches!(key.logical_key, Key::Named(NamedKey::Enter)) {
                self.save(context);
                return EventFlow::Consumed;
            }
        }

        match event {
            Event::Host(HostEvent::Focused { focused: false }) => {
                if let Some(session) = &mut self.session {
                    let _ = session.text.handle_event(context, event);
                }
                self.save_button.cancel();
                self.cancel_button.cancel();
                self.delete_button.cancel();
                context.set_cursor(CursorIcon::Default);
                context.window.request_redraw();
            }
            Event::Input(InputEvent::CursorMoved { position, .. }) => {
                self.cursor = position.as_vec2();
                if let Some(session) = &mut self.session {
                    let _ = session.text.handle_event(context, event);
                }
                let buttons_changed = self.save_button.pointer_moved(u64::MAX, self.cursor, true)
                    | self.cancel_button.pointer_moved(u64::MAX, self.cursor, true)
                    | self.delete_button.pointer_moved(u64::MAX, self.cursor, true);
                if buttons_changed {
                    context.window.request_redraw();
                }
                let delete_hovered = self.layout.is_some_and(|layout| {
                    layout.delete.is_some() && self.delete_button.contains(self.cursor)
                });
                let cursor = if self.save_button.contains(self.cursor)
                    || self.cancel_button.contains(self.cursor)
                    || delete_hovered
                {
                    CursorIcon::Pointer
                } else {
                    self.session
                        .as_ref()
                        .map_or(CursorIcon::Default, |session| session.text.cursor_icon())
                };
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
                    context.window.request_redraw();
                } else if let Some(session) = &mut self.session {
                    let _ = session.text.handle_event(context, event);
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
                if let Some(session) = &mut self.session {
                    let _ = session.text.handle_event(context, event);
                }
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
                        match context.state.transact(vec![CalendarTxn::DeleteEvent { id }]) {
                            Ok(_) => self.close(context),
                            Err(error) => {
                                if let Some(session) = &mut self.session {
                                    session.error = Some(error.to_string());
                                }
                                context.window.request_redraw();
                            }
                        }
                    }
                } else if save != ButtonRelease::Ignored
                    || cancel != ButtonRelease::Ignored
                    || delete != ButtonRelease::Ignored
                {
                    context.window.request_redraw();
                }
            }
            _ => {
                if let Some(session) = &mut self.session {
                    let _ = session.text.handle_event(context, event);
                }
            }
        }
        EventFlow::Consumed
    }
}
