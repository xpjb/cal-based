use std::mem::size_of;

use based::{
    glam::{UVec2, Vec2, Vec4},
    widget::{
        sanscale::{Sanscale, raw as text},
        wgpu::{Wgpu, WgpuFrame, raw as wgpu},
    },
};
use bytemuck::{Pod, Zeroable};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub position: Vec2,
    pub size: Vec2,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            position: Vec2::new(x, y),
            size: Vec2::new(width, height),
        }
    }

    pub fn right(self) -> f32 {
        self.position.x + self.size.x
    }

    pub fn bottom(self) -> f32 {
        self.position.y + self.size.y
    }

    pub fn center(self) -> Vec2 {
        self.position + self.size * 0.5
    }

    pub fn contains(self, point: Vec2) -> bool {
        point.cmpge(self.position).all() && point.cmple(self.position + self.size).all()
    }

    pub fn inset(self, amount: f32) -> Self {
        Self {
            position: self.position + Vec2::splat(amount),
            size: (self.size - Vec2::splat(amount * 2.0)).max(Vec2::ZERO),
        }
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let min = self.position.max(other.position);
        let max = (self.position + self.size).min(other.position + other.size);
        (max.cmpgt(min).all()).then_some(Self {
            position: min,
            size: max - min,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalAlign {
    Left,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalAlign {
    Top,
    Center,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

enum PaintCommand {
    Quads { first: u32, vertices: u32 },
    Text(text::Draw),
}

enum PreparedCommand {
    Quads { first: u32, vertices: u32 },
    Text(text::Batch),
}

pub struct FrameCanvas {
    viewport: UVec2,
    vertices: Vec<Vertex>,
    commands: Vec<PaintCommand>,
}

impl FrameCanvas {
    pub fn new(viewport: UVec2) -> Self {
        Self {
            viewport,
            vertices: Vec::with_capacity(4096),
            commands: Vec::with_capacity(512),
        }
    }

    pub fn quad(&mut self, rect: Rect, color: Vec4) {
        if rect.size.x <= 0.0 || rect.size.y <= 0.0 || color.w <= 0.0 {
            return;
        }
        let min = rect.position;
        let max = rect.position + rect.size;
        let viewport = self.viewport.as_vec2().max(Vec2::ONE);
        let clip = |point: Vec2| {
            point / viewport * Vec2::new(2.0, -2.0) + Vec2::new(-1.0, 1.0)
        };
        let points = [
            min,
            Vec2::new(max.x, min.y),
            max,
            min,
            max,
            Vec2::new(min.x, max.y),
        ];
        let first = self.vertices.len() as u32;
        for point in points {
            self.vertices.push(Vertex {
                position: clip(point).to_array(),
                color: color.to_array(),
            });
        }
        if let Some(PaintCommand::Quads {
            first: previous,
            vertices,
        }) = self.commands.last_mut()
            && *previous + *vertices == first
        {
            *vertices += 6;
        } else {
            self.commands.push(PaintCommand::Quads { first, vertices: 6 });
        }
    }

    pub fn border(&mut self, rect: Rect, width: f32, color: Vec4) {
        self.quad(Rect::new(rect.position.x, rect.position.y, rect.size.x, width), color);
        self.quad(
            Rect::new(rect.position.x, rect.bottom() - width, rect.size.x, width),
            color,
        );
        self.quad(Rect::new(rect.position.x, rect.position.y, width, rect.size.y), color);
        self.quad(
            Rect::new(rect.right() - width, rect.position.y, width, rect.size.y),
            color,
        );
    }

    pub fn text(
        &mut self,
        service: &mut Sanscale,
        content: &str,
        rect: Rect,
        size: f32,
        color: Vec4,
        horizontal: HorizontalAlign,
        vertical: VerticalAlign,
        wrap: bool,
    ) -> bool {
        if content.is_empty() || rect.size.x <= 0.0 || rect.size.y <= 0.0 || size <= 0.0 {
            return false;
        }
        let mut style = service.style;
        style.wrap_em = wrap.then_some((rect.size.x / size).max(1.0));
        style.align = if wrap {
            match horizontal {
                HorizontalAlign::Left => text::Align::Left,
                HorizontalAlign::Center => text::Align::Center,
            }
        } else {
            text::Align::Left
        };
        let Some(handle) = service.raw.shape_transient(content, &style) else {
            return false;
        };
        let layout = service.raw.measure(handle);
        let extent = Vec2::new(layout.width_em(), layout.height_em()) * size;
        let x = if wrap {
            rect.position.x
        } else {
            match horizontal {
                HorizontalAlign::Left => rect.position.x,
                HorizontalAlign::Center => rect.center().x - extent.x * 0.5,
            }
        };
        let y = match vertical {
            VerticalAlign::Top => rect.position.y,
            VerticalAlign::Center => rect.center().y - extent.y * 0.5,
        };
        let origin = Vec2::new(x, y);
        self.commands.push(PaintCommand::Text(text::Draw {
            block: handle,
            at: text::Vec2::new(origin.x, origin.y),
            size,
            color: text::Color(color.to_array()),
            clip: Some(text::Rect::new(
                rect.position.x,
                rect.position.y,
                rect.size.x,
                rect.size.y,
            )),
            ..Default::default()
        }));
        true
    }

    pub fn draw_text(
        &mut self,
        handle: text::ShapedHandle,
        origin: Vec2,
        size: f32,
        color: Vec4,
        clip: Rect,
    ) {
        self.commands.push(PaintCommand::Text(text::Draw {
            block: handle,
            at: text::Vec2::new(origin.x, origin.y),
            size,
            color: text::Color(color.to_array()),
            clip: Some(text::Rect::new(
                clip.position.x,
                clip.position.y,
                clip.size.x,
                clip.size.y,
            )),
            ..Default::default()
        }));
    }
}

pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    format_generation: u64,
    buffer: wgpu::Buffer,
    capacity: usize,
}

impl Renderer {
    pub fn new(gpu: &Wgpu) -> Self {
        let capacity = 4096;
        Self {
            pipeline: create_pipeline(gpu),
            format_generation: gpu.format_generation(),
            buffer: create_buffer(gpu, capacity),
            capacity,
        }
    }

    pub fn flush(
        &mut self,
        gpu: &Wgpu,
        service: &mut Sanscale,
        frame: &mut WgpuFrame,
        canvas: FrameCanvas,
    ) {
        if self.format_generation != gpu.format_generation() {
            self.pipeline = create_pipeline(gpu);
            self.format_generation = gpu.format_generation();
        }
        if canvas.vertices.len() > self.capacity {
            self.capacity = canvas.vertices.len().next_power_of_two();
            self.buffer = create_buffer(gpu, self.capacity);
        }
        if !canvas.vertices.is_empty() {
            gpu.queue
                .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&canvas.vertices));
        }

        let mut prepared = Vec::with_capacity(canvas.commands.len());
        let mut index = 0;
        while index < canvas.commands.len() {
            match canvas.commands[index] {
                PaintCommand::Quads { first, vertices } => {
                    prepared.push(PreparedCommand::Quads { first, vertices });
                    index += 1;
                }
                PaintCommand::Text(_) => {
                    let start = index;
                    while index < canvas.commands.len()
                        && matches!(canvas.commands[index], PaintCommand::Text(_))
                    {
                        index += 1;
                    }
                    let draws: Vec<_> = canvas.commands[start..index]
                        .iter()
                        .filter_map(|command| match command {
                            PaintCommand::Text(draw) => Some(*draw),
                            PaintCommand::Quads { .. } => None,
                        })
                        .collect();
                    prepared.push(PreparedCommand::Text(service.raw.prepare(
                        &gpu.device,
                        &gpu.queue,
                        &draws,
                    )));
                }
            }
        }

        let mut pass = frame.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cal-based-frame"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &frame.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let viewport = canvas.viewport;
        for command in prepared {
            match command {
                PreparedCommand::Quads { first, vertices } => {
                    pass.set_scissor_rect(0, 0, viewport.x.max(1), viewport.y.max(1));
                    pass.set_pipeline(&self.pipeline);
                    pass.set_vertex_buffer(0, self.buffer.slice(..));
                    pass.draw(first..first + vertices, 0..1);
                }
                PreparedCommand::Text(batch) => {
                    for (segment_index, segment) in batch.segments().iter().enumerate() {
                        if let Some(clip) = segment.clip {
                            let x = clip.x.max(0.0).floor() as u32;
                            let y = clip.y.max(0.0).floor() as u32;
                            let right = (clip.x + clip.width)
                                .min(viewport.x as f32)
                                .ceil()
                                .max(x as f32) as u32;
                            let bottom = (clip.y + clip.height)
                                .min(viewport.y as f32)
                                .ceil()
                                .max(y as f32) as u32;
                            if right == x || bottom == y {
                                continue;
                            }
                            pass.set_scissor_rect(x, y, right - x, bottom - y);
                        } else {
                            pass.set_scissor_rect(0, 0, viewport.x.max(1), viewport.y.max(1));
                        }
                        service.raw.draw_segment(&mut pass, &batch, segment_index);
                    }
                }
            }
        }
    }
}

fn create_buffer(gpu: &Wgpu, capacity: usize) -> wgpu::Buffer {
    gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cal-based-rectangles"),
        size: (capacity.max(1) * size_of::<Vertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_pipeline(gpu: &Wgpu) -> wgpu::RenderPipeline {
    const SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec4f,
};

@vertex
fn vertex(@location(0) position: vec2f, @location(1) color: vec4f) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4f(position, 0.0, 1.0);
    output.color = color;
    return output;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4f {
    return input.color;
}
"#;
    let shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cal-based-rectangles"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cal-based-rectangles"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
    gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cal-based-rectangles"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment"),
            targets: &[Some(wgpu::ColorTargetState {
                format: gpu.format(),
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}
