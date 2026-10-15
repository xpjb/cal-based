mod calendar;
mod editor;
mod paint;
mod settings;
mod ui;

pub(crate) use calendar::*;
pub(crate) use editor::*;
pub(crate) use paint::*;
pub(crate) use settings::*;
pub(crate) use ui::*;

use crate::*;
use std::{env, path::PathBuf};

use based::{
    Event, EventFlow, HostEvent, Widget,
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

    pub fn scale_factor(&self) -> f32 {
        (self.window.scale_factor() as f32).clamp(1.0, 3.0)
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

    pub fn set_cursor(&self, cursor: CursorIcon) {
        if let Some(window) = self.window.raw() {
            window.set_cursor(cursor);
        }
    }

    pub fn quad(&mut self, rect: Rect, color: Vec4) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.quad(rect, color);
        }
    }

    pub fn rounded_rect(&mut self, rect: Rect, radius: f32, color: Vec4) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.rounded_rect(rect, radius, color);
        }
    }

    pub fn surface(
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

    pub fn gradient(&mut self, rect: Rect, colors: [Vec4; 4]) {
        if let Some(frame) = self.frame.as_deref_mut() {
            frame.gradient(rect, colors);
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
    app: CalendarWidget,
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
            if self.ready.is_none() {
                let gpu = match pollster::block_on(Wgpu::new(&self.window, WgpuConfig::default())) {
                    Ok(gpu) => gpu,
                    Err(error) => {
                        host.fail(error);
                        return EventFlow::NotConsumed;
                    }
                };
                let text = match Sanscale::from_static(&gpu, include_bytes!("../../assets/Roboto.ttf")) {
                    Ok(text) => text,
                    Err(error) => {
                        host.fail(error);
                        return EventFlow::NotConsumed;
                    }
                };
                let model = match AppModel::open(&self.data_path) {
                    Ok(model) => model,
                    Err(error) => {
                        host.fail(error.to_string());
                        return EventFlow::NotConsumed;
                    }
                };
                let renderer = Renderer::new(&gpu);
                self.ready = Some(Ready {
                    app: CalendarWidget::new(),
                    model,
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


pub fn run() -> Result<(), String> {
    let data_path = env::var_os("CAL_BASED_DATA").map(PathBuf::from).or_else(|| {
        ProjectDirs::from("dev", "xpjb", "cal-based")
            .map(|directories| directories.data_local_dir().join("calendar.json"))
    });
    let data_path = data_path.ok_or_else(|| "Windows local data directory is unavailable".to_owned())?;
    winit::run(Host::new(data_path))
}
