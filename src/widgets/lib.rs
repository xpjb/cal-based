mod calendar;
mod day;
mod editor;
mod event;
mod paint;
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
use std::{env, path::PathBuf};

use based::{
    Event, EventFlow, HostEvent, UserEventSender, Widget,
    backend::winit::{self, WinitContext, raw::window::CursorIcon},
    glam::{UVec2, Vec2, Vec4},
    web_time::Instant,
    widget::{
        Input,
        sanscale::{Sanscale, raw as text},
        wgpu::{Wgpu, WgpuConfig, raw as wgpu},
        window::{Window, WindowConfig},
    },
};
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
        if let Some(window) = self.window.raw() {
            window.set_cursor(cursor);
        }
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

struct Runtime {
    calendar: CalendarWidget,
    state: CalendarState,
    input: Input,
    text: Sanscale,
    renderer: Renderer,
    gpu: Wgpu,
}

impl Runtime {
    fn dispatch(
        &mut self,
        window: &Window,
        frame: Option<&mut FrameCanvas>,
        event: &AppEvent,
    ) -> (EventFlow, Option<Instant>) {
        let _ = self.input.handle_event(&mut (), event);
        let mut next_wake = None;
        let mut context = AppContext {
            window,
            input: &self.input,
            state: &mut self.state,
            text: &mut self.text,
            viewport: self.gpu.size(),
            scale: (window.scale_factor() as f32).clamp(1.0, 3.0),
            now: Instant::now(),
            frame,
            next_wake: &mut next_wake,
        };
        let flow = self.calendar.handle_event(&mut context, event);
        (flow, next_wake)
    }
}

struct App {
    window: Window,
    runtime: Option<Runtime>,
    data_path: PathBuf,
    events: UserEventSender<StateEvent>,
}

impl Widget<WinitContext<'_>, AppEvent> for App {
    fn handle_event(&mut self, winit: &mut WinitContext<'_>, event: &AppEvent) -> EventFlow {
        let destroying = matches!(
            event,
            Event::Host(HostEvent::TargetDestroyed | HostEvent::WindowDestroyed)
        );
        if destroying {
            let mut flow = EventFlow::NotConsumed;
            if let Some(runtime) = &mut self.runtime {
                let result = runtime.dispatch(&self.window, None, event);
                flow = result.0;
                let _ = runtime.text.handle_event(&mut runtime.gpu, event);
                let _ = runtime.gpu.handle_event(&mut self.window, event);
            }
            let _ = self.window.handle_event(winit, event);
            return flow;
        }

        let _ = self.window.handle_event(winit, event);
        if matches!(event, Event::Host(HostEvent::TargetCreated)) {
            if !self.window.target_available() {
                return EventFlow::NotConsumed;
            }
            if self.runtime.is_none() {
                let gpu = match pollster::block_on(Wgpu::new(&self.window, WgpuConfig::default())) {
                    Ok(gpu) => gpu,
                    Err(error) => {
                        winit.fail(error);
                        return EventFlow::NotConsumed;
                    }
                };
                let text = match Sanscale::from_static(&gpu, include_bytes!("../../assets/Roboto.ttf")) {
                    Ok(text) => text,
                    Err(error) => {
                        winit.fail(error);
                        return EventFlow::NotConsumed;
                    }
                };
                let state = match CalendarState::open(
                    &self.data_path,
                    self.events.clone(),
                ) {
                    Ok(state) => state,
                    Err(error) => {
                        winit.fail(error.to_string());
                        return EventFlow::NotConsumed;
                    }
                };
                let renderer = Renderer::new(&gpu);
                self.runtime = Some(Runtime {
                    calendar: CalendarWidget::new(),
                    state,
                    input: Input::default(),
                    text,
                    renderer,
                    gpu,
                });
                if let Some(window) = self.window.raw() {
                    window.set_min_inner_size(Some(
                        based::backend::winit::raw::dpi::LogicalSize::new(720.0, 520.0),
                    ));
                    window.set_ime_allowed(false);
                }
            }
        }

        let Some(runtime) = &mut self.runtime else {
            return EventFlow::NotConsumed;
        };
        let _ = runtime.gpu.handle_event(&mut self.window, event);
        let _ = runtime.text.handle_event(&mut runtime.gpu, event);

        let redraw = matches!(event, Event::Host(HostEvent::RedrawRequested));
        let (flow, next_wake) = if redraw {
            let Some(mut frame) = runtime.gpu.begin_frame() else {
                return EventFlow::NotConsumed;
            };
            frame.clear(wgpu::Color {
                r: BACKGROUND.x as f64,
                g: BACKGROUND.y as f64,
                b: BACKGROUND.z as f64,
                a: 1.0,
            });
            runtime.text.set_pixel_transform(&runtime.gpu);
            let mut canvas = FrameCanvas::new(runtime.gpu.size());
            let result = runtime.dispatch(&self.window, Some(&mut canvas), event);
            runtime
                .renderer
                .flush(&runtime.gpu, &mut runtime.text, &mut frame, canvas);
            runtime.gpu.present(frame);
            result
        } else {
            runtime.dispatch(&self.window, None, event)
        };
        if redraw {
            self.window.set_visible(true);
        }

        if let Some(error) = runtime.gpu.take_error() {
            winit.fail(error);
        } else if matches!(event, Event::Host(HostEvent::CloseRequested))
            && flow == EventFlow::NotConsumed
        {
            winit.exit();
        } else if matches!(event, Event::Host(HostEvent::AboutToWait)) {
            let control = next_wake.map_or(based::LoopControl::Wait, |instant| {
                based::LoopControl::WaitUntil { instant }
            });
            winit::apply_loop_control(winit.event_loop, control);
        }
        flow
    }
}

pub fn run() -> Result<(), String> {
    let data_path = env::var_os("CAL_BASED_DATA").map(PathBuf::from).or_else(|| {
        ProjectDirs::from("dev", "xpjb", "cal-based")
            .map(|directories| directories.data_local_dir().join("calendar.json"))
    });
    let data_path = data_path.ok_or_else(|| "Windows local data directory is unavailable".to_owned())?;
    winit::run(|events| App {
        window: Window::new(WindowConfig {
            title: "Cal · based".into(),
            size: UVec2::new(1180, 820),
            visible: false,
            ..Default::default()
        }),
        runtime: None,
        data_path,
        events,
    })
}
