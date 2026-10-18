use crate::*;

use based::{
    CursorIcon, ElementState, Event, EventFlow, HostEvent, InputEvent, Key, MouseButton, NamedKey,
    Widget,
    glam::{Vec2, Vec4},
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

#[derive(Clone, Copy, Default)]
struct EditorLayout {
    panel: Rect,
    save: Rect,
    cancel: Rect,
    delete: Option<Rect>,
}

pub struct EditorWidget {
    session: Option<Session>,
    layout: Option<EditorLayout>,
    cursor: Vec2,
    save_button: Button<TextButton>,
    cancel_button: Button<TextButton>,
    delete_button: Button<TextButton>,
}

impl EditorWidget {
    pub fn new() -> Self {
        Self {
            session: None,
            layout: None,
            cursor: Vec2::ZERO,
            save_button: Button::new(TextButton::new("Save", 13.0, PRIMARY_BUTTON)),
            cancel_button: Button::new(TextButton::new("Cancel", 13.0, NEUTRAL_BUTTON)),
            delete_button: Button::new(TextButton::new("Delete", 13.0, DANGER_BUTTON)),
        }
    }

    pub fn is_open(&self) -> bool {
        self.session.is_some()
    }

    fn open(
        &mut self,
        context: &mut AppServices<'_>,
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
        context: &mut AppServices<'_>,
        date: NaiveDate,
        description: String,
    ) {
        self.open(context, EditorTarget::Day(date), description);
    }

    pub fn open_event(&mut self, context: &mut AppServices<'_>, event: CalendarEvent) {
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

    fn close(&mut self, context: &mut AppServices<'_>) {
        self.session = None;
        self.layout = None;
        context.window.set_ime_allowed(false);
        context.set_cursor(CursorIcon::Default);
        context.window.request_redraw();
    }

    fn save(&mut self, context: &mut AppServices<'_>) {
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

impl Widget<AppServices<'_>, AppEvent> for EditorWidget {
    fn handle_event(&mut self, context: &mut AppServices<'_>, event: &AppEvent) -> EventFlow {
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
            self.layout = Some(EditorLayout {
                panel,
                save,
                cancel,
                delete,
            });

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
            Label::new(title, 22.0, TEXT).paint(
                context,
                Rect::new(
                    panel.position.x + 24.0 * scale,
                    panel.position.y + 18.0 * scale,
                    panel.size.x - 48.0 * scale,
                    36.0 * scale,
                ),
            );
            Label::new(&subtitle, 14.0, ACCENT).paint(
                context,
                Rect::new(
                    panel.position.x + 24.0 * scale,
                    panel.position.y + 60.0 * scale,
                    panel.size.x - 48.0 * scale,
                    28.0 * scale,
                ),
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
                Label::new(error, 12.5, Vec4::new(1.0, 0.55, 0.58, 1.0))
                    .paint(context, footer);
            }

            {
                let mut button_context = ViewBinding { parent: &mut *context, rect: cancel };
                let _ = self.cancel_button.handle_event(&mut button_context, event);
            }
            let _ = std::mem::take(&mut self.cancel_button.activated);
            {
                let mut button_context = ViewBinding { parent: &mut *context, rect: save };
                let _ = self.save_button.handle_event(&mut button_context, event);
            }
            let _ = std::mem::take(&mut self.save_button.activated);
            if let Some(delete) = delete {
                let mut button_context = ViewBinding { parent: &mut *context, rect: delete };
                let _ = self.delete_button.handle_event(&mut button_context, event);
            }
            let _ = std::mem::take(&mut self.delete_button.activated);
            return EventFlow::Consumed;
        }

        let layout = self.layout.unwrap_or_default();
        let save_flow = {
            let mut button_context = ViewBinding { parent: &mut *context, rect: layout.save };
            self.save_button.handle_event(&mut button_context, event)
        };
        let save = std::mem::take(&mut self.save_button.activated);
        let cancel_flow = {
            let mut button_context = ViewBinding { parent: &mut *context, rect: layout.cancel };
            self.cancel_button.handle_event(&mut button_context, event)
        };
        let cancel = std::mem::take(&mut self.cancel_button.activated);
        let delete_flow = if let Some(rect) = layout.delete {
            let mut button_context = ViewBinding { parent: &mut *context, rect };
            self.delete_button.handle_event(&mut button_context, event)
        } else {
            EventFlow::NotConsumed
        };
        let delete = std::mem::take(&mut self.delete_button.activated);
        let buttons_consumed = save_flow == EventFlow::Consumed
            || cancel_flow == EventFlow::Consumed
            || delete_flow == EventFlow::Consumed;
        if save {
            self.save(context);
            return EventFlow::Consumed;
        }
        if cancel {
            self.close(context);
            return EventFlow::Consumed;
        }
        if delete {
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
                context.set_cursor(CursorIcon::Default);
                context.window.request_redraw();
            }
            Event::Input(InputEvent::CursorMoved { position, .. }) => {
                self.cursor = position.as_vec2();
                if let Some(session) = &mut self.session {
                    let _ = session.text.handle_event(context, event);
                }
                let delete_hovered = layout
                    .delete
                    .is_some_and(|rect| rect.contains(self.cursor));
                let cursor = if layout.save.contains(self.cursor)
                    || layout.cancel.contains(self.cursor)
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
                if !buttons_consumed
                    && let Some(session) = &mut self.session
                {
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
                if !buttons_consumed
                    && let Some(session) = &mut self.session
                {
                    let _ = session.text.handle_event(context, event);
                }
            }
            _ if buttons_consumed => {}
            _ => {
                if let Some(session) = &mut self.session {
                    let _ = session.text.handle_event(context, event);
                }
            }
        }
        EventFlow::Consumed
    }
}
