use std::mem::size_of;

use based::{
    glam::{UVec2, Vec2, Vec4},
    widget::wgpu::{Wgpu, WgpuFrame, raw as wgpu},
};
use super::sanscale::{Sanscale, raw as text};
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
        self.size.cmpgt(Vec2::ZERO).all()
            && point.cmpge(self.position).all()
            && point.cmple(self.position + self.size).all()
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
    local: [f32; 2],
    half_size: [f32; 2],
    radius: f32,
    border_width: f32,
    fill: [f32; 4],
    border: [f32; 4],
}

enum PaintCommand {
    Surfaces { first: u32, vertices: u32, clip: Rect },
    Text(text::Draw),
}

enum PreparedCommand {
    Surfaces { first: u32, vertices: u32, clip: Rect },
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

    pub fn quad(&mut self, rect: Rect, color: Vec4, clip: Rect) {
        self.push_rect(rect, 0.0, 0.0, [color; 4], Vec4::ZERO, clip);
    }

    pub fn rounded_rect(&mut self, rect: Rect, radius: f32, color: Vec4, clip: Rect) {
        self.push_rect(rect, radius, 0.0, [color; 4], Vec4::ZERO, clip);
    }

    pub fn surface(
        &mut self,
        rect: Rect,
        radius: f32,
        border_width: f32,
        fill: Vec4,
        border: Vec4,
        clip: Rect,
    ) {
        self.push_rect(rect, radius, border_width, [fill; 4], border, clip);
    }

    pub fn gradient(&mut self, rect: Rect, colors: [Vec4; 4], clip: Rect) {
        self.push_rect(rect, 0.0, 0.0, colors, Vec4::ZERO, clip);
    }

    fn push_rect(
        &mut self,
        rect: Rect,
        radius: f32,
        border_width: f32,
        fill: [Vec4; 4],
        border: Vec4,
        clip: Rect,
    ) {
        if rect.size.x <= 0.0
            || rect.size.y <= 0.0
            || rect.intersection(clip).is_none()
            || (fill.iter().all(|color| color.w <= 0.0) && border.w <= 0.0)
        {
            return;
        }
        let radius = radius
            .max(0.0)
            .min(rect.size.x * 0.5)
            .min(rect.size.y * 0.5);
        let border_width = border_width
            .max(0.0)
            .min(rect.size.x * 0.5)
            .min(rect.size.y * 0.5);
        let feather = if radius > 0.0 || border_width > 0.0 {
            1.0
        } else {
            0.0
        };
        let half_size = rect.size * 0.5;
        let extent = half_size + Vec2::splat(feather);
        let center = rect.center();
        let min = center - extent;
        let max = center + extent;
        let viewport = self.viewport.as_vec2().max(Vec2::ONE);
        let to_clip_space = |point: Vec2| {
            point / viewport * Vec2::new(2.0, -2.0) + Vec2::new(-1.0, 1.0)
        };
        let first = self.vertices.len() as u32;
        let corners = [
            (min, -extent, fill[0]),
            (
                Vec2::new(max.x, min.y),
                Vec2::new(extent.x, -extent.y),
                fill[1],
            ),
            (max, extent, fill[2]),
            (min, -extent, fill[0]),
            (max, extent, fill[2]),
            (
                Vec2::new(min.x, max.y),
                Vec2::new(-extent.x, extent.y),
                fill[3],
            ),
        ];
        self.vertices
            .extend(corners.into_iter().map(|(position, local, fill)| Vertex {
                position: to_clip_space(position).to_array(),
                local: local.to_array(),
                half_size: half_size.to_array(),
                radius,
                border_width,
                fill: fill.to_array(),
                border: border.to_array(),
            }));
        if let Some(PaintCommand::Surfaces {
            first: previous,
            vertices,
            clip: previous_clip,
        }) = self.commands.last_mut()
            && *previous + *vertices == first
            && *previous_clip == clip
        {
            *vertices += 6;
        } else {
            self.commands.push(PaintCommand::Surfaces { first, vertices: 6, clip });
        }
    }

