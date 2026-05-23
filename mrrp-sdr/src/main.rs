pub mod app;
pub mod config;
pub mod directories;
pub mod ui;

use anyhow::Error;
use clap::Parser;
use dotenvy::dotenv;
use eframe::NativeOptions;
use egui::ViewportBuilder;

use crate::{
    app::App,
    config::Config,
    directories::Directories,
};

fn main() -> Result<(), Error> {
    let _ = dotenv();
    tracing_subscriber::fmt::init();

    let _args = Args::parse();

    let directories = Directories::new()?;

    let config = Config::read_or_default(directories.config_path())?;

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
            persistence_path: Some(directories.egui_persist_path()),
            ..Default::default()
        },
        Box::new(|cc| {
            let wgpu = cc.wgpu_render_state.as_ref().expect("wgpu render state");
            tracing::debug!(adapter = ?wgpu.device.adapter_info());

            // eframe doesn't give us some info we need in the paint callback, so we need to
            // store it in the callback resources.
            wgpu.renderer
                .write()
                .callback_resources
                .insert(RenderConfig {
                    target_texture_format: wgpu.target_format,
                });

            Ok(Box::new(App::new(directories, config)))
        }),
    )?;

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    // todo
}

#[derive(Clone, Debug)]
pub struct RenderConfig {
    pub target_texture_format: wgpu::TextureFormat,
}
