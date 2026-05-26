#![allow(unused)]

use std::sync::Arc;

use bytemuck::{
    Pod,
    Zeroable,
};
use eframe::egui_wgpu;
use palette::LinSrgba;
use parking_lot::{
    Mutex,
    RwLock,
};
use wgpu::util::DeviceExt;

use crate::ui::RenderConfig;

#[derive(Clone, Debug, Default)]
pub struct SpectrumBuffer {
    state: Arc<RwLock<State>>,
}

impl SpectrumBuffer {
    pub fn push(&mut self, spectrum: &[f32]) {
        todo!();
    }
}

#[derive(Debug)]
pub struct Spectrum {
    buffer: SpectrumBuffer,
}

impl Spectrum {
    pub fn new(buffer: &SpectrumBuffer) -> Self {
        Self {
            buffer: buffer.clone(),
        }
    }
}

impl egui::Widget for Spectrum {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui.allocate_response(
            ui.available_size(),
            egui::Sense::HOVER | egui::Sense::CLICK | egui::Sense::DRAG,
        );

        if !ui.is_sizing_pass() && ui.is_rect_visible(response.rect) {
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                response.rect,
                PaintCallback {
                    buffer: self.buffer,
                },
            ));
        }

        response
    }
}

#[derive(Debug)]
struct PaintCallback {
    buffer: SpectrumBuffer,
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
        //
        // note: unfortunately there is no easy way to do this without always getting
        // the RenderConfig first

        let render_config = callback_resources
            .get::<RenderConfig>()
            .expect("RenderConfig");
        let target_texture_format = render_config.target_texture_format;
        let pipeline = callback_resources
            .entry()
            .or_insert_with(|| Pipeline::new(device, target_texture_format));

        // stream data to GPU
        let mut buffer = self.buffer.state.write();
        buffer.flush(device, queue, &pipeline);

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

        let state = self.buffer.state.read();

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
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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

        let shader = device.create_shader_module(wgpu::include_wgsl!("spectrum_gpu.wgsl"));

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
    config: Config,
    config_changed: bool,
    config_buffer: Option<wgpu::Buffer>,

    queued_data: Option<Vec<f32>>,
    data_buffer: Option<wgpu::Buffer>,

    bind_group: Option<wgpu::BindGroup>,
}

impl State {
    fn flush(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, pipeline: &Pipeline) {
        // update config buffer
        let config_buffer = self.config_buffer.get_or_insert_with(|| {
            tracing::debug!("creating spectrum config buffer");

            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("spectrum config"),
                contents: bytemuck::bytes_of(&self.config),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
            });

            self.config_changed = false;

            // note: no need to recreate a bind group here, since we can't have one already
            // anyway
            assert!(self.bind_group.is_none());

            buffer
        });

        if self.config_changed {
            tracing::debug!("writing spectrum config buffer");

            queue.write_buffer(&config_buffer, 0, bytemuck::bytes_of(&self.config));
            self.config_changed = false;
        }

        // update data buffer
        if let Some(data) = self.queued_data.take() {
            let data_bytes = bytemuck::cast_slice(&data);

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

                let data_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
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
        }
        // if we don't have data, we still want to have a data buffer, so we can render
        else if self.data_buffer.is_none() {
            self.data_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("spectrum config"),
                size: u64::try_from(size_of::<f32>()).expect("usize -> u64 overflow")
                    * u64::from(self.config.f_resolution),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            }));
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

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
struct Config {
    min_db: f32,
    max_db: f32,
    f_resolution: u32,
    _padding: u32,
    fg_color: LinSrgba<f32>,
    bg_color: LinSrgba<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min_db: -100.0,
            max_db: 0.0,
            f_resolution: 4096,
            _padding: 0,
            fg_color: LinSrgba::new(1.0, 0.0, 1.0, 1.0),
            bg_color: LinSrgba::new(0.0, 1.0, 0.0, 1.0),
        }
    }
}
