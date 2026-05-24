pub mod app;
pub mod cli;
pub mod config;
pub mod directories;
pub mod hal;
pub mod ui;

use anyhow::Error;
use clap::Parser;
use dotenvy::dotenv;
use eframe::NativeOptions;
use egui::ViewportBuilder;
use rtl_sdr_rs::RtlSdr;

use crate::{
    app::App,
    cli::{
        Cli,
        Command,
        UiCommand,
    },
    config::Config,
    directories::Directories,
};

fn main() -> Result<(), Error> {
    let _ = dotenv();
    tracing_subscriber::fmt::init();

    let args = Cli::parse();

    let directories = Directories::new()?;

    let config = Config::read_or_default(directories.config_path())?;

    match args.command.unwrap_or_default() {
        Command::ListDevices => {
            for device in hal::radio::list_devices()? {
                println!("{device:?}");
            }
        }
        Command::Ui(command) => {
            run_ui(directories, config, command)?;
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
pub struct RenderConfig {
    pub target_texture_format: wgpu::TextureFormat,
}

fn run_ui(directories: Directories, config: Config, command: UiCommand) -> Result<(), Error> {
    let egui_persist_path = directories.state_dir().join("egui.json");
    tracing::debug!(?egui_persist_path);

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

            // add phosphor icons
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(App::new(directories, config, command)))
        }),
    )?;

    Ok(())
}
