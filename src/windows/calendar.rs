use std::time::Duration;

use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, MouseButton,
    NamedKey, ScrollDelta, TouchPhase, Widget,
    backend::winit::raw::window::CursorIcon,
    glam::{Vec2, Vec4},
    web_time::Instant,
};
use cal_based::model::{CalendarEvent, CalendarTxn, EventId, assign_event_lanes};
use chrono::{Datelike, Days, Local, NaiveDate};
use uuid::Uuid;

use super::{
    AppContext,
    editor::EditorWidget,
    paint::{HorizontalAlign, Rect, VerticalAlign},
    settings::SettingsWidget,
    ui::{
        ACCENT, BACKGROUND, Button, ButtonRelease, GRID, MUTED, NEUTRAL_BUTTON, PRIMARY_BUTTON,
        StateColors, SurfaceState, SurfaceStyle, TEXT, paint_surface,
    },
};

const HEADER: Vec4 = Vec4::new(0.065, 0.078, 0.097, 1.0);
const WEEKDAY_BAR: Vec4 = Vec4::new(0.052, 0.063, 0.078, 1.0);
const LONG_PRESS: Duration = Duration::from_millis(360);
const CLICK_SLOP: f32 = 7.0;
const MAX_EVENT_LANES: usize = 4;
const MOUSE_POINTER: u64 = u64::MAX;
const DAY_SURFACE: SurfaceStyle = SurfaceStyle {
    fill: StateColors::new(
        Vec4::ZERO,
        Vec4::new(0.40, 0.66, 0.96, 0.025),
        Vec4::new(0.40, 0.66, 0.96, 0.07),
        Vec4::new(0.40, 0.66, 0.96, 0.10),
    ),
    border: StateColors::new(
        Vec4::ZERO,
        Vec4::ZERO,
        Vec4::new(0.54, 0.76, 1.0, 0.52),
        Vec4::new(0.64, 0.82, 1.0, 0.82),
    ),
};
const HANDLE_SURFACE: SurfaceStyle = SurfaceStyle {
    fill: StateColors::new(
        Vec4::new(1.0, 1.0, 1.0, 0.30),
        Vec4::new(1.0, 1.0, 1.0, 0.76),
        Vec4::new(1.0, 1.0, 1.0, 0.92),
        Vec4::ONE,
    ),
    border: StateColors::new(Vec4::ZERO, Vec4::ZERO, Vec4::ZERO, Vec4::ZERO),
};
const EVENT_COLORS: [Vec4; 7] = [
    Vec4::new(0.16, 0.34, 0.53, 1.0),
    Vec4::new(0.12, 0.39, 0.34, 1.0),
    Vec4::new(0.34, 0.24, 0.50, 1.0),
    Vec4::new(0.47, 0.29, 0.13, 1.0),
    Vec4::new(0.47, 0.21, 0.29, 1.0),
    Vec4::new(0.23, 0.39, 0.20, 1.0),
    Vec4::new(0.18, 0.35, 0.43, 1.0),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GestureKind {
    Mouse,
    Touch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Endpoint {
    Start,
    End,
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

#[derive(Clone, Copy)]
struct EventHit {
    rect: Rect,
    id: EventId,
    endpoint: Option<(Endpoint, NaiveDate)>,
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

struct WeekVisual {
    start: NaiveDate,
    rect: Rect,
}

struct EventVisual {
    id: EventId,
    start_date: NaiveDate,
    end_date: NaiveDate,
    description: String,
    draft: bool,
}

struct EventStrip {
    rect: Rect,
    id: EventId,
    description: String,
    color: Vec4,
    draft: bool,
    start_handle: Option<Rect>,
    end_handle: Option<Rect>,
    fixed_for_start: NaiveDate,
    fixed_for_end: NaiveDate,
}

struct Navigation {
    target_week: NaiveDate,
    last_tick: Instant,
}

pub struct CalendarWidget {
    first_week: NaiveDate,
    scroll: f32,
    navigation: Option<Navigation>,
    pointer: PointerState,
    cursor: Vec2,
    hits: HitMap,
    today_button: Button,
    settings_button: Button,
    settings: SettingsWidget,
    editor: EditorWidget,
}

impl CalendarWidget {
    pub fn new() -> Self {
        let today = Local::now().date_naive();
        Self {
            first_week: week_start(today),
            scroll: 0.0,
            navigation: None,
            pointer: PointerState::Idle,
            cursor: Vec2::ZERO,
            hits: HitMap::default(),
            today_button: Button::default(),
            settings_button: Button::default(),
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
        if self.settings_button.press(pointer, point)
            || self.today_button.press(pointer, point)
        {
            self.navigation = None;
            self.pointer = PointerState::Idle;
            context.request_redraw();
            return;
        }
        self.navigation = None;
        if let Some(target) = self.hits.target_at(point) {
            self.pointer = PointerState::Pressed {
                pointer,
                target,
                started: context.now(),
                origin: point,
                current: point,
                kind,
            };
            context.request_redraw();
        }
    }

    fn pointer_move(
        &mut self,
        context: &mut AppContext<'_>,
        pointer: u64,
        point: Vec2,
        hover_capable: bool,
    ) {
        self.today_button
            .pointer_moved(pointer, point, hover_capable);
        self.settings_button
            .pointer_moved(pointer, point, hover_capable);
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
                    && point.distance(origin) > CLICK_SLOP * context.scale_factor();
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
                            let endpoint_date = context
                                .model
                                .calendar()
                                .event(id)
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
                _ if self.today_button.contains(point)
                    || self.settings_button.contains(point) =>
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
        context.request_redraw();
    }

    fn pointer_up(&mut self, context: &mut AppContext<'_>, pointer: u64, point: Vec2) {
        let settings = self.settings_button.release(pointer, point);
        let today_button = self.today_button.release(pointer, point);
        if settings == ButtonRelease::Activated {
            self.settings.open(context);
            self.pointer = PointerState::Idle;
            if pointer == MOUSE_POINTER {
                context.set_cursor(CursorIcon::Default);
            }
            context.request_redraw();
            return;
        }
        if today_button == ButtonRelease::Activated {
            let today = Local::now().date_naive();
            self.navigation = Some(Navigation {
                target_week: week_start(today),
                last_tick: context.now(),
            });
            self.pointer = PointerState::Idle;
            if pointer == MOUSE_POINTER {
                context.set_cursor(CursorIcon::Default);
            }
            context.request_redraw();
            return;
        }
        if settings != ButtonRelease::Ignored || today_button != ButtonRelease::Ignored {
            if pointer == MOUSE_POINTER {
                context.set_cursor(CursorIcon::Default);
            }
            context.request_redraw();
            return;
        }
        let state = std::mem::replace(&mut self.pointer, PointerState::Idle);
        match state {
            PointerState::Pressed {
                pointer: active,
                target,
                origin,
                ..
            } if active == pointer => {
                if point.distance(origin) <= CLICK_SLOP * context.scale_factor() {
                    match target {
                        PressTarget::Day(date) => {
                            let description = context
                                .model
                                .calendar()
                                .day_description(date)
                                .to_owned();
                            self.editor.open_day(context, date, description);
                        }
                        PressTarget::Event(id) | PressTarget::Endpoint { id, .. } => {
                            if let Some(event) = context.model.calendar().event(id).cloned() {
                                self.editor.open_event(context, event);
                            }
                        }
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
                if let Err(error) = context.model.transact(vec![CalendarTxn::CreateEvent {
                    id: Uuid::new_v4(),
                    start_date,
                    end_date,
                    description: String::new(),
                }]) {
                    eprintln!("create event: {error}");
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
                if let Err(error) = context.model.transact(vec![operation]) {
                    eprintln!("resize event: {error}");
                }
            }
            other => self.pointer = other,
        }
        if pointer == MOUSE_POINTER {
            context.set_cursor(CursorIcon::Default);
        }
        context.request_redraw();
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

impl Widget<AppContext<'_>, Event> for CalendarWidget {
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &Event) -> EventFlow {
        if matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            if let Some(point) = context.input.pointer() {
                self.cursor = point;
            }
            let viewport = context.viewport().as_vec2();
            let scale = context.scale_factor();
            let header_height = 64.0 * scale;
            let weekday_height = 36.0 * scale;
            let calendar_top = header_height + weekday_height;
            let row_height = (viewport.x / 7.0 * 1.02).clamp(184.0 * scale, 228.0 * scale);
            let cell_width = viewport.x / 7.0;
            let grid_width = scale.max(1.0);
            let today = Local::now().date_naive();
            let day_colors = context.model.calendar().settings().day_colors;
            self.today_button.set_rect(Rect::new(
                viewport.x - 124.0 * scale,
                11.0 * scale,
                104.0 * scale,
                42.0 * scale,
            ));
            self.settings_button.set_rect(Rect::new(
                self.today_button.rect().position.x - 116.0 * scale,
                11.0 * scale,
                104.0 * scale,
                42.0 * scale,
            ));
            self.today_button.set_hover(self.cursor);
            self.settings_button.set_hover(self.cursor);

            if let Some(navigation) = &self.navigation {
                let target_week = navigation.target_week;
                let elapsed = context.now().duration_since(navigation.last_tick);
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
                        navigation.last_tick = context.now();
                    }
                    context.request_redraw();
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
                    let weekend = column == 0 || column == 6;
                    let color = if date < today {
                        if weekend {
                            day_colors.past_weekend
                        } else {
                            day_colors.past_weekday
                        }
                    } else if date == today {
                        if weekend {
                            day_colors.today_weekend
                        } else {
                            day_colors.today_weekday
                        }
                    } else if weekend {
                        day_colors.upcoming_weekend
                    } else {
                        day_colors.upcoming_weekday
                    };
                    context.quad(rect, Vec4::from_array(color.rgba()));
                    if date == today {
                        context.rounded_rect(
                            Rect::new(
                                rect.position.x + 8.0 * scale,
                                rect.position.y + 8.0 * scale,
                                36.0 * scale,
                                34.0 * scale,
                            ),
                            8.0 * scale,
                            ACCENT,
                        );
                    }
                    let visible = Rect::new(
                        rect.position.x,
                        rect.position.y.max(calendar_top),
                        rect.size.x,
                        (rect.bottom() - rect.position.y.max(calendar_top)).max(0.0),
                    );
                    if visible.size.y > 0.0 {
                        self.hits.days.push(DayHit { rect: visible, date });
                    }
                }
            }

            let first_visible = weeks.first().map_or(self.first_week, |week| week.start);
            let last_visible = weeks
                .last()
                .and_then(|week| week.start.checked_add_days(Days::new(6)))
                .unwrap_or(first_visible);
            let mut visual_events: Vec<EventVisual> = context
                .model
                .calendar()
                .events_between(first_visible, last_visible)
                .cloned()
                .map(|event| EventVisual {
                    id: event.id,
                    start_date: event.start_date,
                    end_date: event.end_date,
                    description: event.description,
                    draft: false,
                })
                .collect();
            if let PointerState::Creating { origin, current, .. } = self.pointer {
                visual_events.push(EventVisual {
                    id: Uuid::from_u128(u128::MAX),
                    start_date: origin.min(current),
                    end_date: origin.max(current),
                    description: String::new(),
                    draft: true,
                });
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
            let lane_events: Vec<_> = visual_events
                .iter()
                .map(|event| CalendarEvent {
                    id: event.id,
                    start_date: event.start_date,
                    end_date: event.end_date,
                    description: event.description.clone(),
                })
                .collect();
            let lanes = assign_event_lanes(&lane_events);
            let strip_height = 24.0 * scale;
            let strip_pitch = 28.0 * scale;
            let mut strips = Vec::new();
            for event in &visual_events {
                let lane = lanes.get(&event.id).copied().unwrap_or(0);
                if lane >= MAX_EVENT_LANES {
                    continue;
                }
                for week in &weeks {
                    let Some(week_end) = week.start.checked_add_days(Days::new(6)) else {
                        continue;
                    };
                    if event.end_date < week.start || event.start_date > week_end {
                        continue;
                    }
                    let segment_start = event.start_date.max(week.start);
                    let segment_end = event.end_date.min(week_end);
                    let start_column = segment_start
                        .signed_duration_since(week.start)
                        .num_days() as f32;
                    let end_column = segment_end
                        .signed_duration_since(week.start)
                        .num_days() as f32;
                    let starts_here = event.start_date >= week.start;
                    let ends_here = event.end_date <= week_end;
                    let left_pad = if starts_here { 5.0 * scale } else { 0.0 };
                    let right_pad = if ends_here { 5.0 * scale } else { 0.0 };
                    let rect = Rect::new(
                        start_column * cell_width + left_pad,
                        week.rect.bottom()
                            - 8.0 * scale
                            - strip_height
                            - lane as f32 * strip_pitch,
                        ((end_column - start_column + 1.0) * cell_width - left_pad - right_pad)
                            .max(2.0),
                        strip_height,
                    );
                    let start_handle = starts_here.then_some(Rect::new(
                        rect.position.x,
                        rect.position.y - 4.0 * scale,
                        14.0 * scale,
                        rect.size.y + 8.0 * scale,
                    ));
                    let end_handle = ends_here.then_some(Rect::new(
                        rect.right() - 14.0 * scale,
                        rect.position.y - 4.0 * scale,
                        14.0 * scale,
                        rect.size.y + 8.0 * scale,
                    ));
                    strips.push(EventStrip {
                        rect,
                        id: event.id,
                        description: event.description.clone(),
                        color: if event.draft {
                            ACCENT
                        } else {
                            EVENT_COLORS[(event.id.as_u128() as usize) % EVENT_COLORS.len()]
                        },
                        draft: event.draft,
                        start_handle,
                        end_handle,
                        fixed_for_start: event.end_date,
                        fixed_for_end: event.start_date,
                    });
                }
            }

            for strip in &strips {
                if !strip.draft {
                    self.hits.events.push(EventHit {
                        rect: strip.rect,
                        id: strip.id,
                        endpoint: None,
                    });
                    if let Some(rect) = strip.start_handle {
                        self.hits.events.push(EventHit {
                            rect,
                            id: strip.id,
                            endpoint: Some((Endpoint::Start, strip.fixed_for_start)),
                        });
                    }
                    if let Some(rect) = strip.end_handle {
                        self.hits.events.push(EventHit {
                            rect,
                            id: strip.id,
                            endpoint: Some((Endpoint::End, strip.fixed_for_end)),
                        });
                    }
                }
            }

            let modal_open = self.editor.is_open() || self.settings.is_open();
            let hovered_target = (!modal_open
                && !self.today_button.contains(self.cursor)
                && !self.settings_button.contains(self.cursor))
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
                if state != SurfaceState::Idle {
                    paint_surface(
                        context,
                        hit.rect,
                        0.0,
                        if state == SurfaceState::Hovered {
                            0.0
                        } else {
                            2.0 * scale
                        },
                        state,
                        DAY_SURFACE,
                    );
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
                    let cell = Rect::new(
                        column as f32 * cell_width,
                        week.rect.position.y,
                        cell_width,
                        row_height,
                    );
                    let number = date.day().to_string();
                    let is_today = date == today;
                    let _ = context.text(
                        &number,
                        Rect::new(
                            cell.position.x + 8.0 * scale,
                            cell.position.y + 8.0 * scale,
                            36.0 * scale,
                            34.0 * scale,
                        ),
                        18.0 * scale,
                        if is_today { BACKGROUND } else { TEXT },
                        HorizontalAlign::Center,
                        VerticalAlign::Center,
                        false,
                    );
                    if date.day() == 1 || column == 0 {
                        let month = date.format("%b").to_string().to_uppercase();
                        let _ = context.text(
                            &month,
                            Rect::new(
                                cell.position.x + 50.0 * scale,
                                cell.position.y + 10.0 * scale,
                                cell_width - 58.0 * scale,
                                28.0 * scale,
                            ),
                            12.5 * scale,
                            MUTED,
                            HorizontalAlign::Left,
                            VerticalAlign::Center,
                            false,
                        );
                    }
                    let description = context.model.calendar().day_description(date).to_owned();
                    if !description.is_empty() {
                        let _ = context.text(
                            &description,
                            Rect::new(
                                cell.position.x + 9.0 * scale,
                                cell.position.y + 48.0 * scale,
                                cell_width - 18.0 * scale,
                                (row_height - 158.0 * scale).max(22.0 * scale),
                            ),
                            14.0 * scale,
                            Vec4::new(0.73, 0.77, 0.82, 1.0),
                            HorizontalAlign::Left,
                            VerticalAlign::Top,
                            true,
                        );
                    }
                }
            }

            for strip in &strips {
                let pointer_state = match &self.pointer {
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
                let state = if strip.draft {
                    SurfaceState::Dragging
                } else {
                    pointer_state
                };
                let light = Vec4::new(1.0, 1.0, 1.0, strip.color.w);
                let dark = Vec4::new(0.0, 0.0, 0.0, strip.color.w);
                paint_surface(
                    context,
                    strip.rect,
                    7.0 * scale,
                    1.5 * scale,
                    state,
                    SurfaceStyle {
                        fill: StateColors::new(
                            strip.color,
                            strip.color.lerp(light, 0.14),
                            strip.color.lerp(dark, 0.12),
                            strip.color.lerp(light, 0.08),
                        ),
                        border: StateColors::new(
                            Vec4::ZERO,
                            Vec4::new(0.82, 0.91, 1.0, 0.78),
                            Vec4::new(0.91, 0.96, 1.0, 0.96),
                            Vec4::ONE,
                        ),
                    },
                );
                for (endpoint, handle) in [
                    (Endpoint::Start, strip.start_handle),
                    (Endpoint::End, strip.end_handle),
                ] {
                    let Some(handle) = handle else {
                        continue;
                    };
                    let handle_state = match &self.pointer {
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
                    };
                    paint_surface(
                        context,
                        handle.inset(4.0 * scale),
                        3.0 * scale,
                        0.0,
                        handle_state,
                        HANDLE_SURFACE,
                    );
                }
            }
            for strip in &strips {
                if !strip.description.is_empty() {
                    let description = strip.description.lines().next().unwrap_or("");
                    let _ = context.text(
                        description,
                        strip.rect.inset(4.0 * scale),
                        13.5 * scale,
                        Vec4::ONE,
                        HorizontalAlign::Center,
                        VerticalAlign::Center,
                        false,
                    );
                }
            }

            context.quad(Rect::new(0.0, header_height, viewport.x, weekday_height), WEEKDAY_BAR);
            context.quad(
                Rect::new(0.0, calendar_top - grid_width, viewport.x, grid_width),
                GRID,
            );
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
            if let Some(error) = context.model.last_error() {
                let _ = context.text(
                    &error.to_string(),
                    Rect::new(
                        360.0 * scale,
                        11.0 * scale,
                        (self.settings_button.rect().position.x - 380.0 * scale).max(0.0),
                        42.0 * scale,
                    ),
                    12.5 * scale,
                    Vec4::new(1.0, 0.55, 0.58, 1.0),
                    HorizontalAlign::Left,
                    VerticalAlign::Center,
                    false,
                );
            }
            self.settings_button
                .paint(context, "Settings", 14.0, scale, NEUTRAL_BUTTON);
            self.today_button
                .paint(context, "Today", 14.0, scale, PRIMARY_BUTTON);

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

        match event {
            Event::Host(HostEvent::TargetCreated | HostEvent::Resized { .. }) => {
                context.request_redraw();
            }
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
                    if context.now() >= deadline {
                        match target {
                            PressTarget::Day(origin) => {
                                let current = self.date_at(current).unwrap_or(origin);
                                self.pointer = PointerState::Creating {
                                    pointer,
                                    origin,
                                    current,
                                };
                                context.request_redraw();
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
                                context.request_redraw();
                            }
                            PressTarget::Event(_) => {}
                        }
                    } else if !matches!(target, PressTarget::Event(_)) {
                        context.wake_at(deadline);
                    }
                }
                if self.navigation.is_some() {
                    context.wake_at(context.now() + Duration::from_millis(16));
                    context.request_redraw();
                }
            }
            Event::Input(InputEvent::CursorMoved { position, .. }) => {
                self.cursor = position.as_vec2();
                self.pointer_move(context, MOUSE_POINTER, self.cursor, true);
            }
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            }) => {
                if let Some(point) = context.input.pointer() {
                    self.cursor = point;
                }
                self.pointer_down(context, MOUSE_POINTER, self.cursor, GestureKind::Mouse);
            }
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            }) => {
                if let Some(point) = context.input.pointer() {
                    self.cursor = point;
                }
                self.pointer_up(context, MOUSE_POINTER, self.cursor);
            }
            Event::Input(InputEvent::Touch(touch)) => {
                let point = touch.position.as_vec2();
                match touch.phase {
                    TouchPhase::Started => {
                        self.pointer_down(context, touch.id, point, GestureKind::Touch)
                    }
                    TouchPhase::Moved => self.pointer_move(context, touch.id, point, false),
                    TouchPhase::Ended => self.pointer_up(context, touch.id, point),
                    TouchPhase::Cancelled => {
                        self.pointer = PointerState::Idle;
                        self.today_button.cancel();
                        self.settings_button.cancel();
                        context.request_redraw();
                    }
                }
            }
            Event::Input(InputEvent::MouseWheel { delta, .. }) => {
                let scale = context.scale_factor();
                let amount = match delta {
                    ScrollDelta::Lines { delta } => delta.y * 64.0 * scale,
                    ScrollDelta::Pixels { delta } => delta.y as f32,
                };
                let row_height = (context.viewport().x as f32 / 7.0 * 1.02)
                    .clamp(184.0 * scale, 228.0 * scale);
                self.navigation = None;
                self.move_scroll(-amount, row_height);
                self.pointer = PointerState::Idle;
                self.today_button.cancel();
                self.settings_button.cancel();
                context.set_cursor(CursorIcon::Default);
                context.request_redraw();
            }
            Event::Input(InputEvent::Keyboard(key))
                if key.state == ElementState::Pressed
                    && matches!(key.logical_key, Key::Named(NamedKey::Escape)) =>
            {
                self.pointer = PointerState::Idle;
                self.navigation = None;
                self.today_button.cancel();
                self.settings_button.cancel();
                context.set_cursor(CursorIcon::Default);
                context.request_redraw();
            }
            Event::Host(HostEvent::Focused { focused: false }) => {
                self.pointer = PointerState::Idle;
                self.today_button.cancel();
                self.settings_button.cancel();
                context.set_cursor(CursorIcon::Default);
                context.request_redraw();
            }
            _ => {}
        }
        EventFlow::NotConsumed
    }
}

fn week_start(date: NaiveDate) -> NaiveDate {
    date.checked_sub_days(Days::new(date.weekday().num_days_from_sunday() as u64))
        .unwrap_or(date)
}
