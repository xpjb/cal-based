use crate::*;

use based::{Event, EventFlow, Widget, glam::Vec4};
use chrono::{Days, NaiveDate};
use uuid::Uuid;

const MAX_LANES: usize = 4;
const DRAFT_ID: EventId = Uuid::from_u128(u128::MAX);
const HANDLE_SURFACE: SurfaceStyle = SurfaceStyle {
    fill: StateColors::new(
        Vec4::new(1.0, 1.0, 1.0, 0.30),
        Vec4::new(1.0, 1.0, 1.0, 0.76),
        Vec4::new(1.0, 1.0, 1.0, 0.92),
        Vec4::ONE,
    ),
    border: StateColors::new(Vec4::ZERO, Vec4::ZERO, Vec4::ZERO, Vec4::ZERO),
};
const COLORS: [Vec4; 7] = [
    Vec4::new(0.16, 0.34, 0.53, 1.0),
    Vec4::new(0.12, 0.39, 0.34, 1.0),
    Vec4::new(0.34, 0.24, 0.50, 1.0),
    Vec4::new(0.47, 0.29, 0.13, 1.0),
    Vec4::new(0.47, 0.21, 0.29, 1.0),
    Vec4::new(0.23, 0.39, 0.20, 1.0),
    Vec4::new(0.18, 0.35, 0.43, 1.0),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    Start,
    End,
}

pub struct WeekVisual {
    pub start: NaiveDate,
    pub rect: Rect,
}

#[derive(Clone, Copy)]
pub struct EventHit {
    pub rect: Rect,
    pub id: EventId,
    pub endpoint: Option<(Endpoint, NaiveDate)>,
}

impl CalendarEvent {
    pub fn draft(start_date: NaiveDate, end_date: NaiveDate) -> Self {
        Self {
            id: DRAFT_ID,
            start_date,
            end_date,
            description: String::new(),
            draft: true,
        }
    }

    pub fn append_strips(
        &self,
        strips: &mut Vec<EventStrip>,
        weeks: &[WeekVisual],
        cell_width: f32,
        scale: f32,
        lane: usize,
    ) {
        if lane >= MAX_LANES {
            return;
        }
        let strip_height = 24.0 * scale;
        let strip_pitch = 28.0 * scale;
        for week in weeks {
            let Some(week_end) = week.start.checked_add_days(Days::new(6)) else {
                continue;
            };
            if self.end_date < week.start || self.start_date > week_end {
                continue;
            }
            let segment_start = self.start_date.max(week.start);
            let segment_end = self.end_date.min(week_end);
            let start_column = segment_start
                .signed_duration_since(week.start)
                .num_days() as f32;
            let end_column = segment_end
                .signed_duration_since(week.start)
                .num_days() as f32;
            let starts_here = self.start_date >= week.start;
            let ends_here = self.end_date <= week_end;
            let left_pad = if starts_here { 5.0 * scale } else { 0.0 };
            let right_pad = if ends_here { 5.0 * scale } else { 0.0 };
            let rect = Rect::new(
                start_column * cell_width + left_pad,
                week.rect.bottom() - 8.0 * scale - strip_height - lane as f32 * strip_pitch,
                ((end_column - start_column + 1.0) * cell_width - left_pad - right_pad)
                    .max(2.0),
                strip_height,
            );
            strips.push(EventStrip {
                rect,
                id: self.id,
                description: self.description.clone(),
                color: if self.draft {
                    ACCENT
                } else {
                    COLORS[(self.id.as_u128() as usize) % COLORS.len()]
                },
                draft: self.draft,
                start_handle: starts_here.then_some(Rect::new(
                    rect.position.x,
                    rect.position.y - 4.0 * scale,
                    14.0 * scale,
                    rect.size.y + 8.0 * scale,
                )),
                end_handle: ends_here.then_some(Rect::new(
                    rect.right() - 14.0 * scale,
                    rect.position.y - 4.0 * scale,
                    14.0 * scale,
                    rect.size.y + 8.0 * scale,
                )),
                fixed_for_start: self.end_date,
                fixed_for_end: self.start_date,
            });
        }
    }
}

pub struct EventStrip {
    rect: Rect,
    pub id: EventId,
    description: String,
    color: Vec4,
    draft: bool,
    start_handle: Option<Rect>,
    end_handle: Option<Rect>,
    fixed_for_start: NaiveDate,
    fixed_for_end: NaiveDate,
}

impl EventStrip {
    pub fn append_hits(&self, hits: &mut Vec<EventHit>) {
        if self.draft {
            return;
        }
        hits.push(EventHit {
            rect: self.rect,
            id: self.id,
            endpoint: None,
        });
        if let Some(rect) = self.start_handle {
            hits.push(EventHit {
                rect,
                id: self.id,
                endpoint: Some((Endpoint::Start, self.fixed_for_start)),
            });
        }
        if let Some(rect) = self.end_handle {
            hits.push(EventHit {
                rect,
                id: self.id,
                endpoint: Some((Endpoint::End, self.fixed_for_end)),
            });
        }
    }

    pub fn paint_surface(
        &self,
        context: &mut AppContext<'_>,
        state: SurfaceState,
        handle_states: [SurfaceState; 2],
        scale: f32,
    ) {
        let state = if self.draft {
            SurfaceState::Dragging
        } else {
            state
        };
        let light = Vec4::new(1.0, 1.0, 1.0, self.color.w);
        let dark = Vec4::new(0.0, 0.0, 0.0, self.color.w);
        paint_surface(
            context,
            self.rect,
            7.0 * scale,
            1.5 * scale,
            state,
            SurfaceStyle {
                fill: StateColors::new(
                    self.color,
                    self.color.lerp(light, 0.14),
                    self.color.lerp(dark, 0.12),
                    self.color.lerp(light, 0.08),
                ),
                border: StateColors::new(
                    Vec4::ZERO,
                    Vec4::new(0.82, 0.91, 1.0, 0.78),
                    Vec4::new(0.91, 0.96, 1.0, 0.96),
                    Vec4::ONE,
                ),
            },
        );
        for (handle, state) in [self.start_handle, self.end_handle]
            .into_iter()
            .zip(handle_states)
        {
            if let Some(handle) = handle {
                paint_surface(
                    context,
                    handle.inset(4.0 * scale),
                    3.0 * scale,
                    0.0,
                    state,
                    HANDLE_SURFACE,
                );
            }
        }
    }

    pub fn paint_text(&self, context: &mut AppContext<'_>, scale: f32) {
        if self.description.is_empty() {
            return;
        }
        let description = self.description.lines().next().unwrap_or("");
        Label::centered(description, 13.5, Vec4::ONE)
            .paint(context, self.rect.inset(4.0 * scale));
    }
}

impl Widget<AppContext<'_>, AppEvent> for CalendarEvent {
    fn handle_event(&mut self, _context: &mut AppContext<'_>, event: &AppEvent) -> EventFlow {
        if let Event::User(StateEvent::Value {
            key: CalendarKey::Event(id),
            result: Ok(CalendarValue::Event(Some(event))),
            ..
        }) = event
            && *id == self.id
        {
            self.clone_from(event);
        }
        EventFlow::NotConsumed
    }
}
