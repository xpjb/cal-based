mod calendar;
mod day;
mod editor;
mod event;
mod paint;
mod scroll;
mod settings;
mod text_area;
mod text_edit;
mod ui;

pub(crate) use calendar::*;
pub(crate) use day::*;
pub(crate) use editor::*;
pub(crate) use event::*;
pub(crate) use paint::*;
pub(crate) use settings::*;
pub(crate) use ui::*;

use crate::*;
#[cfg(not(target_arch = "wasm32"))]
use std::{env, path::PathBuf};

use based::{
    Event, EventFlow, HostEvent, Wake, Widget,
    backend::winit,
    glam::{UVec2, Vec2, Vec4},
    web_time::Instant,
    widget::{
        Input,
        sanscale::{Sanscale, raw as text},
        wgpu::{Wgpu, WgpuConfig, raw as wgpu},
        window::{CursorIcon, Window, WindowConfig},
    },
};
#[cfg(not(target_arch = "wasm32"))]
use directories::ProjectDirs;

pub(crate) type AppEvent = Event<StateEvent>;

pub(crate) struct AppContext<'a> {
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
    frame: Option<&'a mut FrameCanvas>,
    next_wake: &'a mut Option<Instant>,
}

impl AppContext<'_> {
    fn wake_at(&mut self, instant: Instant) {
        if self.next_wake.is_none_or(|current| instant < current) {
            *self.next_wake = Some(instant);
        }
    }

    fn set_cursor(&self, cursor: CursorIcon) {
        self.window.set_cursor(cursor);
    }

    fn quad(&mut self, rect: Rect, color: Vec4) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.quad(rect, color);
        }
    }

    fn rounded_rect(&mut self, rect: Rect, radius: f32, color: Vec4) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.rounded_rect(rect, radius, color);
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
            frame.surface(rect, radius, border_width, fill, border);
        }
    }

    fn gradient(&mut self, rect: Rect, colors: [Vec4; 4]) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.gradient(rect, colors);
        }
    }

    fn text(
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

    fn draw_text(
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

struct App {
    calendar: CalendarWidget,
    state: CalendarState,
    input: Input,
    text: Sanscale,
    renderer: Renderer,
    gpu: Wgpu,
    window: Window,
    wake: Wake,
}

impl App {
    fn dispatch(
        &mut self,
        frame: Option<&mut FrameCanvas>,
        event: &AppEvent,
    ) -> (EventFlow, Option<Instant>) {
        let _ = self.input.handle_event(&mut (), event);
        let mut next_wake = None;
        let mut context = AppContext {
            window: &self.window,
            input: &self.input,
            state: &mut self.state,
            text: &mut self.text,
            viewport: self.gpu.size(),
            scale: (self.window.scale_factor() as f32).clamp(1.0, 3.0),
            now: Instant::now(),
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

        if let Some(error) = self.gpu.take_error() {
            panic!("{error}");
        }
        if matches!(event, Event::Host(HostEvent::AboutToWait))
            && let Some(instant) = next_wake
        {
            self.wake
                .after(instant.saturating_duration_since(Instant::now()));
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
    winit::run(
        WindowConfig {
            title: "Cal · based".into(),
            size: UVec2::new(1180, 820),
            visible: false,
            ..Default::default()
        },
        move |window, events, wake| async move {
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
