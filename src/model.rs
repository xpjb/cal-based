use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use based::UserEventSender;
use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub type EventId = Uuid;
pub type RequestId = u64;

pub trait StateClient {
    type Key;
    type Operation;
    type Error;

    fn request(&mut self, key: Self::Key) -> Result<RequestId, Self::Error>;
    fn transact(
        &mut self,
        operations: Vec<Self::Operation>,
    ) -> Result<RequestId, Self::Error>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayRecord {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub events: BTreeSet<EventId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: EventId,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn rgba(self) -> [f32; 4] {
        [self.r, self.g, self.b, 255].map(|channel| {
            let value = channel as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DayColors {
    pub past_weekday: RgbColor,
    pub today_weekday: RgbColor,
    pub upcoming_weekday: RgbColor,
    pub past_weekend: RgbColor,
    pub today_weekend: RgbColor,
    pub upcoming_weekend: RgbColor,
}

impl Default for DayColors {
    fn default() -> Self {
        Self {
            past_weekday: RgbColor::new(30, 35, 42),
            today_weekday: RgbColor::new(54, 91, 124),
            upcoming_weekday: RgbColor::new(46, 55, 66),
            past_weekend: RgbColor::new(21, 27, 35),
            today_weekend: RgbColor::new(45, 75, 103),
            upcoming_weekend: RgbColor::new(34, 44, 55),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarSettings {
    pub day_colors: DayColors,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarData {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    days: BTreeMap<NaiveDate, DayRecord>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    events: BTreeMap<EventId, CalendarEvent>,
    #[serde(default)]
    settings: CalendarSettings,
}

impl CalendarData {
    pub fn day(&self, date: NaiveDate) -> Option<&DayRecord> {
        self.days.get(&date)
    }

    pub fn event(&self, id: EventId) -> Option<&CalendarEvent> {
        self.events.get(&id)
    }

    pub fn apply_transactions(
        &mut self,
        operations: &[CalendarTxn],
    ) -> Result<(), StateError> {
        let mut next = self.clone();
        for operation in operations {
            match operation {
                CalendarTxn::SetDayDescription { date, description } => {
                    validate_text(description)?;
                    let day = next.days.entry(*date).or_default();
                    day.description.clone_from(description);
                    if day.description.is_empty() && day.events.is_empty() {
                        next.days.remove(date);
                    }
                }
                CalendarTxn::CreateEvent {
                    id,
                    start_date,
                    end_date,
                    description,
                } => {
                    validate_event_range(*start_date, *end_date)?;
                    validate_text(description)?;
                    if next.events.contains_key(id) {
                        return Err(StateError::DuplicateEvent(*id));
                    }
                    let event = CalendarEvent {
                        id: *id,
                        start_date: *start_date,
                        end_date: *end_date,
                        description: description.clone(),
                    };
                    next.add_memberships(&event);
                    next.events.insert(*id, event);
                }
                CalendarTxn::SetEventDescription { id, description } => {
                    validate_text(description)?;
                    let event = next
                        .events
                        .get_mut(id)
                        .ok_or(StateError::MissingEvent(*id))?;
                    event.description.clone_from(description);
                }
                CalendarTxn::SetEventStart { id, start_date } => {
                    let old = next
                        .events
                        .get(id)
                        .cloned()
                        .ok_or(StateError::MissingEvent(*id))?;
                    validate_event_range(*start_date, old.end_date)?;
                    next.remove_memberships(&old);
                    let event = next.events.get_mut(id).expect("event checked above");
                    event.start_date = *start_date;
                    let updated = event.clone();
                    next.add_memberships(&updated);
                }
                CalendarTxn::SetEventEnd { id, end_date } => {
                    let old = next
                        .events
                        .get(id)
                        .cloned()
                        .ok_or(StateError::MissingEvent(*id))?;
                    validate_event_range(old.start_date, *end_date)?;
                    next.remove_memberships(&old);
                    let event = next.events.get_mut(id).expect("event checked above");
                    event.end_date = *end_date;
                    let updated = event.clone();
                    next.add_memberships(&updated);
                }
                CalendarTxn::DeleteEvent { id } => {
                    let event = next
                        .events
                        .remove(id)
                        .ok_or(StateError::MissingEvent(*id))?;
                    next.remove_memberships(&event);
                }
                CalendarTxn::SetSettings { settings } => {
                    next.settings.clone_from(settings);
                }
            }
        }
        *self = next;
        Ok(())
    }

    fn add_memberships(&mut self, event: &CalendarEvent) {
        let mut date = event.start_date;
        loop {
            self.days.entry(date).or_default().events.insert(event.id);
            if date == event.end_date {
                break;
            }
            let Some(next) = date.checked_add_days(Days::new(1)) else {
                break;
            };
            date = next;
        }
    }

    fn remove_memberships(&mut self, event: &CalendarEvent) {
        let mut date = event.start_date;
        loop {
            if let Some(day) = self.days.get_mut(&date) {
                day.events.remove(&event.id);
                if day.description.is_empty() && day.events.is_empty() {
                    self.days.remove(&date);
                }
            }
            if date == event.end_date {
                break;
            }
            let Some(next) = date.checked_add_days(Days::new(1)) else {
                break;
            };
            date = next;
        }
    }
}

fn validate_text(text: &str) -> Result<(), StateError> {
    if text.len() > 65_535 {
        Err(StateError::DescriptionTooLong)
    } else {
        Ok(())
    }
}

fn validate_event_range(start: NaiveDate, end: NaiveDate) -> Result<(), StateError> {
    if start > end {
        return Err(StateError::InvalidEventRange { start, end });
    }
    if end.signed_duration_since(start).num_days() > 366_000 {
        return Err(StateError::EventRangeTooLong);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarTxn {
    SetDayDescription {
        date: NaiveDate,
        description: String,
    },
    CreateEvent {
        id: EventId,
        start_date: NaiveDate,
        end_date: NaiveDate,
        description: String,
    },
    SetEventDescription {
        id: EventId,
        description: String,
    },
    SetEventStart {
        id: EventId,
        start_date: NaiveDate,
    },
    SetEventEnd {
        id: EventId,
        end_date: NaiveDate,
    },
    DeleteEvent {
        id: EventId,
    },
    SetSettings {
        settings: CalendarSettings,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CalendarKey {
    Day(NaiveDate),
    Event(EventId),
    Settings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalendarValue {
    Day(Option<DayRecord>),
    Event(Option<CalendarEvent>),
    Settings(CalendarSettings),
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum StateError {
    #[error("event {0} already exists")]
    DuplicateEvent(EventId),
    #[error("event {0} does not exist")]
    MissingEvent(EventId),
    #[error("event end {end} precedes start {start}")]
    InvalidEventRange { start: NaiveDate, end: NaiveDate },
    #[error("event range is too long")]
    EventRangeTooLong,
    #[error("description exceeds 65535 UTF-8 bytes")]
    DescriptionTooLong,
    #[error("request ID space is exhausted")]
    RequestIdExhausted,
    #[error("read calendar state: {0}")]
    Read(String),
    #[error("decode calendar state: {0}")]
    Decode(String),
    #[error("write calendar state: {0}")]
    Write(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateEvent {
    Value {
        request_id: RequestId,
        key: CalendarKey,
        result: Result<CalendarValue, StateError>,
    },
}

pub struct CalendarState {
    data: CalendarData,
    path: PathBuf,
    next_request_id: RequestId,
    events: UserEventSender<StateEvent>,
}

impl CalendarState {
    pub fn open(
        path: impl Into<PathBuf>,
        events: UserEventSender<StateEvent>,
    ) -> Result<Self, StateError> {
        let path = path.into();
        let mut data = if path.exists() {
            let bytes = fs::read(&path).map_err(|error| StateError::Read(error.to_string()))?;
            serde_json::from_slice(&bytes)
                .map_err(|error| StateError::Decode(error.to_string()))?
        } else {
            CalendarData::default()
        };
        for day in data.days.values() {
            validate_text(&day.description)?;
        }
        data.days.retain(|_, day| {
            day.events.clear();
            !day.description.is_empty()
        });
        for (id, event) in &mut data.events {
            event.id = *id;
        }
        let stored_events: Vec<_> = data.events.values().cloned().collect();
        for event in stored_events {
            validate_event_range(event.start_date, event.end_date)?;
            validate_text(&event.description)?;
            data.add_memberships(&event);
        }
        Ok(Self {
            data,
            path,
            next_request_id: 1,
            events,
        })
    }

    fn next_request(&mut self) -> Result<RequestId, StateError> {
        let request_id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or(StateError::RequestIdExhausted)?;
        Ok(request_id)
    }

    fn send_value(&self, request_id: RequestId, key: CalendarKey) {
        let value = match key {
            CalendarKey::Day(date) => CalendarValue::Day(self.data.days.get(&date).cloned()),
            CalendarKey::Event(id) => CalendarValue::Event(self.data.events.get(&id).cloned()),
            CalendarKey::Settings => CalendarValue::Settings(self.data.settings.clone()),
        };
        let _ = self.events.send(StateEvent::Value {
            request_id,
            key,
            result: Ok(value),
        });
    }
}

impl StateClient for CalendarState {
    type Key = CalendarKey;
    type Operation = CalendarTxn;
    type Error = StateError;

    fn request(&mut self, key: Self::Key) -> Result<RequestId, Self::Error> {
        let request_id = self.next_request()?;
        self.send_value(request_id, key);
        Ok(request_id)
    }

    fn transact(
        &mut self,
        operations: Vec<Self::Operation>,
    ) -> Result<RequestId, Self::Error> {
        let request_id = self.next_request()?;
        let keys: BTreeSet<_> = operations
            .iter()
            .map(|operation| match operation {
                CalendarTxn::SetDayDescription { date, .. } => CalendarKey::Day(*date),
                CalendarTxn::CreateEvent { id, .. }
                | CalendarTxn::SetEventDescription { id, .. }
                | CalendarTxn::SetEventStart { id, .. }
                | CalendarTxn::SetEventEnd { id, .. }
                | CalendarTxn::DeleteEvent { id } => CalendarKey::Event(*id),
                CalendarTxn::SetSettings { .. } => CalendarKey::Settings,
            })
            .collect();
        let mut next = self.data.clone();
        next.apply_transactions(&operations)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| StateError::Write(error.to_string()))?;
        }
        let bytes = serde_json::to_vec_pretty(&next)
            .map_err(|error| StateError::Write(error.to_string()))?;
        fs::write(&self.path, bytes).map_err(|error| StateError::Write(error.to_string()))?;
        self.data = next;
        for key in keys {
            self.send_value(request_id, key);
        }
        Ok(request_id)
    }
}

pub fn assign_event_lanes<'a>(
    events: impl IntoIterator<Item = &'a CalendarEvent>,
) -> BTreeMap<EventId, usize> {
    let mut events: Vec<_> = events.into_iter().collect();
    events.sort_by_key(|event| (event.start_date, event.end_date, event.id));
    let mut lane_ends: Vec<NaiveDate> = Vec::new();
    let mut lanes = BTreeMap::new();
    for event in events {
        let lane = lane_ends
            .iter()
            .position(|end| *end < event.start_date)
            .unwrap_or(lane_ends.len());
        if lane == lane_ends.len() {
            lane_ends.push(event.end_date);
        } else {
            lane_ends[lane] = event.end_date;
        }
        lanes.insert(event.id, lane);
    }
    lanes
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn state_events() -> (UserEventSender<StateEvent>, Arc<Mutex<Vec<StateEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let sender = UserEventSender::new(move |event| {
            captured.lock().unwrap().push(event);
            Ok(())
        });
        (sender, events)
    }

    #[test]
    fn transactions_keep_day_memberships_in_sync() {
        let id = Uuid::from_u128(1);
        let mut data = CalendarData::default();
        data.apply_transactions(&[CalendarTxn::CreateEvent {
            id,
            start_date: date(10),
            end_date: date(12),
            description: "trip".into(),
        }])
        .unwrap();
        for day in 10..=12 {
            assert!(data.day(date(day)).unwrap().events.contains(&id));
        }

        data.apply_transactions(&[CalendarTxn::SetEventStart {
            id,
            start_date: date(11),
        }])
        .unwrap();
        assert!(data.day(date(10)).is_none());
        for day in 11..=12 {
            assert!(data.day(date(day)).unwrap().events.contains(&id));
        }

        data.apply_transactions(&[CalendarTxn::DeleteEvent { id }])
            .unwrap();
        assert!(data.event(id).is_none());
        assert!(data.day(date(11)).is_none());
        assert!(data.day(date(12)).is_none());
    }

    #[test]
    fn transaction_batches_are_atomic() {
        let id = Uuid::from_u128(2);
        let mut data = CalendarData::default();
        let result = data.apply_transactions(&[
            CalendarTxn::SetDayDescription {
                date: date(10),
                description: "kept out".into(),
            },
            CalendarTxn::SetEventDescription {
                id,
                description: "missing".into(),
            },
        ]);
        assert_eq!(result, Err(StateError::MissingEvent(id)));
        assert_eq!(data, CalendarData::default());
    }

    #[test]
    fn calendar_state_persists_and_sends_responses() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("calendar.json");
        let id = Uuid::from_u128(3);
        let (sender, events) = state_events();
        let mut state = CalendarState::open(&path, sender.clone()).unwrap();
        let transaction_id = state
            .transact(vec![CalendarTxn::CreateEvent {
                id,
                start_date: date(9),
                end_date: date(11),
                description: "conference".into(),
            }])
            .unwrap();
        let StateEvent::Value {
            request_id,
            key,
            result,
        } = events.lock().unwrap().pop().unwrap();
        assert_eq!(request_id, transaction_id);
        assert_eq!(key, CalendarKey::Event(id));
        assert!(matches!(
            result,
            Ok(CalendarValue::Event(Some(CalendarEvent { description, .. })))
                if description == "conference"
        ));

        let request_id = state.request(CalendarKey::Event(id)).unwrap();
        let StateEvent::Value {
            request_id: response_id,
            key,
            result,
        } = events.lock().unwrap().pop().unwrap();
        assert_eq!(response_id, request_id);
        assert_eq!(key, CalendarKey::Event(id));
        assert!(matches!(
            result,
            Ok(CalendarValue::Event(Some(CalendarEvent { description, .. })))
                if description == "conference"
        ));

        let mut settings = CalendarSettings::default();
        settings.day_colors.past_weekday = RgbColor::new(12, 34, 56);
        state
            .transact(vec![CalendarTxn::SetSettings {
                settings: settings.clone(),
            }])
            .unwrap();
        events.lock().unwrap().clear();
        state.request(CalendarKey::Settings).unwrap();
        let StateEvent::Value { result, .. } = events.lock().unwrap().pop().unwrap();
        assert_eq!(result, Ok(CalendarValue::Settings(settings.clone())));

        let mut reopened = CalendarState::open(path, sender).unwrap();
        events.lock().unwrap().clear();
        reopened.request(CalendarKey::Event(id)).unwrap();
        let StateEvent::Value { result, .. } = events.lock().unwrap().pop().unwrap();
        assert!(matches!(result, Ok(CalendarValue::Event(Some(_)))));
        reopened.request(CalendarKey::Settings).unwrap();
        let StateEvent::Value { result, .. } = events.lock().unwrap().pop().unwrap();
        assert_eq!(result, Ok(CalendarValue::Settings(settings)));
    }

    #[test]
    fn loading_rebuilds_path_memberships_from_global_events() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("calendar.json");
        let key = Uuid::from_u128(30);
        let embedded = Uuid::from_u128(31);
        fs::write(
            &path,
            format!(
                r#"{{"days":{{"2026-10-01":{{"events":["{embedded}"]}}}},"events":{{"{key}":{{"id":"{embedded}","start_date":"2026-10-10","end_date":"2026-10-11"}}}}}}"#
            ),
        )
        .unwrap();
        let (sender, events) = state_events();
        let mut state = CalendarState::open(path, sender).unwrap();
        state.request(CalendarKey::Event(key)).unwrap();
        let StateEvent::Value { result, .. } = events.lock().unwrap().pop().unwrap();
        assert!(matches!(
            result,
            Ok(CalendarValue::Event(Some(CalendarEvent { id, .. }))) if id == key
        ));
        for day in [10, 11] {
            state.request(CalendarKey::Day(date(day))).unwrap();
            let StateEvent::Value { result, .. } = events.lock().unwrap().pop().unwrap();
            assert!(matches!(
                result,
                Ok(CalendarValue::Day(Some(day))) if day.events.contains(&key)
            ));
        }
        state.request(CalendarKey::Day(date(1))).unwrap();
        let StateEvent::Value { result, .. } = events.lock().unwrap().pop().unwrap();
        assert_eq!(result, Ok(CalendarValue::Day(None)));
    }

    #[test]
    fn interval_coloring_starts_at_the_bottom_lane() {
        let events = [
            CalendarEvent {
                id: Uuid::from_u128(1),
                start_date: date(10),
                end_date: date(13),
                description: String::new(),
            },
            CalendarEvent {
                id: Uuid::from_u128(2),
                start_date: date(11),
                end_date: date(11),
                description: String::new(),
            },
            CalendarEvent {
                id: Uuid::from_u128(3),
                start_date: date(14),
                end_date: date(14),
                description: String::new(),
            },
        ];
        let lanes = assign_event_lanes(&events);
        assert_eq!(lanes[&events[0].id], 0);
        assert_eq!(lanes[&events[1].id], 1);
        assert_eq!(lanes[&events[2].id], 0);
    }
}
