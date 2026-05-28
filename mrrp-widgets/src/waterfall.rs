#![allow(unused)]

use std::{
    cmp::Ordering,
    collections::VecDeque,
    num::NonZero,
    ops::Range,
    sync::Arc,
};

use bytemuck::{
    Pod,
    Zeroable,
};
use egui::{
    Color32,
    Vec2,
};
use parking_lot::{
    Mutex,
    RwLock,
    RwLockWriteGuard,
};
use wgpu::util::{
    DeviceExt,
    StagingBelt,
};

use crate::{
    GetWidgetRenderState,
    RenderState,
    util::{
        color32_to_linrgba,
        ring_buffer::{
            RingBufferAllocator,
            Slice,
        },
        staging::{
            StagingPool,
            StagingTransaction,
        },
    },
};

#[derive(Debug)]
pub struct WaterfallView<'a> {
    state: &'a WaterfallState,
    desired_size: Vec2,
    style: WaterfallStyle,
    min_db: f32,
    max_db: f32,
}

impl<'a> WaterfallView<'a> {
    pub fn new(state: &'a WaterfallState) -> Self {
        Self {
            state,
            desired_size: Vec2::INFINITY,
            style: Default::default(),
            min_db: -100.0,
            max_db: 0.0,
        }
    }

    pub fn desired_size(mut self, size: Vec2) -> Self {
        self.desired_size = size;
        self
    }

    pub fn desired_width(mut self, width: f32) -> Self {
        self.desired_size.x = width;
        self
    }

    pub fn desired_height(mut self, height: f32) -> Self {
        self.desired_size.y = height;
        self
    }

    pub fn style(mut self, style: WaterfallStyle) -> Self {
        self.style = style;
        self
    }
}

impl<'a> egui::Widget for WaterfallView<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui.allocate_response(
            ui.available_size(),
            egui::Sense::HOVER | egui::Sense::CLICK | egui::Sense::DRAG,
        );

        if !ui.is_sizing_pass() && ui.is_rect_visible(response.rect) {
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                response.rect,
                PaintCallback {
                    shared_state: self.state.shared_state.clone(),
                    config: ConfigData::new(&self.style, self.min_db, self.max_db),
                },
            ));
        }

        response
    }
}

#[derive(Clone, Debug, Default)]
pub struct WaterfallState {
    shared_state: Arc<RwLock<State>>,
}

impl WaterfallState {
    pub fn update(&self) -> WaterfallStateUpdateGuard<'_> {
        let mut state = self.shared_state.write();
        WaterfallStateUpdateGuard { state }
    }
}

#[derive(Debug)]
pub struct WaterfallStateUpdateGuard<'a> {
    state: RwLockWriteGuard<'a, State>,
}

impl<'a> WaterfallStateUpdateGuard<'a> {
    pub fn push(&mut self, line: WaterfallLine) {
        self.state.queued_lines.push(line);
    }
}

#[derive(Clone, Debug)]
pub struct WaterfallStyle {
    pub background_color: Color32,
    pub foreground_color1: Color32,
    pub foreground_color2: Color32,
}

impl Default for WaterfallStyle {
    fn default() -> Self {
        Self {
            background_color: Color32::BLACK,
            foreground_color1: Color32::from_rgba_unmultiplied(200, 0, 200, 255),
            foreground_color2: Color32::from_rgba_unmultiplied(64, 0, 64, 255),
        }
    }
}

#[derive(Debug)]
struct PaintCallback {
    shared_state: Arc<RwLock<State>>,
    config: ConfigData,
}

