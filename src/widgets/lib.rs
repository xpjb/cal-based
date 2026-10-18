mod button;
mod calendar;
mod day;
mod editor;
mod event;
mod input;
mod label;
mod paint;
mod sanscale;
mod scroll;
mod settings;
mod text_area;
mod text_button;
mod text_edit;
mod ui;

pub use button::*;
pub use calendar::*;
pub use day::*;
pub use editor::*;
pub use event::*;
pub use input::*;
pub use label::*;
pub use paint::*;
pub use sanscale::*;
pub use scroll::*;
pub use settings::*;
pub use text_area::*;
pub use text_button::*;
pub use ui::*;

use crate::*;
#[cfg(not(target_arch = "wasm32"))]
use std::{env, path::PathBuf};

use based::{
    BasedOptions, CursorIcon, ElementState, Event, EventFlow, Host, HostEvent, InputEvent,
    MouseButton, TouchPhase, Waker, Widget, Window, WindowOptions,
    glam::{UVec2, Vec2, Vec4},
    web_time::Instant,
    widget::wgpu::{Wgpu, WgpuConfig, raw as wgpu},
};
use sanscale::raw as text;
#[cfg(not(target_arch = "wasm32"))]
use directories::ProjectDirs;

pub type AppEvent = Event<StateEvent>;

pub struct AppContext<'a> {
    window: &'a Window,
    input: &'a Input,
    state: &'a mut dyn StateClient<
        Key = CalendarKey,
        Operation = CalendarTxn,
        Error = StateError,
    >,
    text: &'a mut Sanscale,
    viewport: UVec2,
    scale: f32,
    now: Instant,
    clip: Rect,
    frame: Option<&'a mut FrameCanvas>,
    next_wake: &'a mut Option<Instant>,
}

impl AppContext<'_> {
    fn pointer_event(&self, event: &AppEvent) -> Option<(u64, Option<Vec2>, TouchPhase, bool)> {
        Some(match event {
            Event::Input(InputEvent::CursorMoved { position, .. }) =>
                (u64::MAX, Some(position.as_vec2()), TouchPhase::Moved, true),
            Event::Input(InputEvent::CursorLeft { .. }) =>
                (u64::MAX, None, TouchPhase::Cancelled, true),
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state,
                ..
            }) => (
                u64::MAX,
                self.input.pointer(),
                if *state == ElementState::Pressed { TouchPhase::Started } else { TouchPhase::Ended },
                true,
            ),
            Event::Input(InputEvent::Touch(touch)) =>
                (touch.id, Some(touch.position.as_vec2()), touch.phase, false),
            _ => return None,
        })
    }

    fn wake_at(&mut self, instant: Instant) {
        if self.next_wake.is_none_or(|current| instant < current) {
            *self.next_wake = Some(instant);
        }
    }

    fn set_cursor(&self, cursor: CursorIcon) {
        self.window.set_cursor(cursor);
    }

    fn hit_test(&self, rect: Rect, point: Vec2) -> bool {
        self.clip.contains(point) && rect.contains(point)
    }

    fn quad(&mut self, rect: Rect, color: Vec4) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.quad(rect, color, self.clip);
        }
    }

    fn rounded_rect(&mut self, rect: Rect, radius: f32, color: Vec4) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.rounded_rect(rect, radius, color, self.clip);
        }
    }

    fn surface(
        &mut self,
        rect: Rect,
        radius: f32,
        border_width: f32,
        fill: Vec4,
        border: Vec4,
    ) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.surface(rect, radius, border_width, fill, border, self.clip);
        }
    }

    fn gradient(&mut self, rect: Rect, colors: [Vec4; 4]) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.gradient(rect, colors, self.clip);
        }
    }

    fn draw_text(
        &mut self,
        handle: text::ShapedHandle,
        origin: Vec2,
        size: f32,
        color: Vec4,
        clip: Rect,
    ) {
        if let Some(clip) = clip.intersection(self.clip)
            && let Some(frame) = self.frame.as_deref_mut()
        {
            frame.draw_text(handle, origin, size, color, clip);
        }
    }
}

struct App {
    calendar: CalendarWidget,
    state: CalendarState,
    input: Input,
    text: Sanscale,
    renderer: Renderer,
    gpu: Wgpu,
    window: Window,
    wake: Waker,
}

