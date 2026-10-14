mod editor;
mod paint;

use std::{env, path::PathBuf, time::Duration};

use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, ModifiersState, MouseButton,
    NamedKey, ScrollDelta, TouchPhase, Widget,
    backend::winit::{self, WinitContext},
    glam::{UVec2, Vec2, Vec4},
    web_time::Instant,
    widget::{
        Input,
        sanscale::{Sanscale, raw as text},
        wgpu::{Wgpu, WgpuConfig, raw as wgpu},
        window::{Window, WindowConfig},
    },
};
use cal_based::model::{
    AppModel, CalendarEvent, CalendarTxn, EventId, StateError, assign_event_lanes,
};
use chrono::{Datelike, Days, Local, NaiveDate};
use directories::ProjectDirs;
use uuid::Uuid;

use self::{
    editor::EditorWidget,
    paint::{FrameCanvas, HorizontalAlign, Rect, Renderer, VerticalAlign},
};

const BACKGROUND: Vec4 = Vec4::new(0.035, 0.043, 0.055, 1.0);
const HEADER: Vec4 = Vec4::new(0.065, 0.078, 0.097, 1.0);
const WEEKDAY_BAR: Vec4 = Vec4::new(0.052, 0.063, 0.078, 1.0);
const CELL: Vec4 = Vec4::new(0.043, 0.052, 0.065, 1.0);
const WEEKEND: Vec4 = Vec4::new(0.038, 0.046, 0.057, 1.0);
const GRID: Vec4 = Vec4::new(0.15, 0.175, 0.21, 1.0);
const TEXT: Vec4 = Vec4::new(0.90, 0.92, 0.95, 1.0);
const MUTED: Vec4 = Vec4::new(0.49, 0.54, 0.61, 1.0);
const ACCENT: Vec4 = Vec4::new(0.40, 0.66, 0.96, 1.0);
const LONG_PRESS: Duration = Duration::from_millis(360);
const CLICK_SLOP: f32 = 7.0;
const MAX_EVENT_LANES: usize = 4;

pub struct AppContext<'a> {
    window: &'a Window,
    gpu: &'a Wgpu,
    pub input: &'a Input,
    pub model: &'a mut AppModel,
    pub(crate) text: &'a mut Sanscale,
    frame: Option<&'a mut FrameCanvas>,
    now: Instant,
    next_wake: &'a mut Option<Instant>,
}

impl AppContext<'_> {
    pub fn viewport(&self) -> UVec2 {
        self.gpu.size()
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn wake_at(&mut self, instant: Instant) {
        if self.next_wake.is_none_or(|current| instant < current) {
            *self.next_wake = Some(instant);
        }
    }

    pub fn now(&self) -> Instant {
        self.now
    }

    pub fn set_ime_allowed(&self, allowed: bool) {
        if let Some(window) = self.window.raw() {
            window.set_ime_allowed(allowed);
        }
    }

    pub fn quad(&mut self, rect: Rect, color: Vec4) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.quad(rect, color);
        }
    }

    pub fn border(&mut self, rect: Rect, width: f32, color: Vec4) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.border(rect, width, color);
        }
    }

    pub fn text(
        &mut self,
        content: &str,
        rect: Rect,
        size: f32,
        color: Vec4,
        horizontal: HorizontalAlign,
        vertical: VerticalAlign,
        wrap: bool,
    ) -> bool {
        self.frame.as_deref_mut().is_some_and(|frame| {
            frame.text(
                self.text,
                content,
                rect,
                size,
                color,
                horizontal,
                vertical,
                wrap,
            )
        })
    }

    pub fn draw_text(
        &mut self,
        handle: text::ShapedHandle,
        origin: Vec2,
        size: f32,
        color: Vec4,
        clip: Rect,
    ) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.draw_text(handle, origin, size, color, clip);
        }
    }
}

struct Ready {
    app: CalendarApp,
    model: AppModel,
    input: Input,
    text: Sanscale,
    renderer: Renderer,
    gpu: Wgpu,
}

impl Ready {
    fn handle(
        &mut self,
        window: &Window,
        frame: Option<&mut FrameCanvas>,
        event: &Event,
    ) -> (EventFlow, Option<Instant>) {
        let _ = self.input.handle_event(&mut (), event);
        if self.model.poll() {
            window.request_redraw();
        }
        let mut next_wake = None;
        let mut context = AppContext {
            window,
            gpu: &self.gpu,
            input: &self.input,
            model: &mut self.model,
            text: &mut self.text,
            frame,
            now: Instant::now(),
            next_wake: &mut next_wake,
        };
        let flow = self.app.handle_event(&mut context, event);
        (flow, next_wake)
    }

