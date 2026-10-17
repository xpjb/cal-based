use std::sync::Arc;

use based::{Event, EventFlow, HostEvent, Widget, widget::wgpu::Wgpu};

pub use ::sanscale as raw;

pub struct Sanscale {
    pub raw: raw::TextService,
    pub style: raw::Style,
    format_generation: u64,
}

impl Sanscale {
    pub fn from_static(gpu: &Wgpu, bytes: &'static [u8]) -> Result<Self, String> {
        let mut text = raw::TextService::new();
        let font = text
            .map_font(Arc::new(bytes), 0)
            .map_err(|error| error.to_string())?;
        let chain = text
            .register_chain(&[font])
            .map_err(|error| error.to_string())?;
        let style = raw::Style {
            chain,
            wrap_em: None,
            align: raw::Align::Left,
            line_spacing: 1.2,
        };
        text.set_target(&gpu.device, gpu.format());
        Ok(Self {
            raw: text,
            style,
            format_generation: gpu.format_generation(),
        })
    }

    fn sync_target(&mut self, gpu: &Wgpu) {
        if self.format_generation != gpu.format_generation() {
            self.raw.set_target(&gpu.device, gpu.format());
            self.format_generation = gpu.format_generation();
        }
    }

    pub fn set_pixel_transform(&mut self, gpu: &Wgpu) {
        self.raw
            .set_transform(raw::TextService::pixel_ortho(gpu.size().x, gpu.size().y));
    }
}

impl<T> Widget<Wgpu, Event<T>> for Sanscale {
    fn handle_event(&mut self, gpu: &mut Wgpu, event: &Event<T>) -> EventFlow {
        if matches!(
            event,
            Event::Host(HostEvent::TargetCreated | HostEvent::RedrawRequested)
        ) {
            self.sync_target(gpu);
        }
        EventFlow::NotConsumed
    }
}