impl App {
    fn dispatch(
        &mut self,
        frame: Option<&mut FrameCanvas>,
        event: &AppEvent,
    ) -> (EventFlow, Option<Instant>) {
        let _ = self.input.handle_event(&mut (), event);
        let mut next_wake = None;
        let viewport = self.gpu.size();
        let mut context = AppContext {
            window: &self.window,
            input: &self.input,
            state: &mut self.state,
            text: &mut self.text,
            viewport,
            scale: (self.window.scale_factor() as f32).clamp(1.0, 3.0),
            now: Instant::now(),
            clip: Rect::new(0.0, 0.0, viewport.x as f32, viewport.y as f32),
            frame,
            next_wake: &mut next_wake,
        };
        let flow = self.calendar.handle_event(&mut context, event);
        (flow, next_wake)
    }
}

impl Widget<(), AppEvent> for App {
    fn handle_event(&mut self, _context: &mut (), event: &AppEvent) -> EventFlow {
        let destroying = matches!(
            event,
            Event::Host(HostEvent::TargetDestroyed | HostEvent::WindowDestroyed)
        );
        if destroying {
            let flow = self.dispatch(None, event).0;
            let _ = self.text.handle_event(&mut self.gpu, event);
            let _ = self.gpu.handle_event(&mut self.window, event);
            let _ = self.window.handle_event(&mut (), event);
            return flow;
        }

        let _ = self.window.handle_event(&mut (), event);
        let _ = self.gpu.handle_event(&mut self.window, event);
        let _ = self.text.handle_event(&mut self.gpu, event);

        let redraw = matches!(event, Event::Host(HostEvent::RedrawRequested));
        let (flow, next_wake) = if redraw {
            let Some(mut frame) = self.gpu.begin_frame() else {
                return EventFlow::NotConsumed;
            };
            frame.clear(wgpu::Color {
                r: BACKGROUND.x as f64,
                g: BACKGROUND.y as f64,
                b: BACKGROUND.z as f64,
                a: 1.0,
            });
            self.text.set_pixel_transform(&self.gpu);
            let mut canvas = FrameCanvas::new(self.gpu.size());
            let result = self.dispatch(Some(&mut canvas), event);
            self.renderer
                .flush(&self.gpu, &mut self.text, &mut frame, canvas);
            self.gpu.present(frame);
            result
        } else {
            self.dispatch(None, event)
        };
        if redraw {
            self.window.set_visible(true);
        }
        if matches!(event, Event::Host(HostEvent::TargetCreated)) {
            let initial_redraw = Event::Host(HostEvent::RedrawRequested);
            let _ = self.handle_event(&mut (), &initial_redraw);
            self.window.set_visible(true);
            self.window.request_redraw();
        }

        if let Some(error) = self.gpu.take_error() {
            panic!("{error}");
        }
        if matches!(event, Event::Host(HostEvent::AboutToWait))
            && let Some(instant) = next_wake
        {
            self.wake
                .wake_me_in(instant.saturating_duration_since(Instant::now()));
        }
        flow
    }
}

pub fn run() -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    let data_path = env::var_os("CAL_BASED_DATA").map(PathBuf::from).or_else(|| {
        ProjectDirs::from("dev", "xpjb", "cal-based")
            .map(|directories| directories.data_local_dir().join("calendar.json"))
    });
    #[cfg(not(target_arch = "wasm32"))]
    let data_path = data_path
        .ok_or_else(|| "Local data directory is unavailable".to_owned())?;
    based::run(
        BasedOptions::window(WindowOptions {
            title: "Cal · based".into(),
            size: UVec2::new(1180, 820),
            visible: false,
            ..Default::default()
        }),
        move |host: Host, events: based::Sender<StateEvent>, wake: Waker| async move {
            let window = host
                .into_window()
                .ok_or_else(|| "cal-based requires a window host".to_owned())?;
            window.set_min_inner_size(Some(UVec2::new(720, 520)));
            window.set_ime_allowed(false);
            let gpu = Wgpu::new(&window, WgpuConfig::default()).await?;
            let text = Sanscale::from_static(&gpu, include_bytes!("../../assets/Roboto.ttf"))?;
            #[cfg(not(target_arch = "wasm32"))]
            let state = CalendarState::open(&data_path, events.clone())
                .map_err(|error| error.to_string())?;
            #[cfg(target_arch = "wasm32")]
            let state = CalendarState::memory(events.clone());
            let renderer = Renderer::new(&gpu);
            Ok(App {
                calendar: CalendarWidget::new(),
                state,
                input: Input::default(),
                text,
                renderer,
                gpu,
                window,
                wake,
            })
        },
    )
}
