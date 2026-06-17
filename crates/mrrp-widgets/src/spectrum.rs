#![allow(unused)]

use std::{
    num::NonZero,
    ops::RangeInclusive,
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
use nalgebra::{
    Matrix4,
    Rotation3,
    Vector3,
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
    SpectrumFrame,
    colormap::ColorMap,
    util::{
        color32_to_linrgba,
        staging::{
            ChunkSize,
            StagingPool,
        },
    },
};

#[derive(Debug)]
pub struct SpectrumView<'a> {
    state: &'a SpectrumState,
    desired_size: Vec2,
    style: SpectrumStyle,
    frequency_range: RangeInclusive<f32>,
    db_range: RangeInclusive<f32>,
}

impl<'a> SpectrumView<'a> {
    pub fn new(state: &'a SpectrumState) -> Self {
        Self {
            state,
            desired_size: Vec2::INFINITY,
            style: Default::default(),
            frequency_range: 0.0..=1000000.0,
            db_range: -100.0..=0.0,
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

    pub fn frequency_range(mut self, range: RangeInclusive<f32>) -> Self {
        self.frequency_range = range;
        self
    }

    pub fn db_range(mut self, range: RangeInclusive<f32>) -> Self {
        self.db_range = range;
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
                    config: ConfigData {
                        view_matrix: make_view_matrix(
                            *self.frequency_range.start(),
                            *self.frequency_range.end(),
                            *self.db_range.start(),
                            *self.db_range.end(),
                            [false, false],
                            0.0,
                        ),
                        background_color: color32_to_linrgba(self.style.background_color),
                        background_color_signal: color32_to_linrgba(
                            self.style.background_color_signal,
                        ),
                        min_db: *self.db_range.start(),
                        max_db: *self.db_range.end(),
                        _padding: [0; 2],
                    },
                    color_map: self.style.color_map.clone(),
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
    pub fn spectrum_frame_mut(&mut self) -> &mut SpectrumFrame {
        self.state.queued_dirty = true;
        self.state.queued_data.get_or_insert_with(Default::default)
    }

    pub fn data_mut(&mut self) -> &mut Vec<f32> {
        &mut self.spectrum_frame_mut().data
    }

    pub fn set_frequency_range(&mut self, range: RangeInclusive<f32>) {
        let frame = self.spectrum_frame_mut();
        frame.start_frequency = *range.start();
        frame.end_frequency = *range.end();
    }
}

#[derive(Clone, Debug)]
pub struct SpectrumStyle {
    /// The background color where there is no signal
    pub background_color: Color32,

    /// The background color where there is a signal.
    ///
    /// This is the color used above the shown signal level.
    pub background_color_signal: Color32,

    /// The color map to use for signal levels
    pub color_map: ColorMap,
}

impl Default for SpectrumStyle {
    fn default() -> Self {
        Self {
            background_color: Color32::TRANSPARENT,
            background_color_signal: Color32::BLACK,
            color_map: ColorMap::default(),
        }
    }
}

#[derive(Debug)]
struct PaintCallback {
    shared_state: Arc<RwLock<State>>,
    config: ConfigData,
    color_map: ColorMap,
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

        let color_map = self.color_map.buffer(device);

        // stream data to GPU
        let mut state = self.shared_state.write();
        state.flush(device, egui_encoder, &pipeline, &self.config, color_map);

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
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // colormap
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
    queued_data: Option<SpectrumFrame>,

    /// If the host-side data buffer has been changed since it was last sent to
    /// the GPU
    queued_dirty: bool,

    /// The GPU-side data buffer
    data_buffer: Option<wgpu::Buffer>,

    /// Bind group of config and data buffer
    bind_group: Option<wgpu::BindGroup>,

    /// the color map buffer that is currently in the bind group. this is so
    /// that we can check if this changes later
    color_map_in_bind_group: Option<wgpu::Buffer>,

    staging_pool: Option<StagingPool>,
}

impl State {
    fn flush(
        &mut self,
        device: &wgpu::Device,
        command_encoder: &mut wgpu::CommandEncoder,
        pipeline: &Pipeline,
        config: &ConfigData,
        color_map: wgpu::Buffer,
    ) {
        // if we don't have any data yet, we can't estimate buffer requirements and thus
        // not get a staging transaction. so better wait
        let Some(frame) = &self.queued_data
        else {
            return;
        };
        if frame.data.is_empty() {
            return;
        }

        let required_buffer_size_bytes =
            u64::try_from(frame.data.len() * size_of::<f32>() + size_of::<SpectrumDataHeader>())
                .expect("usize -> u64 overflow");

        let mut staging = self
            .staging_pool
            .get_or_insert_with(|| {
                StagingPool::new(
                    ChunkSize {
                        chunk_size: required_buffer_size_bytes,
                        adaptive: true,
                    },
                    "spectrum",
                )
            })
            .begin();

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
            config_changed = false;

            buffer
        });

        if config_changed {
            tracing::trace!("writing spectrum config buffer");

            staging.write_buffer_from_slice(
                config_buffer.slice(..),
                bytemuck::bytes_of(config),
                device,
                command_encoder,
            );

            self.config = Some(*config);
        }

        // check if color map changed
        if self
            .color_map_in_bind_group
            .as_ref()
            .is_none_or(|current| current != &color_map)
        {
            self.bind_group = None;
            self.color_map_in_bind_group = Some(color_map.clone());
        }

        if self.queued_dirty {
            // if the the host-buffer is now bigger than the gpu buffer, we need to
            // reallocate
            if self
                .data_buffer
                .as_ref()
                .is_some_and(|buffer| buffer.size() < required_buffer_size_bytes)
            {
                self.data_buffer = None;
                self.bind_group = None;
            }

            // upload data to gpu

            let header = SpectrumDataHeader {
                start_frequency: frame.start_frequency,
                end_frequency: frame.end_frequency,
            };

            let header_bytes = bytemuck::bytes_of(&header);
            let data_bytes = bytemuck::cast_slice(&frame.data);

            let write_to_buffer = |mut view_mut: wgpu::BufferViewMut| {
                view_mut
                    .slice(..header_bytes.len())
                    .copy_from_slice(header_bytes);
                view_mut
                    .slice(header_bytes.len()..header_bytes.len() + data_bytes.len())
                    .copy_from_slice(data_bytes);
            };

            if let Some(data_buffer) = &self.data_buffer {
                // copy to existing buffer

                let view_mut = staging.write_buffer(
                    data_buffer.slice(..required_buffer_size_bytes),
                    device,
                    command_encoder,
                );
                write_to_buffer(view_mut);
            }
            else {
                // need to create a new buffer
                tracing::debug!("creating spectrum data buffer");

                let data_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("spectrum config"),
                    size: required_buffer_size_bytes,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
                    mapped_at_creation: true,
                });

                let mut view_mut = data_buffer.get_mapped_range_mut(..);
                write_to_buffer(view_mut);

                data_buffer.unmap();

                self.data_buffer = Some(data_buffer);
                self.bind_group = None;
            }

            self.queued_dirty = false;
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
                        resource: color_map.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: data_buffer.as_entire_binding(),
                    },
                ],
            }));
        }

        staging.commit(command_encoder);
    }
}

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
struct SpectrumDataHeader {
    start_frequency: f32,
    end_frequency: f32,
}

