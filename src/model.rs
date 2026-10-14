use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub type EventId = Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericResponse<K, V, E> {
    pub request_id: u64,
    pub request: K,
    pub result: Result<V, E>,
}

pub trait LazyState {
    type Key;
    type Value;
    type Operation;
    type Error;

    fn request(&mut self, key: Self::Key) -> Result<u64, Self::Error>;
    fn transact(&mut self, operations: Vec<Self::Operation>) -> Result<(), Self::Error>;
    fn poll_response(
        &mut self,
    ) -> Option<GenericResponse<Self::Key, Self::Value, Self::Error>>;
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarData {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    days: BTreeMap<NaiveDate, DayRecord>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    events: BTreeMap<EventId, CalendarEvent>,
}

impl CalendarData {
    pub fn day(&self, date: NaiveDate) -> Option<&DayRecord> {
        self.days.get(&date)
    }

    pub fn day_description(&self, date: NaiveDate) -> &str {
        self.days
            .get(&date)
            .map_or("", |day| day.description.as_str())
    }

    pub fn event(&self, id: EventId) -> Option<&CalendarEvent> {
        self.events.get(&id)
    }

    pub fn events(&self) -> impl Iterator<Item = &CalendarEvent> {
        self.events.values()
    }

    pub fn events_between(
        &self,
        first: NaiveDate,
        last: NaiveDate,
    ) -> impl Iterator<Item = &CalendarEvent> {
        self.events
            .values()
            .filter(move |event| event.start_date <= last && event.end_date >= first)
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
            }
        }
        *self = next;
        Ok(())
    }

    pub fn replace_event_snapshot(&mut self, id: EventId, event: Option<CalendarEvent>) {
        if let Some(old) = self.events.remove(&id) {
            self.remove_memberships(&old);
        }
        if let Some(event) = event {
            self.add_memberships(&event);
            self.events.insert(id, event);
        }
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

    fn rebuild_memberships(&mut self) -> Result<(), StateError> {
        self.days.retain(|_, day| {
            day.events.clear();
            !day.description.is_empty()
        });
        let events: Vec<_> = self.events.values().cloned().collect();
        for event in events {
            validate_event_range(event.start_date, event.end_date)?;
            validate_text(&event.description)?;
            self.add_memberships(&event);
        }
        Ok(())
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
}

impl CalendarTxn {
    fn confirmation_key(&self) -> CalendarKey {
        match self {
            Self::SetDayDescription { date, .. } => CalendarKey::Day(*date),
            Self::CreateEvent { id, .. }
            | Self::SetEventDescription { id, .. }
            | Self::SetEventStart { id, .. }
            | Self::SetEventEnd { id, .. }
            | Self::DeleteEvent { id } => CalendarKey::Event(*id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CalendarKey {
    Snapshot,
    Day(NaiveDate),
    Event(EventId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalendarValue {
    Snapshot(CalendarData),
    Day(Option<DayRecord>),
    Event(Option<CalendarEvent>),
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

pub struct LocalCalendarState {
    data: CalendarData,
    path: Option<PathBuf>,
    next_request_id: u64,
    responses: VecDeque<GenericResponse<CalendarKey, CalendarValue, StateError>>,
}

impl LocalCalendarState {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StateError> {
        let path = path.into();
        let mut data = if path.exists() {
            let bytes = fs::read(&path).map_err(|error| StateError::Read(error.to_string()))?;
            serde_json::from_slice(&bytes)
                .map_err(|error| StateError::Decode(error.to_string()))?
        } else {
            CalendarData::default()
        };
        data.rebuild_memberships()?;
        Ok(Self {
            data,
            path: Some(path),
            next_request_id: 1,
            responses: VecDeque::new(),
        })
    }

    pub fn memory(data: CalendarData) -> Self {
        Self {
            data,
            path: None,
            next_request_id: 1,
            responses: VecDeque::new(),
        }
    }
}

impl LazyState for LocalCalendarState {
    type Key = CalendarKey;
    type Value = CalendarValue;
    type Operation = CalendarTxn;
    type Error = StateError;

    fn request(&mut self, key: Self::Key) -> Result<u64, Self::Error> {
        let request_id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or(StateError::RequestIdExhausted)?;
        let value = match key {
            CalendarKey::Snapshot => CalendarValue::Snapshot(self.data.clone()),
            CalendarKey::Day(date) => CalendarValue::Day(self.data.days.get(&date).cloned()),
            CalendarKey::Event(id) => CalendarValue::Event(self.data.events.get(&id).cloned()),
        };
        self.responses.push_back(GenericResponse {
            request_id,
            request: key,
            result: Ok(value),
        });
        Ok(request_id)
    }

    fn transact(&mut self, operations: Vec<Self::Operation>) -> Result<(), Self::Error> {
        let mut next = self.data.clone();
        next.apply_transactions(&operations)?;
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| StateError::Write(error.to_string()))?;
            }
            let bytes = serde_json::to_vec_pretty(&next)
                .map_err(|error| StateError::Write(error.to_string()))?;
            fs::write(path, bytes).map_err(|error| StateError::Write(error.to_string()))?;
        }
        self.data = next;
        Ok(())
    }

    fn poll_response(
        &mut self,
    ) -> Option<GenericResponse<Self::Key, Self::Value, Self::Error>> {
        self.responses.pop_front()
    }
}

pub struct AppModel {
    state: LocalCalendarState,
    calendar: CalendarData,
    latest_requests: HashMap<CalendarKey, u64>,
    loaded: bool,
    last_error: Option<StateError>,
}

impl AppModel {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateError> {
        Self::new(LocalCalendarState::open(path.as_ref())?)
    }

    pub fn memory(data: CalendarData) -> Result<Self, StateError> {
        Self::new(LocalCalendarState::memory(data))
    }

    fn new(mut state: LocalCalendarState) -> Result<Self, StateError> {
        let request = state.request(CalendarKey::Snapshot)?;
        let mut latest_requests = HashMap::new();
        latest_requests.insert(CalendarKey::Snapshot, request);
        Ok(Self {
            state,
            calendar: CalendarData::default(),
            latest_requests,
            loaded: false,
            last_error: None,
        })
    }

    pub fn calendar(&self) -> &CalendarData {
        &self.calendar
    }

    pub fn loaded(&self) -> bool {
        self.loaded
    }

    pub fn last_error(&self) -> Option<&StateError> {
        self.last_error.as_ref()
    }

    pub fn clear_error(&mut self) {
        self.last_error = None;
    }

    pub fn transact(&mut self, operations: Vec<CalendarTxn>) -> Result<(), StateError> {
        let mut predicted = self.calendar.clone();
        predicted.apply_transactions(&operations)?;
        self.state.transact(operations.clone())?;
        self.calendar = predicted;

        let keys: BTreeSet<_> = operations
            .iter()
            .map(CalendarTxn::confirmation_key)
            .collect();
        for key in keys {
            match self.state.request(key) {
                Ok(request_id) => {
                    self.latest_requests.insert(key, request_id);
                }
                Err(error) => self.last_error = Some(error),
            }
        }
        Ok(())
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Some(response) = self.state.poll_response() {
            if self.latest_requests.get(&response.request) != Some(&response.request_id) {
                continue;
            }
            match (response.request, response.result) {
                (CalendarKey::Snapshot, Ok(CalendarValue::Snapshot(calendar))) => {
                    self.calendar = calendar;
                    self.loaded = true;
                    changed = true;
                }
                (CalendarKey::Day(date), Ok(CalendarValue::Day(day))) => {
                    match day {
                        Some(day) => {
                            self.calendar.days.insert(date, day);
                        }
                        None => {
                            self.calendar.days.remove(&date);
                        }
                    }
                    changed = true;
                }
                (CalendarKey::Event(id), Ok(CalendarValue::Event(event))) => {
                    self.calendar.replace_event_snapshot(id, event);
                    changed = true;
                }
                (_, Ok(_)) => {
                    self.last_error = Some(StateError::Decode(
                        "state returned a value for the wrong key".into(),
                    ));
                }
                (_, Err(error)) => self.last_error = Some(error),
            }
        }
        changed
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
    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
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
    fn local_state_persists_and_answers_lazy_requests() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("calendar.json");
        let id = Uuid::from_u128(3);
        let mut state = LocalCalendarState::open(&path).unwrap();
        state
            .transact(vec![CalendarTxn::CreateEvent {
                id,
                start_date: date(9),
                end_date: date(11),
                description: "conference".into(),
            }])
            .unwrap();
        let request_id = state.request(CalendarKey::Event(id)).unwrap();
        let response = state.poll_response().unwrap();
        assert_eq!(response.request_id, request_id);
        assert!(matches!(
            response.result,
            Ok(CalendarValue::Event(Some(CalendarEvent { description, .. })))
                if description == "conference"
        ));

        let mut reopened = LocalCalendarState::open(path).unwrap();
        reopened.request(CalendarKey::Snapshot).unwrap();
        let response = reopened.poll_response().unwrap();
        assert!(matches!(
            response.result,
            Ok(CalendarValue::Snapshot(data)) if data.event(id).is_some()
        ));
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

    #[test]
    fn app_model_predicts_then_accepts_local_confirmation() {
        let mut model = AppModel::memory(CalendarData::default()).unwrap();
        assert!(model.poll());
        let id = Uuid::from_u128(4);
        model
            .transact(vec![CalendarTxn::CreateEvent {
                id,
                start_date: date(10),
                end_date: date(12),
                description: String::new(),
            }])
            .unwrap();
        assert!(model.calendar().event(id).is_some());
        assert!(model.poll());
        assert!(model.calendar().event(id).is_some());
    }
}
