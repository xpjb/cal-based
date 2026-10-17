use crate::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, NamedKey, ScrollDelta,
    TouchPhase, Widget,
    glam::{Vec2, Vec4},
    web_time::Instant,
    widget::window::CursorIcon,
};
use chrono::{Datelike, Days, Local, NaiveDate};
use uuid::Uuid;

const HEADER: Vec4 = Vec4::new(0.065, 0.078, 0.097, 1.0);
const WEEKDAY_BAR: Vec4 = Vec4::new(0.052, 0.063, 0.078, 1.0);
const LONG_PRESS: Duration = Duration::from_millis(360);
const CLICK_SLOP: f32 = 7.0;
const MOUSE_POINTER: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GestureKind {
    Mouse,
    Touch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PressTarget {
    Day(NaiveDate),
    Event(EventId),
    Endpoint {
        id: EventId,
        endpoint: Endpoint,
        fixed: NaiveDate,
    },
}

enum PointerState {
    Idle,
    Pressed {
        pointer: u64,
        target: PressTarget,
        started: Instant,
        origin: Vec2,
        current: Vec2,
        kind: GestureKind,
    },
    Creating {
        pointer: u64,
        origin: NaiveDate,
        current: NaiveDate,
    },
    Resizing {
        pointer: u64,
        id: EventId,
        endpoint: Endpoint,
        fixed: NaiveDate,
        current: NaiveDate,
    },
}

#[derive(Clone, Copy)]
struct DayHit {
    rect: Rect,
    date: NaiveDate,
}

#[derive(Default)]
struct HitMap {
    days: Vec<DayHit>,
    events: Vec<EventHit>,
}

impl HitMap {
    fn target_at(&self, point: Vec2) -> Option<PressTarget> {
        self.events
            .iter()
            .rev()
            .find_map(|hit| {
                let (endpoint, fixed) = hit.endpoint?;
                hit.rect.contains(point).then_some(PressTarget::Endpoint {
                    id: hit.id,
                    endpoint,
                    fixed,
                })
            })
            .or_else(|| {
                self.events
                    .iter()
                    .rev()
                    .find(|hit| hit.endpoint.is_none() && hit.rect.contains(point))
                    .map(|hit| PressTarget::Event(hit.id))
            })
            .or_else(|| {
                self.days
                    .iter()
                    .find(|hit| hit.rect.contains(point))
                    .map(|hit| PressTarget::Day(hit.date))
            })
    }
}

struct Navigation {
    target_week: NaiveDate,
    last_tick: Instant,
}

pub struct CalendarWidget {
    days: BTreeMap<NaiveDate, DayWidget>,
    events: BTreeMap<EventId, CalendarEvent>,
    requested_events: BTreeSet<EventId>,
    calendar_settings: CalendarSettings,
    settings_requested: bool,
    last_error: Option<StateError>,
    first_week: NaiveDate,
    scroll: f32,
    navigation: Option<Navigation>,
    pointer: PointerState,
    cursor: Vec2,
    hits: HitMap,
    today_button: Button<TextButton>,
    settings_button: Button<TextButton>,
    settings: SettingsWidget,
    editor: EditorWidget,
}

impl CalendarWidget {
    pub fn new() -> Self {
        let today = Local::now().date_naive();
        Self {
            days: BTreeMap::new(),
            events: BTreeMap::new(),
            requested_events: BTreeSet::new(),
            calendar_settings: CalendarSettings::default(),
            settings_requested: false,
            last_error: None,
            first_week: week_start(today),
            scroll: 0.0,
            navigation: None,
            pointer: PointerState::Idle,
            cursor: Vec2::ZERO,
            hits: HitMap::default(),
            today_button: Button::text("Today", 14.0, PRIMARY_BUTTON),
            settings_button: Button::text("Settings", 14.0, NEUTRAL_BUTTON),
            settings: SettingsWidget::new(),
            editor: EditorWidget::new(),
        }
    }

    fn date_at(&self, point: Vec2) -> Option<NaiveDate> {
        self.hits
            .days
            .iter()
            .find(|hit| hit.rect.contains(point))
            .map(|hit| hit.date)
    }

    fn pointer_down(
        &mut self,
        context: &mut AppContext<'_>,
        pointer: u64,
        point: Vec2,
        kind: GestureKind,
    ) {
        self.navigation = None;
        if let Some(target) = self.hits.target_at(point) {
            self.pointer = PointerState::Pressed {
                pointer,
                target,
                started: context.now,
                origin: point,
                current: point,
                kind,
            };
            context.window.request_redraw();
        }
    }

    fn pointer_move(
        &mut self,
        context: &mut AppContext<'_>,
        pointer: u64,
        point: Vec2,
        hover_capable: bool,
        button_hovered: bool,
    ) {
        let hovered_date = self.date_at(point);
        let state = std::mem::replace(&mut self.pointer, PointerState::Idle);
        self.pointer = match state {
            PointerState::Pressed {
                pointer: active,
                target,
                started,
                origin,
                kind,
                ..
            } if active == pointer => {
                let dragging = kind == GestureKind::Mouse
                    && point.distance(origin) > CLICK_SLOP * context.scale;
                if dragging {
                    match target {
                        PressTarget::Day(origin) => PointerState::Creating {
                            pointer,
                            origin,
                            current: hovered_date.unwrap_or(origin),
                        },
                        PressTarget::Endpoint {
                            id,
                            endpoint,
                            fixed,
                        } => {
                            let endpoint_date = self
                                .events
                                .get(&id)
                                .map(|event| match endpoint {
                                    Endpoint::Start => event.start_date,
                                    Endpoint::End => event.end_date,
                                })
                                .unwrap_or(fixed);
                            PointerState::Resizing {
                                pointer,
                                id,
                                endpoint,
                                fixed,
                                current: hovered_date.unwrap_or(endpoint_date),
                            }
                        }
                        PressTarget::Event(_) => PointerState::Pressed {
                            pointer,
                            target,
                            started,
                            origin,
                            current: point,
                            kind,
                        },
                    }
                } else {
                    PointerState::Pressed {
                        pointer,
                        target,
                        started,
                        origin,
                        current: point,
                        kind,
                    }
                }
            }
            PointerState::Creating {
                pointer: active,
                origin,
                current,
            } if active == pointer => PointerState::Creating {
                pointer,
                origin,
                current: hovered_date.unwrap_or(current),
            },
            PointerState::Resizing {
                pointer: active,
                id,
                endpoint,
                fixed,
                current,
            } if active == pointer => PointerState::Resizing {
                pointer,
                id,
                endpoint,
                fixed,
                current: hovered_date.unwrap_or(current),
            },
            other => other,
        };
        if hover_capable {
            let cursor = match self.pointer {
                PointerState::Creating { .. } => CursorIcon::Pointer,
                PointerState::Resizing { .. } => CursorIcon::EwResize,
                _ if button_hovered =>
                {
                    CursorIcon::Pointer
                }
                _ => match self.hits.target_at(point) {
                    Some(PressTarget::Endpoint { .. }) => CursorIcon::EwResize,
                    Some(PressTarget::Day(_) | PressTarget::Event(_)) => CursorIcon::Pointer,
                    None => CursorIcon::Default,
                },
            };
            context.set_cursor(cursor);
        }
        context.window.request_redraw();
    }

    fn pointer_up(&mut self, context: &mut AppContext<'_>, pointer: u64, point: Vec2) {
        let state = std::mem::replace(&mut self.pointer, PointerState::Idle);
        match state {
            PointerState::Pressed {
                pointer: active,
                target,
                origin,
                ..
            } if active == pointer => {
                if point.distance(origin) <= CLICK_SLOP * context.scale {
                    match target {
                        PressTarget::Day(date) => {
                            if let Some(description) = self
                                .days
                                .get(&date)
                                .and_then(DayWidget::description)
                                .map(str::to_owned)
                            {
                                self.editor.open_day(context, date, description);
                            }
                        }
                        PressTarget::Event(id) => {
                            if let Some(event) = self.events.get(&id).cloned() {
                                self.editor.open_event(context, event);
                            }
                        }
                        PressTarget::Endpoint { .. } => {}
                    }
                }
            }
            PointerState::Creating {
                pointer: active,
                origin,
                current,
            } if active == pointer => {
                let (start_date, end_date) = if origin <= current {
                    (origin, current)
                } else {
                    (current, origin)
                };
                if let Err(error) = context.state.transact(vec![CalendarTxn::CreateEvent {
                    id: Uuid::new_v4(),
                    start_date,
                    end_date,
                    description: String::new(),
                }]) {
                    self.last_error = Some(error);
                }
            }
            PointerState::Resizing {
                pointer: active,
                id,
                endpoint,
                fixed,
                current,
            } if active == pointer => {
                let operation = match endpoint {
                    Endpoint::Start if current >= fixed => CalendarTxn::DeleteEvent { id },
                    Endpoint::End if current <= fixed => CalendarTxn::DeleteEvent { id },
                    Endpoint::Start => CalendarTxn::SetEventStart {
                        id,
                        start_date: current,
                    },
                    Endpoint::End => CalendarTxn::SetEventEnd {
                        id,
                        end_date: current,
                    },
                };
                if let Err(error) = context.state.transact(vec![operation]) {
                    self.last_error = Some(error);
                }
            }
            other => self.pointer = other,
        }
        if pointer == MOUSE_POINTER {
            context.set_cursor(CursorIcon::Default);
        }
        context.window.request_redraw();
    }

    fn move_scroll(&mut self, delta: f32, row_height: f32) {
        self.scroll += delta;
        let rows = (self.scroll / row_height).floor() as i64;
        if rows == 0 {
            return;
        }
        let days = rows.unsigned_abs().saturating_mul(7);
        let next = if rows > 0 {
            self.first_week.checked_add_days(Days::new(days))
        } else {
            self.first_week.checked_sub_days(Days::new(days))
        };
        if let Some(next) = next {
            self.first_week = next;
            self.scroll -= rows as f32 * row_height;
        }
    }
}

impl Widget<AppContext<'_>, AppEvent> for CalendarWidget {
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &AppEvent) -> EventFlow {
        let viewport = context.viewport.as_vec2();
        let scale = context.scale;
        let today_button = Rect::new(
            viewport.x - 124.0 * scale,
            11.0 * scale,
            104.0 * scale,
            42.0 * scale,
        );
        let settings_button = Rect::new(
            today_button.position.x - 116.0 * scale,
            11.0 * scale,
            104.0 * scale,
            42.0 * scale,
        );
        if let Event::User(StateEvent::Value { key, result, .. }) = event {
            for day in self.days.values_mut() {
                let _ = day.handle_event(context, event);
            }
            match (key, result) {
                (CalendarKey::Event(id), Ok(CalendarValue::Event(Some(calendar_event)))) => {
                    self.requested_events.remove(id);
                    if calendar_event.id != *id {
                        self.last_error = Some(StateError::Decode(
                            "state returned an event with the wrong ID".into(),
                        ));
                    } else if let Some(stored_event) = self.events.get_mut(id) {
                        let _ = stored_event.handle_event(context, event);
                    } else {
                        self.events
                            .insert(*id, calendar_event.clone());
                    }
                }
                (CalendarKey::Event(id), Ok(CalendarValue::Event(None))) => {
                    self.requested_events.remove(id);
                    self.events.remove(id);
                }
                (CalendarKey::Event(_), Err(error)) => {
                    self.last_error = Some(error.clone());
                }
                (_, Err(error)) => self.last_error = Some(error.clone()),
                (CalendarKey::Day(_), Ok(CalendarValue::Day(_))) => {}
                (CalendarKey::Settings, Ok(CalendarValue::Settings(settings))) => {
                    self.calendar_settings = settings.clone();
                }
                _ => {
                    self.last_error = Some(StateError::Decode(
                        "state returned a value for the wrong key".into(),
                    ));
                }
            }
            let _ = self.editor.handle_event(context, event);
            let _ = self.settings.handle_event(context, event);
            context.window.request_redraw();
            return EventFlow::NotConsumed;
        }

        if matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            if let Some(point) = context.input.pointer() {
                self.cursor = point;
            }
            let header_height = 64.0 * scale;
            let weekday_height = 36.0 * scale;
            let calendar_top = header_height + weekday_height;
            let row_height = (viewport.x / 7.0 * 1.02).clamp(184.0 * scale, 228.0 * scale);
            let cell_width = viewport.x / 7.0;
            let grid_width = scale.max(1.0);
            let today = Local::now().date_naive();
            let day_colors = self.calendar_settings.day_colors;
            if let Some(navigation) = &self.navigation {
                let target_week = navigation.target_week;
                let elapsed = context.now.duration_since(navigation.last_tick);
                let weeks = target_week
                    .signed_duration_since(self.first_week)
                    .num_days() as f32
                    / 7.0;
                let error = weeks * row_height - self.scroll;
                if error.abs() <= 0.45 * scale {
                    self.first_week = target_week;
                    self.scroll = 0.0;
                    self.navigation = None;
                } else {
                    let proportion = (1.0 - (-7.5 * elapsed.as_secs_f32()).exp()).min(0.28);
                    self.move_scroll(error * proportion, row_height);
                    if let Some(navigation) = &mut self.navigation {
                        navigation.last_tick = context.now;
                    }
                    context.window.request_redraw();
                }
            }
            let rows = (((viewport.y - calendar_top + self.scroll).max(0.0) / row_height)
                .ceil() as usize)
                + 2;
            self.hits = HitMap::default();
            let mut weeks = Vec::with_capacity(rows);
            for index in 0..rows {
                let Some(start) = self
                    .first_week
                    .checked_add_days(Days::new((index * 7) as u64))
                else {
                    break;
                };
                weeks.push(WeekVisual {
                    start,
                    rect: Rect::new(
                        0.0,
                        calendar_top - self.scroll + index as f32 * row_height,
                        viewport.x,
                        row_height,
                    ),
                });
            }
            let interest_start = weeks.first().map_or(self.first_week, |week| week.start);
            let interest_end = weeks
                .last()
                .and_then(|week| week.start.checked_add_days(Days::new(6)))
                .unwrap_or(interest_start);
            self.days
                .retain(|date, _| *date >= interest_start && *date <= interest_end);
            for week in &weeks {
                for column in 0..7 {
                    let Some(date) = week.start.checked_add_days(Days::new(column as u64)) else {
                        continue;
                    };
                    if !self.days.contains_key(&date) {
                        self.days.insert(date, DayWidget::new(date));
                        if let Err(error) = context.state.request(CalendarKey::Day(date)) {
                            self.last_error = Some(error);
                        }
                    }
                }
            }

            context.quad(Rect::new(0.0, 0.0, viewport.x, viewport.y), BACKGROUND);
            for week in &weeks {
                for column in 0..7 {
                    let Some(date) = week.start.checked_add_days(Days::new(column as u64)) else {
                        continue;
                    };
                    let rect = Rect::new(
                        column as f32 * cell_width,
                        week.rect.position.y,
                        cell_width,
                        row_height,
                    );
                    if let Some(day) = self.days.get(&date)
                        && let Some(visible) = day.paint_background(
                            context,
                            rect,
                            today,
                            day_colors,
                            calendar_top,
                            scale,
                        )
                    {
                        self.hits.days.push(DayHit { rect: visible, date });
                    }
                }
            }

            let active_event_ids: BTreeSet<_> = self
                .days
                .values()
                .flat_map(DayWidget::event_ids)
                .collect();
            self.events.retain(|id, event| {
                active_event_ids.contains(id)
                    || (event.start_date <= interest_end && event.end_date >= interest_start)
            });
            self.requested_events
                .retain(|id| active_event_ids.contains(id));
            for id in active_event_ids {
                if !self.events.contains_key(&id) && self.requested_events.insert(id)
                    && let Err(error) = context.state.request(CalendarKey::Event(id))
                {
                    self.last_error = Some(error);
                }
            }
            let mut visual_events: Vec<CalendarEvent> = self
                .events
                .values()
                .filter(|event| {
                    event.start_date <= interest_end && event.end_date >= interest_start
                })
                .cloned()
                .collect();
            if let PointerState::Creating { origin, current, .. } = self.pointer {
                visual_events.push(CalendarEvent::draft(origin.min(current), origin.max(current)));
            }
            if let PointerState::Resizing {
                id,
                endpoint,
                fixed,
                current,
                ..
            } = self.pointer
                && let Some(event) = visual_events.iter_mut().find(|event| event.id == id)
            {
                match endpoint {
                    Endpoint::Start => event.start_date = current,
                    Endpoint::End => event.end_date = current,
                }
                if event.start_date > event.end_date {
                    event.start_date = fixed.min(current);
                    event.end_date = fixed.max(current);
                    event.draft = true;
                }
            }
            let lanes = assign_event_lanes(&visual_events);
            let mut strips = Vec::new();
            for event in &visual_events {
                let lane = lanes.get(&event.id).copied().unwrap_or(0);
                event.append_strips(&mut strips, &weeks, cell_width, scale, lane);
            }

            for strip in &strips {
                strip.append_hits(&mut self.hits.events);
            }

            let modal_open = self.editor.is_open() || self.settings.is_open();
            let hovered_target = (!modal_open
                && !today_button.contains(self.cursor)
                && !settings_button.contains(self.cursor))
                .then(|| self.hits.target_at(self.cursor))
                .flatten();
            let pressed_target = if modal_open {
                None
            } else {
                match &self.pointer {
                    PointerState::Pressed { target, .. } => Some(*target),
                    _ => None,
                }
            };
            let dragged_range = match &self.pointer {
                PointerState::Creating {
                    origin, current, ..
                } => Some(((*origin).min(*current), (*origin).max(*current))),
                PointerState::Resizing { id, .. } => visual_events
                    .iter()
                    .find(|event| event.id == *id)
                    .map(|event| (event.start_date, event.end_date)),
                _ => None,
            };
            for hit in &self.hits.days {
                let state = if dragged_range
                    .is_some_and(|(start, end)| hit.date >= start && hit.date <= end)
                {
                    SurfaceState::Dragging
                } else if pressed_target == Some(PressTarget::Day(hit.date)) {
                    SurfaceState::Pressed
                } else if hovered_target == Some(PressTarget::Day(hit.date)) {
                    SurfaceState::Hovered
                } else {
                    SurfaceState::Idle
                };
                if let Some(day) = self.days.get(&hit.date) {
                    day.paint_interaction(context, hit.rect, state, scale);
                }
            }

            for week in &weeks {
                for column in 0..7 {
                    let rect = Rect::new(
                        column as f32 * cell_width,
                        week.rect.position.y,
                        cell_width,
                        row_height,
                    );
                    context.quad(
                        Rect::new(
                            rect.right() - grid_width,
                            rect.position.y,
                            grid_width,
                            rect.size.y,
                        ),
                        GRID,
                    );
                    context.quad(
                        Rect::new(
                            rect.position.x,
                            rect.bottom() - grid_width,
                            rect.size.x,
                            grid_width,
                        ),
                        GRID,
                    );
                }
            }

            for week in &weeks {
                for column in 0..7 {
                    let Some(date) = week.start.checked_add_days(Days::new(column as u64)) else {
                        continue;
                    };
                    if let Some(day) = self.days.get(&date) {
                        day.paint_text(
                            context,
                            Rect::new(
                                column as f32 * cell_width,
                                week.rect.position.y,
                                cell_width,
                                row_height,
                            ),
                            today,
                            scale,
                        );
                    }
                }
            }

            for strip in &strips {
                let state = match &self.pointer {
                    PointerState::Resizing { id, .. } if *id == strip.id => {
                        SurfaceState::Dragging
                    }
                    PointerState::Pressed { target, .. }
                        if matches!(
                            target,
                            PressTarget::Event(id) | PressTarget::Endpoint { id, .. }
                                if *id == strip.id
                        ) => SurfaceState::Pressed,
                    _ if matches!(
                        hovered_target,
                        Some(PressTarget::Event(id) | PressTarget::Endpoint { id, .. })
                            if id == strip.id
                    ) => SurfaceState::Hovered,
                    _ => SurfaceState::Idle,
                };
                let handle_states = [Endpoint::Start, Endpoint::End].map(|endpoint| {
                    match &self.pointer {
                        PointerState::Resizing {
                            id,
                            endpoint: active,
                            ..
                        } if *id == strip.id && *active == endpoint => SurfaceState::Dragging,
                        PointerState::Pressed {
                            target: PressTarget::Endpoint {
                                id,
                                endpoint: active,
                                ..
                            },
                            ..
                        } if *id == strip.id && *active == endpoint => SurfaceState::Pressed,
                        _ if hovered_target.is_some_and(|target| {
                            matches!(
                                target,
                                PressTarget::Endpoint {
                                    id,
                                    endpoint: active,
                                    ..
                                } if id == strip.id && active == endpoint
                            )
                        }) => SurfaceState::Hovered,
                        _ => SurfaceState::Idle,
                    }
                });
                strip.paint_surface(context, state, handle_states, scale);
            }
            for strip in &strips {
                strip.paint_text(context, scale);
            }

            context.quad(Rect::new(0.0, header_height, viewport.x, weekday_height), WEEKDAY_BAR);
            context.quad(
                Rect::new(0.0, calendar_top - grid_width, viewport.x, grid_width),
                GRID,
            );
            for column in 1..7 {
                context.quad(
                    Rect::new(
                        column as f32 * cell_width - grid_width,
                        header_height,
                        grid_width,
                        weekday_height,
                    ),
                    GRID,
                );
            }
            for (column, weekday) in ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]
                .into_iter()
                .enumerate()
            {
                let _ = context.text(
                    weekday,
                    Rect::new(column as f32 * cell_width, header_height, cell_width, weekday_height),
                    13.0 * scale,
                    if column == 0 || column == 6 { MUTED } else { TEXT },
                    HorizontalAlign::Center,
                    VerticalAlign::Center,
                    false,
                );
            }

            context.quad(Rect::new(0.0, 0.0, viewport.x, header_height), HEADER);
            context.quad(
                Rect::new(0.0, header_height - grid_width, viewport.x, grid_width),
                GRID,
            );
            let title_week = if self.scroll >= row_height * 0.5 {
                self.first_week
                    .checked_add_days(Days::new(7))
                    .unwrap_or(self.first_week)
            } else {
                self.first_week
            };
            let title_date = title_week
                .checked_add_days(Days::new(3))
                .unwrap_or(title_week);
            let month_title = title_date.format("%B %Y").to_string();
            let _ = context.text(
                &month_title,
                Rect::new(20.0 * scale, 11.0 * scale, 320.0 * scale, 42.0 * scale),
                22.0 * scale,
                TEXT,
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );
            if let Some(error) = &self.last_error {
                let _ = context.text(
                    &error.to_string(),
                    Rect::new(
                        360.0 * scale,
                        11.0 * scale,
                        (settings_button.position.x - 380.0 * scale).max(0.0),
                        42.0 * scale,
                    ),
                    12.5 * scale,
                    Vec4::new(1.0, 0.55, 0.58, 1.0),
                    HorizontalAlign::Left,
                    VerticalAlign::Center,
                    false,
                );
            }
            let _ = self.settings_button.handle_event(context, event, settings_button, ());
            let _ = self.today_button.handle_event(context, event, today_button, ());

            let _ = self.editor.handle_event(context, event);
            let _ = self.settings.handle_event(context, event);
            return EventFlow::NotConsumed;
        }

        if self.settings.is_open() {
            return self.settings.handle_event(context, event);
        }
        if self.editor.is_open() {
            return self.editor.handle_event(context, event);
        }

        let pointer_event = context.pointer_event(event);
        let (settings_flow, settings) =
            self.settings_button.handle_event(context, event, settings_button, ());
        let (today_flow, today) =
            self.today_button.handle_event(context, event, today_button, ());
        let buttons_consumed = settings_flow == EventFlow::Consumed
            || today_flow == EventFlow::Consumed;
        if settings {
            self.settings
                .open(context, self.calendar_settings.clone());
            self.pointer = PointerState::Idle;
            context.set_cursor(CursorIcon::Default);
            return EventFlow::Consumed;
        }
        if today {
            let today = Local::now().date_naive();
            self.navigation = Some(Navigation {
                target_week: week_start(today),
                last_tick: context.now,
            });
            self.pointer = PointerState::Idle;
            context.set_cursor(CursorIcon::Default);
            context.window.request_redraw();
            return EventFlow::Consumed;
        }
        if buttons_consumed {
            self.navigation = None;
            self.pointer = PointerState::Idle;
            if !pointer_event.is_some_and(|(_, _, phase, hover)| {
                hover && phase == TouchPhase::Moved
            }) {
                return EventFlow::Consumed;
            }
        }
        if let Some((pointer, point, phase, hover)) = pointer_event {
            if hover
                && let Some(point) = point
            {
                self.cursor = point;
            }
            match (phase, point) {
                (TouchPhase::Started, Some(point)) => self.pointer_down(context, pointer, point,
                    if hover { GestureKind::Mouse } else { GestureKind::Touch }),
                (TouchPhase::Moved, Some(point)) =>
                    self.pointer_move(context, pointer, point, hover, buttons_consumed),
                (TouchPhase::Ended, Some(point)) => {
                    self.pointer_up(context, pointer, point)
                }
                (TouchPhase::Cancelled, _) => {
                    self.pointer = PointerState::Idle;
                    context.window.request_redraw();
                }
                _ => {}
            }
            return match buttons_consumed { true => EventFlow::Consumed, false => EventFlow::NotConsumed };
        }

        match event {
            Event::Host(HostEvent::TargetCreated) => {
                if !self.settings_requested {
                    self.settings_requested = true;
                    if let Err(error) = context.state.request(CalendarKey::Settings) {
                        self.last_error = Some(error);
                    }
                }
                context.window.request_redraw();
            }
            Event::Host(HostEvent::Resized { .. }) => context.window.request_redraw(),
            Event::Host(HostEvent::AboutToWait) => {
                if let PointerState::Pressed {
                    pointer,
                    target,
                    started,
                    current,
                    kind: GestureKind::Touch,
                    ..
                } = self.pointer
                {
                    let deadline = started + LONG_PRESS;
                    if context.now >= deadline {
                        match target {
                            PressTarget::Day(origin) => {
                                let current = self.date_at(current).unwrap_or(origin);
                                self.pointer = PointerState::Creating {
                                    pointer,
                                    origin,
                                    current,
                                };
                                context.window.request_redraw();
                            }
                            PressTarget::Endpoint {
                                id,
                                endpoint,
                                fixed,
                            } => {
                                let current = self.date_at(current).unwrap_or(fixed);
                                self.pointer = PointerState::Resizing {
                                    pointer,
                                    id,
                                    endpoint,
                                    fixed,
                                    current,
                                };
                                context.window.request_redraw();
                            }
                            PressTarget::Event(_) => {}
                        }
                    } else if !matches!(target, PressTarget::Event(_)) {
                        context.wake_at(deadline);
                    }
                }
                if self.navigation.is_some() {
                    context.wake_at(context.now + Duration::from_millis(16));
                    context.window.request_redraw();
                }
            }
            Event::Input(InputEvent::MouseWheel { delta, .. }) => {
                let scale = context.scale;
                let amount = match delta {
                    ScrollDelta::Lines { delta } => delta.y * 64.0 * scale,
                    ScrollDelta::Pixels { delta } => delta.y as f32,
                };
                let row_height = (context.viewport.x as f32 / 7.0 * 1.02)
                    .clamp(184.0 * scale, 228.0 * scale);
                self.navigation = None;
                self.move_scroll(-amount, row_height);
                self.pointer = PointerState::Idle;
                context.set_cursor(CursorIcon::Default);
                context.window.request_redraw();
            }
            Event::Input(InputEvent::Keyboard(key))
                if key.state == ElementState::Pressed
                    && matches!(key.logical_key, Key::Named(NamedKey::Escape)) =>
            {
                self.pointer = PointerState::Idle;
                self.navigation = None;
                context.set_cursor(CursorIcon::Default);
                context.window.request_redraw();
            }
            Event::Host(HostEvent::Focused { focused: false }) => {
                self.pointer = PointerState::Idle;
                context.set_cursor(CursorIcon::Default);
                context.window.request_redraw();
            }
            _ => {}
        }
        if buttons_consumed {
            EventFlow::Consumed
        } else {
            EventFlow::NotConsumed
        }
    }
}

fn week_start(date: NaiveDate) -> NaiveDate {
    date.checked_sub_days(Days::new(date.weekday().num_days_from_sunday() as u64))
        .unwrap_or(date)
}
