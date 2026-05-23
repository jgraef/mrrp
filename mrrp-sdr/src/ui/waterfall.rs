use eframe::egui_wgpu;

use crate::RenderConfig;

#[derive(Debug)]
pub struct Waterfall {
    // todo
}

impl Waterfall {
    pub fn new() -> Self {
        Self {}
    }
}

impl egui::Widget for Waterfall {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui.allocate_response(
            ui.available_size(),
            egui::Sense::HOVER | egui::Sense::CLICK | egui::Sense::DRAG,
        );

        if !ui.is_sizing_pass() && ui.is_rect_visible(response.rect) {
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                response.rect,
                PaintCallback {},
            ));
        }

        response
    }
}

#[derive(Debug)]
struct PaintCallback {
    // todo
}

impl egui_wgpu::CallbackTrait for PaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
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
        let _pipeline = callback_resources
            .entry()
            .or_insert_with(|| Pipeline::new(device, target_texture_format));

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

        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.draw(0..3, 0..1);

        // todo
    }
}

#[derive(Debug)]
struct Pipeline {
    pipeline: wgpu::RenderPipeline,
}

impl Pipeline {
    pub fn new(device: &wgpu::Device, target_texture_format: wgpu::TextureFormat) -> Self {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("waterfall"),
            bind_group_layouts: &[],
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

        Self { pipeline }
    }
}