    fn redraw(
        &mut self,
        window: &Window,
        event: &Event,
    ) -> Option<(EventFlow, Option<Instant>)> {
        let mut frame = self.gpu.begin_frame()?;
        frame.clear(wgpu::Color {
            r: BACKGROUND.x as f64,
            g: BACKGROUND.y as f64,
            b: BACKGROUND.z as f64,
            a: 1.0,
        });
        self.text.set_pixel_transform(&self.gpu);
        let mut canvas = FrameCanvas::new(self.gpu.size());
        let result = self.handle(window, Some(&mut canvas), event);
        self.renderer
            .flush(&self.gpu, &mut self.text, &mut frame, canvas);
        self.gpu.present(frame);
        Some(result)
    }
}

struct Host {
    window: Window,
    ready: Option<Ready>,
    data_path: PathBuf,
    reveal_after_first_present: bool,
}

impl Host {
    fn new(data_path: PathBuf) -> Self {
        Self {
            window: Window::new(WindowConfig {
                title: "Cal · based".into(),
                size: UVec2::new(1180, 820),
                visible: false,
                ..Default::default()
            }),
            ready: None,
            data_path,
            reveal_after_first_present: true,
        }
    }

    fn initialize(&mut self) -> Result<(), String> {
        if self.ready.is_some() {
            return Ok(());
        }
        let gpu = pollster::block_on(Wgpu::new(&self.window, WgpuConfig::default()))?;
        let text = Sanscale::from_static(&gpu, include_bytes!("../assets/Roboto.ttf"))?;
        let renderer = Renderer::new(&gpu);
        let model = AppModel::open(&self.data_path).map_err(|error| error.to_string())?;
        self.ready = Some(Ready {
            app: CalendarApp::new(),
            model,
            input: Input::default(),
            text,
            renderer,
            gpu,
        });
        if let Some(window) = self.window.raw() {
            window.set_min_inner_size(Some(based::backend::winit::raw::dpi::LogicalSize::new(
                720.0, 520.0,
            )));
            window.set_ime_allowed(false);
        }
        Ok(())
    }
}

impl Widget<WinitContext<'_>, Event> for Host {
    fn handle_event(&mut self, host: &mut WinitContext<'_>, event: &Event) -> EventFlow {
        let destroying = matches!(
            event,
            Event::Host(HostEvent::TargetDestroyed | HostEvent::WindowDestroyed)
        );
        if destroying {
            let mut flow = EventFlow::NotConsumed;
            if let Some(ready) = &mut self.ready {
                let result = ready.handle(&self.window, None, event);
                flow = result.0;
                let _ = ready.text.handle_event(&mut ready.gpu, event);
                let _ = ready.gpu.handle_event(&mut self.window, event);
            }
            let _ = self.window.handle_event(host, event);
            return flow;
        }

        let _ = self.window.handle_event(host, event);
        if matches!(event, Event::Host(HostEvent::TargetCreated)) {
            if !self.window.target_available() {
                return EventFlow::NotConsumed;
            }
            if let Err(error) = self.initialize() {
                host.fail(error);
                return EventFlow::NotConsumed;
            }
        }

        let Some(ready) = &mut self.ready else {
            return EventFlow::NotConsumed;
        };
        let _ = ready.gpu.handle_event(&mut self.window, event);
        let _ = ready.text.handle_event(&mut ready.gpu, event);

        let result = if matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            ready.redraw(&self.window, event)
        } else {
            Some(ready.handle(&self.window, None, event))
        };
        let Some((flow, next_wake)) = result else {
            return EventFlow::NotConsumed;
        };

        if matches!(event, Event::Host(HostEvent::TargetCreated)) {
            let redraw = Event::Host(HostEvent::RedrawRequested);
            let _ = ready.gpu.handle_event(&mut self.window, &redraw);
            let _ = ready.text.handle_event(&mut ready.gpu, &redraw);
            let _ = ready.redraw(&self.window, &redraw);
            if self.reveal_after_first_present {
                self.window.set_visible(true);
                self.reveal_after_first_present = false;
            }
            self.window.request_redraw();
        }

        if let Some(error) = ready.gpu.take_error() {
            host.fail(error);
        } else if matches!(event, Event::Host(HostEvent::CloseRequested))
            && flow == EventFlow::NotConsumed
        {
            host.exit();
        } else if matches!(event, Event::Host(HostEvent::AboutToWait)) {
            let control = next_wake.map_or(based::LoopControl::Wait, |instant| {
                based::LoopControl::WaitUntil { instant }
            });
            winit::apply_loop_control(host.event_loop, control);
        }
        flow
    }
}