impl egui_wgpu::CallbackTrait for PaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // create the render pipeline, if it doesn't exist
        let render_state = callback_resources.expect_widget_render_state();
        let pipeline = callback_resources
            .entry()
            .or_insert_with(|| Pipeline::new(device, render_state.target_texture_format));

        // stream data to GPU
        let mut state = self.shared_state.write();
        state.flush(device, egui_encoder, &pipeline, &self.config);

        vec![]
    }

    fn finish_prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _egui_encoder: &mut wgpu::CommandEncoder,
        _callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // todo
        vec![]
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let pipeline = callback_resources.get::<Pipeline>().expect("pipeline");

        let state = self.shared_state.read();

        if let Some(bind_group) = &state.bind_group {
            render_pass.set_pipeline(&pipeline.pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}

#[derive(Debug)]
struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Pipeline {
    pub fn new(device: &wgpu::Device, target_texture_format: wgpu::TextureFormat) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("waterfall"),
            entries: &[
                // config
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // index
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // data
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("waterfall"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("waterfall.wgsl"));

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("waterfall"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_texture_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::all(),
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
        }
    }
}

#[derive(Debug, Default)]
struct State {
    /// The config that is currently stored in the GPU buffer
    config: Option<ConfigData>,

    /// The GPU buffer storing the config
    config_buffer: Option<wgpu::Buffer>,

    /// Lines queued on the CPU-side
    queued_lines: Vec<WaterfallLine>,

    // Ring buffer holding the lines
    ring_buffer: RingBuffer,

    /// Bind group of config and data buffer
    bind_group: Option<wgpu::BindGroup>,

    staging_pool: Option<StagingPool>,
}

impl State {
    fn flush(
        &mut self,
        device: &wgpu::Device,
        mut command_encoder: &mut wgpu::CommandEncoder,
        pipeline: &Pipeline,
        config: &ConfigData,
    ) {
        // we need a line first. otherwise we can't determine a good size for the
        // staging buffers. until then we can't even upload the config.
        let Some(first_line) = self.queued_lines.first()
        else {
            return;
        };

        // if the lines are empty, we can't really do anything either
        let Some(chunk_size) = NonZero::new(first_line.bytes_len())
        else {
            return;
        };

        // get staging transaction
        let mut staging = self
            .staging_pool
            .get_or_insert_with(|| {
                tracing::debug!(chunk_size, "creating staging buffer");
                StagingPool::new(chunk_size, "waterfall-staging")
            })
            .begin();

        // update config buffer
        let mut config_changed = self
            .config
            .as_ref()
            .is_some_and(|current| config != current);

        let config_buffer = self.config_buffer.get_or_insert_with(|| {
            tracing::debug!("creating waterfall config buffer");

            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("waterfall config"),
                contents: bytemuck::bytes_of(config),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            });

            // note: no need to recreate a bind group here, since we can't have one already
            // anyway
            assert!(self.bind_group.is_none());

            self.config = Some(*config);

            buffer
        });

        if config_changed {
            tracing::debug!("writing waterfall config buffer");

            let config_bytes = bytemuck::bytes_of(config);
            staging.write_buffer_from_slice(
                config_buffer.slice(..),
                config_bytes,
                &device,
                &mut command_encoder,
            );

            self.config = Some(*config);
        }

        let buffers_reallocated =
            self.ring_buffer
                .push_back(&self.queued_lines, &mut command_encoder, &mut staging);

        if buffers_reallocated {
            // if buffers have been reallocated, we need to recreate the bind group
            self.bind_group = None;
        }

        // create bind group
        if self.bind_group.is_none()
            && let (Some(config_buffer), Some(index_buffer), Some(data_buffer)) = (
                &self.config_buffer,
                &self.ring_buffer.index_buffer,
                &self.ring_buffer.data_buffer,
            )
        {
            tracing::debug!("creating waterfall bind group");

            self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("waterfall"),
                layout: &pipeline.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: config_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: index_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: data_buffer.as_entire_binding(),
                    },
                ],
            }));
        }

        // commit staging transaction
        staging.commit(&mut command_encoder);
    }
}

#[derive(Debug, Default)]
struct RingBuffer {
    line_capacity: usize,
    lines: VecDeque<Line>,

    allocator: RingBufferAllocator,

