use crate::*;

use based::{Event, EventFlow, Widget, glam::Vec4};
use chrono::{Datelike, NaiveDate, Weekday};

const SURFACE: SurfaceStyle = SurfaceStyle {
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

pub struct DayWidget {
    date: NaiveDate,
    record: Option<DayRecord>,
}

impl DayWidget {
    pub fn new(date: NaiveDate) -> Self {
        Self { date, record: None }
    }

    pub fn description(&self) -> Option<&str> {
        self.record
            .as_ref()
            .map(|record| record.description.as_str())
    }

    pub fn event_ids(&self) -> impl Iterator<Item = EventId> + '_ {
        self.record
            .iter()
            .flat_map(|record| record.events.iter().copied())
    }

    pub fn paint_background(
        &self,
        context: &mut AppContext<'_>,
        rect: Rect,
        today: NaiveDate,
        colors: DayColors,
        calendar_top: f32,
        scale: f32,
    ) -> Option<Rect> {
        let weekend = matches!(self.date.weekday(), Weekday::Sun | Weekday::Sat);
        let color = if self.date < today {
            if weekend {
                colors.past_weekend
            } else {
                colors.past_weekday
            }
        } else if self.date == today {
            if weekend {
                colors.today_weekend
            } else {
                colors.today_weekday
            }
        } else if weekend {
            colors.upcoming_weekend
        } else {
            colors.upcoming_weekday
        };
        context.quad(rect, Vec4::from_array(color.rgba()));
        if self.date == today {
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
        let top = rect.position.y.max(calendar_top);
        let visible = Rect::new(
            rect.position.x,
            top,
            rect.size.x,
            (rect.bottom() - top).max(0.0),
        );
        (visible.size.y > 0.0).then_some(visible)
    }

    pub fn paint_interaction(
        &self,
        context: &mut AppContext<'_>,
        rect: Rect,
        state: SurfaceState,
        scale: f32,
    ) {
        if state != SurfaceState::Idle {
            paint_surface(
                context,
                rect,
                0.0,
                if state == SurfaceState::Hovered {
                    0.0
                } else {
                    2.0 * scale
                },
                state,
                SURFACE,
            );
        }
    }

    pub fn paint_text(
        &self,
        context: &mut AppContext<'_>,
        cell: Rect,
        today: NaiveDate,
        scale: f32,
    ) {
        let _ = context.text(
            &self.date.day().to_string(),
            Rect::new(
                cell.position.x + 8.0 * scale,
                cell.position.y + 8.0 * scale,
                36.0 * scale,
                34.0 * scale,
            ),
            18.0 * scale,
            if self.date == today { BACKGROUND } else { TEXT },
            HorizontalAlign::Center,
            VerticalAlign::Center,
            false,
        );
        if self.date.day() == 1 || self.date.weekday() == Weekday::Sun {
            let month = self.date.format("%b").to_string().to_uppercase();
            let _ = context.text(
                &month,
                Rect::new(
                    cell.position.x + 50.0 * scale,
                    cell.position.y + 10.0 * scale,
                    cell.size.x - 58.0 * scale,
                    28.0 * scale,
                ),
                12.5 * scale,
                MUTED,
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );
        }
        let Some(description) = self.description().filter(|description| !description.is_empty())
        else {
            return;
        };
        let _ = context.text(
            description,
            Rect::new(
                cell.position.x + 9.0 * scale,
                cell.position.y + 48.0 * scale,
                cell.size.x - 18.0 * scale,
                (cell.size.y - 158.0 * scale).max(22.0 * scale),
            ),
            14.0 * scale,
            Vec4::new(0.73, 0.77, 0.82, 1.0),
            HorizontalAlign::Left,
            VerticalAlign::Top,
            true,
        );
    }
}

impl Widget<AppContext<'_>, AppEvent> for DayWidget {
    fn handle_event(&mut self, _context: &mut AppContext<'_>, event: &AppEvent) -> EventFlow {
        let Event::User(UserEvent::State(StateEvent::Value { key, result, .. })) = event else {
            return EventFlow::NotConsumed;
        };
        match (key, result) {
            (CalendarKey::Day(date), Ok(CalendarValue::Day(record))) if *date == self.date => {
                self.record = Some(record.clone().unwrap_or_default());
            }
            (CalendarKey::Event(id), Ok(CalendarValue::Event(event))) => {
                if let Some(record) = &mut self.record {
                    if event.as_ref().is_some_and(|event| {
                        event.start_date <= self.date && event.end_date >= self.date
                    }) {
                        record.events.insert(*id);
                    } else {
                        record.events.remove(id);
                    }
                }
            }
            _ => {}
        }
        EventFlow::NotConsumed
    }
}
