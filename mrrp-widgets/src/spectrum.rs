#![allow(unused)]

use std::sync::Arc;

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
use wgpu::{
    TextureViewDimension::D1,
    util::DeviceExt,
};

use crate::{
    GetWidgetRenderState,
    util::color32_to_linrgba,
};

#[derive(Debug)]
pub struct SpectrumView<'a> {
    state: &'a SpectrumState,
    desired_size: Vec2,
    style: SpectrumStyle,
    min_db: f32,
    max_db: f32,
}

impl<'a> SpectrumView<'a> {
    pub fn new(state: &'a SpectrumState) -> Self {
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

    pub fn style(mut self, style: SpectrumStyle) -> Self {
        self.style = style;
        self
    }
}

impl<'a> egui::Widget for SpectrumView<'a> {
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
pub struct SpectrumState {
    shared_state: Arc<RwLock<State>>,
}

impl SpectrumState {
    pub fn new(data: Vec<f32>) -> Self {
        Self {
            shared_state: Arc::new(RwLock::new(State {
                config: None,
                config_buffer: None,
                queued_data: data,
                queued_dirty: true,
                data_buffer: None,
                bind_group: None,
            })),
        }
    }

    pub fn update(&self) -> SpectrumStateUpdateGuard<'_> {
        let mut state = self.shared_state.write();
        SpectrumStateUpdateGuard { state }
    }
}

#[derive(Debug)]
pub struct SpectrumStateUpdateGuard<'a> {
    state: RwLockWriteGuard<'a, State>,
}

impl<'a> SpectrumStateUpdateGuard<'a> {
    pub fn data_mut(&mut self) -> &mut Vec<f32> {
        self.state.queued_dirty = true;
        &mut self.state.queued_data
    }
}

#[derive(Clone, Debug)]
pub struct SpectrumStyle {
    pub background_color: Color32,
    pub foreground_color1: Color32,
    pub foreground_color2: Color32,
}

impl Default for SpectrumStyle {
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
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // create the render pipeline, if it doesn't exist
        let render_state = callback_resources.expect_widget_render_state();
        let pipeline = callback_resources
            .entry()
            .or_insert_with(|| Pipeline::new(device, render_state.target_texture_format));

        // stream data to GPU
        let mut state = self.shared_state.write();
        state.flush(device, queue, &pipeline, &self.config);

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
            label: Some("spectrum"),
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
                // data
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
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("spectrum"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("spectrum.wgsl"));

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("spectrum"),
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

    /// Host-side data buffer
    queued_data: Vec<f32>,

    /// If the host-side data buffer has been changed since it was last sent to
    /// the GPU
    queued_dirty: bool,

    /// The GPU-side data buffer
    data_buffer: Option<wgpu::Buffer>,

    /// Bind group of config and data buffer
    bind_group: Option<wgpu::BindGroup>,
}

impl State {
    fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &Pipeline,
        config: &ConfigData,
    ) {
        // update config buffer
        let mut config_changed = self
            .config
            .as_ref()
            .is_some_and(|current| config != current);

        let config_buffer = self.config_buffer.get_or_insert_with(|| {
            tracing::debug!("creating spectrum config buffer");

            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("spectrum config"),
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
            tracing::debug!("writing spectrum config buffer");

            queue.write_buffer(&config_buffer, 0, bytemuck::bytes_of(config));

            self.config = Some(*config);
        }

        let buffer_size = self.queued_data.len();
        let buffer_size_bytes =
            u64::try_from(buffer_size * size_of::<f32>()).expect("usize -> u64 overflow");

        if buffer_size != 0 {
            // if the the host-buffer is now bigger than the gpu buffer, we need to
            // reallocate
            if self
                .data_buffer
                .as_ref()
                .is_some_and(|buffer| buffer.size() < buffer_size_bytes)
            {
                self.data_buffer = None;
            }

            // upload data to gpu if dirty
            if self.queued_dirty {
                let data_bytes = bytemuck::cast_slice(&self.queued_data);

                // data doesn't fit, so we'll have to get a new one anyway.
                if let Some(data_buffer) = &self.data_buffer
                    && u64::try_from(data_bytes.len()).expect("usize -> u64 overflow")
                        > data_buffer.size()
                {
                    self.data_buffer = None;
                    self.bind_group = None;
                }

                // create buffer if we don't have one
                let mut data_written = false;
                let data_buffer = self.data_buffer.get_or_insert_with(|| {
                    tracing::debug!("creating spectrum data buffer");

                    let data_buffer =
                        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("spectrum config"),
                            contents: data_bytes,
                            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
                        });

                    // when we create the buffer we can always initialize it right away, so we don't
                    // need to write to it anymore.
                    data_written = true;

                    data_buffer
                });

                if !data_written {
                    queue.write_buffer(data_buffer, 0, data_bytes);
                }

                self.queued_dirty = false;
            }
            // if we don't have data, we still want to have a data buffer, so we can render
            else if self.data_buffer.is_none() {
                self.data_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("spectrum config"),
                    size: buffer_size_bytes,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
                    mapped_at_creation: false,
                }));
            }
        }

        // create bind group
        if self.bind_group.is_none()
            && let (Some(config_buffer), Some(data_buffer)) =
                (&self.config_buffer, &self.data_buffer)
        {
            tracing::debug!("creating spectrum bind group");

            self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("spectrum"),
                layout: &pipeline.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: config_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: data_buffer.as_entire_binding(),
                    },
                ],
            }));
        }
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
    pub fn new(style: &SpectrumStyle, min_db: f32, max_db: f32) -> Self {
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