#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
#[repr(C)]
struct ConfigData {
    view_matrix: Matrix4<f32>,
    background_color: [f32; 4],
    background_color_signal: [f32; 4],
    min_db: f32,
    max_db: f32,
    _padding: [u32; 2],
}

fn make_view_matrix(
    start_frequency: f32,
    end_frequency: f32,
    min_db: f32,
    max_db: f32,
    flip: [bool; 2],
    rotate: f32,
) -> Matrix4<f32> {
    let mut matrix = Matrix4::identity();

    // rotate
    if rotate != 0.0 {
        matrix = Rotation3::from_axis_angle(&Vector3::z_axis(), rotate).to_homogeneous() * matrix;
    }

    // flip
    matrix.append_nonuniform_scaling_mut(&Vector3::new(
        if flip[0] { 1.0 } else { -1.0 },
        if flip[1] { -1.0 } else { 1.0 },
        1.0,
    ));

    // non-uniform scaling
    matrix.append_nonuniform_scaling_mut(&Vector3::new(
        0.5 * (end_frequency - start_frequency),
        0.5 * (max_db - min_db),
        1.0,
    ));

    // translation
    matrix.append_translation_mut(&Vector3::new(
        0.5 * (start_frequency + end_frequency),
        0.5 * (min_db + max_db),
        0.0,
    ));

    matrix
}
