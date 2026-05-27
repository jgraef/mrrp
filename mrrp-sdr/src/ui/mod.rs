pub mod about_window;
pub mod app;
pub mod debug_window;
pub mod dock;
pub mod menu;
pub mod mock;
pub mod radio;
pub mod state;
pub mod widgets;

use anyhow::Error;
use eframe::NativeOptions;
use egui::ViewportBuilder;

use crate::{
    cli::UiCommand,
    config::Config,
    directories::Directories,
    ui::app::App,
};

pub fn run_app(directories: Directories, config: Config, command: UiCommand) -> Result<(), Error> {
    let egui_persist_path = directories.state_dir().join("egui.json");
    tracing::debug!(?egui_persist_path);

    // create and enter tokio runtime. this way we still have full control over the
    // main thread. the ui has to run an event loop on the main thread, so we can't
    // give it to tokio.
    //
    // but we still can use the full tokio runtime now, to e.g. spawn futures.
    let tokio_runtime = tokio::runtime::Runtime::new()?;
    let _runtime_guard = tokio_runtime.enter();

    eframe::run_native(
        "mrrp-sdr",
        NativeOptions {
            viewport: ViewportBuilder {
                title: Some("mrrp-sdr".to_owned()),
                app_id: Some("mrrp-sdr".to_owned()),
                ..Default::default()
            },
            renderer: eframe::Renderer::Wgpu,
            run_and_return: true,
            persist_window: true,
            persistence_path: Some(egui_persist_path),
            ..Default::default()
        },
        Box::new(|cc| {
            mrrp_widgets::initialize_wgpu_rendering(
                &cc.egui_ctx,
                cc.wgpu_render_state
                    .as_ref()
                    .expect("wgpu_render_state not present"),
            );

            let mut fonts = egui::FontDefinitions::default();

            // add phosphor icons

            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

            // add dseg font
            // for now we'll just include_bytes one variant
            // todo: make these an option in the theme
            fonts.font_data.insert(
                "dseg".into(),
                egui::FontData::from_static(include_bytes!("../../assets/DSEG7Modern-Regular.ttf"))
                    .into(),
            );

            fonts
                .families
                .insert(egui::FontFamily::Name("dseg".into()), vec!["dseg".into()]);

            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(App::new(
                directories,
                config,
                command,
                &cc.egui_ctx,
                cc.storage.expect("persist"),
            )))
        }),
    )?;

    Ok(())
}

#[derive(Clone, Debug)]
pub struct RenderConfig {
    pub target_texture_format: wgpu::TextureFormat,
}