    index_buffer: Option<wgpu::Buffer>,
    data_buffer: Option<wgpu::Buffer>,
}

impl RingBuffer {
    fn new(byte_capacity: u64, line_capacity: usize) -> Self {
        Self {
            line_capacity,
            lines: VecDeque::new(),
            allocator: RingBufferAllocator::new(byte_capacity),
            index_buffer: None,
            data_buffer: None,
        }
    }

    fn set_line_capacity(&mut self, line_capacity: usize) {
        self.truncate_front(line_capacity);
        self.line_capacity = line_capacity;
    }

    fn truncate_front(&mut self, new_length: usize) {
        let num_drop = self.lines.len().saturating_sub(new_length);
        for _ in 0..num_drop {
            debug_assert!(self.pop_front().is_some());
        }
    }

    fn pop_front(&mut self) -> Option<Line> {
        let line = self.lines.pop_front()?;
        assert!(self.allocator.free(line.slice));
        Some(line)
    }

    fn push_back(
        &mut self,
        lines: &[WaterfallLine],
        command_encoder: &mut wgpu::CommandEncoder,
        staging: &mut StagingTransaction,
        device: &wgpu::Device,
    ) -> bool {
        if self.line_capacity == 0 || lines.is_empty() {
            return false;
        }

        // check if we can skip some queued lines
        let num_skip = lines.len().saturating_sub(self.line_capacity);
        let lines = &lines[num_skip..];

        // truncate from front to make enough room
        self.truncate_front(self.line_capacity - lines.len());

        // check available space
        let total_required: u64 = lines.iter().map(|line| line.bytes_len()).sum();
        if let Some(data_buffer) = &self.data_buffer
            && total_required <= self.allocator.available()
        {
            // allocate new lines and write them to the staging buffer
            for line in lines {
                let slice = self
                    .allocator
                    .allocate(line.bytes_len())
                    .expect("allocation failed");

                self.lines.push_back(Line { slice });
                let data = bytemuck::cast_slice(&line.data);

                for (source, destination) in slice.iter_with_source() {
                    staging.write_buffer_from_slice(
                        data_buffer.slice(destination),
                        &data[source],
                        device,
                        command_encoder,
                    );
                }
            }

            todo!("write index");
            //false
        }
        else {
            // need to reallocate

            let new_capacity = (self.allocator.capacity() * 2).min(total_required);
            let mut new_allocator = RingBufferAllocator::new(new_capacity);

            for range in self.allocator.allocated().iter() {
                let [new_range, new_range_empty] = new_allocator
                    .allocate(range.len())
                    .expect("new allocator doesn't have enough space")
                    .parts();
                assert!(new_range_empty.is_empty(), "new allocation not contigious");
            }

            todo!("realloc");
        }
    }
}

#[derive(Debug)]
struct Line {
    slice: Slice,
}

#[derive(Clone, Debug)]
pub struct WaterfallLine {
    pub data: Vec<f32>,
    pub start_frequency: f32,
    pub end_frequency: f32,
}

impl WaterfallLine {
    fn bytes_len(&self) -> u64 {
        u64::try_from(std::mem::size_of::<f32>() * self.data.len()).unwrap()
    }
}

#[derive(Clone, Copy, Debug, Pod, Zeroable, Default, PartialEq)]
#[repr(C)]
struct ConfigData {
    min_db: f32,
    max_db: f32,
    _padding: [u32; 2],
    background_color: [f32; 4],
    foreground_color1: [f32; 4],
    foreground_color2: [f32; 4],
}

impl ConfigData {
    pub fn new(style: &WaterfallStyle, min_db: f32, max_db: f32) -> Self {
        Self {
            min_db,
            max_db,
            _padding: [0; 2],
            background_color: color32_to_linrgba(style.background_color),
            foreground_color1: color32_to_linrgba(style.foreground_color1),
            foreground_color2: color32_to_linrgba(style.foreground_color2),
        }
    }
}
