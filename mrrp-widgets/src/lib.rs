pub mod colormap;
pub mod frequency_dial;
pub mod spectrum;
pub(crate) mod util;
pub mod waterfall;

use egui_wgpu::{
    CallbackResources,
    RenderState,
};

pub fn initialize_wgpu_rendering(ctx: &egui::Context, render_state: &RenderState) {
    tracing::debug!(adapter = ?render_state.device.adapter_info());

    let widget_render_state = WidgetRenderState {
        device: render_state.device.clone(),
        queue: render_state.queue.clone(),
        target_texture_format: render_state.target_format,
    };

    let callback_resources = &mut render_state.renderer.write().callback_resources;

    // eframe doesn't give us some info we need in the paint callback, so we need to
    // store it in the callback resources.
    callback_resources.insert(widget_render_state.clone());

    // we also sometimes want access to the device and queue where we only have
    // access to egui's context.
    ctx.data_mut(|data| data.insert_temp(egui::Id::NULL, widget_render_state));
}

#[derive(Clone, Debug)]
#[allow(unused)]
pub(crate) struct WidgetRenderState {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub target_texture_format: wgpu::TextureFormat,
}

pub(crate) trait GetWidgetRenderState {
    fn widget_render_state(&self) -> Option<WidgetRenderState>;

    fn expect_widget_render_state(&self) -> WidgetRenderState {
        {
            self.widget_render_state()
                .expect("WidgetRenderState not found. You need to call initialize_wgpu_rendering.")
        }
    }
}

impl GetWidgetRenderState for egui::Context {
    fn widget_render_state(&self) -> Option<WidgetRenderState> {
        self.data(|data| data.get_temp(egui::Id::NULL))
    }
}

impl GetWidgetRenderState for CallbackResources {
    fn widget_render_state(&self) -> Option<WidgetRenderState> {
        self.get().cloned()
    }
}