#[derive(Clone, Copy)]
enum Endpoint {
    Start,
    End,
}

#[derive(Clone, Copy)]
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
    today: Option<Rect>,
    days: Vec<DayHit>,
    events: Vec<EventHit>,
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

struct CalendarApp {
    first_week: NaiveDate,
    scroll: f32,
    pointer: PointerState,
    cursor: Vec2,
    modifiers: ModifiersState,
    hits: HitMap,
    editor: EditorWidget,
}

impl CalendarApp {
    fn new() -> Self {
        let today = Local::now().date_naive();
        Self {
            first_week: week_start(today)
                .checked_sub_days(Days::new(14))
                .unwrap_or_else(|| week_start(today)),
            scroll: 0.0,
            pointer: PointerState::Idle,
            cursor: Vec2::ZERO,
            modifiers: ModifiersState::empty(),
            hits: HitMap::default(),
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

    fn pointer_down(&mut self, context: &mut AppContext<'_>, pointer: u64, point: Vec2) {
        if self.hits.today.is_some_and(|rect| rect.contains(point)) {
            let today = Local::now().date_naive();
            self.first_week = week_start(today)
                .checked_sub_days(Days::new(14))
                .unwrap_or_else(|| week_start(today));
            self.scroll = 0.0;
            self.pointer = PointerState::Idle;
            context.request_redraw();
            return;
        }
        let endpoint = self
            .hits
            .events
            .iter()
            .rev()
            .find(|hit| hit.endpoint.is_some() && hit.rect.contains(point));
        let target = endpoint
            .map(|hit| {
                let (endpoint, fixed) = hit.endpoint.expect("filtered above");
                PressTarget::Endpoint {
                    id: hit.id,
                    endpoint,
                    fixed,
                }
            })
            .or_else(|| {
                self.hits
                    .events
                    .iter()
                    .rev()
                    .find(|hit| hit.endpoint.is_none() && hit.rect.contains(point))
                    .map(|hit| PressTarget::Event(hit.id))
            })
            .or_else(|| self.date_at(point).map(PressTarget::Day));
        if let Some(target) = target {
            self.pointer = PointerState::Pressed {
                pointer,
                target,
                started: context.now(),
                origin: point,
                current: point,
            };
            context.request_redraw();
        }
    }

    fn pointer_move(&mut self, context: &mut AppContext<'_>, pointer: u64, point: Vec2) {
        let hovered_date = self.date_at(point);
        match &mut self.pointer {
            PointerState::Pressed {
                pointer: active,
                current,
                ..
            } if *active == pointer => *current = point,
            PointerState::Creating {
                pointer: active,
                current,
                ..
            }
            | PointerState::Resizing {
                pointer: active,
                current,
                ..
            } if *active == pointer => {
                if let Some(date) = hovered_date {
                    *current = date;
                }
            }
            _ => return,
        }
        context.request_redraw();
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
                if point.distance(origin) <= CLICK_SLOP {
                    match target {
                        PressTarget::Day(date) => {
                            let description = context.model.calendar().day_description(date).to_owned();
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
                    Endpoint::Start if current > fixed => CalendarTxn::DeleteEvent { id },
                    Endpoint::End if current < fixed => CalendarTxn::DeleteEvent { id },
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
        context.request_redraw();
    }

    fn normalize_scroll(&mut self, row_height: f32) {
        while self.scroll >= row_height {
            self.scroll -= row_height;
            if let Some(next) = self.first_week.checked_add_days(Days::new(7)) {
                self.first_week = next;
            }
        }
        while self.scroll < 0.0 {
            self.scroll += row_height;
            if let Some(previous) = self.first_week.checked_sub_days(Days::new(7)) {
                self.first_week = previous;
            }
        }
    }
}

impl Widget<AppContext<'_>, Event> for CalendarApp {
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &Event) -> EventFlow {
        if matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            let viewport = context.viewport().as_vec2();
            let header_height = 68.0;
            let weekday_height = 30.0;
            let calendar_top = header_height + weekday_height;
            let row_height = (viewport.x / 7.0 * 0.92).clamp(150.0, 190.0);
            let cell_width = viewport.x / 7.0;
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
                    context.quad(rect, if column == 0 || column == 6 { WEEKEND } else { CELL });
                    context.quad(
                        Rect::new(rect.right() - 1.0, rect.position.y, 1.0, rect.size.y),
                        GRID,
                    );
                    context.quad(
                        Rect::new(rect.position.x, rect.bottom() - 1.0, rect.size.x, 1.0),
                        GRID,
                    );
                    if date == Local::now().date_naive() {
                        context.quad(
                            Rect::new(rect.position.x + 7.0, rect.position.y + 7.0, 29.0, 27.0),
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
                    let today = date == Local::now().date_naive();
                    let _ = context.text(
                        &number,
                        Rect::new(cell.position.x + 8.0, cell.position.y + 7.0, 28.0, 26.0),
                        14.0,
                        if today { BACKGROUND } else { TEXT },
                        HorizontalAlign::Center,
                        VerticalAlign::Center,
                        false,
                    );
                    if date.day() == 1 || column == 0 {
                        let month = date.format("%b").to_string().to_uppercase();
                        let _ = context.text(
                            &month,
                            Rect::new(cell.position.x + 41.0, cell.position.y + 8.0, cell_width - 48.0, 24.0),
                            10.0,
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
                                cell.position.x + 8.0,
                                cell.position.y + 37.0,
                                cell_width - 16.0,
                                (row_height - 125.0).max(18.0),
                            ),
                            11.5,
                            Vec4::new(0.73, 0.77, 0.82, 1.0),
                            HorizontalAlign::Left,
                            VerticalAlign::Top,
                            true,
                        );
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
            let strip_height = 17.0;
            let strip_pitch = 20.0;
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
                    let left_pad = if starts_here { 4.0 } else { 0.0 };
                    let right_pad = if ends_here { 4.0 } else { 0.0 };
                    let rect = Rect::new(
                        start_column * cell_width + left_pad,
                        week.rect.bottom() - 6.0 - strip_height - lane as f32 * strip_pitch,
                        ((end_column - start_column + 1.0) * cell_width - left_pad - right_pad)
                            .max(2.0),
                        strip_height,
                    );
                    let start_handle = starts_here.then_some(Rect::new(
                        rect.position.x,
                        rect.position.y - 3.0,
                        11.0,
                        rect.size.y + 6.0,
                    ));
                    let end_handle = ends_here.then_some(Rect::new(
                        rect.right() - 11.0,
                        rect.position.y - 3.0,
                        11.0,
                        rect.size.y + 6.0,
                    ));
                    strips.push(EventStrip {
                        rect,
                        id: event.id,
                        description: event.description.clone(),
                        color: if event.draft { ACCENT } else { event_color(event.id) },
                        draft: event.draft,
                        start_handle,
                        end_handle,
                        fixed_for_start: event.end_date,
                        fixed_for_end: event.start_date,
                    });
                }
            }

            for strip in &strips {
                context.quad(strip.rect, strip.color);
                if strip.draft {
                    context.border(strip.rect, 2.0, Vec4::new(0.82, 0.91, 1.0, 1.0));
                }
                if let Some(handle) = strip.start_handle {
                    context.quad(handle.inset(3.0), Vec4::new(1.0, 1.0, 1.0, 0.34));
                }
                if let Some(handle) = strip.end_handle {
                    context.quad(handle.inset(3.0), Vec4::new(1.0, 1.0, 1.0, 0.34));
                }
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
            for strip in &strips {
                if !strip.description.is_empty() {
                    let description = strip.description.lines().next().unwrap_or("");
                    let _ = context.text(
                        description,
                        strip.rect.inset(3.0),
                        10.5,
                        Vec4::ONE,
                        HorizontalAlign::Center,
                        VerticalAlign::Center,
                        false,
                    );
                }
            }

            context.quad(Rect::new(0.0, header_height, viewport.x, weekday_height), WEEKDAY_BAR);
            context.quad(Rect::new(0.0, calendar_top - 1.0, viewport.x, 1.0), GRID);
            for (column, weekday) in ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]
                .into_iter()
                .enumerate()
            {
                let _ = context.text(
                    weekday,
                    Rect::new(column as f32 * cell_width, header_height, cell_width, weekday_height),
                    10.5,
                    if column == 0 || column == 6 { MUTED } else { TEXT },
                    HorizontalAlign::Center,
                    VerticalAlign::Center,
                    false,
                );
            }

            context.quad(Rect::new(0.0, 0.0, viewport.x, header_height), HEADER);
            context.quad(Rect::new(0.0, header_height - 1.0, viewport.x, 1.0), GRID);
            let _ = context.text(
                "CAL",
                Rect::new(18.0, 8.0, 90.0, 34.0),
                24.0,
                TEXT,
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );
            let center_date = self
                .first_week
                .checked_add_days(Days::new(((rows / 2) * 7) as u64))
                .unwrap_or(self.first_week);
            let month_title = center_date.format("%B %Y").to_string();
            let _ = context.text(
                &month_title,
                Rect::new(105.0, 9.0, 260.0, 30.0),
                17.0,
                Vec4::new(0.73, 0.78, 0.85, 1.0),
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );
            let hint = context.model.last_error().map_or_else(
                || "Tap a day to write · long press and drag to make an event · long press an event end to resize".to_owned(),
                StateError::to_string,
            );
            let _ = context.text(
                &hint,
                Rect::new(19.0, 41.0, viewport.x - 150.0, 20.0),
                10.5,
                if context.model.last_error().is_some() {
                    Vec4::new(1.0, 0.55, 0.58, 1.0)
                } else {
                    MUTED
                },
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );
            let today_button = Rect::new(viewport.x - 108.0, 16.0, 88.0, 36.0);
            self.hits.today = Some(today_button);
            context.quad(today_button, Vec4::new(0.16, 0.24, 0.34, 1.0));
            context.border(today_button, 1.0, Vec4::new(0.34, 0.46, 0.61, 1.0));
            let _ = context.text(
                "TODAY",
                today_button,
                11.5,
                TEXT,
                HorizontalAlign::Center,
                VerticalAlign::Center,
                false,
            );

            let _ = self.editor.handle_event(context, event);
            return EventFlow::NotConsumed;
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
            }
            Event::Input(InputEvent::ModifiersChanged { modifiers }) => {
                self.modifiers = *modifiers;
            }
            Event::Input(InputEvent::CursorMoved { position, .. }) => {
                self.cursor = position.as_vec2();
                self.pointer_move(context, u64::MAX, self.cursor);
            }
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            }) => {
                if let Some(point) = context.input.pointer() {
                    self.cursor = point;
                }
                self.pointer_down(context, u64::MAX, self.cursor);
            }
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            }) => {
                if let Some(point) = context.input.pointer() {
                    self.cursor = point;
                }
                self.pointer_up(context, u64::MAX, self.cursor);
            }
            Event::Input(InputEvent::Touch(touch)) => {
                let point = touch.position.as_vec2();
                match touch.phase {
                    TouchPhase::Started => self.pointer_down(context, touch.id, point),
                    TouchPhase::Moved => self.pointer_move(context, touch.id, point),
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.pointer_up(context, touch.id, point)
                    }
                }
            }
            Event::Input(InputEvent::MouseWheel { delta, .. }) => {
                let amount = match delta {
                    ScrollDelta::Lines { delta } => delta.y * 52.0,
                    ScrollDelta::Pixels { delta } => delta.y as f32,
                };
                let row_height = (context.viewport().x as f32 / 7.0 * 0.92).clamp(150.0, 190.0);
                self.scroll -= amount;
                self.normalize_scroll(row_height);
                self.pointer = PointerState::Idle;
                context.request_redraw();
            }
            Event::Input(InputEvent::Keyboard(key))
                if key.state == ElementState::Pressed
                    && matches!(key.logical_key, Key::Named(NamedKey::Escape)) =>
            {
                self.pointer = PointerState::Idle;
                context.request_redraw();
            }
            Event::Host(HostEvent::Focused { focused: false }) => {
                self.pointer = PointerState::Idle;
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

fn event_color(id: EventId) -> Vec4 {
    const COLORS: [Vec4; 7] = [
        Vec4::new(0.29, 0.49, 0.72, 1.0),
        Vec4::new(0.22, 0.55, 0.50, 1.0),
        Vec4::new(0.49, 0.38, 0.67, 1.0),
        Vec4::new(0.64, 0.43, 0.23, 1.0),
        Vec4::new(0.65, 0.34, 0.43, 1.0),
        Vec4::new(0.34, 0.53, 0.31, 1.0),
        Vec4::new(0.29, 0.47, 0.57, 1.0),
    ];
    COLORS[(id.as_u128() as usize) % COLORS.len()]
}

pub fn run() -> Result<(), String> {
    let data_path = env::var_os("CAL_BASED_DATA").map(PathBuf::from).or_else(|| {
        ProjectDirs::from("dev", "xpjb", "cal-based")
            .map(|directories| directories.data_local_dir().join("calendar.json"))
    });
    let data_path = data_path.ok_or_else(|| "Windows local data directory is unavailable".to_owned())?;
    winit::run(Host::new(data_path))
}