    pub(super) fn text(
        &mut self,
        service: &mut Sanscale,
        content: &str,
        rect: Rect,
        size: f32,
        color: Vec4,
        horizontal: HorizontalAlign,
        vertical: VerticalAlign,
        wrap: bool,
        clip: Rect,
    ) {
        let Some(clip) = rect.intersection(clip) else {
            return;
        };
        if content.is_empty() || rect.size.x <= 0.0 || rect.size.y <= 0.0 || size <= 0.0 {
            return;
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
            return;
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
                clip.position.x,
                clip.position.y,
                clip.size.x,
                clip.size.y,
            )),
            ..Default::default()
        }));
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
                PaintCommand::Surfaces { first, vertices, clip } => {
                    prepared.push(PreparedCommand::Surfaces { first, vertices, clip });
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
                            PaintCommand::Surfaces { .. } => None,
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
        let scissor = |x: f32, y: f32, width: f32, height: f32| {
            let left = x.max(0.0).floor() as u32;
            let top = y.max(0.0).floor() as u32;
            let right = (x + width)
                .min(viewport.x as f32)
                .ceil()
                .max(left as f32) as u32;
            let bottom = (y + height)
                .min(viewport.y as f32)
                .ceil()
                .max(top as f32) as u32;
            (right > left && bottom > top).then_some((left, top, right - left, bottom - top))
        };
        for command in prepared {
            match command {
                PreparedCommand::Surfaces { first, vertices, clip } => {
                    let Some((x, y, width, height)) = scissor(
                        clip.position.x,
                        clip.position.y,
                        clip.size.x,
                        clip.size.y,
                    ) else {
                        continue;
                    };
                    pass.set_scissor_rect(x, y, width, height);
                    pass.set_pipeline(&self.pipeline);
                    pass.set_vertex_buffer(0, self.buffer.slice(..));
                    pass.draw(first..first + vertices, 0..1);
                }
                PreparedCommand::Text(batch) => {
                    for (segment_index, segment) in batch.segments().iter().enumerate() {
                        if let Some(clip) = segment.clip {
                            let Some((x, y, width, height)) =
                                scissor(clip.x, clip.y, clip.width, clip.height)
                            else {
                                continue;
                            };
                            pass.set_scissor_rect(x, y, width, height);
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
    @location(0) local: vec2f,
    @location(1) half_size: vec2f,
    @location(2) radius: f32,
    @location(3) border_width: f32,
    @location(4) fill: vec4f,
    @location(5) border: vec4f,
};

fn signed_distance(point: vec2f, half_size: vec2f, radius: f32) -> f32 {
    let corner = abs(point) - half_size + vec2f(radius);
    return length(max(corner, vec2f(0.0)))
        + min(max(corner.x, corner.y), 0.0)
        - radius;
}

@vertex
fn vertex(
    @location(0) position: vec2f,
    @location(1) local: vec2f,
    @location(2) half_size: vec2f,
    @location(3) radius: f32,
    @location(4) border_width: f32,
    @location(5) fill: vec4f,
    @location(6) border: vec4f,
) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4f(position, 0.0, 1.0);
    output.local = local;
    output.half_size = half_size;
    output.radius = radius;
    output.border_width = border_width;
    output.fill = fill;
    output.border = border;
    return output;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4f {
    if input.radius <= 0.0 && input.border_width <= 0.0 {
        return input.fill;
    }
    let outer_distance = signed_distance(input.local, input.half_size, input.radius);
    let width = max(fwidth(outer_distance), 0.0001);
    let outer = clamp(0.5 - outer_distance / width, 0.0, 1.0);
    let inner_half = max(
        input.half_size - vec2f(input.border_width),
        vec2f(0.0001),
    );
    let inner_radius = max(input.radius - input.border_width, 0.0);
    let inner_distance = signed_distance(input.local, inner_half, inner_radius);
    let inner = clamp(0.5 - inner_distance / width, 0.0, 1.0);
    let border_coverage = max(outer - inner, 0.0);
    let fill_alpha = input.fill.a * inner;
    let border_alpha = input.border.a * border_coverage;
    let alpha = fill_alpha + border_alpha;
    let premultiplied = input.fill.rgb * fill_alpha
        + input.border.rgb * border_alpha;
    let color = premultiplied / max(alpha, 0.0001);
    return vec4f(color, alpha);
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
    let attributes = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32,
        4 => Float32,
        5 => Float32x4,
        6 => Float32x4,
    ];
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
